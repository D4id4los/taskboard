// SPDX-License-Identifier: AGPL-3.0-only
//! Tier 2 integration tests for the app crate: the *daemon journey*
//! against the dockerized Nextcloud (`docs/testing_strategy.org` §8).
//! This is the automated Phase 5 exit demo — a headless in-process
//! daemon pulling, pushing, and shutting down gracefully against a real
//! Deck server, booted through `taskboard_app::bootstrap` with the env
//! credential store.
//!
//! Secretless by construction — the setup script mints the app password
//! into `TASKBOARD_IT_DOCKER_TOKEN`; skip, never fail, without the tier
//! env.
//!
//! Run locally:
//!
//! ```sh
//! eval "$(scripts/nextcloud_it_setup.sh up)" &&
//!   cargo nextest run -p taskboard-app --run-ignored only \
//!     -E 'test(it_nextcloud_docker)'
//! ```

#![allow(clippy::too_many_lines)] // live-tier journey tests read as one narrative

use std::sync::Arc;
use std::time::Duration;

use taskboard_app::bootstrap::bootstrap;
use taskboard_app::config::{AppConfig, CredentialStoreKind};
use taskboard_domain::persistence::TaskRepository as _;
use taskboard_sync_nextcloud::{DeckClient, DeckColor, NewCard, StackFilter};

mod common;

use common::{EnvOverride, env_lock};

/// The outer wall-clock deadline for one tier test.
const LIVE_DEADLINE: Duration = Duration::from_secs(300);

#[tokio::test]
#[ignore = "requires the dockerized Nextcloud tier (scripts/nextcloud_it_setup.sh up)"]
async fn it_nextcloud_docker_daemon_pull_push_shutdown() {
    let Some(cfg) = common::live_docker_config() else {
        return; // skip, never fail: tier not configured
    };
    common::with_deadline(daemon_journey(&cfg))
        .await
        .expect("test must finish within the live deadline");
}

#[tokio::test]
#[ignore = "requires the dockerized Nextcloud tier (scripts/nextcloud_it_setup.sh up)"]
async fn it_nextcloud_docker_shutdown_mid_cycle_is_bounded() {
    let Some(cfg) = common::live_docker_config() else {
        return;
    };
    common::with_deadline(shutdown_mid_cycle(&cfg))
        .await
        .expect("test must finish within the live deadline");
}

/// T1: the daemon journey — create board+stack+card server-side, seed
/// the board binding through the storage API, bootstrap, observe the
/// pull, push a task through the engine handle, observe it on the
/// server, shut down cleanly, and reopen the database with a drained
/// outbox.
async fn daemon_journey(cfg: &common::LiveCfg) {
    let _env = env_lock();
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("taskboard.db");

    let client = DeckClient::new(&cfg.url, &cfg.user, &cfg.token).expect("base URL");
    let run = common::run_id();
    let board = common::retry(|| async {
        client
            .create_board(&run, &DeckColor::from_hex("00c2e0").expect("hex"))
            .await
    })
    .await
    .expect("board creation must succeed");
    let stack = common::retry(|| async {
        client
            .create_stack(board.id, &format!("{run}-col"), 0)
            .await
    })
    .await
    .expect("stack creation must succeed");
    let server_card = common::retry(|| async {
        client
            .create_card(
                board.id,
                stack.id,
                &NewCard {
                    title: format!("{run}-remote"),
                    order: Some(0),
                    description: None,
                    duedate: None,
                    kind: None,
                },
            )
            .await
    })
    .await
    .expect("server-side card creation must succeed");

    // Seed the board binding through the storage crate's public API.
    {
        let repo = Arc::new(
            taskboard_storage_sqlite::open(&db_path)
                .await
                .expect("seed db"),
        );
        let (handle, join) = taskboard_storage_sqlite::spawn_storage_actor(repo);
        handle
            .apply(vec![taskboard_domain::PersistenceAction::UpsertBoard(
                taskboard_domain::Board {
                    id: taskboard_domain::BoardId::from_uuid(uuid::Uuid::from_u128(0x00DA_000E)),
                    remote: Some(taskboard_domain::RemoteBoardId(board.id)),
                    title: run.clone(),
                    color: taskboard_domain::Color::new("00c2e0"),
                    archived: false,
                    deleted: false,
                    remote_seen: None,
                },
            )])
            .await
            .expect("seed applies");
        drop(handle);
        join.await.expect("seed actor exits");
    }

    // Bootstrap the daemon with the env credential store.
    let _password = EnvOverride::set("TASKBOARD_APP_PASSWORD", &cfg.token);
    let mut config = AppConfig::default();
    config.nextcloud.server_url = Some(cfg.url.clone());
    config.nextcloud.username = Some(cfg.user.clone());
    config.nextcloud.credential_store = CredentialStoreKind::Env;
    config.storage.db_path = Some(db_path.clone());
    let app = bootstrap(config, Arc::new(taskboard_app::secrets::EnvCredentialStore))
        .await
        .expect("bootstrap");

    // Pull: the server-created card appears in the published state.
    app.engine
        .execute(taskboard_domain::StateCommand::RequestSync)
        .await
        .expect("RequestSync accepted");
    eventually(&app, |state| {
        state
            .tasks
            .values()
            .any(|t| t.remote.is_some_and(|r| r.card.get() == server_card.id))
    })
    .await;

    // Push: a task created through the engine handle lands on the server.
    let stack_id = app
        .engine
        .shared_state()
        .load_full()
        .stacks
        .values()
        .find(|s| s.remote.as_ref().is_some_and(|r| r.stack.get() == stack.id))
        .map(|s| s.id)
        .expect("the remote stack is adopted locally");
    let taskboard_domain::CommandOutcome::CreatedTask(task_id) = app
        .engine
        .execute(taskboard_domain::StateCommand::CreateTask {
            title: format!("{run}-local"),
            stack: stack_id,
            order: 1,
        })
        .await
        .expect("CreateTask accepted")
    else {
        panic!("wrong receipt for CreateTask");
    };
    eventually(&app, |state| {
        state
            .tasks
            .get(&task_id)
            .is_some_and(|t| t.remote.is_some())
    })
    .await;
    let binding = app.engine.shared_state().load_full().tasks[&task_id]
        .remote
        .expect("bound");
    let stacks = common::retry(|| async { client.stacks(board.id, StackFilter::Active).await })
        .await
        .expect("server listing");
    assert!(
        stacks
            .iter()
            .flat_map(|s| &s.cards)
            .any(|c| c.id == binding.card.get() && c.title == format!("{run}-local")),
        "the pushed card must exist on the server"
    );

    // Graceful shutdown, then the database reopens with a drained outbox.
    let outcome = app.shutdown().await;
    assert!(!outcome.actor_aborted, "idle shutdown must not abort");

    let repo = taskboard_storage_sqlite::open(&db_path)
        .await
        .expect("reopen");
    let state = repo.load().await.expect("reload");
    assert_eq!(
        state.outbox.len(),
        0,
        "every pushed op completed; the outbox drains"
    );

    let deleted = common::retry(|| async { client.delete_board(board.id).await }).await;
    assert!(deleted.is_ok(), "board teardown must succeed");
}

/// T2: a shutdown racing an in-flight cycle against the real server
/// completes within the bounded budget and leaves a consistent database
/// (the queued op is either completed-and-drained or intact — never
/// half-applied).
async fn shutdown_mid_cycle(cfg: &common::LiveCfg) {
    let _env = env_lock();
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("taskboard.db");

    let client = DeckClient::new(&cfg.url, &cfg.user, &cfg.token).expect("base URL");
    let run = common::run_id();
    let board = common::retry(|| async {
        client
            .create_board(&run, &DeckColor::from_hex("00c2e0").expect("hex"))
            .await
    })
    .await
    .expect("board creation must succeed");
    let stack = common::retry(|| async {
        client
            .create_stack(board.id, &format!("{run}-col"), 0)
            .await
    })
    .await
    .expect("stack creation must succeed");

    {
        let repo = Arc::new(
            taskboard_storage_sqlite::open(&db_path)
                .await
                .expect("seed db"),
        );
        let (handle, join) = taskboard_storage_sqlite::spawn_storage_actor(repo);
        handle
            .apply(vec![taskboard_domain::PersistenceAction::UpsertBoard(
                taskboard_domain::Board {
                    id: taskboard_domain::BoardId::from_uuid(uuid::Uuid::from_u128(0x00DB_000E)),
                    remote: Some(taskboard_domain::RemoteBoardId(board.id)),
                    title: run.clone(),
                    color: taskboard_domain::Color::new("00c2e0"),
                    archived: false,
                    deleted: false,
                    remote_seen: None,
                },
            )])
            .await
            .expect("seed applies");
        drop(handle);
        join.await.expect("seed actor exits");
    }

    let _password = EnvOverride::set("TASKBOARD_APP_PASSWORD", &cfg.token);
    let mut config = AppConfig::default();
    config.nextcloud.server_url = Some(cfg.url.clone());
    config.nextcloud.username = Some(cfg.user.clone());
    config.nextcloud.credential_store = CredentialStoreKind::Env;
    config.storage.db_path = Some(db_path.clone());
    config.app.shutdown_timeout_secs = 0; // force the bounded-abort path
    let app = bootstrap(config, Arc::new(taskboard_app::secrets::EnvCredentialStore))
        .await
        .expect("bootstrap");

    // Queue a local op, then race the shutdown against the cycle.
    eventually(&app, |state| {
        state
            .stacks
            .values()
            .any(|s| s.remote.as_ref().is_some_and(|r| r.stack.get() == stack.id))
    })
    .await;
    let stack_id = app
        .engine
        .shared_state()
        .load_full()
        .stacks
        .values()
        .find(|s| s.remote.as_ref().is_some_and(|r| r.stack.get() == stack.id))
        .map(|s| s.id)
        .expect("stack adopted");
    app.engine
        .execute(taskboard_domain::StateCommand::CreateTask {
            title: format!("{run}-raced"),
            stack: stack_id,
            order: 0,
        })
        .await
        .expect("CreateTask accepted");
    app.engine
        .execute(taskboard_domain::StateCommand::RequestSync)
        .await
        .expect("RequestSync accepted");

    let outcome = app.shutdown().await;
    assert!(
        outcome.actor_aborted,
        "the zero budget must exercise the bounded-abort path"
    );

    // The database reopens; the queued op is consistent: either the
    // cycle completed it (remote binding present, outbox drained) or it
    // is still queued intact — never half-applied.
    let repo = taskboard_storage_sqlite::open(&db_path)
        .await
        .expect("reopen");
    let state = repo.load().await.expect("reload");
    let raced: Vec<_> = state
        .tasks
        .values()
        .filter(|t| t.title == format!("{run}-raced"))
        .collect();
    assert_eq!(raced.len(), 1, "the raced task survives");
    let bound = raced[0].remote.is_some();
    assert_eq!(
        state.outbox.is_empty(),
        bound,
        "outbox drained iff the create completed (never half-applied)"
    );

    let deleted = common::retry(|| async { client.delete_board(board.id).await }).await;
    assert!(deleted.is_ok(), "board teardown must succeed");
}

async fn eventually(app: &taskboard_app::App, pred: impl Fn(&taskboard_domain::AppState) -> bool) {
    // Poll with a wall-clock deadline (live tier: real server latency,
    // real polling intervals); never a fixed wait.
    let deadline = std::time::Instant::now() + LIVE_DEADLINE;
    while !pred(&app.engine.shared_state().load_full()) {
        assert!(
            std::time::Instant::now() < deadline,
            "predicate not satisfied within the live deadline"
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}
