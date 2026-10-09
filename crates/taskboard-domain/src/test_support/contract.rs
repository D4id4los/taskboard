// SPDX-License-Identifier: MIT OR Apache-2.0
//! Repository contract harness.
//!
//! Any [`TaskRepository`] implementation must satisfy these behavioral
//! facts. `InMemoryRepository` runs them in-repo (proven at build time);
//! Phase 2 re-runs the identical harness against the sqlite adapter by
//! calling [`assert_task_repository_contract`] from its own tests.

use std::task::{Context, Poll, Waker};

use chrono::{TimeZone, Utc};

use crate::clock::{Clock, SystemClock};
use crate::entities::{Task, TaskClocks};
use crate::ids::{StackId, TaskId};
use crate::ops::{LocalOp, OpId, PendingOp};
use crate::persistence::{
    PersistedState, PersistenceAction, SyncValidators, TaskRepository, ValidatorKey,
};

/// Runs the full contract against `repo`, awaiting futures synchronously.
///
/// # Panics
///
/// Panics on the first violated contract clause (with the clause name), or
/// if an implementation's future stays `Pending` under the harness' no-op
/// waker (contract implementations must make progress without a reactor).
#[allow(clippy::too_many_lines)] // one sequential contract; splitting hides the flow
pub fn assert_task_repository_contract<R: TaskRepository>(repo: &R) {
    // A repo starts loadable, even if empty.
    let initial: PersistedState = block_on(repo.load()).expect("load");

    // Entities of every kind inserted by a batch are returned by the next
    // load, and CompleteOp removes the enqueued op from the outbox without
    // touching entities.
    let board = crate::entities::Board {
        id: crate::ids::BoardId::from(uuid::Uuid::from_u128(11)),
        remote: None,
        title: "board".into(),
        color: crate::entities::Color::new("ff0000"),
        archived: false,
        deleted: false,
        remote_seen: None,
    };
    let stack = crate::entities::Stack {
        id: StackId::from(uuid::Uuid::from_u128(2)),
        remote: None,
        board: board.id,
        title: "stack".into(),
        order: 0,
        archived: false,
        deleted: false,
        clocks: stack_clocks_at(0),
        remote_seen: None,
    };
    let label = crate::entities::Label {
        id: crate::ids::LabelId::from(uuid::Uuid::from_u128(12)),
        remote: None,
        board: board.id,
        title: "label".into(),
        color: crate::entities::Color::new("00ff00"),
        deleted: false,
        clocks: label_clocks_at(0),
        remote_seen: None,
    };
    let task = Task {
        id: TaskId::from(uuid::Uuid::from_u128(1)),
        remote: None,
        title: "contract".into(),
        description: String::new(),
        duedate: None,
        done: None,
        stack: StackId::from(uuid::Uuid::from_u128(2)),
        order: 0,
        labels: std::collections::BTreeSet::new(),
        archived: false,
        deleted: false,
        clocks: clock_at(0),
        remote_seen: None,
    };
    let op = PendingOp {
        op_id: OpId(uuid::Uuid::from_u128(3)),
        op: LocalOp::CreateTask(task.id),
        queued_at: SystemClock.now(),
    };
    let batch = vec![
        PersistenceAction::UpsertBoard(board.clone()),
        PersistenceAction::UpsertStack(stack.clone()),
        PersistenceAction::UpsertLabel(label.clone()),
        PersistenceAction::UpsertTask(task.clone()),
        PersistenceAction::EnqueueOp(op.clone()),
    ];
    block_on(repo.apply(batch)).expect("apply batch");

    let after: PersistedState = block_on(repo.load()).expect("reload");
    assert_eq!(
        after.boards.get(&board.id),
        Some(&board),
        "upserted board must round-trip"
    );
    assert_eq!(
        after.stacks.get(&stack.id),
        Some(&stack),
        "upserted stack must round-trip"
    );
    assert_eq!(
        after.labels.get(&label.id),
        Some(&label),
        "upserted label must round-trip"
    );
    assert_eq!(
        after.tasks.get(&task.id),
        Some(&task),
        "upserted task must round-trip"
    );
    assert_eq!(after.outbox.len(), initial.outbox.len() + 1);
    assert!(
        after.outbox.iter().any(|o| o.op_id == op.op_id),
        "enqueued op must be present"
    );

    block_on(repo.apply(vec![PersistenceAction::CompleteOp(op.op_id)])).expect("complete op");
    let after = block_on(repo.load()).expect("reload");
    assert!(
        !after.outbox.iter().any(|o| o.op_id == op.op_id),
        "CompleteOp must remove the op"
    );
    assert_eq!(
        after.tasks.get(&task.id),
        Some(&task),
        "CompleteOp must not touch entities"
    );

    // FailOp on an already-absent id is a no-op, not an error.
    block_on(repo.apply(vec![PersistenceAction::FailOp(op.op_id)])).expect("fail absent op");

    // Validators upsert and overwrite.
    let key = ValidatorKey::Boards;
    let v = SyncValidators {
        etag: Some("\"v1\"".into()),
        last_modified: None,
    };
    block_on(repo.apply(vec![PersistenceAction::UpsertValidators(key, v.clone())]))
        .expect("validators");
    let after = block_on(repo.load()).expect("reload");
    assert_eq!(
        after.validators.get(&key),
        Some(&v),
        "validators must round-trip"
    );
}

fn stack_clocks_at(secs: i64) -> crate::entities::StackClocks {
    let t = Utc.timestamp_opt(secs, 0).unwrap();
    crate::entities::StackClocks {
        title: t,
        order: t,
        deleted: t,
    }
}

fn label_clocks_at(secs: i64) -> crate::entities::LabelClocks {
    let t = Utc.timestamp_opt(secs, 0).unwrap();
    crate::entities::LabelClocks {
        title: t,
        color: t,
        deleted: t,
    }
}

fn clock_at(secs: i64) -> TaskClocks {
    let t = Utc.timestamp_opt(secs, 0).unwrap();
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
}

/// Executor-free await via the built-in no-op waker: polls once and panics
/// if the future reports `Pending` (the fakes must be ready).
fn block_on<T>(fut: impl std::future::Future<Output = T>) -> T {
    let mut fut = Box::pin(fut);
    match fut.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("repository futures must not actually wait"),
    }
}

#[cfg(test)]
mod tests {
    use super::assert_task_repository_contract;
    use crate::test_support::memory::InMemoryRepository;

    #[test]
    fn in_memory_repository_satisfies_contract() {
        assert_task_repository_contract(&InMemoryRepository::new());
    }

    #[test]
    fn in_memory_repository_preserves_preloaded_state() {
        let mut state = crate::persistence::PersistedState::default();
        state.sync.pending_ops = 7;
        let repo = crate::test_support::memory::InMemoryRepository::with_state(state);
        assert_eq!(repo.snapshot().sync.pending_ops, 7);
    }
}
