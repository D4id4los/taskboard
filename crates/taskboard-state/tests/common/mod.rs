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

/// An id guaranteed absent from the current scenario: burned from a fresh
/// counter far past any id count a scenario can reach.
pub fn stranger_task_id() -> TaskId {
    let ids = CountingIds::new();
    for _ in 0..1_000 {
        ids.new_op_id();
    }
    ids.new_task_id()
}
