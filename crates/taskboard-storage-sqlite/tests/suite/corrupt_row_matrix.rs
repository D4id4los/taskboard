// SPDX-License-Identifier: MIT OR Apache-2.0
//! The corrupt-row decode matrix (B2 plan §5, #1039 + #1038): one
//! injected bad value per fixture, assertions on the typed
//! `CorruptionDetail` and the surviving row set — never on message
//! text. Entity tables abort with full typed context; journal rows
//! quarantine with signal; the healthy path emits nothing (case 11).

use crate::common::FileDb;
use chrono::TimeZone as _;
use sqlx::SqlitePool;
use taskboard_domain::idgen::IdGenerator as _;
use taskboard_domain::ids::JournalId;
use taskboard_domain::journal::{JournalEntry, NotUndoableReason, UndoFacts};
use taskboard_domain::messages::StateCommand;
use taskboard_domain::persistence::{
    CorruptionDetail, DecodeReason, DecodeStage, PersistedTable, PersistenceAction,
    RepositoryError, TaskRepository,
};
use taskboard_domain::test_support::CountingIds;

fn ts(secs: i64) -> chrono::DateTime<chrono::Utc> {
    chrono::Utc.timestamp_opt(secs, 0).unwrap()
}

fn board() -> taskboard_domain::entities::Board {
    taskboard_domain::entities::Board {
        id: taskboard_domain::ids::BoardId::from(uuid::Uuid::from_u128(0xb0a5)),
        remote: None,
        title: "b".into(),
        color: taskboard_domain::entities::Color::new("0000ff"),
        archived: false,
        deleted: false,
        clocks: taskboard_domain::entities::BoardClocks {
            title: taskboard_domain::entities::BoardClocks::epoch(),
            color: taskboard_domain::entities::BoardClocks::epoch(),
            archived: taskboard_domain::entities::BoardClocks::epoch(),
            deleted: taskboard_domain::entities::BoardClocks::epoch(),
        },
        authors: taskboard_domain::entities::BoardAuthors::default(),
        remote_seen: None,
    }
}

fn journal_entry(ids: &CountingIds, parent: Option<JournalId>, created_at: i64) -> JournalEntry {
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

/// Seeds a parent-chained journal line and leaves the head on the last
/// entry (the `RecordJournal` single-advance rule).
async fn seed_chain(db: &mut FileDb, ids: &CountingIds, length: usize) -> Vec<JournalEntry> {
    let mut entries = Vec::with_capacity(length);
    let mut parent = None;
    for k in 0..length {
        let entry = journal_entry(ids, parent, 100 + i64::try_from(k).expect("small"));
        parent = Some(entry.id);
        entries.push(entry);
    }
    db.repo()
        .apply(
            entries
                .iter()
                .cloned()
                .map(PersistenceAction::RecordJournal)
                .collect(),
        )
        .await
        .expect("seed journal");
    entries
}

async fn corrupt(db: &mut FileDb, sql: &'static str, bind: String) {
    let pool = SqlitePool::connect(&format!("sqlite://{}", db.path().display()))
        .await
        .expect("raw connection");
    sqlx::query(sql)
        .bind(bind)
        .execute(&pool)
        .await
        .expect("corrupting update");
    pool.close().await;
    db.reopen().await;
}

/// The quarantined journal row's id, by chain position.
fn entry_id(entries: &[JournalEntry], index: usize) -> String {
    entries[index].id.as_uuid().to_string()
}

fn detail_of(err: &RepositoryError) -> CorruptionDetail {
    err.corruption_detail().expect("typed decode context")
}

// ---- Entity aborts with typed context (cases 1–3, 9) ----------------

#[tokio::test]
async fn entity_abort_head_board_row_names_table_row_stage_reason() {
    let mut db = FileDb::new().await;
    db.repo()
        .apply(vec![PersistenceAction::UpsertBoard(board())])
        .await
        .expect("seed");
    corrupt(
        &mut db,
        "UPDATE boards SET au_title = 'not-a-uuid' WHERE id = ?1",
        uuid::Uuid::from_u128(0xb0a5).to_string(),
    )
    .await;

    let err = db.repo().load().await.expect_err("entity abort");
    let detail = detail_of(&err);
    assert_eq!(detail.table, Some(PersistedTable::Boards));
    assert_eq!(detail.row, Some(uuid::Uuid::from_u128(0xb0a5)));
    assert_eq!(detail.stage, Some(DecodeStage::Column));
    assert_eq!(detail.reason, Some(DecodeReason::UnknownEnumVariant));
    db.close().await;
}

#[tokio::test]
async fn entity_abort_middle_habit_log_row_names_the_date_reason() {
    let mut db = FileDb::new().await;
    let habit = taskboard_domain::Habit {
        id: taskboard_domain::ids::HabitId::from(uuid::Uuid::from_u128(0x00ab)),
        board: taskboard_domain::ids::BoardId::from(uuid::Uuid::from_u128(0xb0a5)),
        title: "habit".into(),
        description: String::new(),
        target: taskboard_domain::HabitTarget::PerDay(
            taskboard_domain::habit::NonZeroCount::new(1).unwrap(),
        ),
        order: 0,
        anchor_time: None,
        anchor_day: None,
        scheduled_days: None,
        reminder: None,
        clocks: taskboard_domain::HabitClocks::all(ts(100)),
        continued_from: None,
        deleted: false,
        by: None,

        paused_until: None,
    };
    // The chunk-5 C5 re-key: the legacy log tables have no writer, so
    // the entity-abort case rides the EVENT lane (the family's live
    // decode surface).
    let event = taskboard_domain::habit_event::HabitEvent {
        id: taskboard_domain::habit_event::HabitEventId::from(uuid::Uuid::from_u128(0x00ab_0001)),
        habit: habit.id,
        kind: taskboard_domain::habit_event::HabitEventKind::Completion,
        at: ts(1_000),
        by: None,
        deleted: None,
    };
    db.repo()
        .apply(vec![
            PersistenceAction::UpsertHabit(habit.clone()),
            PersistenceAction::UpsertHabitEvent(event),
        ])
        .await
        .expect("seed habit event");
    corrupt(
        &mut db,
        "UPDATE habit_event SET at = 'not-a-clock' WHERE id = ?1",
        format!("{}", uuid::Uuid::from_u128(0x00ab_0001)),
    )
    .await;

    let err = db.repo().load().await.expect_err("entity abort");
    let detail = detail_of(&err);
    assert_eq!(detail.table, Some(PersistedTable::HabitEvent));
    assert_eq!(detail.row, Some(uuid::Uuid::from_u128(0x00ab_0001)));
    assert_eq!(detail.reason, Some(DecodeReason::MalformedClock));
    db.close().await;
}

#[tokio::test]
async fn entity_abort_validator_row_names_the_table_without_a_row_key() {
    let mut db = FileDb::new().await;
    db.repo()
        .apply(vec![PersistenceAction::UpsertValidators(
            taskboard_domain::persistence::ValidatorKey::Boards,
            taskboard_domain::persistence::SyncValidators::default(),
        )])
        .await
        .expect("seed validator");
    corrupt(
        &mut db,
        "UPDATE sync_metadata SET key = 'garbage' WHERE key = 'boards'",
        "boards".to_owned(),
    )
    .await;

    let err = db.repo().load().await.expect_err("entity abort");
    let detail = detail_of(&err);
    assert_eq!(detail.table, Some(PersistedTable::Validators));
    assert_eq!(detail.row, None, "the validator row has no uuid key");
    assert_eq!(detail.reason, Some(DecodeReason::UnknownEnumVariant));
    db.close().await;
}

#[tokio::test]
async fn a_missing_head_singleton_row_aborts_with_typed_context() {
    let mut db = FileDb::new().await;
    {
        let pool = SqlitePool::connect(&format!("sqlite://{}", db.path().display()))
            .await
            .expect("raw connection");
        sqlx::query("DELETE FROM command_journal_head")
            .execute(&pool)
            .await
            .expect("delete singleton");
        pool.close().await;
    }

    let err = db.repo().load().await.expect_err("fail-closed singleton");
    let detail = detail_of(&err);
    assert_eq!(detail.table, Some(PersistedTable::CommandJournalHead));
    assert_eq!(detail.row, None);
    db.close().await;
}

// ---- Journal quarantine with signal (cases 4–8, 12) ------------------

#[tokio::test]
async fn journal_head_row_payload_quarantine_drops_the_line_and_repins_to_none() {
    let mut db = FileDb::new().await;
    let ids = CountingIds::new();
    let entries = seed_chain(&mut db, &ids, 2).await;
    corrupt(
        &mut db,
        "UPDATE command_journal SET payload = 'not json' WHERE id = ?1",
        entry_id(&entries, 0),
    )
    .await;

    let state = db.repo().load().await.expect("quarantined load succeeds");
    assert!(state.journal.entries.is_empty(), "both rows dropped");
    assert_eq!(state.journal.head, None, "emptied line repins to none");

    let diagnostics = db
        .repo()
        .load_diagnostics()
        .expect("diagnostics source")
        .take_load_diagnostics()
        .expect("degraded load published diagnostics");
    assert!(diagnostics.dropped >= 1, "floor, not ceiling");
    assert_eq!(diagnostics.details.len(), 2, "bad row + orphan");
    let head_row = &diagnostics.details[0];
    assert_eq!(head_row.table, Some(PersistedTable::CommandJournal));
    assert_eq!(head_row.row, Some(entries[0].id.as_uuid()));
    assert_eq!(head_row.stage, Some(DecodeStage::Payload));
    assert_eq!(head_row.reason, Some(DecodeReason::SerdePayload));
    db.close().await;
}

#[tokio::test]
async fn journal_middle_quarantine_drops_the_orphaned_descendants_only() {
    let mut db = FileDb::new().await;
    let ids = CountingIds::new();
    let entries = seed_chain(&mut db, &ids, 4).await;
    corrupt(
        &mut db,
        "UPDATE command_journal SET payload = 'not json' WHERE id = ?1",
        entry_id(&entries, 1),
    )
    .await;

    let state = db.repo().load().await.expect("quarantined load succeeds");
    let survivors: Vec<_> = state.journal.entries.iter().map(|e| e.id).collect();
    assert_eq!(
        survivors,
        vec![entries[0].id],
        "the pre-gap prefix survives"
    );
    assert_eq!(state.journal.head, Some(entries[0].id));

    let diagnostics = db
        .repo()
        .load_diagnostics()
        .expect("diagnostics source")
        .take_load_diagnostics()
        .expect("degraded load published diagnostics");
    assert_eq!(diagnostics.dropped, 3, "bad row + two descendants");
    assert_eq!(diagnostics.details.len(), 3);
    assert_eq!(diagnostics.head, Some(entries[0].id));
    db.close().await;
}

#[tokio::test]
async fn journal_tail_quarantine_repins_the_head_to_the_parent() {
    let mut db = FileDb::new().await;
    let ids = CountingIds::new();
    let entries = seed_chain(&mut db, &ids, 2).await;
    corrupt(
        &mut db,
        "UPDATE command_journal SET payload = 'not json' WHERE id = ?1",
        entry_id(&entries, 1),
    )
    .await;

    let state = db.repo().load().await.expect("quarantined load succeeds");
    assert_eq!(state.journal.entries.len(), 1);
    assert_eq!(
        state.journal.head,
        Some(entries[0].id),
        "the head repins to the surviving parent"
    );
    db.close().await;
}

#[tokio::test]
async fn journal_multi_row_multi_arm_quarantine_details_every_arm() {
    let mut db = FileDb::new().await;
    let ids = CountingIds::new();
    let entries = seed_chain(&mut db, &ids, 3).await;
    // Two distinct failure arms in one load: a bad id-column row and a
    // bad payload row.
    corrupt(
        &mut db,
        "UPDATE command_journal SET created_at = 'not-a-clock' WHERE id = ?1",
        entry_id(&entries, 1),
    )
    .await;
    corrupt(
        &mut db,
        "UPDATE command_journal SET payload = 'not json' WHERE id = ?1",
        entry_id(&entries, 2),
    )
    .await;

    let state = db.repo().load().await.expect("quarantined load succeeds");
    let survivors: Vec<_> = state.journal.entries.iter().map(|e| e.id).collect();
    assert_eq!(
        survivors,
        vec![entries[0].id],
        "the untouched root survives"
    );
    let diagnostics = db
        .repo()
        .load_diagnostics()
        .expect("diagnostics source")
        .take_load_diagnostics()
        .expect("degraded load published diagnostics");
    assert_eq!(diagnostics.dropped, 2);
    assert_eq!(diagnostics.head, Some(entries[0].id));
    let stages: Vec<_> = diagnostics.details.iter().filter_map(|d| d.stage).collect();
    assert!(stages.contains(&DecodeStage::Column), "{stages:?}");
    assert!(stages.contains(&DecodeStage::Payload), "{stages:?}");
    db.close().await;
}

#[tokio::test]
async fn a_lineage_rooted_before_the_gap_survives_the_quarantine() {
    let mut db = FileDb::new().await;
    let ids = CountingIds::new();
    let first = journal_entry(&ids, None, 100);
    let second = journal_entry(&ids, Some(first.id), 200);
    let second_id = second.id;
    // An independent second line rooted at `None` after the gap.
    let third = journal_entry(&ids, None, 300);
    let fourth = journal_entry(&ids, Some(third.id), 400);
    db.repo()
        .apply(vec![
            PersistenceAction::RecordJournal(first.clone()),
            PersistenceAction::RecordJournal(second),
            PersistenceAction::RecordJournal(third.clone()),
            PersistenceAction::RecordJournal(fourth.clone()),
        ])
        .await
        .expect("seed journal");
    corrupt(
        &mut db,
        "UPDATE command_journal SET payload = 'not json' WHERE id = ?1",
        second_id.as_uuid().to_string(),
    )
    .await;

    let state = db.repo().load().await.expect("quarantined load succeeds");
    let mut survivors: Vec<_> = state.journal.entries.iter().map(|e| e.id).collect();
    survivors.sort();
    let mut expected = vec![first.id, third.id, fourth.id];
    expected.sort();
    assert_eq!(
        survivors, expected,
        "the later independent lineage survives"
    );
    assert_eq!(
        state.journal.head,
        Some(fourth.id),
        "a surviving head stays"
    );
    db.close().await;
}

#[tokio::test]
async fn a_payload_missing_a_struct_field_is_typed_serde_payload() {
    let mut db = FileDb::new().await;
    let ids = CountingIds::new();
    let entries = seed_chain(&mut db, &ids, 1).await;
    corrupt(
        &mut db,
        "UPDATE command_journal SET payload = '{}' WHERE id = ?1",
        entry_id(&entries, 0),
    )
    .await;

    let state = db.repo().load().await.expect("quarantined load succeeds");
    assert_eq!(state.journal.entries.len(), 0);
    let diagnostics = db
        .repo()
        .load_diagnostics()
        .expect("diagnostics source")
        .take_load_diagnostics()
        .expect("degraded load published diagnostics");
    let detail = &diagnostics.details[0];
    assert_eq!(detail.table, Some(PersistedTable::CommandJournal));
    assert_eq!(detail.stage, Some(DecodeStage::Payload));
    assert_eq!(detail.reason, Some(DecodeReason::SerdePayload));
    db.close().await;
}

// ---- Healthy delta pin (case 11) -------------------------------------

#[tokio::test]
async fn a_healthy_load_publishes_no_diagnostics_and_keeps_the_head() {
    let mut db = FileDb::new().await;
    let ids = CountingIds::new();
    let entries = seed_chain(&mut db, &ids, 2).await;

    let state = db.repo().load().await.expect("healthy load");
    assert_eq!(state.journal.entries.len(), 2);
    assert_eq!(state.journal.head, Some(entries[1].id));
    assert!(
        db.repo()
            .load_diagnostics()
            .expect("diagnostics source")
            .take_load_diagnostics()
            .is_none(),
        "the healthy path publishes nothing"
    );
    db.close().await;
}

// ---- Property: one injected undecodable row (plan §5) ----------------

use proptest::prelude::*;

proptest! {
    #![proptest_config(taskboard_domain::test_support::proptest_config(32))]

    /// A journal with one randomly injected undecodable row among `n`
    /// valid chained rows: the survivors are exactly the valid rows whose
    /// parent chains avoid the bad row (the gap-free prefix), the head
    /// stays in the survivor set (or `None`), and the load returns `Ok`.
    /// Pure and deterministic (fixed seeds via `CountingIds`, no clock).
    #[test]
    fn one_injected_undecodable_row_keeps_exactly_the_gap_free_prefix(
        n in 1usize..=10,
        bad in 0usize..10,
    ) {
        let bad = bad % n;
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        rt.block_on(async move {
            let result: Result<(), proptest::test_runner::TestCaseError> = async {
                let mut db = FileDb::new().await;
            let ids = CountingIds::new();
            let entries = seed_chain(&mut db, &ids, n).await;
            corrupt(
                &mut db,
                "UPDATE command_journal SET payload = 'not json' WHERE id = ?1",
                entry_id(&entries, bad),
            )
            .await;

            let state = db.repo().load().await.expect("quarantined load is Ok");
            let survivors: Vec<JournalId> =
                state.journal.entries.iter().map(|e| e.id).collect();
            let expected: Vec<JournalId> =
                entries[..bad].iter().map(|e| e.id).collect();
            prop_assert_eq!(&survivors, &expected);
            let head = state.journal.head;
            prop_assert!(
                head.is_none() || survivors.contains(&head.unwrap()),
                "head in survivors or none"
            );
            let diagnostics = db
                .repo()
                .load_diagnostics()
                .expect("diagnostics source")
                .take_load_diagnostics()
                .expect("degraded load published diagnostics");
            prop_assert!(diagnostics.dropped >= 1);
            prop_assert_eq!(diagnostics.head, head);
            db.close().await;
                Ok(())
            }
            .await;
            result
        })
        .expect("property case passes");
    }
}

// ---- #1137 diagnostics hop: watch channel take-once semantics --------

/// Corrupts a row through a raw second connection WITHOUT closing and
/// reopening the fixture: the #1137 take-once tests need two sequential
/// degraded loads on the SAME repository instance (a `reopen` would swap
/// in a fresh diagnostics channel and sidestep the property under test).
async fn corrupt_in_place(db: &FileDb, sql: &'static str, bind: String) {
    let pool = SqlitePool::connect(&format!("sqlite://{}", db.path().display()))
        .await
        .expect("raw connection");
    sqlx::query(sql)
        .bind(bind)
        .execute(&pool)
        .await
        .expect("corrupting update");
    pool.close().await;
}

fn take(db: &FileDb) -> Option<u32> {
    db.repo()
        .load_diagnostics()
        .expect("diagnostics source")
        .take_load_diagnostics()
        .map(|d| d.dropped)
}

/// (a) a degraded load publishes its diagnostics exactly once: the
/// immediate second take is `None` — the watch channel's mark-read-on-
/// consume replaces the Mutex cell's `.take()` (no replay).
#[tokio::test]
async fn a_degraded_loads_diagnostics_are_taken_exactly_once() {
    let mut db = FileDb::new().await;
    let ids = CountingIds::new();
    let entries = seed_chain(&mut db, &ids, 2).await;
    corrupt(
        &mut db,
        "UPDATE command_journal SET payload = 'not json' WHERE id = ?1",
        entry_id(&entries, 0),
    )
    .await;

    db.repo().load().await.expect("quarantined load succeeds");
    let first = take(&db).expect("degraded load published diagnostics");
    assert!(first >= 1, "floor, not ceiling");
    assert!(
        take(&db).is_none(),
        "the immediate second take observes no change"
    );
    db.close().await;
}

/// (b) a healthy load after a degraded one clears a stale pending
/// report — the next take is `None`, never the old load's replay (#1137:
/// the load start sends `None`, preserving the take-once semantics one
/// step earlier than the old Mutex cell did).
#[tokio::test]
async fn a_healthy_load_clears_a_stale_pending_diagnostics_report() {
    let mut db = FileDb::new().await;
    let ids = CountingIds::new();
    let entries = seed_chain(&mut db, &ids, 2).await;
    corrupt(
        &mut db,
        "UPDATE command_journal SET payload = 'not json' WHERE id = ?1",
        entry_id(&entries, 0),
    )
    .await;

    db.repo().load().await.expect("degraded load succeeds");
    assert!(take(&db).is_some(), "the degraded load published");
    // Repair the corruption in place (same repository instance, same
    // channel): the next load quarantines nothing, so it must CLEAR the
    // pending-report state — a `None` send — and the take reads `None`,
    // never the old load's replay. (A second load of the same corrupt
    // file is degraded again by design — one report per degraded load —
    // so the healthy-clear property needs the rows actually repaired.)
    corrupt_in_place(
        &db,
        "DELETE FROM command_journal WHERE id = ?1",
        entry_id(&entries, 0),
    )
    .await;
    corrupt_in_place(
        &db,
        "DELETE FROM command_journal WHERE id = ?1",
        entry_id(&entries, 1),
    )
    .await;
    db.repo().load().await.expect("healthy load succeeds");
    assert!(
        take(&db).is_none(),
        "the healthy load cleared, not replayed"
    );
    db.close().await;
}

/// (c) two sequential degraded loads on the same repository publish
/// each report exactly once, in order — the watch channel carries every
/// load's own report, none replayed, none lost.
#[tokio::test]
async fn two_sequential_degraded_loads_yield_each_diagnostics_exactly_once_in_order() {
    let mut db = FileDb::new().await;
    let ids = CountingIds::new();
    let entries = seed_chain(&mut db, &ids, 2).await;
    corrupt(
        &mut db,
        "UPDATE command_journal SET payload = 'not json' WHERE id = ?1",
        entry_id(&entries, 0),
    )
    .await;

    db.repo()
        .load()
        .await
        .expect("first degraded load succeeds");
    let first = take(&db).expect("first degraded load published");
    assert!(take(&db).is_none(), "first report consumed once");

    // Seed a fresh one-entry line on the surviving (now empty) journal
    // and degrade exactly it — a second degraded load on the SAME
    // repository instance.
    let second_entry = journal_entry(&ids, None, 900);
    db.repo()
        .apply(vec![PersistenceAction::RecordJournal(second_entry.clone())])
        .await
        .expect("seed second line");
    corrupt_in_place(
        &db,
        "UPDATE command_journal SET payload = 'not json' WHERE id = ?1",
        second_entry.id.as_uuid().to_string(),
    )
    .await;

    db.repo()
        .load()
        .await
        .expect("second degraded load succeeds");
    let second = take(&db).expect("second degraded load published its own report");
    assert_ne!(
        first, second,
        "the second report is the second load's, not a replay of the first"
    );
    assert!(take(&db).is_none(), "second report consumed once");
    db.close().await;
}
