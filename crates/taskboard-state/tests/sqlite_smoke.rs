// SPDX-License-Identifier: MIT OR Apache-2.0
//! I1: engine ↔ port adapter ↔ storage actor ↔ sqlite in one routing
//! smoke. One command round-trips durably; a second engine boot on the
//! same file-backed database hydrates the task.
//!
//! Environment gate, not a weakened assertion: the smoke creates a real
//! temp directory (`mkdir` is unavailable under Miri isolation) and
//! drives sqlite FFI, both outside testing-strategy §10's Miri scope —
//! the routing logic it exercises is Miri-covered by the A-series.

use std::sync::Arc;

use taskboard_domain::NotifierMessage;
use taskboard_domain::persistence::TaskRepository;
use taskboard_domain::test_support::CountingIds;
use taskboard_domain::{CommandOutcome, StateCommand, SyncCommand, SyncReport, SystemEvent};
use taskboard_state::{ReminderEngineConfig, spawn_state_engine};

mod common;

use common::{FixedClock, T0};

#[cfg_attr(miri, ignore)] // tempdir mkdir + sqlite FFI (environment gate)
#[tokio::test]
async fn i1_engine_roundtrips_durably_through_the_sqlite_actor() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("taskboard.db");

    // First boot: seed a live board (no command creates boards), then
    // author one stack + one task through the full stack.
    {
        let repo = taskboard_storage_sqlite::open(
            &db_path,
            taskboard_domain::ConnectorId::from("nextcloud"),
        )
        .await
        .expect("db opens");
        let (storage, storage_join) = taskboard_storage_sqlite::spawn_storage_actor(Arc::new(repo));
        let repo_port: Arc<dyn TaskRepository> = Arc::new(storage);
        let board = seeded_board();
        repo_port
            .apply(vec![taskboard_domain::PersistenceAction::UpsertBoard(
                board,
            )])
            .await
            .expect("board seeded");
        let (sync_out, _sync_in) = tokio::sync::mpsc::channel::<SyncCommand>(8);
        let (report_tx, report_rx) = tokio::sync::mpsc::channel::<SyncReport>(8);
        let (system_tx, system_rx) = tokio::sync::broadcast::channel::<SystemEvent>(8);

        let (notifier, _notifications) = tokio::sync::mpsc::channel::<NotifierMessage>(64);
        let (engine, engine_join) = spawn_state_engine(
            repo_port,
            Arc::new(CountingIds::new()),
            FixedClock::new(T0),
            sync_out,
            report_rx,
            system_tx.clone(),
            system_rx,
            notifier,
            ReminderEngineConfig::disabled(),
        )
        .await
        .expect("engine boots");

        let board_id = seeded_board().id;
        let stack = engine
            .execute(StateCommand::CreateStack {
                board: board_id,
                title: "todo".into(),
                order: 1,
            })
            .await
            .expect("accepted");
        let CommandOutcome::CreatedStack(stack_id) = stack else {
            panic!("wrong receipt: {stack:?}");
        };
        engine
            .execute(StateCommand::CreateTask {
                title: "durable".into(),
                stack: stack_id,
                order: 1,
            })
            .await
            .expect("accepted");
        engine.flush().await; // durability barrier before shutdown

        system_tx.send(SystemEvent::Shutdown).expect("broadcast");
        // The drain ends when the reports channel closes — drop the
        // sender (the sync actor's stand-in) before awaiting the engine.
        drop(report_tx);
        let _ = tokio::time::timeout(std::time::Duration::from_secs(60), engine_join)
            .await
            .expect("engine drain completes");
        drop(storage_join); // storage actor keeps its own lifecycle
    }

    // Second boot on the same file: the hydration sees the authored data.
    {
        let repo = taskboard_storage_sqlite::open(
            &db_path,
            taskboard_domain::ConnectorId::from("nextcloud"),
        )
        .await
        .expect("db reopens");
        let (storage, _storage_join) =
            taskboard_storage_sqlite::spawn_storage_actor(Arc::new(repo));
        let repo_port: Arc<dyn TaskRepository> = Arc::new(storage);
        let hydrated = repo_port.load().await.expect("load");
        assert_eq!(hydrated.stacks.len(), 1);
        assert_eq!(hydrated.tasks.len(), 1);
        assert_eq!(
            hydrated
                .tasks
                .values()
                .next()
                .expect("task persisted")
                .title,
            "durable"
        );
        assert_eq!(
            hydrated.sync.aggregate().pending_ops,
            2,
            "outbox depth derived at load"
        );
    }
}

/// A live board entity with a deterministic id.
fn seeded_board() -> taskboard_domain::Board {
    common::board("board")
}

/// The T8 seed: a live board/stack chain, one task due at T0, and a
/// −90 s explicit rule on it (occurrence T0 − 90).
fn due_rule_seed() -> Vec<taskboard_domain::PersistenceAction> {
    use taskboard_domain::entities::Stack;
    use taskboard_domain::idgen::IdGenerator as _;
    use taskboard_domain::reminders::{ReminderOffset, ReminderRule};
    use taskboard_domain::{PersistenceAction, Task, TaskClocks};

    let ids = CountingIds::new();
    let board = seeded_board();
    let stack = Stack {
        id: ids.new_stack_id(),
        remote: None,
        board: board.id,
        title: "s".into(),
        order: 1,
        archived: false,
        deleted: false,
        clocks: taskboard_domain::StackClocks {
            title: common::ts(T0),
            order: common::ts(T0),
            deleted: common::ts(T0),
        },
        remote_seen: None,
    };
    let task = Task {
        id: ids.new_task_id(),
        remote: None,
        title: "due".into(),
        description: String::new(),
        duedate: Some(common::ts(T0)),
        scheduled: None,
        done: None,
        stack: stack.id,
        order: 1,
        labels: std::collections::BTreeSet::new(),
        effort: None,
        importance: None,
        urgency: None,
        recur: None,

        archived: false,
        deleted: false,
        clocks: TaskClocks {
            title: common::ts(T0),
            description: common::ts(T0),
            duedate: common::ts(T0),
            scheduled: common::ts(T0),
            done: common::ts(T0),
            position: common::ts(T0),
            labels: common::ts(T0),
            effort: common::ts(T0),
            importance: common::ts(T0),
            urgency: common::ts(T0),
            recur: common::ts(T0),

            archived: common::ts(T0),
            deleted: common::ts(T0),
        },
        remote_seen: None,
    };
    let rule_id = taskboard_domain::ids::ReminderRuleId::from(uuid::Uuid::from_u128(0x28));
    let rule = ReminderRule {
        id: rule_id,
        task: task.id,
        offset: ReminderOffset::before(chrono::TimeDelta::try_seconds(-90).unwrap())
            .expect("whole-second"),
        created_at: common::ts(T0),
        updated_at: common::ts(T0),
    };
    vec![
        PersistenceAction::UpsertBoard(board),
        PersistenceAction::UpsertStack(stack),
        PersistenceAction::UpsertTask(task),
        PersistenceAction::UpsertReminderRule(rule),
    ]
}

/// T8 (the F11-03 double-fire impossibility, end to end): a −90 s rule
/// fires at its occurrence O; the delivery is persisted; a reload
/// through the real sqlite repo re-derives the *same* occurrence — the
/// whole-second type guarantees the recomputed offset matches the
/// column — so nothing re-fires. Pre-fix, a sub-second offset truncated
/// on write and reloaded to a different occurrence.
#[cfg_attr(miri, ignore)] // tempdir mkdir + sqlite FFI (environment gate)
#[tokio::test]
async fn reloaded_rule_never_refires_a_delivered_occurrence() {
    use taskboard_domain::due_firings;
    use taskboard_domain::reminders::{ReminderDefaults, ReminderOffset};

    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("taskboard.db");

    {
        let repo = taskboard_storage_sqlite::open(
            &db_path,
            taskboard_domain::ConnectorId::from("nextcloud"),
        )
        .await
        .expect("db opens");
        let (storage, storage_join) = taskboard_storage_sqlite::spawn_storage_actor(Arc::new(repo));
        let repo_port: Arc<dyn TaskRepository> = Arc::new(storage);

        repo_port.apply(due_rule_seed()).await.expect("seeded");

        let (sync_out, _sync_in) = tokio::sync::mpsc::channel::<SyncCommand>(8);
        let (report_tx, report_rx) = tokio::sync::mpsc::channel::<SyncReport>(8);
        let (system_tx, system_rx) = tokio::sync::broadcast::channel::<SystemEvent>(8);
        let (notifier, _notifications) = tokio::sync::mpsc::channel::<NotifierMessage>(64);
        let (engine, engine_join) = taskboard_state::spawn_state_engine(
            repo_port,
            Arc::new(CountingIds::new()),
            FixedClock::new(T0), // now ≥ occurrence: the rule is eligible
            sync_out,
            report_rx,
            system_tx.clone(),
            system_rx,
            notifier,
            taskboard_state::ReminderEngineConfig {
                enabled: true,
                tick: std::time::Duration::from_secs(1),
                defaults: ReminderDefaults {
                    default_before: ReminderOffset::at_due(),
                },
            },
        )
        .await
        .expect("engine boots");

        // The immediate first tick fires (rule and implicit default)
        // and records the delivery markers; poll for the marker in the
        // shared state, then flush (the durability barrier).
        tokio::time::timeout(std::time::Duration::from_secs(60), async {
            loop {
                if !engine
                    .shared_state()
                    .load_full()
                    .reminders
                    .deliveries
                    .is_empty()
                {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("the first tick fired and settled");
        engine.flush().await;

        system_tx.send(SystemEvent::Shutdown).expect("broadcast");
        drop(report_tx);
        let _ = tokio::time::timeout(std::time::Duration::from_secs(60), engine_join)
            .await
            .expect("engine drain completes");
        drop(storage_join);
    };

    // Reload through the real sqlite repo and re-evaluate: the delivered
    // occurrence must stay settled.
    let repo =
        taskboard_storage_sqlite::open(&db_path, taskboard_domain::ConnectorId::from("nextcloud"))
            .await
            .expect("db reopens");
    let loaded = repo.load().await.expect("load");
    assert!(
        !loaded.reminders.deliveries.is_empty(),
        "the delivery markers persisted"
    );
    let defaults = ReminderDefaults {
        default_before: ReminderOffset::at_due(),
    };
    let firings = due_firings(
        &loaded.tasks,
        &loaded.reminders,
        &defaults,
        common::ts(T0 + 3_600),
    );
    assert!(
        firings.is_empty(),
        "a delivered occurrence never re-fires after reload: {firings:?}"
    );
}
