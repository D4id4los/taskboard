// SPDX-License-Identifier: MIT OR Apache-2.0
//! Fixtures shared by the state crate's integration suites: the logical
//! test clock, a board-bearing persisted seed, and stranger ids. Each
//! test binary uses a different subset, hence the module-level
//! `dead_code` allowance.

#![allow(dead_code)]

use std::sync::Arc;

use chrono::TimeZone;

use taskboard_domain::idgen::IdGenerator as _;
use taskboard_domain::test_support::CountingIds;
use taskboard_domain::{Board, Clock, PersistedState, TaskId};

pub const T0: i64 = 36_000; // 10:00:00Z

pub fn ts(secs: i64) -> chrono::DateTime<chrono::Utc> {
    chrono::Utc.timestamp_opt(secs, 0).unwrap()
}

/// Logical clock: every message in a scenario observes the same instant
/// unless advanced explicitly.
#[derive(Debug)]
pub struct FixedClock(std::sync::Mutex<i64>);

impl FixedClock {
    pub fn new(secs: i64) -> Arc<Self> {
        Arc::new(Self(std::sync::Mutex::new(secs)))
    }

    pub fn advance(&self, secs: i64) {
        *self.0.lock().expect("poisoned") += secs;
    }
}

impl Clock for FixedClock {
    fn now(&self) -> chrono::DateTime<chrono::Utc> {
        ts(*self.0.lock().expect("poisoned"))
    }
}

/// A live (unbound) board with a deterministic id — no command creates
/// boards, so scenarios seed one.
pub fn board(title: &str) -> Board {
    let ids = CountingIds::new();
    Board {
        id: ids.new_board_id(),
        remote: None,
        title: title.into(),
        color: taskboard_domain::Color::new("0000ff"),
        archived: false,
        deleted: false,
        clocks: taskboard_domain::BoardClocks {
            color: taskboard_domain::BoardClocks::epoch(),
            archived: taskboard_domain::BoardClocks::epoch(),
            deleted: taskboard_domain::BoardClocks::epoch(),
            title: taskboard_domain::BoardClocks::epoch(),
        },
        remote_seen: None,
    }
}

/// A persisted state holding exactly one live board.
pub fn seed_board_state(title: &str) -> PersistedState {
    let board = board(title);
    let mut state = PersistedState::default();
    state.boards.insert(board.id, board);
    state
}

/// [`seed_board_state`] with the board bound: the per-board status
/// semantics (failed reports, offline transitions) address bound boards,
/// so scenarios exercising them seed this shape.
#[must_use]
pub fn seed_bound_board_state(title: &str) -> PersistedState {
    let mut state = seed_board_state(title);
    let board = state.boards.values_mut().next().expect("one board");
    board.remote =
        taskboard_domain::test_support::test_binding(taskboard_domain::RemoteBoardId(42));
    state
}

/// A `Completed` report's validator triple with empty header sets: no
/// `UpsertValidators` action can follow (nothing stored to differ from).
pub fn test_validators() -> taskboard_domain::BoardPullValidators {
    let empty = taskboard_domain::SyncValidators::default();
    taskboard_domain::BoardPullValidators {
        boards: empty.clone(),
        stacks: empty.clone(),
        archived_stacks: empty,
    }
}

/// An id guaranteed absent from the current scenario: burned from a fresh
/// counter far past any id count a scenario can reach.
pub fn stranger_task_id() -> TaskId {
    let ids = CountingIds::new();
    for _ in 0..1_000 {
        ids.new_op_id();
    }
    ids.new_task_id()
}

/// An id guaranteed absent from the current scenario (board analogue of
/// [`stranger_task_id`]).
pub fn stranger_board_id() -> taskboard_domain::BoardId {
    let ids = CountingIds::new();
    for _ in 0..1_000 {
        ids.new_op_id();
    }
    ids.new_board_id()
}

/// Fails the first `failures` applies with `Unavailable`, then delegates —
/// non-mutating on failure, matching transactional reality.
#[derive(Debug)]
pub struct FlakyRepository {
    inner: taskboard_domain::test_support::InMemoryRepository,
    remaining_failures: std::sync::atomic::AtomicU32,
}

impl FlakyRepository {
    pub fn new(failures: u32, state: PersistedState) -> Self {
        Self {
            inner: taskboard_domain::test_support::InMemoryRepository::with_state(state),
            remaining_failures: std::sync::atomic::AtomicU32::new(failures),
        }
    }

    /// Re-arms the failure injector (the edge-trigger test walks the
    /// fail -> recover -> fail cycle).
    pub fn rearm(&self, failures: u32) {
        self.remaining_failures
            .store(failures, std::sync::atomic::Ordering::SeqCst);
    }
}

impl taskboard_domain::persistence::TaskRepository for FlakyRepository {
    fn load(
        &self,
    ) -> taskboard_domain::persistence::BoxFuture<
        '_,
        Result<PersistedState, taskboard_domain::persistence::RepositoryError>,
    > {
        self.inner.load()
    }

    fn apply(
        &self,
        actions: Vec<taskboard_domain::persistence::PersistenceAction>,
    ) -> taskboard_domain::persistence::BoxFuture<
        '_,
        Result<(), taskboard_domain::persistence::RepositoryError>,
    > {
        Box::pin(async move {
            let should_fail = self
                .remaining_failures
                .try_update(
                    std::sync::atomic::Ordering::SeqCst,
                    std::sync::atomic::Ordering::SeqCst,
                    |n| (n > 0).then(|| n - 1),
                )
                .is_ok();
            if should_fail {
                return Err(taskboard_domain::persistence::RepositoryError::Unavailable);
            }
            self.inner.apply(actions).await
        })
    }
}
