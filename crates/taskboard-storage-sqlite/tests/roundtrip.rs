// SPDX-License-Identifier: MIT OR Apache-2.0
//! P1–P4 round-trip proptests and T5–T8 targeted storage semantics tests
//! (plan §10 catalogue).

mod common;

use std::collections::{BTreeMap, BTreeSet};

use chrono::{TimeZone, Utc};
use proptest::prelude::*;
use taskboard_domain::entities::{
    Board, Color, Label, LabelClocks, Stack, StackClocks, Task, TaskClocks,
};
use taskboard_domain::ids::{
    BoardId, LabelId, RemoteBoardId, RemoteCardId, RemoteCardRef, RemoteLabelId, RemoteLabelRef,
    RemoteStackId, RemoteStackRef, StackId, TaskId,
};
use taskboard_domain::outbox::{LocalOp, OpId, PendingOp};
use taskboard_domain::persistence::{
    PersistedState, PersistenceAction, RepositoryError, SyncValidators, TaskRepository,
    ValidatorKey,
};
use taskboard_domain::state::{SyncErrorKind, SyncPhase, SyncStates};
use taskboard_storage_sqlite::open_memory;

use common::actions_from_state;

fn ts(secs: i64) -> chrono::DateTime<Utc> {
    Utc.timestamp_opt(secs, 0).unwrap()
}

fn task_clocks(secs: i64) -> TaskClocks {
    let t = ts(secs);
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

fn stack_clocks(secs: i64) -> StackClocks {
    let t = ts(secs);
    StackClocks {
        title: t,
        order: t,
        deleted: t,
    }
}

fn label_clocks(secs: i64) -> LabelClocks {
    let t = ts(secs);
    LabelClocks {
        title: t,
        color: t,
        deleted: t,
    }
}

fn board(id: u128) -> Board {
    Board {
        id: BoardId::from(uuid::Uuid::from_u128(id)),
        remote: Some(taskboard_domain::RemoteBoardBinding {
            connector: taskboard_domain::ConnectorId::from("nextcloud"),
            board: RemoteBoardId(77),
        }),
        title: "board".into(),
        color: Color::new("ff0000"),
        archived: false,
        deleted: false,
        clocks: taskboard_domain::BoardClocks {
            title: ts(100),
            color: ts(100),
            archived: ts(100),
            deleted: ts(100),
        },
        remote_seen: Some(ts(100)),
    }
}

fn stack(id: u128, board: &Board) -> Stack {
    Stack {
        id: StackId::from(uuid::Uuid::from_u128(id)),
        remote: Some(RemoteStackRef {
            board: RemoteBoardId(77),
            stack: RemoteStackId(7),
        }),
        board: board.id,
        title: "stack".into(),
        order: 3,
        archived: false,
        deleted: false,
        clocks: stack_clocks(1),
        remote_seen: None,
    }
}

fn label(id: u128, board: &Board) -> Label {
    Label {
        id: LabelId::from(uuid::Uuid::from_u128(id)),
        remote: Some(RemoteLabelRef {
            board: RemoteBoardId(77),
            label: RemoteLabelId(9),
        }),
        board: board.id,
        title: "label".into(),
        color: Color::new("00ff00"),
        deleted: false,
        clocks: label_clocks(2),
        remote_seen: Some(ts(101)),
    }
}

fn task(id: u128, stack: &Stack, labels: BTreeSet<LabelId>) -> Task {
    Task {
        id: TaskId::from(uuid::Uuid::from_u128(id)),
        remote: Some(RemoteCardRef {
            board: RemoteBoardId(77),
            stack: RemoteStackId(7),
            card: RemoteCardId(8),
        }),
        title: "task".into(),
        description: "desc".into(),
        duedate: Some(ts(200)),
        done: None,
        stack: stack.id,
        order: 1,
        labels,
        archived: false,
        deleted: false,
        clocks: task_clocks(5),
        remote_seen: Some(ts(102)),
    }
}

// P1 — full-state round trip through one apply batch: everything comes
// back identical except `sync.pending_ops`, which is re-derived from the
// outbox depth.
proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(256))]

    #[test]
    fn persisted_state_roundtrips_through_sqlite(
        state in taskboard_domain::test_support::persisted_state_strategy(),
    ) {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime")
            .block_on(async move {
                let repo = open_memory(taskboard_domain::ConnectorId::from("nextcloud")).await.expect("open");
                repo.apply(actions_from_state(&state)).await.expect("apply");
                let loaded = repo.load().await.expect("load");

                prop_assert_eq!(loaded.boards, state.boards);
                prop_assert_eq!(loaded.stacks, state.stacks);
                prop_assert_eq!(loaded.labels, state.labels);
                prop_assert_eq!(loaded.tasks, state.tasks);
                let expected_pending = state.outbox.len();
                prop_assert_eq!(loaded.outbox, state.outbox);
                prop_assert_eq!(loaded.validators, state.validators);
                prop_assert_eq!(loaded.reminders, state.reminders);
                prop_assert_eq!(loaded.sync.aggregate().phase, state.sync.aggregate().phase);
                prop_assert_eq!(loaded.sync.aggregate().last_success, state.sync.aggregate().last_success);
                prop_assert_eq!(
                    loaded.sync.aggregate().pending_ops as usize,
                    expected_pending,
                    "pending_ops must be derived from the outbox depth"
                );
                Ok::<(), proptest::test_runner::TestCaseError>(())
            })
            .expect("proptest case");
    }
}

/// P2 — outbox order is the enqueue order (`op_seq`), independent of
/// `queued_at`; CompleteOp/FailOp remove by id without disturbing the rest.
#[tokio::test]
async fn outbox_preserves_enqueue_order_across_removals() {
    let repo = open_memory(taskboard_domain::ConnectorId::from("nextcloud"))
        .await
        .expect("open");
    // The ops' task must exist: the per-board pending attribution runs
    // through the entity graph.
    let task_id = TaskId::from(uuid::Uuid::from_u128(1));
    let seed_board = board(91);
    let seed_stack = stack(90, &seed_board);
    let seed_task = task(1, &seed_stack, BTreeSet::default());
    repo.apply(vec![
        PersistenceAction::UpsertBoard(seed_board),
        PersistenceAction::UpsertStack(seed_stack),
        PersistenceAction::UpsertTask(seed_task),
    ])
    .await
    .expect("seed entities");
    let ids: Vec<OpId> = (0..5).map(|i| OpId(uuid::Uuid::from_u128(i))).collect();
    let batch: Vec<PersistenceAction> = ids
        .iter()
        .map(|op_id| {
            PersistenceAction::EnqueueOp(PendingOp {
                op_id: *op_id,
                op: LocalOp::UpdateTask(task_id),
                queued_at: ts(42), // identical seconds: collisions in practice
            })
        })
        .collect();
    repo.apply(batch).await.expect("enqueue");

    let loaded = repo.load().await.expect("load");
    let order: Vec<OpId> = loaded.outbox.iter().map(|o| o.op_id).collect();
    assert_eq!(order, ids, "queue order must be enqueue order");

    repo.apply(vec![PersistenceAction::CompleteOp(ids[2])])
        .await
        .expect("complete");
    repo.apply(vec![PersistenceAction::FailOp(ids[0])])
        .await
        .expect("fail");
    let loaded = repo.load().await.expect("load");
    let order: Vec<OpId> = loaded.outbox.iter().map(|o| o.op_id).collect();
    assert_eq!(order, vec![ids[1], ids[3], ids[4]]);
    assert_eq!(loaded.sync.aggregate().pending_ops, 3);
}

/// P3 — upserting a modified entity overwrites every stored column.
#[tokio::test]
async fn upsert_overwrites_all_columns() {
    let repo = open_memory(taskboard_domain::ConnectorId::from("nextcloud"))
        .await
        .expect("open");
    let b = board(11);
    let s = stack(2, &b);
    repo.apply(vec![
        PersistenceAction::UpsertBoard(b.clone()),
        PersistenceAction::UpsertStack(s.clone()),
    ])
    .await
    .expect("apply");

    let mut renamed = s.clone();
    renamed.title = "renamed".into();
    renamed.order = -4;
    renamed.deleted = true;
    renamed.clocks = stack_clocks(99);
    renamed.remote_seen = None;
    repo.apply(vec![PersistenceAction::UpsertStack(renamed.clone())])
        .await
        .expect("apply");
    let loaded = repo.load().await.expect("load");
    assert_eq!(loaded.stacks.get(&s.id), Some(&renamed));
}

/// P4 — applying the same batch twice is idempotent (no duplicate outbox
/// rows; `op_id` upserts in place).
#[tokio::test]
async fn idempotent_reapply_leaves_same_state() {
    let repo = open_memory(taskboard_domain::ConnectorId::from("nextcloud"))
        .await
        .expect("open");
    let b = board(11);
    let op = PendingOp {
        op_id: OpId(uuid::Uuid::from_u128(3)),
        op: LocalOp::CreateTask(TaskId::from(uuid::Uuid::from_u128(1))),
        queued_at: ts(7),
    };
    let batch = vec![
        PersistenceAction::UpsertBoard(b.clone()),
        PersistenceAction::EnqueueOp(op.clone()),
    ];
    repo.apply(batch.clone()).await.expect("apply once");
    repo.apply(batch).await.expect("apply twice");

    let loaded = repo.load().await.expect("load");
    assert_eq!(loaded.boards.len(), 1);
    assert_eq!(loaded.outbox.len(), 1);
    assert_eq!(loaded.outbox[0].op_id, op.op_id);
}

/// T5 — upserting a task again must not cascade-delete its `task_labels`
/// rows (the `INSERT OR REPLACE` trap).
#[tokio::test]
async fn task_upsert_keeps_label_set() {
    let repo = open_memory(taskboard_domain::ConnectorId::from("nextcloud"))
        .await
        .expect("open");
    let b = board(11);
    let s = stack(2, &b);
    let l = label(12, &b);
    let t = task(1, &s, BTreeSet::from([l.id]));
    let batch = vec![
        PersistenceAction::UpsertBoard(b.clone()),
        PersistenceAction::UpsertStack(s.clone()),
        PersistenceAction::UpsertLabel(l.clone()),
        PersistenceAction::UpsertTask(t.clone()),
    ];
    repo.apply(batch.clone()).await.expect("apply");

    let mut retitled = t.clone();
    retitled.title = "changed".into();
    repo.apply(vec![PersistenceAction::UpsertTask(retitled.clone())])
        .await
        .expect("apply");

    let loaded = repo.load().await.expect("load");
    let stored = loaded.tasks.get(&t.id).expect("task still present");
    assert_eq!(stored.title, "changed");
    assert_eq!(
        stored.labels,
        BTreeSet::from([l.id]),
        "label set must survive the upsert"
    );
}

/// T6 — a mid-batch failure rolls back the whole transaction: neither the
/// entity upsert nor the enqueue survives. The failure is injected with a
/// temporary `RAISE(ABORT)` trigger on outbox inserts.
#[tokio::test]
async fn failed_batch_rolls_back_atomically() {
    let mut db = common::FileDb::new().await;
    let b = board(11);
    let s = stack(2, &b);
    db.repo()
        .apply(vec![
            PersistenceAction::UpsertBoard(b.clone()),
            PersistenceAction::UpsertStack(s.clone()),
        ])
        .await
        .expect("setup");

    // Raw second connection to the same file: installs the sabotage.
    let raw = sqlx::sqlite::SqlitePool::connect(&format!("sqlite://{}", db.path().display()))
        .await
        .expect("raw connect");
    sqlx::query("CREATE TRIGGER sabotage BEFORE INSERT ON outbox BEGIN SELECT RAISE(ABORT, 'sabotage'); END")
        .execute(&raw)
        .await
        .expect("trigger");

    let t = task(1, &s, BTreeSet::new());
    let result = db
        .repo()
        .apply(vec![
            PersistenceAction::UpsertTask(t.clone()),
            PersistenceAction::EnqueueOp(PendingOp {
                op_id: OpId(uuid::Uuid::from_u128(3)),
                op: LocalOp::CreateTask(t.id),
                queued_at: ts(9),
            }),
        ])
        .await;
    assert!(result.is_err(), "the sabotaged batch must fail");

    sqlx::query("DROP TRIGGER sabotage")
        .execute(&raw)
        .await
        .expect("drop trigger");
    raw.close().await;

    let loaded = db.repo().load().await.expect("load after rollback");
    assert!(
        !loaded.tasks.contains_key(&t.id),
        "task upsert must be rolled back"
    );
    assert!(loaded.outbox.is_empty(), "enqueue must be rolled back");
    db.close().await;
}

/// T9 — an `UpdateBoard` op is stored in the outbox `board_id` column with
/// `stack_id` NULL (structural column facts; the cast-era shape would put
/// the board id in `stack_id` and pass every semantic roundtrip).
#[tokio::test]
async fn rename_board_op_lands_in_the_board_column() {
    let board_uuid = uuid::Uuid::from_u128(0xb1);
    let mut db = common::FileDb::new().await;
    let op_id = OpId(uuid::Uuid::from_u128(0xb2));
    db.repo()
        .apply(vec![PersistenceAction::EnqueueOp(PendingOp {
            op_id,
            op: LocalOp::UpdateBoard(BoardId::from(board_uuid)),
            queued_at: ts(42),
        })])
        .await
        .expect("enqueue");

    let raw = sqlx::sqlite::SqlitePool::connect(&format!("sqlite://{}", db.path().display()))
        .await
        .expect("raw connect");
    let (board_id, stack_id): (Option<String>, Option<String>) =
        sqlx::query_as("SELECT board_id, stack_id FROM outbox WHERE op_id = ?")
            .bind(op_id.0.to_string())
            .fetch_one(&raw)
            .await
            .expect("outbox row");
    raw.close().await;
    assert_eq!(board_id.as_deref(), Some(board_uuid.to_string().as_str()));
    assert_eq!(stack_id, None, "the board id must not ride stack_id");

    let loaded = db.repo().load().await.expect("load");
    assert_eq!(
        loaded.outbox.iter().map(|o| o.op).collect::<Vec<_>>(),
        vec![LocalOp::UpdateBoard(BoardId::from(board_uuid))]
    );
    db.close().await;
}

/// T7 — raw-inserted corrupt rows decode to `Err(Corrupted)`, per class.
#[tokio::test]
async fn corrupt_rows_decode_to_corrupted() {
    async fn corrupt_and_load(sql: &'static str) -> Result<PersistedState, RepositoryError> {
        let mut db = common::FileDb::new().await;
        let raw = sqlx::sqlite::SqlitePool::connect(&format!("sqlite://{}", db.path().display()))
            .await
            .expect("raw connect");
        sqlx::query(sql)
            .execute(&raw)
            .await
            .expect("corrupt insert");
        raw.close().await;
        let outcome = db.repo().load().await;
        db.close().await;
        outcome
    }

    // Unparseable uuid in an id column.
    let result = corrupt_and_load("INSERT INTO boards (id, title, color, archived, deleted) VALUES ('not-a-uuid', 'x', 'x', 0, 0)").await;
    assert!(matches!(result, Err(RepositoryError::Corrupted)));

    // Unknown op_kind tag.
    let result = corrupt_and_load(
        "INSERT INTO outbox (op_id, op_kind, queued_at) VALUES ('0197c0ff-eeee-7ccc-8ddd-000000000001', 'teleport_task', '2026-01-01T00:00:00+00:00')",
    )
    .await;
    assert!(matches!(result, Err(RepositoryError::Corrupted)));

    // Missing required id column for the op kind.
    let result = corrupt_and_load(
        "INSERT INTO outbox (op_id, op_kind, queued_at) VALUES ('0197c0ff-eeee-7ccc-8ddd-000000000002', 'create_task', '2026-01-01T00:00:00+00:00')",
    )
    .await;
    assert!(matches!(result, Err(RepositoryError::Corrupted)));

    // rename_board without its board column (the cast-era shape — the
    // board id riding stack_id — is now a rejection, not a silent cast).
    let result = corrupt_and_load(
        "INSERT INTO outbox (op_id, op_kind, queued_at) VALUES ('0197c0ff-eeee-7ccc-8ddd-000000000008', 'rename_board', '2026-01-01T00:00:00+00:00')",
    )
    .await;
    assert!(matches!(result, Err(RepositoryError::Corrupted)));

    // Unknown sync phase tag (on a row whose board exists — the FK).
    let result = corrupt_and_load(
        "INSERT INTO boards (id, title, color, archived, deleted) VALUES ('0197c0ff-eeee-7ccc-8ddd-000000000009', 'x', 'x', 0, 0);
         INSERT INTO sync_status (board, phase) VALUES ('0197c0ff-eeee-7ccc-8ddd-000000000009', 'warp')",
    )
    .await;
    assert!(matches!(result, Err(RepositoryError::Corrupted)));

    // Failed phase without its error kind.
    let result = corrupt_and_load(
        "INSERT INTO boards (id, title, color, archived, deleted) VALUES ('0197c0ff-eeee-7ccc-8ddd-00000000000a', 'x', 'x', 0, 0);
         INSERT INTO sync_status (board, phase) VALUES ('0197c0ff-eeee-7ccc-8ddd-00000000000a', 'failed')",
    )
    .await;
    assert!(matches!(result, Err(RepositoryError::Corrupted)));

    // Note: a malformed board uuid in the status key cannot be inserted
    // past the FK (the key must match a boards row, whose id decodes),
    // so the key-decode guard is defensive only — no SQL-reachable pin.

    // Partial remote binding (board column set, stack column NULL).
    let result = corrupt_and_load(
        "INSERT INTO boards (id, title, color, archived, deleted) VALUES ('0197c0ff-eeee-7ccc-8ddd-000000000003', 'x', 'x', 0, 0);
         INSERT INTO stacks (id, board, remote_board_id, remote_stack_id, title, sort_order, archived, deleted, ck_title, ck_sort_order, ck_deleted) VALUES ('0197c0ff-eeee-7ccc-8ddd-000000000004', '0197c0ff-eeee-7ccc-8ddd-000000000003', 5, NULL, 's', 0, 0, 0, '2026-01-01T00:00:00+00:00', '2026-01-01T00:00:00+00:00', '2026-01-01T00:00:00+00:00')",
    )
    .await;
    assert!(matches!(result, Err(RepositoryError::Corrupted)));

    // Unparseable clock text in a task's ck_ column.
    let result = corrupt_and_load(
        "INSERT INTO boards (id, title, color, archived, deleted) VALUES ('0197c0ff-eeee-7ccc-8ddd-000000000005', 'x', 'x', 0, 0);
         INSERT INTO stacks (id, board, title, sort_order, archived, deleted, ck_title, ck_sort_order, ck_deleted) VALUES ('0197c0ff-eeee-7ccc-8ddd-000000000006', '0197c0ff-eeee-7ccc-8ddd-000000000005', 's', 0, 0, 0, '2026-01-01T00:00:00+00:00', '2026-01-01T00:00:00+00:00', '2026-01-01T00:00:00+00:00');
         INSERT INTO tasks (id, stack, title, description, sort_order, archived, deleted, ck_title, ck_description, ck_duedate, ck_done, ck_position, ck_labels, ck_archived, ck_deleted) VALUES ('0197c0ff-eeee-7ccc-8ddd-000000000007', '0197c0ff-eeee-7ccc-8ddd-000000000006', 't', '', 0, 0, 0, 'junk', '2026-01-01T00:00:00+00:00', '2026-01-01T00:00:00+00:00', '2026-01-01T00:00:00+00:00', '2026-01-01T00:00:00+00:00', '2026-01-01T00:00:00+00:00', '2026-01-01T00:00:00+00:00', '2026-01-01T00:00:00+00:00')",
    )
    .await;
    assert!(matches!(result, Err(RepositoryError::Corrupted)));

    // The singleton row is gone (per-board rebuild; zero rows is the
    // healthy fresh-install shape, so only a *malformed* key corrupts).
}

/// T8 — state survives a close/reopen cycle on a file database (and the
/// second open runs zero pending migrations).
#[tokio::test]
async fn state_survives_reopen() {
    let mut db = common::FileDb::new().await;
    let board_id = board(11).id;
    let mut sync = SyncStates::new();
    sync.set_stored(
        board_id,
        SyncPhase::Failed {
            last_error: SyncErrorKind::Network,
        },
        Some(ts(123)),
    );
    let state = PersistedState {
        boards: BTreeMap::from([(board_id, board(11))]),
        sync,
        ..PersistedState::default()
    };
    db.repo()
        .apply(actions_from_state(&state))
        .await
        .expect("apply");
    let expected = db.repo().load().await.expect("load");
    db.reopen().await;

    let loaded = db.repo().load().await.expect("load");
    assert_eq!(loaded, expected);
    db.close().await;
}

/// Validators for a `stacks:<board>` key round-trip (the key form that the
/// contract harness does not cover).
#[tokio::test]
async fn per_board_validators_roundtrip() {
    let repo = open_memory(taskboard_domain::ConnectorId::from("nextcloud"))
        .await
        .expect("open");
    let key = ValidatorKey::Stacks(RemoteBoardId(55));
    let validators = SyncValidators {
        etag: Some("\"etag\"".into()),
        last_modified: Some("Tue, 01 Jan 2026 00:00:00 GMT".into()),
    };
    repo.apply(vec![PersistenceAction::UpsertValidators(
        key,
        validators.clone(),
    )])
    .await
    .expect("apply");
    let loaded = repo.load().await.expect("load");
    assert_eq!(loaded.validators.get(&key), Some(&validators));
}

/// `DeleteSyncStatus` drops exactly the named board's row; the other
/// board's stored phase/last-success survives (the unbind cleanup).
#[tokio::test]
async fn delete_sync_status_roundtrip() {
    let repo = open_memory(taskboard_domain::ConnectorId::from("nextcloud"))
        .await
        .expect("open");
    let first = board(71);
    let second = board(72);
    let status = |phase: SyncPhase| taskboard_domain::SyncStatus {
        phase,
        last_success: Some(ts(400)),
        pending_ops: 0, // derived, never stored
    };
    repo.apply(vec![
        PersistenceAction::UpsertBoard(first.clone()),
        PersistenceAction::UpsertBoard(second.clone()),
        PersistenceAction::UpsertSyncStatus(first.id, status(SyncPhase::Syncing)),
        PersistenceAction::UpsertSyncStatus(second.id, status(SyncPhase::Idle)),
    ])
    .await
    .expect("seed boards and status rows");

    repo.apply(vec![PersistenceAction::DeleteSyncStatus(first.id)])
        .await
        .expect("delete one row");
    let loaded = repo.load().await.expect("load");
    // The projection re-derives a default row for every board row; the
    // *stored* part (phase, last success) is what the delete dropped.
    let deleted_view = loaded.sync.get(first.id).expect("derived row present");
    assert_eq!(deleted_view.phase, SyncPhase::Idle);
    assert_eq!(deleted_view.last_success, None);
    let survivor = loaded.sync.get(second.id).expect("the other board's row");
    assert_eq!(survivor.phase, SyncPhase::Idle);
    assert_eq!(survivor.last_success, Some(ts(400)));

    // Absent row = no-op (idempotent repeat).
    repo.apply(vec![PersistenceAction::DeleteSyncStatus(first.id)])
        .await
        .expect("repeat delete is a no-op");
}
