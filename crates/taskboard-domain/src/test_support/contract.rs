// SPDX-License-Identifier: MIT OR Apache-2.0
//! Repository contract harness.
//!
//! Any [`TaskRepository`] implementation must satisfy these behavioral
//! facts. `InMemoryRepository` runs them in-repo (proven at build time);
//! Phase 2 re-runs the identical harness against the sqlite adapter by
//! calling [`assert_task_repository_contract_async`] from its own async
//! tests.

use std::task::{Context, Poll, Waker};

use chrono::{TimeZone, Utc};

use crate::clock::{Clock, SystemClock};
use crate::entities::{Task, TaskClocks};
use crate::ids::{StackId, TaskId};
use crate::outbox::{LocalOp, OpId, PendingOp};
use crate::persistence::{
    PersistedState, PersistenceAction, SyncValidators, TaskRepository, ValidatorKey,
};
use crate::state::SyncStatus;

/// Runs the full contract against `repo`, awaiting futures with the caller's
/// runtime (real IO implementations need a reactor; the in-memory fake is
/// ready under any executor).
///
/// # Panics
///
/// Panics on the first violated contract clause (with the clause name).
#[allow(clippy::too_many_lines)] // one sequential contract; splitting hides the flow
pub async fn assert_task_repository_contract_async<R: TaskRepository>(repo: &R) {
    // A repo starts loadable, even if empty.
    let initial: PersistedState = repo.load().await.expect("load");

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
    repo.apply(batch).await.expect("apply batch");

    let after: PersistedState = repo.load().await.expect("reload");
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
    assert_eq!(
        after.sync.pending_ops as usize,
        after.outbox.len(),
        "pending_ops must be derived from the outbox depth"
    );

    repo.apply(vec![PersistenceAction::CompleteOp(op.op_id)])
        .await
        .expect("complete op");
    let after = repo.load().await.expect("reload");
    assert!(
        !after.outbox.iter().any(|o| o.op_id == op.op_id),
        "CompleteOp must remove the op"
    );
    assert_eq!(
        after.tasks.get(&task.id),
        Some(&task),
        "CompleteOp must not touch entities"
    );
    assert_eq!(
        after.sync.pending_ops as usize,
        after.outbox.len(),
        "completing an op must move the derived pending_ops"
    );

    // FailOp on an already-absent id is a no-op, not an error.
    repo.apply(vec![PersistenceAction::FailOp(op.op_id)])
        .await
        .expect("fail absent op");

    // Validators upsert and overwrite.
    let key = ValidatorKey::Boards;
    let v = SyncValidators {
        etag: Some("\"v1\"".into()),
        last_modified: None,
    };
    repo.apply(vec![PersistenceAction::UpsertValidators(key, v.clone())])
        .await
        .expect("validators");
    let after = repo.load().await.expect("reload");
    assert_eq!(
        after.validators.get(&key),
        Some(&v),
        "validators must round-trip"
    );

    // Sync status persists phase + last success; pending_ops stays derived
    // from the (non-empty) outbox, not from the status payload.
    let op2 = PendingOp {
        op_id: OpId(uuid::Uuid::from_u128(4)),
        op: LocalOp::UpdateTask(task.id),
        queued_at: SystemClock.now(),
    };
    repo.apply(vec![PersistenceAction::EnqueueOp(op2.clone())])
        .await
        .expect("enqueue for sync status");
    let status = SyncStatus {
        phase: crate::state::SyncPhase::Offline,
        last_success: Some(Utc.timestamp_opt(1_700_000_000, 0).unwrap()),
        pending_ops: 999, // stored payload drift; must be ignored on load
    };
    repo.apply(vec![PersistenceAction::UpsertSyncStatus(status.clone())])
        .await
        .expect("sync status");
    let after = repo.load().await.expect("reload");
    assert_eq!(after.sync.phase, status.phase, "sync phase must round-trip");
    assert_eq!(
        after.sync.last_success, status.last_success,
        "last_success must round-trip"
    );
    assert_eq!(
        after.sync.pending_ops as usize,
        after.outbox.len(),
        "pending_ops must be re-derived, not taken from the status payload"
    );

    repo.apply(vec![PersistenceAction::CompleteOp(op2.op_id)])
        .await
        .expect("complete second op");
    let after = repo.load().await.expect("reload");
    assert_eq!(
        after.sync.pending_ops as usize,
        after.outbox.len(),
        "outbox depth remains the single source of truth"
    );

    // Re-enqueueing an existing op id replaces the entry in place and keeps
    // its original queue position (the sqlite upsert-on-op_id semantics the
    // storage adapter implements; both implementations are pinned here).
    let first = PendingOp {
        op_id: OpId(uuid::Uuid::from_u128(5)),
        op: LocalOp::CreateStack(stack.id),
        queued_at: SystemClock.now(),
    };
    let second = PendingOp {
        op_id: OpId(uuid::Uuid::from_u128(6)),
        op: LocalOp::CreateLabel(label.id),
        queued_at: SystemClock.now(),
    };
    repo.apply(vec![
        PersistenceAction::EnqueueOp(first.clone()),
        PersistenceAction::EnqueueOp(second.clone()),
    ])
    .await
    .expect("enqueue pair");
    let replacement = PendingOp {
        op_id: first.op_id,
        op: LocalOp::RenameStack(stack.id),
        queued_at: SystemClock.now(),
    };
    repo.apply(vec![PersistenceAction::EnqueueOp(replacement.clone())])
        .await
        .expect("re-enqueue existing id");
    let after = repo.load().await.expect("reload");
    assert_eq!(
        after.outbox,
        vec![replacement, second],
        "re-enqueue must replace in place, keeping the queue position"
    );
    assert_eq!(
        after.sync.pending_ops as usize,
        after.outbox.len(),
        "re-enqueue must not duplicate queue depth"
    );
}

/// Runs the full contract against `repo`, awaiting futures synchronously.
///
/// Only for implementations whose futures are immediately ready (the
/// in-memory fake); real IO backends need
/// [`assert_task_repository_contract_async`] under an async runtime.
///
/// # Panics
///
/// Panics on the first violated contract clause (with the clause name), or
/// if an implementation's future stays `Pending` under the harness' no-op
/// waker (contract implementations must make progress without a reactor).
pub fn assert_task_repository_contract<R: TaskRepository>(repo: &R) {
    block_on(assert_task_repository_contract_async(repo));
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
pub(crate) fn block_on<T>(fut: impl std::future::Future<Output = T>) -> T {
    let mut fut = Box::pin(fut);
    match fut.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("repository futures must not actually wait"),
    }
}

#[cfg(test)]
mod tests {
    use super::assert_task_repository_contract;
    use crate::persistence::PersistedState;
    use crate::persistence::TaskRepository;
    use crate::state::{SyncPhase, SyncStatus};
    use crate::test_support::memory::InMemoryRepository;

    #[test]
    fn in_memory_repository_satisfies_contract() {
        assert_task_repository_contract(&InMemoryRepository::new());
    }

    #[test]
    fn in_memory_repository_preserves_preloaded_state_currently_pins_derivation() {
        // Deliberate contract change (phase 2 decision 9): `pending_ops` is
        // never stored; it is re-derived from the outbox depth at load. A
        // preloaded status keeps its phase and last_success; a hand-set
        // pending_ops counter does not survive the load boundary.
        let state = PersistedState {
            sync: SyncStatus {
                phase: SyncPhase::Offline,
                last_success: None,
                pending_ops: 7,
            },
            ..PersistedState::default()
        };
        let repo = InMemoryRepository::with_state(state);
        let loaded = crate::test_support::contract::block_on(repo.load()).expect("load");
        assert_eq!(loaded.sync.phase, SyncPhase::Offline);
        assert_eq!(loaded.sync.pending_ops, 0);
    }
}
