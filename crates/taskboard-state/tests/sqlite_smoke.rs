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

use taskboard_domain::persistence::TaskRepository;
use taskboard_domain::test_support::CountingIds;
use taskboard_domain::{CommandOutcome, StateCommand, SyncCommand, SyncReport, SystemEvent};
use taskboard_state::spawn_state_engine;

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
        let repo = taskboard_storage_sqlite::open(&db_path)
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

        let (engine, engine_join) = spawn_state_engine(
            repo_port,
            Arc::new(CountingIds::new()),
            FixedClock::new(T0),
            sync_out,
            report_rx,
            system_rx,
        )
        .await
        .expect("engine boots");

        let stack = engine
            .execute(StateCommand::CreateStack {
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
        let repo = taskboard_storage_sqlite::open(&db_path)
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
        assert_eq!(hydrated.sync.pending_ops, 2, "outbox depth derived at load");
    }
}

/// A live board entity with a deterministic id.
fn seeded_board() -> taskboard_domain::Board {
    common::board("board")
}
