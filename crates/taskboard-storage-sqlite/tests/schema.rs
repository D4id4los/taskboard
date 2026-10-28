// SPDX-License-Identifier: MIT OR Apache-2.0
//! Lockstep assertion 2 (#244, D-F3): the migrated `tasks` schema
//! matches the domain's `TASK_FIELDS` manifest column-for-column, plus
//! the 0007 default-inheritance and corrupt-decode semantics.

mod common;

use std::collections::{BTreeMap, BTreeSet};

use sqlx::{Row, SqlitePool};
use taskboard_domain::entities::{BoardClocks, Effort, TASK_FIELDS, Task, TaskClocks};
use taskboard_domain::ids::{StackId, TaskId};
use taskboard_domain::persistence::{PersistenceAction, RepositoryError, TaskRepository};

use common::FileDb;

async fn task_columns(db: &SqlitePool) -> BTreeSet<String> {
    let rows = sqlx::query("PRAGMA table_info(tasks)")
        .fetch_all(db)
        .await
        .expect("table_info");
    rows.iter()
        .map(|row| row.get::<String, _>("name"))
        .collect()
}

/// The one non-clock task column per manifest row that is not verbatim:
/// `order` is `sort_order`, `labels` is the link table (asserted
/// separately), `position` names both `stack` and `order`.
fn expected_value_columns() -> BTreeSet<String> {
    let mut cols = BTreeSet::new();
    for (_, fields, _) in TASK_FIELDS {
        for field in *fields {
            let column = match *field {
                "order" => "sort_order".to_string(),
                other => other.to_string(),
            };
            cols.insert(column);
        }
    }
    cols
}

#[tokio::test]
async fn tasks_schema_matches_the_task_fields_manifest() {
    let mut db = FileDb::new().await;
    let pool = SqlitePool::connect(&format!("sqlite://{}", db.path().display()))
        .await
        .expect("raw connection");

    let columns = task_columns(&pool).await;

    // The ck_* columns are exactly the manifest's clock set.
    let ck_columns: BTreeSet<String> = columns
        .iter()
        .filter(|c| c.starts_with("ck_"))
        .map(|c| c.strip_prefix("ck_").expect("prefix").to_string())
        .collect();
    let manifest_clocks: BTreeSet<String> = TASK_FIELDS
        .iter()
        .map(|(clock, _, _)| (*clock).to_string())
        .collect();
    // `ck_labels` is vestigial since 0012 (the membership map owns the
    // label clocks): NOT NULL without a DEFAULT keeps the column in every
    // INSERT, but it is never read again (ADR 0005: additive-only).
    let mut ck_columns = ck_columns;
    ck_columns.remove("labels");
    assert_eq!(ck_columns, manifest_clocks);

    // One value column per manifest task field (with the documented
    // `order` → `sort_order` mapping); `labels` rides the link table.
    let mut expected = expected_value_columns();
    expected.remove("labels");
    assert!(
        expected.is_subset(&columns),
        "missing value columns: {expected:?}"
    );

    // The label memberships ride `task_labels` with exactly
    // (task, label, state, ck_membership) — the OR-set columns (#129).
    let label_rows = sqlx::query("PRAGMA table_info(task_labels)")
        .fetch_all(&pool)
        .await
        .expect("table_info");
    let label_columns: BTreeSet<String> = label_rows
        .iter()
        .map(|row| row.get::<String, _>("name"))
        .collect();
    assert_eq!(
        label_columns,
        BTreeSet::from([
            "task".to_string(),
            "label".to_string(),
            "state".to_string(),
            "ck_membership".to_string(),
        ])
    );

    // The rule-clock column exists (D-S9's reserved slot); the codec
    // deliberately does not read or write it until the wiring chunk.
    let rule_rows = sqlx::query("PRAGMA table_info(reminder_rules)")
        .fetch_all(&pool)
        .await
        .expect("table_info");
    let rule_columns: BTreeSet<String> = rule_rows
        .iter()
        .map(|row| row.get::<String, _>("name"))
        .collect();
    assert!(rule_columns.contains("updated_at"));

    pool.close().await;
    db.close().await;
}

#[tokio::test]
async fn insert_without_the_family_columns_inherits_epoch_defaults() {
    let mut db = FileDb::new().await;
    let pool = SqlitePool::connect(&format!("sqlite://{}", db.path().display()))
        .await
        .expect("raw connection");
    // A raw pre-family INSERT (the 0006-and-older writer shape): the
    // epoch defaults must make every remote stamp win, bit-for-bit.
    // Board and stack parents first — the raw path bypasses the repo's
    // referential assembly on purpose.
    db.repo()
        .apply(vec![
            PersistenceAction::UpsertBoard(board()),
            PersistenceAction::UpsertStack(stack()),
        ])
        .await
        .expect("parents");
    sqlx::query(
        r"INSERT INTO tasks (id, stack, title, description, sort_order, archived, deleted,
                             ck_title, ck_description, ck_duedate, ck_done, ck_position,
                             ck_labels, ck_archived, ck_deleted)
           VALUES ('00000000-0000-0000-0000-00000000beef', '00000000-0000-0000-0000-00000000feed',
                   't', 'd', 1, 0, 0,
                   '0001-01-01T00:00:00+00:00', '0001-01-01T00:00:00+00:00',
                   '0001-01-01T00:00:00+00:00', '0001-01-01T00:00:00+00:00',
                   '0001-01-01T00:00:00+00:00', '0001-01-01T00:00:00+00:00',
                   '0001-01-01T00:00:00+00:00', '0001-01-01T00:00:00+00:00')",
    )
    .execute(&pool)
    .await
    .expect("raw insert");
    pool.close().await;

    let state = db.repo().load().await.expect("load");
    let task = state
        .tasks
        .values()
        .find(|t| t.title == "t")
        .expect("task loaded");
    assert_eq!(task.scheduled, None);
    assert_eq!(task.effort, None);
    assert_eq!(task.importance, None);
    assert_eq!(task.urgency, None);
    assert_eq!(task.recur, None);
    assert_eq!(task.clocks.scheduled, BoardClocks::epoch());
    assert_eq!(task.clocks.effort, BoardClocks::epoch());
    assert_eq!(task.clocks.importance, BoardClocks::epoch());
    assert_eq!(task.clocks.urgency, BoardClocks::epoch());
    assert_eq!(task.clocks.recur, BoardClocks::epoch());
    db.close().await;
}

fn sample_rule() -> taskboard_domain::recur::RecurRule {
    taskboard_domain::recur::RecurRule {
        series: taskboard_domain::SeriesId::from_uuid(uuid::Uuid::from_u128(0x5e)),
        anchor: chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
        pattern: taskboard_domain::recur::RecurPattern::Daily {
            every: taskboard_domain::recur::EveryDays::new(2).unwrap(),
        },
        bound: taskboard_domain::recur::RecurBound::None,
        continued_from: None,
    }
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
            title: BoardClocks::epoch(),
            color: BoardClocks::epoch(),
            archived: BoardClocks::epoch(),
            deleted: BoardClocks::epoch(),
        },
        remote_seen: None,
    }
}

fn stack() -> taskboard_domain::entities::Stack {
    taskboard_domain::entities::Stack {
        id: StackId::from(uuid::Uuid::from_u128(0xfeed)),
        remote: None,
        board: taskboard_domain::ids::BoardId::from(uuid::Uuid::from_u128(0xb0a5)),
        title: "s".into(),
        order: 1,
        archived: false,
        deleted: false,
        clocks: taskboard_domain::entities::StackClocks {
            title: BoardClocks::epoch(),
            order: BoardClocks::epoch(),
            deleted: BoardClocks::epoch(),
        },
        remote_seen: None,
    }
}

/// One epoch-clocked task under a seeded stack id; tests overwrite the
/// field under test.
fn minimal_task() -> Task {
    Task {
        id: TaskId::from(uuid::Uuid::from_u128(1)),
        remote: None,
        title: "t".into(),
        description: String::new(),
        duedate: None,
        scheduled: None,
        done: None,
        stack: StackId::from(uuid::Uuid::from_u128(0xfeed)),
        order: 0,
        label_memberships: BTreeMap::new(),
        effort: None,
        importance: None,
        urgency: None,
        recur: None,

        archived: false,
        deleted: false,
        clocks: TaskClocks {
            title: BoardClocks::epoch(),
            description: BoardClocks::epoch(),
            duedate: BoardClocks::epoch(),
            scheduled: BoardClocks::epoch(),
            done: BoardClocks::epoch(),
            position: BoardClocks::epoch(),
            effort: BoardClocks::epoch(),
            importance: BoardClocks::epoch(),
            urgency: BoardClocks::epoch(),
            recur: BoardClocks::epoch(),

            archived: BoardClocks::epoch(),
            deleted: BoardClocks::epoch(),
        },
        remote_seen: None,
        remote_baseline: None,
    }
}

#[tokio::test]
async fn effort_beyond_the_i64_fence_is_a_typed_corruption() {
    let mut db = FileDb::new().await;
    let mut task = minimal_task();
    task.effort = Some(Effort::new(u64::MAX));
    let err = db
        .repo()
        .apply(vec![PersistenceAction::UpsertTask(task)])
        .await
        .expect_err("overflow rejected");
    assert!(matches!(err, RepositoryError::Corrupted));
    db.close().await;
}

#[tokio::test]
async fn unknown_level_text_in_the_db_is_a_typed_corruption() {
    let mut db = FileDb::new().await;
    let mut task = minimal_task();
    task.importance = Some(taskboard_domain::entities::Importance::Normal);
    seed(&mut db, task).await;
    let pool = SqlitePool::connect(&format!("sqlite://{}", db.path().display()))
        .await
        .expect("raw connection");
    sqlx::query("UPDATE tasks SET importance = 'apocalyptic', urgency = NULL WHERE id = ?1")
        .bind(uuid_of_first_task(&pool).await)
        .execute(&pool)
        .await
        .expect("corrupting update");
    pool.close().await;

    let err = db.repo().load().await.expect_err("corrupt decode");
    assert!(matches!(err, RepositoryError::Corrupted));
    db.close().await;
}

#[tokio::test]
async fn corrupt_recur_json_in_the_db_is_a_typed_corruption() {
    let mut db = FileDb::new().await;
    let mut task = minimal_task();
    task.recur = Some(sample_rule());
    seed(&mut db, task).await;
    let pool = SqlitePool::connect(&format!("sqlite://{}", db.path().display()))
        .await
        .expect("raw connection");
    sqlx::query("UPDATE tasks SET recur = '{\"every\": 0}' WHERE id = ?1")
        .bind(uuid_of_first_task(&pool).await)
        .execute(&pool)
        .await
        .expect("corrupting update");
    pool.close().await;

    let err = db.repo().load().await.expect_err("corrupt decode");
    assert!(matches!(err, RepositoryError::Corrupted));
    db.close().await;
}

#[tokio::test]
async fn negative_effort_text_in_the_db_is_a_typed_corruption() {
    let mut db = FileDb::new().await;
    let mut task = minimal_task();
    task.effort = Some(Effort::new(900));
    seed(&mut db, task).await;
    let pool = SqlitePool::connect(&format!("sqlite://{}", db.path().display()))
        .await
        .expect("raw connection");
    sqlx::query("UPDATE tasks SET effort = -5 WHERE id = ?1")
        .bind(uuid_of_first_task(&pool).await)
        .execute(&pool)
        .await
        .expect("corrupting update");
    pool.close().await;

    let err = db.repo().load().await.expect_err("corrupt decode");
    assert!(matches!(err, RepositoryError::Corrupted));
    db.close().await;
}

/// Seeds the parent board + stack, then the task under test.
async fn seed(db: &mut FileDb, task: Task) {
    db.repo()
        .apply(vec![
            PersistenceAction::UpsertBoard(board()),
            PersistenceAction::UpsertStack(stack()),
            PersistenceAction::UpsertTask(task),
        ])
        .await
        .expect("seed");
}

async fn uuid_of_first_task(pool: &SqlitePool) -> String {
    let row = sqlx::query("SELECT id FROM tasks ORDER BY id LIMIT 1")
        .fetch_one(pool)
        .await
        .expect("seeded task");
    row.get::<String, _>("id")
}
