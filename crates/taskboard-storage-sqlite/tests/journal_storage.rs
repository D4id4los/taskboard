// SPDX-License-Identifier: MIT OR Apache-2.0
//! The command journal's storage contract (ADR 0019 §10, plan §14.6):
//! migration shape, codec round-trip, the D11 eviction mirror,
//! `MarkJournalPushed` idempotence, and the R-U10 head-survives-restart
//! pin. `TaskRepository::load` is the single read path (upgrade pins use
//! `connect.rs`'s raw-pool harness).

use chrono::{TimeZone, Utc};

use taskboard_domain::idgen::IdGenerator as _;
use taskboard_domain::journal::{
    EntityStamps, JOURNAL_CAP, JournalEntity, JournalEntry, JournalState, NotUndoableReason,
    UndoFacts, UndoWatermark,
};
use taskboard_domain::messages::StateCommand;
use taskboard_domain::persistence::{PersistenceAction, RepositoryError, TaskRepository};
use taskboard_domain::test_support::CountingIds;
use taskboard_storage_sqlite::{SqliteTaskRepository, open_memory};

fn ts(secs: i64) -> chrono::DateTime<Utc> {
    Utc.timestamp_opt(secs, 0).unwrap()
}

fn entry(
    ids: &CountingIds,
    parent: Option<taskboard_domain::JournalId>,
    created_at: i64,
) -> JournalEntry {
    JournalEntry {
        id: ids.new_journal_id(),
        parent,
        command: StateCommand::RequestSync,
        facts: UndoFacts::NotUndoable(NotUndoableReason::BoardCeremony),
        ops: Vec::new(),
        pushed: false,
        board: None,
        created_at: ts(created_at),
    }
}

fn undoable_entry(ids: &CountingIds, task_id: taskboard_domain::TaskId) -> JournalEntry {
    JournalEntry {
        id: ids.new_journal_id(),
        parent: None,
        command: StateCommand::DeleteTask { id: task_id },
        facts: UndoFacts::PreImages {
            images: Vec::new(),
            watermark: UndoWatermark(
                [(
                    JournalEntity::Task(task_id),
                    EntityStamps::Task {
                        clocks: taskboard_domain::TaskClocks {
                            title: ts(1),
                            description: ts(1),
                            duedate: ts(1),
                            scheduled: ts(1),
                            done: ts(1),
                            position: ts(1),
                            effort: ts(1),
                            importance: ts(1),
                            urgency: ts(1),
                            recur: ts(1),
                            archived: ts(1),
                            deleted: ts(1),
                        },
                        memberships: std::collections::BTreeMap::default(),
                    },
                )]
                .into_iter()
                .collect(),
            ),
        },
        ops: vec![ids.new_op_id()],
        pushed: false,
        board: None,
        created_at: ts(2),
    }
}

async fn repo() -> SqliteTaskRepository {
    open_memory(taskboard_domain::ids::ConnectorId::from("test"))
        .await
        .expect("open in-memory repo")
}

#[tokio::test]
async fn a_fresh_database_hydrates_an_empty_journal_with_a_null_head() {
    let repo = repo().await;
    let state = repo.load().await.expect("load");
    assert_eq!(state.journal, JournalState::default());
}

#[tokio::test]
async fn record_journal_lands_entry_and_head_across_a_reload() {
    let repo = repo().await;
    let ids = CountingIds::new();
    let first = entry(&ids, None, 100);
    let second = entry(&ids, Some(first.id), 200);
    repo.apply(vec![
        PersistenceAction::RecordJournal(first.clone()),
        PersistenceAction::RecordJournal(second.clone()),
    ])
    .await
    .expect("apply");
    let reloaded = repo.load().await.expect("reload");
    assert_eq!(reloaded.journal.entries, vec![first, second.clone()]);
    assert_eq!(reloaded.journal.head, Some(second.id));
}

#[tokio::test]
async fn move_journal_head_survives_a_restart() {
    // R-U10's v1 contract: the head is persisted state — apply, reload,
    // same head.
    let repo = repo().await;
    let ids = CountingIds::new();
    let first = entry(&ids, None, 100);
    let second = entry(&ids, Some(first.id), 200);
    repo.apply(vec![
        PersistenceAction::RecordJournal(first.clone()),
        PersistenceAction::RecordJournal(second.clone()),
    ])
    .await
    .expect("apply");
    repo.apply(vec![PersistenceAction::MoveJournalHead(Some(first.id))])
        .await
        .expect("apply");
    let reloaded = repo.load().await.expect("reload");
    assert_eq!(reloaded.journal.head, Some(first.id));
    repo.apply(vec![PersistenceAction::MoveJournalHead(None)])
        .await
        .expect("apply");
    let reloaded = repo.load().await.expect("reload");
    assert_eq!(reloaded.journal.head, None);
}

#[tokio::test]
async fn mark_journal_pushed_is_idempotent_on_the_real_repo() {
    let repo = repo().await;
    let ids = CountingIds::new();
    // Two entries sharing one op id (the multi-entry op-set shape,
    // D-U5 risk (a)): one completion flips both — each exactly once.
    let op = ids.new_op_id();
    let mut a = entry(&ids, None, 100);
    let mut b = entry(&ids, Some(a.id), 200);
    a.ops = vec![op];
    b.ops = vec![op];
    repo.apply(vec![
        PersistenceAction::RecordJournal(a.clone()),
        PersistenceAction::RecordJournal(b.clone()),
    ])
    .await
    .expect("apply");
    repo.apply(vec![
        PersistenceAction::MarkJournalPushed(a.id),
        PersistenceAction::MarkJournalPushed(b.id),
        PersistenceAction::MarkJournalPushed(a.id),
    ])
    .await
    .expect("apply");
    let reloaded = repo.load().await.expect("reload");
    assert!(reloaded.journal.entry(a.id).unwrap().pushed);
    assert!(reloaded.journal.entry(b.id).unwrap().pushed);
}

#[tokio::test]
async fn the_eviction_mirror_prunes_rows_a_reload_cannot_resurrect() {
    // D11: the sqlite mirror runs the same pure decision inside the
    // record transaction — a pruned journal stays pruned across load().
    let repo = repo().await;
    let ids = CountingIds::new();
    let mut actions = Vec::new();
    for i in 0..JOURNAL_CAP {
        actions.push(PersistenceAction::RecordJournal(entry(
            &ids,
            None,
            i64::try_from(i).unwrap(),
        )));
    }
    repo.apply(actions).await.expect("seed the cap");
    let oldest = {
        let state = repo.load().await.expect("load");
        state.journal.entries[0].id
    };
    // One more entry beyond the cap and past the floor: the oldest row
    // must go — and stay gone.
    let fresh = entry(&ids, None, 100 * 86_400);
    repo.apply(vec![PersistenceAction::RecordJournal(fresh)])
        .await
        .expect("apply");
    let reloaded = repo.load().await.expect("reload");
    assert_eq!(reloaded.journal.entries.len(), JOURNAL_CAP);
    assert!(reloaded.journal.entry(oldest).is_none());
}

#[tokio::test]
async fn an_undoable_payload_round_trips_through_the_json_blob() {
    let repo = repo().await;
    let ids = CountingIds::new();
    let task_id = taskboard_domain::TaskId::from_uuid(ids.new_task_id().as_uuid());
    let entry = undoable_entry(&ids, task_id);
    repo.apply(vec![PersistenceAction::RecordJournal(entry.clone())])
        .await
        .expect("apply");
    let reloaded = repo.load().await.expect("reload");
    assert_eq!(reloaded.journal.entries, vec![entry]);
}

#[tokio::test]
async fn a_corrupt_payload_is_typed_corrupted_at_load() {
    // The real corruption path: file-backed db, one row, payload
    // mangled through raw SQL.
    let file = tempfile::Builder::new().suffix(".db").tempfile().unwrap();
    let connector = taskboard_domain::ids::ConnectorId::from("test");
    let repo = taskboard_storage_sqlite::open(file.path(), connector.clone())
        .await
        .expect("open");
    let ids = CountingIds::new();
    repo.apply(vec![PersistenceAction::RecordJournal(entry(
        &ids, None, 100,
    ))])
    .await
    .expect("apply");
    let raw = sqlx::SqlitePool::connect(&format!("sqlite://{}", file.path().display()))
        .await
        .expect("raw connect");
    sqlx::query("UPDATE command_journal SET payload = 'not json'")
        .execute(&raw)
        .await
        .expect("corrupt");
    raw.close().await;
    let err = repo.load().await.expect_err("corruption is typed");
    assert!(matches!(err, RepositoryError::Corrupted));
}
