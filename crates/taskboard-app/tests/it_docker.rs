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
use taskboard_domain::persistence::TaskRepository as _;
use taskboard_sync_nextcloud::{DeckClient, NewCard, StackFilter};

mod common;

use common::{EnvOverride, create_board_and_stack, env_lock, live_config, seed_board_binding};

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
    let _password = EnvOverride::set("TASKBOARD_APP_PASSWORD", &cfg.token);
    let client = DeckClient::new(&cfg.url, &cfg.user, &cfg.token).expect("base URL");
    let run = common::run_id();
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("taskboard.db");
    let (board, stack) = create_board_and_stack(&client, &run).await;
    seed_board_binding(&db_path, 0x00DA_000E, board.id, &run).await;

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

    // Bootstrap the daemon with the env credential store.
    let config = live_config(cfg, &db_path);
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
    let _password = EnvOverride::set("TASKBOARD_APP_PASSWORD", &cfg.token);
    let client = DeckClient::new(&cfg.url, &cfg.user, &cfg.token).expect("base URL");
    let run = common::run_id();
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("taskboard.db");
    let (board, stack) = create_board_and_stack(&client, &run).await;
    seed_board_binding(&db_path, 0x00DB_000E, board.id, &run).await;

    // Bootstrap the daemon with the env credential store, forcing the
    // bounded-abort path.
    let mut config = live_config(cfg, &db_path);
    config.app.shutdown_timeout_secs = 0;
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

/// J4: the stand-alone publish journey — author offline with zero
/// server configured (`boards create` → `stacks add` → `tasks add`),
/// then attach via `login` + `boards publish`: the local board, stack,
/// and task land on a fresh Deck board, and a fresh invocation still
/// serves the local ids (binding preserved).
#[tokio::test]
#[ignore = "requires the dockerized Nextcloud tier (scripts/nextcloud_it_setup.sh up)"]
#[allow(clippy::items_after_statements)] // the journey reads as one narrative
async fn it_nextcloud_docker_cli_standalone_publish() {
    use taskboard_app::commands::boards;
    use taskboard_app::commands::login;
    use taskboard_app::commands::stacks;
    use taskboard_app::commands::tasks;
    use taskboard_app::config::{AppConfig, StorageSection};

    let Some(cfg) = common::live_docker_config() else {
        return; // skip, never fail: tier not configured
    };
    common::with_deadline(standalone_publish_journey(&cfg))
        .await
        .expect("test must finish within the live deadline");

    async fn standalone_publish_journey(cfg: &common::LiveCfg) {
        let _env = common::env_lock();
        let run = common::run_id();
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("taskboard.db");

        // Phase 1 — fully stand-alone: no server_url, no credentials.
        let standalone = AppConfig {
            storage: StorageSection {
                db_path: Some(db_path.clone()),
            },
            ..AppConfig::default()
        };
        let no_store: Arc<dyn taskboard_app::secrets::CredentialStore> =
            Arc::new(common::FakeCredentialStore::default());
        let board_receipt = boards::create(standalone.clone(), Arc::clone(&no_store), &run, None)
            .await
            .expect("stand-alone board create");
        let stack_receipt = stacks::add(
            standalone.clone(),
            Arc::clone(&no_store),
            &format!("{run}-col"),
            None,
        )
        .await
        .expect("stand-alone stack add");
        let task_receipt = tasks::add(
            standalone,
            Arc::clone(&no_store),
            &format!("{run}-task"),
            None,
        )
        .await
        .expect("stand-alone task add");

        // Phase 2 — attach: login, then publish the local board.
        let _password = common::EnvOverride::set("TASKBOARD_APP_PASSWORD", &cfg.token);
        let client = DeckClient::new(&cfg.url, &cfg.user, &cfg.token).expect("base URL");
        let config_path = dir.path().join("taskboard.toml");
        let raw = AppConfig::extract_raw(Some(&config_path)).expect("raw config");
        let store: Arc<dyn taskboard_app::secrets::CredentialStore> =
            Arc::new(taskboard_app::secrets::EnvCredentialStore);
        let mut prompts = common::FakePrompts::new(&[&cfg.token]);
        let receipt = login::run(
            &config_path,
            raw,
            Some(cfg.url.clone()),
            Some(cfg.user.clone()),
            &mut prompts,
            &store,
        )
        .await
        .expect("login verifies");
        assert!(receipt.verified);

        let attached = common::live_config(cfg, &db_path);
        let publish = boards::publish(
            attached.clone(),
            Arc::clone(&store),
            Duration::from_secs(120),
        )
        .await
        .expect("publish succeeds");
        let remote_board_id = publish.board.id;

        // Server-side truth: the published board holds the local stack
        // and card, with the authored titles.
        let stacks =
            common::retry(|| async { client.stacks(remote_board_id, StackFilter::Active).await })
                .await
                .expect("server listing");
        assert_eq!(stacks.len(), 1, "the local stack was pushed");
        assert_eq!(stacks[0].title, format!("{run}-col"));
        assert!(
            stacks[0]
                .cards
                .iter()
                .any(|c| c.title == format!("{run}-task")),
            "the local task was pushed onto the published board"
        );

        // A fresh invocation still serves the local ids (binding
        // preserved, no re-adoption duplicates).
        let list = tasks::list(attached, Arc::clone(&store), None)
            .await
            .expect("list");
        assert_eq!(list.stacks.len(), 1);
        assert_eq!(list.stacks[0].id, stack_receipt.id);
        assert_eq!(list.stacks[0].tasks.len(), 1);
        assert_eq!(list.stacks[0].tasks[0].id, task_receipt.id);
        assert_eq!(list.stacks[0].tasks[0].title, format!("{run}-task"));
        let _ = board_receipt;

        let deleted = common::retry(|| async { client.delete_board(remote_board_id).await }).await;
        assert!(deleted.is_ok(), "board teardown must succeed");
    }
}
