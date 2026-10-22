// SPDX-License-Identifier: MIT OR Apache-2.0
//! C1/C2 — the domain contract harness run against the sqlite adapter,
//! on `:memory:` and on a file-backed database (the plan's Phase 2 exit
//! criterion).

mod common;

use common::FileDb;
use taskboard_domain::test_support::contract::assert_task_repository_contract_async;
use taskboard_storage_sqlite::open_memory;

#[tokio::test]
async fn sqlite_memory_repository_satisfies_contract() {
    let repo = open_memory(taskboard_domain::ConnectorId::from("nextcloud"))
        .await
        .expect("open memory db");
    assert_task_repository_contract_async(&repo).await;
}

#[tokio::test]
async fn sqlite_file_repository_satisfies_contract() {
    let mut db = FileDb::new().await;
    assert_task_repository_contract_async(db.repo()).await;
    db.close().await;
}

/// Per-board status parity (multi-board chunk 1): the fake and sqlite
/// agree on two boards' stored rows, and both rows survive a reload —
/// the lockstep the re-keyed `UpsertSyncStatus` action requires.
#[tokio::test]
async fn per_board_status_roundtrip_and_fake_parity() {
    use taskboard_domain::outbox::{LocalOp, OpId, PendingOp};
    use taskboard_domain::persistence::PersistenceAction;
    use taskboard_domain::state::{SyncErrorKind, SyncPhase, SyncStatus};
    use taskboard_domain::{entities, ids};

    use taskboard_domain::persistence::TaskRepository as _;

    let clock = chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00+00:00")
        .ok()
        .map(chrono::DateTime::<chrono::Utc>::from)
        .expect("fixed stamp");
    let board = |raw: u128, remote: Option<u64>| entities::Board {
        id: ids::BoardId::from(uuid::Uuid::from_u128(raw)),
        remote: remote.map(|board| ids::RemoteBoardBinding {
            connector: taskboard_domain::ConnectorId::from("nextcloud"),
            board: ids::RemoteBoardId(board),
        }),
        title: format!("board {raw}"),
        color: taskboard_domain::Color::new("0000ff"),
        archived: false,
        deleted: false,
        clocks: taskboard_domain::BoardClocks {
            title: taskboard_domain::BoardClocks::epoch(),
            color: taskboard_domain::BoardClocks::epoch(),
            archived: taskboard_domain::BoardClocks::epoch(),
            deleted: taskboard_domain::BoardClocks::epoch(),
        },
        remote_seen: None,
    };
    let first = board(0xd1, Some(101));
    let second = board(0xd2, Some(202));
    let status = |phase: SyncPhase, at: Option<chrono::DateTime<chrono::Utc>>| SyncStatus {
        phase,
        last_success: at,
        pending_ops: 0, // derived, never stored
        counters: taskboard_domain::SyncCounters::default(),
        ..SyncStatus::default()
    };

    let mut actions = vec![
        PersistenceAction::UpsertBoard(first.clone()),
        PersistenceAction::UpsertBoard(second.clone()),
    ];
    actions.push(PersistenceAction::UpsertSyncStatus(
        first.id,
        status(
            SyncPhase::Failed {
                last_error: SyncErrorKind::Network,
            },
            Some(clock),
        ),
    ));
    actions.push(PersistenceAction::UpsertSyncStatus(
        second.id,
        status(SyncPhase::Offline, None),
    ));

    let fake = taskboard_domain::test_support::InMemoryRepository::new();
    fake.apply(actions.clone()).await.expect("fake applies");

    let repo = open_memory(taskboard_domain::ConnectorId::from("nextcloud"))
        .await
        .expect("open memory db");
    repo.apply(actions).await.expect("sqlite applies");

    // A fabricated op keeps the derivation's pending counts nonzero but
    // equal on both sides (parity is the point, not the depth).
    let op = PendingOp {
        op_id: OpId(uuid::Uuid::from_u128(0xd01)),
        op: LocalOp::UpdateBoard(first.id),
        queued_at: clock,
    };
    fake.apply(vec![PersistenceAction::EnqueueOp(op.clone())])
        .await
        .expect("fake enqueues");
    repo.apply(vec![PersistenceAction::EnqueueOp(op)])
        .await
        .expect("sqlite enqueues");

    let fake_state = fake.load().await.expect("fake loads");
    let sqlite_state = repo.load().await.expect("sqlite loads");
    assert_eq!(
        fake_state.sync, sqlite_state.sync,
        "fake and sqlite agree on the whole per-board map"
    );
    let first_row = sqlite_state.sync.get(first.id).expect("first row");
    assert_eq!(
        first_row.phase,
        SyncPhase::Failed {
            last_error: SyncErrorKind::Network
        },
        "the first board's stored phase survives reload"
    );
    assert_eq!(first_row.last_success, Some(clock));
    assert!(
        sqlite_state.sync.get(second.id).is_some(),
        "the second board's row survives reload"
    );
    assert_eq!(
        sqlite_state.sync.aggregate().pending_ops,
        1,
        "the fabricated op counts to its board exactly once"
    );
    repo.close().await;
}
