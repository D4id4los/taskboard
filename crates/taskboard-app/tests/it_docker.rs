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
use taskboard_sync_nextcloud::{DeckClient, NewCard, StackFilter, Validators};

mod common;

use common::{
    EnvOverride, create_board_and_stack, env_lock, live_config, retry, seed_board_binding,
};

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
    let app = bootstrap(
        config,
        Arc::new(taskboard_app::secrets::EnvCredentialStore),
        Arc::new(taskboard_app::notifications::LogSink),
    )
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

    let repo =
        taskboard_storage_sqlite::open(&db_path, taskboard_domain::ConnectorId::from("nextcloud"))
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
    let app = bootstrap(
        config,
        Arc::new(taskboard_app::secrets::EnvCredentialStore),
        Arc::new(taskboard_app::notifications::LogSink),
    )
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
    let repo =
        taskboard_storage_sqlite::open(&db_path, taskboard_domain::ConnectorId::from("nextcloud"))
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
    use taskboard_app::commands::connect;
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
            None,
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
        let receipt = connect::run(
            &config_path,
            raw,
            taskboard_app::commands::connect::ConnectArgs {
                server: Some(cfg.url.clone()),
                user: Some(cfg.user.clone()),
                ..Default::default()
            },
            &mut prompts,
            &store,
        )
        .await
        .expect("connect verifies");
        assert!(receipt.verified);

        let attached = common::live_config(cfg, &db_path);
        let publish = boards::publish(
            attached.clone(),
            Arc::clone(&store),
            None,
            Duration::from_secs(120),
            None,
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

/// J5: the editing surface lands server-side — a fresh stand-alone
/// board/stack/task is published (the J4 shape), then every WI-3 edit
/// verb runs against the bound core: `boards rename`, `stacks
/// rename`/`move`, `labels add`/`edit`, `tasks edit`/`label`/`delete`;
/// one sync drains the outbox and the raw `DeckClient` verifies the
/// server-side truth. Local `tasks list` still serves the survivors.
#[tokio::test]
#[ignore = "requires the dockerized Nextcloud tier (scripts/nextcloud_it_setup.sh up)"]
async fn it_nextcloud_docker_cli_editing_push() {
    let Some(cfg) = common::live_docker_config() else {
        return; // skip, never fail: tier not configured
    };
    common::with_deadline(editing_push_journey(&cfg))
        .await
        .expect("test must finish within the live deadline");
}

async fn editing_push_journey(cfg: &common::LiveCfg) {
    use taskboard_app::commands::{boards, connect, labels, stacks, sync, tasks};
    use taskboard_app::config::{AppConfig, StorageSection};
    use taskboard_domain::TaskChanges;

    let _env = common::env_lock();
    let run = common::run_id();
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("taskboard.db");

    // Phase 1 — stand-alone authoring: board + stack + two tasks.
    let standalone = AppConfig {
        storage: StorageSection {
            db_path: Some(db_path.clone()),
        },
        ..AppConfig::default()
    };
    let no_store: Arc<dyn taskboard_app::secrets::CredentialStore> =
        Arc::new(common::FakeCredentialStore::default());
    boards::create(standalone.clone(), Arc::clone(&no_store), &run, None)
        .await
        .expect("board");
    let stack_receipt = stacks::add(standalone.clone(), Arc::clone(&no_store), None, "col", None)
        .await
        .expect("stack");
    let survivor = tasks::add(standalone.clone(), Arc::clone(&no_store), "survivor", None)
        .await
        .expect("task");
    let doomed = tasks::add(standalone, Arc::clone(&no_store), "doomed", None)
        .await
        .expect("task");

    // Phase 2 — attach: login, publish, edit against the bound core.
    let _password = common::EnvOverride::set("TASKBOARD_APP_PASSWORD", &cfg.token);
    let client = DeckClient::new(&cfg.url, &cfg.user, &cfg.token).expect("base URL");
    let config_path = dir.path().join("taskboard.toml");
    let raw = AppConfig::extract_raw(Some(&config_path)).expect("raw config");
    let store: Arc<dyn taskboard_app::secrets::CredentialStore> =
        Arc::new(taskboard_app::secrets::EnvCredentialStore);
    let mut prompts = common::FakePrompts::new(&[&cfg.token]);
    connect::run(
        &config_path,
        raw,
        taskboard_app::commands::connect::ConnectArgs {
            server: Some(cfg.url.clone()),
            user: Some(cfg.user.clone()),
            ..Default::default()
        },
        &mut prompts,
        &store,
    )
    .await
    .expect("connect verifies");
    let attached = common::live_config(cfg, &db_path);
    let publish = boards::publish(
        attached.clone(),
        Arc::clone(&store),
        None,
        Duration::from_secs(120),
        None,
    )
    .await
    .expect("publish succeeds");
    let remote_board_id = publish.board.id;

    // The editing surface, both backends, on the bound board:
    let renamed = boards::rename(
        attached.clone(),
        Arc::clone(&store),
        None,
        &format!("{run}-renamed"),
    )
    .await
    .expect("boards rename");
    assert_eq!(renamed.title, format!("{run}-renamed"));
    let renamed_stack = stacks::rename(
        attached.clone(),
        Arc::clone(&store),
        "col",
        &format!("{run}-col2"),
    )
    .await
    .expect("stacks rename");
    assert_eq!(renamed_stack.title, format!("{run}-col2"));
    let moved = stacks::move_(
        attached.clone(),
        Arc::clone(&store),
        &format!("{run}-col2"),
        4,
    )
    .await
    .expect("stacks move");
    assert_eq!(moved.order, 4);
    let _urgent = labels::add(
        attached.clone(),
        Arc::clone(&store),
        None,
        "urgent",
        Some("ff0000"),
    )
    .await
    .expect("labels add");
    let _ = labels::edit(
        attached.clone(),
        Arc::clone(&store),
        "urgent",
        Some("critical".to_owned()),
        Some("0000ff".to_owned()),
    )
    .await
    .expect("labels edit");
    let edited = tasks::edit(
        attached.clone(),
        Arc::clone(&store),
        &survivor.id,
        TaskChanges {
            title: Some(format!("{run}-edited")),
            description: Some("the body".to_owned()),
            duedate: Some(Some(
                chrono::NaiveDate::from_ymd_opt(2026, 10, 20)
                    .and_then(|d| d.and_hms_opt(0, 0, 0))
                    .expect("midnight")
                    .and_utc(),
            )),
            archived: None,
        },
    )
    .await
    .expect("tasks edit");
    assert_eq!(edited.title, format!("{run}-edited"));
    let labeled = tasks::label(
        attached.clone(),
        Arc::clone(&store),
        &survivor.id,
        "critical",
    )
    .await
    .expect("tasks label");
    assert_eq!(labeled.labels, vec!["critical".to_owned()]);
    let deleted = tasks::delete(attached.clone(), Arc::clone(&store), &doomed.id)
        .await
        .expect("tasks delete");
    assert_eq!(deleted.id, doomed.id);

    // One sync drains the outbox.
    let outcome = sync::run(
        attached.clone(),
        Arc::clone(&store),
        false,
        Duration::from_secs(120),
        None,
    )
    .await
    .expect("sync runs");
    assert!(matches!(
        outcome,
        sync::SyncOutcome::Succeeded(_) | sync::SyncOutcome::NoWait(_)
    ));

    // Server-side truth via the raw DeckClient.
    let server_stacks =
        common::retry(|| async { client.stacks(remote_board_id, StackFilter::Active).await })
            .await
            .expect("server listing");
    assert_eq!(server_stacks.len(), 1);
    assert_eq!(server_stacks[0].title, format!("{run}-col2"));
    assert_eq!(server_stacks[0].order, 4);
    assert_eq!(server_stacks[0].cards.len(), 1, "the doomed card is gone");
    let server_card = &server_stacks[0].cards[0];
    assert_eq!(server_card.title, format!("{run}-edited"));
    assert!(server_card.duedate.is_some(), "the due date was pushed");
    // Deck's stack *listing* omits card labels; the card detail is the
    // authoritative read for the assignment.
    let server_card = common::retry(|| async {
        client
            .card(
                remote_board_id,
                server_stacks[0].id,
                server_stacks[0].cards[0].id,
            )
            .await
    })
    .await
    .expect("card detail");

    let server_board = common::retry(|| async { client.board(remote_board_id).await })
        .await
        .expect("server board");
    assert_eq!(server_board.title, format!("{run}-renamed"));
    let critical = server_board
        .labels
        .iter()
        .find(|l| l.title == "critical" && l.color.as_str() == "0000ff")
        .expect("the edited label exists server-side");
    assert!(
        server_card.labels.iter().any(|l| l.id == critical.id),
        "the label is attached to the card"
    );

    // Local truth: the survivors list with the edited fields.
    let list = tasks::list(attached, Arc::clone(&store), None)
        .await
        .expect("list");
    assert_eq!(list.stacks[0].id, stack_receipt.id);
    assert_eq!(list.stacks[0].tasks.len(), 1);
    assert_eq!(list.stacks[0].tasks[0].title, format!("{run}-edited"));
    assert_eq!(list.stacks[0].tasks[0].labels, vec!["critical".to_owned()]);
}

/// J8: sync over IPC (runbook step 12's automated counterpart): the
/// *binary* daemon serves the dockerized tier, and a one-shot `sync`
/// rides the socket — a fresh cycle outcome, no per-command boot.
#[tokio::test]
#[ignore = "requires the dockerized Nextcloud tier (scripts/nextcloud_it_setup.sh up)"]
async fn it_nextcloud_docker_sync_over_ipc() {
    let Some(cfg) = common::live_docker_config() else {
        return; // skip, never fail: tier not configured
    };
    common::with_deadline(async {
        let _env = env_lock();
        let _password = EnvOverride::set("TASKBOARD_APP_PASSWORD", &cfg.token);

        let dir = tempfile::tempdir().expect("tempdir");
        let config = dir.path().join("taskboard.toml");
        common::write_test_config(&config, Some(&cfg.url), Some(&cfg.user)).expect("write config");
        let client = DeckClient::new(&cfg.url, &cfg.user, &cfg.token).expect("base URL");
        let run = common::run_id();
        let (board, stack) = create_board_and_stack(&client, &run).await;
        seed_board_binding(
            &dir.path().join("taskboard.db"),
            0x00DA_001C5,
            board.id,
            &run,
        )
        .await;

        let mut daemon = std::process::Command::new(env!("CARGO_BIN_EXE_taskboard"))
            .args(["--config", config.to_str().expect("utf8 path"), "daemon"])
            .env("TASKBOARD_APP_PASSWORD", &cfg.token)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("spawn daemon");
        // Socket readiness: status answers daemon:true over IPC.
        let binary = env!("CARGO_BIN_EXE_taskboard");
        let ready = std::time::Instant::now() + LIVE_DEADLINE;
        loop {
            let output = tokio::process::Command::new(binary)
                .args([
                    "--config",
                    config.to_str().expect("utf8"),
                    "--json",
                    "status",
                ])
                .env("TASKBOARD_APP_PASSWORD", &cfg.token)
                .output()
                .await
                .expect("status runs");
            if String::from_utf8_lossy(&output.stdout).contains("\"daemon\":true") {
                break;
            }
            assert!(
                std::time::Instant::now() < ready,
                "daemon never served status"
            );
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }

        // A server-side card appears via the IPC-served list after a
        // cycle; `sync` over IPC produces a fresh cycle outcome (exit 0).
        let server_card = common::retry(|| async {
            client
                .create_card(
                    board.id,
                    stack.id,
                    &NewCard {
                        title: format!("{run}-ipc"),
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
        let deadline = std::time::Instant::now() + LIVE_DEADLINE;
        loop {
            let output = tokio::process::Command::new(binary)
                .args(["--config", config.to_str().expect("utf8"), "sync"])
                .env("TASKBOARD_APP_PASSWORD", &cfg.token)
                .output()
                .await
                .expect("sync runs");
            assert_eq!(
                output.status.code(),
                Some(0),
                "sync over ipc exits 0: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            let list = tokio::process::Command::new(binary)
                .args(["--config", config.to_str().expect("utf8"), "tasks", "list"])
                .env("TASKBOARD_APP_PASSWORD", &cfg.token)
                .output()
                .await
                .expect("tasks list runs");
            if String::from_utf8_lossy(&list.stdout).contains(&server_card.title) {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "the card never pulled"
            );
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }

        // Graceful remote stop.
        let stop = tokio::process::Command::new(binary)
            .args(["--config", config.to_str().expect("utf8"), "daemon", "stop"])
            .env("TASKBOARD_APP_PASSWORD", &cfg.token)
            .output()
            .await
            .expect("daemon stop runs");
        assert_eq!(stop.status.code(), Some(0));
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        loop {
            if daemon.try_wait().expect("poll daemon").is_some() {
                break;
            }
            assert!(std::time::Instant::now() < deadline, "daemon did not exit");
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("test must finish within the live deadline");
}

// -------------------------------------------------------------------------
// The v0.1.x additive-upgrade boot test (design §3.9): a pre-0006 db +
// scalar config boot the new build; the synthesis-of-one connector adopts
// the world. Two halves: a hermetic boot-time half (runs everywhere) and
// the full merge+drain half (dockerized, MD-5).

/// The hermetic half: boot against a refused server asserts the upgrade's
/// boot-time contract only — migration 0006 ran over the v0.1.x db, the
/// binding decodes tagged-default, the synthesized connector owns the
/// world, and the singleton's non-idle phase settles via the offline-first
/// path. Merge + drain assertions live in the dockerized half.
#[tokio::test]
async fn v0_1_x_additive_upgrade_boot_settles_offline_without_a_server() {
    let _env = env_lock();
    let _password = EnvOverride::set("TASKBOARD_APP_PASSWORD", "hermetic-password");
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("taskboard.db");
    common::write_v01x_db(&db_path, 77, 7, 55, "v01x local rename").await;

    let config = common::live_config(
        &common::LiveCfg {
            url: "http://127.0.0.1:9".into(), // refused fast
            user: "v01x".into(),
            token: String::new(),
        },
        &db_path,
    );
    config.validate().expect("config valid");

    let app = bootstrap(
        config,
        Arc::new(taskboard_app::secrets::EnvCredentialStore),
        Arc::new(taskboard_app::notifications::LogSink),
    )
    .await
    .expect("the v0.1.x db boots");

    // Decode + synthesis: the binding is the synthesized connector's
    // tagged-default decode (NULL column ⇒ "nextcloud", board 77).
    eventually(&app, |state| {
        state.boards.values().any(|b| {
            b.remote
                .as_ref()
                .is_some_and(|r| r.connector.as_str() == "nextcloud" && r.board.get() == 77)
        })
    })
    .await;

    // The singleton's `syncing` phase hydrates onto the bound board's row
    // (migration backfill) and settles via the offline-first failure path:
    // the first cycle reports `Failed { Network }` (no pushes), and the
    // engine files it under the bound board.
    eventually(&app, |state| {
        state.sync.iter().any(|(board, row)| {
            matches!(
                row.phase,
                taskboard_domain::SyncPhase::Failed {
                    last_error: taskboard_domain::SyncErrorKind::Network,
                }
            ) && state
                .boards
                .get(board)
                .and_then(|b| b.remote.as_ref())
                .is_some_and(|r| r.board.get() == 77)
        })
    })
    .await;

    let outcome = app.shutdown().await;
    assert!(!outcome.actor_aborted);
}

/// The tier-2 half: the full §3.9 assertion list against the dockerized
/// Nextcloud — synthesized connector, tagged-default decode, status-row
/// hydration + settle, a cycle merging per the unchanged policy (epoch
/// clocks: the remote board's title/color win), and the queued op
/// draining (its effect visible remotely through one typed read).
#[tokio::test]
#[ignore = "requires the dockerized Nextcloud tier (scripts/nextcloud_it_setup.sh up)"]
async fn it_nextcloud_docker_v0_1_x_additive_upgrade_boot() {
    let Some(cfg) = common::live_docker_config() else {
        return; // skip, never fail: tier not configured
    };
    common::with_deadline(v0_1_x_additive_upgrade(&cfg))
        .await
        .expect("test must finish within the live deadline");
}

async fn v0_1_x_additive_upgrade(cfg: &common::LiveCfg) {
    let _env = env_lock();
    let _password = EnvOverride::set("TASKBOARD_APP_PASSWORD", &cfg.token);
    let client = DeckClient::new(&cfg.url, &cfg.user, &cfg.token).expect("base URL");
    let run = common::run_id();
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("taskboard.db");
    let (board, stack) = create_board_and_stack(&client, &run).await;
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
    let local_title = format!("{run}-local-rename");
    let ids =
        common::write_v01x_db(&db_path, board.id, stack.id, server_card.id, &local_title).await;

    let config = common::live_config(cfg, &db_path);
    let app = bootstrap(
        config,
        Arc::new(taskboard_app::secrets::EnvCredentialStore),
        Arc::new(taskboard_app::notifications::LogSink),
    )
    .await
    .expect("the v0.1.x db boots");

    app.engine
        .execute(taskboard_domain::StateCommand::RequestSync)
        .await
        .expect("RequestSync accepted");

    // (1) tagged-default decode + synthesis; (2) the hydrated status row
    // settled (last_success set, phase Idle); (3) the merge ran per the
    // unchanged policy — the epoch-clocked local board loses title and
    // color to the remote; (4) the queued op drained.
    eventually(&app, |state| {
        let Some(b) = state
            .boards
            .get(&taskboard_domain::BoardId::from_uuid(ids.board))
        else {
            return false;
        };
        let binding_ok = b
            .remote
            .as_ref()
            .is_some_and(|r| r.connector.as_str() == "nextcloud" && r.board.get() == board.id);
        let row = state.sync.get(b.id);
        let settled = row.is_some_and(|r| {
            r.phase == taskboard_domain::SyncPhase::Idle && r.last_success.is_some()
        });
        let merged = b.title == run && b.color.as_str() == "00c2e0";
        let drained = row.is_some_and(|r| r.pending_ops == 0);
        binding_ok && settled && merged && drained
    })
    .await;

    // The op's effect is visible remotely (one typed read): the local
    // rename won the push and the pull adopted the echo.
    let stacks = common::retry(|| async {
        client
            .fetch_stacks(board.id, StackFilter::Active, &Validators::default())
            .await
            .map(|fetched| fetched.data)
    })
    .await
    .expect("remote read")
    .expect("stacks listed");
    let card_title = stacks
        .iter()
        .flat_map(|s| &s.cards)
        .find(|c| c.id == server_card.id)
        .map(|c| c.title.clone())
        .expect("the bound card survives the cycle");
    assert_eq!(
        card_title, local_title,
        "the queued rename drained remotely"
    );

    let outcome = app.shutdown().await;
    assert!(!outcome.actor_aborted);
}

/// The two-connector tier-2 exit criterion (D5-18): two Nextcloud
/// instances live simultaneously, one board bound to each, per-connector
/// health visible in the per-board rows — one connector down flips only
/// its own boards `Offline` while the other keeps cycling (no global
/// `NetworkLost`), and the second `docker stop` crosses the all-down
/// boundary (the global fires).
///
/// Both instances are test-owned (suffixed compose projects on private
/// ports) so the outage orchestration cannot disturb the sibling tier
/// tests sharing the singleton instance.
#[tokio::test]
#[ignore = "requires docker and the tier context (TASKBOARD_IT_DOCKER_URL)"]
async fn it_nextcloud_docker_two_connectors_live_and_partial_outage() {
    // Tier gate: the tier context marker (the shared harness's export
    // block). This test owns its two instances, so only intent and a
    // docker host are required.
    if std::env::var("TASKBOARD_IT_DOCKER_URL").is_err() {
        eprintln!(
            "SKIP (honest, MD-5): the two-connector tier test needs the tier context \
             (eval \"scripts/nextcloud_it_setup.sh up\"); the hermetic N=2 pins in \
             bootstrap::connector_module_tests cover the registry and authority \
             plumbing, the dockerized journeys here are the live-server half"
        );
        return;
    }
    // The shared LIVE_DEADLINE budgets one journey; this test adds two
    // instance spawns to it, so it carries its own larger budget.
    // `two_connectors_live_and_partial_outage` panics on any assertion;
    // only the budget needs guarding here.
    if tokio::time::timeout(
        Duration::from_mins(15),
        two_connectors_live_and_partial_outage(),
    )
    .await
    .is_err()
    {
        panic!("the two-connector journey exceeded its 15-minute budget");
    }
}

// All exits — assertion panics included — tear the instances down
// (Drop runs on unwind; leaked containers poison the next run).
struct Cleanup(common::OwnedInstance);
impl Drop for Cleanup {
    fn drop(&mut self) {
        // Best-effort container restore, then the volume teardown.
        let _ = common::docker_cli(&["start", &format!("taskboard-it-nextcloud{}", self.0.suffix)]);
        common::teardown_instance_sync(&self.0.suffix);
    }
}

async fn two_connectors_live_and_partial_outage() {
    // Two test-owned instances, brought up sequentially: concurrent
    // compose invocations on the shared compose file proved flaky on
    // this runner (one instance was stopped mid-run); sequential spawns
    // are the harness's own convention.
    let up = |suffix: &'static str, port: u16| {
        tokio::task::spawn_blocking(move || common::spawn_instance_sync(suffix, port))
    };
    let a = up("-c5a", 8390)
        .await
        .expect("instance A join")
        .unwrap_or_else(|err| panic!("instance A did not come up: {err}"));
    let b = up("-c5b", 8391)
        .await
        .expect("instance B join")
        .unwrap_or_else(|err| panic!("instance B did not come up: {err}"));

    // All exits — assertion panics included — tear both instances down
    // (Drop runs on unwind; leaked containers poison the next run).
    let _cleanup_a = Cleanup(a.clone());
    let _cleanup_b = Cleanup(b.clone());

    two_connector_body(&a, &b)
        .await
        .expect("the two-connector journey");
}

type BoxError = Box<dyn std::error::Error + Send + Sync>;

async fn two_connector_body(
    a: &common::OwnedInstance,
    b: &common::OwnedInstance,
) -> Result<(), BoxError> {
    use taskboard_app::config::{AppConfig, ConnectorCfg, CredentialStoreKind, SyncBackend};
    use taskboard_domain::SyncPhase;

    let client_a = DeckClient::new(&a.url, &a.user, &a.token)?;
    let client_b = DeckClient::new(&b.url, &b.user, &b.token)?;
    let run = common::run_id();
    let dir = tempfile::tempdir()?;
    let db_path = dir.path().join("taskboard.db");

    // One board + stack + card per server: the pull has content on both
    // connectors from the first cycle.
    let (board_a, stack_a) = create_board_and_stack(&client_a, &run).await;
    let (board_b, stack_b) = create_board_and_stack(&client_b, &run).await;
    let _ = retry(|| async {
        client_a
            .create_card(
                board_a.id,
                stack_a.id,
                &NewCard {
                    title: format!("{run}-a"),
                    order: Some(0),
                    description: None,
                    duedate: None,
                    kind: None,
                },
            )
            .await
    })
    .await?;
    let _ = retry(|| async {
        client_b
            .create_card(
                board_b.id,
                stack_b.id,
                &NewCard {
                    title: format!("{run}-b"),
                    order: Some(0),
                    description: None,
                    duedate: None,
                    kind: None,
                },
            )
            .await
    })
    .await?;

    // The cross-connector world, fixtured in the db (chunk 5 has no
    // ceremony verb that mints a non-default tag; chunk 6's move-board
    // surface does): alpha owns A's board, beta owns B's.
    common::seed_tagged_board_binding(&db_path, 0x00C5_000A, board_a.id, "alpha", &run).await;
    common::seed_tagged_board_binding(&db_path, 0x00C5_000B, board_b.id, "beta", &run).await;

    // A real [[sync.connectors]] config, both entries nextcloud, short
    // poll so cycles are observable; credentials via the keyed fixed
    // store (two identities).
    let mut config = AppConfig::default();
    config.sync.connectors = Some(vec![
        ConnectorCfg {
            name: "alpha".into(),
            backend: SyncBackend::Nextcloud,
            server_url: a.url.clone(),
            username: a.user.clone(),
        },
        ConnectorCfg {
            name: "beta".into(),
            backend: SyncBackend::Nextcloud,
            server_url: b.url.clone(),
            username: b.user.clone(),
        },
    ]);
    config.nextcloud.credential_store = CredentialStoreKind::Env;
    config.sync.poll_interval_secs = 1;
    config.sync.backoff_initial_secs = 1;
    config.sync.backoff_max_secs = 5;
    config.app.shutdown_timeout_secs = 5;
    config.storage.db_path = Some(db_path.clone());
    config.validate()?;

    let mut store = common::KeyedFixedStore::with(&a.url, &a.user, &a.token);
    store.add(&b.url, &b.user, &b.token);
    let app = bootstrap(
        config,
        std::sync::Arc::new(store),
        std::sync::Arc::new(taskboard_app::notifications::LogSink),
    )
    .await?;
    let mut events = app.system.subscribe();

    let board_id = |raw: u128| taskboard_domain::BoardId::from_uuid(uuid::Uuid::from_u128(raw));
    let alpha_board = board_id(0x00C5_000A);
    let beta_board = board_id(0x00C5_000B);

    eprintln!("tier: both instances up; boards + cards created; db fixtured");
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_target(true)
        .try_init();
    // (1) Two connectors live: both boards' rows settle Idle with a
    // last_success, each binding tagged with its own connector, and the
    // server-side cards landed through each connector's own actor.
    eventually(&app, |state| {
        state.sync.iter().count() == 2
            && state
                .sync
                .iter()
                .all(|(_, row)| matches!(row.phase, SyncPhase::Idle) && row.last_success.is_some())
    })
    .await;
    let state = app.engine.shared_state().load_full();
    for (board, tag, remote) in [
        (alpha_board, "alpha", board_a.id),
        (beta_board, "beta", board_b.id),
    ] {
        let bound = &state.boards[&board];
        let binding = bound.remote.as_ref().expect("seeded binding");
        assert_eq!(binding.connector.as_str(), tag, "{tag}'s binding tag");
        assert_eq!(binding.board.get(), remote, "{tag}'s remote board");
    }
    let pulled_both = state.tasks.values().filter(|t| t.remote.is_some()).count();
    drop(state);
    assert_eq!(pulled_both, 2, "each connector pulled its own board's card");

    // `docker stop` is the new tier-only orchestration step (D5-18):
    // bounded by the spawn_blocking join, recorded done via the flag.
    let stop = |suffix: &str| {
        let container = format!("taskboard-it-nextcloud{suffix}");
        tokio::task::spawn_blocking(move || common::docker_cli(&["stop", &container]))
    };

    // (2) One down: alpha's container stops — its boards flip Offline
    // while beta keeps cycling (its last_success advances) and the
    // global banner stays silent.
    let stop_a = stop(&a.suffix);
    stop_a.await.expect("stop join").expect("docker stop alpha");
    let beta_before = app
        .engine
        .shared_state()
        .load_full()
        .sync
        .get(beta_board)
        .and_then(|row| row.last_success)
        .expect("beta settled before the outage");
    eventually(&app, |state| {
        state
            .sync
            .get(alpha_board)
            .is_some_and(|row| matches!(row.phase, SyncPhase::Offline))
    })
    .await;
    eventually(&app, |state| {
        state
            .sync
            .get(beta_board)
            .and_then(|row| row.last_success)
            .is_some_and(|at| at > beta_before)
    })
    .await;
    eprintln!("tier: alpha down, its boards Offline, beta cycling, global silent");
    drain_no_global(&mut events, "while beta responds").await?;

    eprintln!("tier: stopping beta; awaiting the global NetworkLost");
    // (3) All down: stopping beta crosses the boundary — the authority
    // emits the global NetworkLost.
    let stop_b = stop(&b.suffix);
    stop_b.await.expect("stop join").expect("docker stop beta");
    let deadline = std::time::Instant::now() + Duration::from_secs(120);
    loop {
        match tokio::time::timeout(Duration::from_secs(5), events.recv()).await {
            Ok(Ok(taskboard_domain::SystemEvent::NetworkLost)) => break,
            Ok(Ok(_)) => {}
            Ok(Err(err)) => return Err(err.into()),
            Err(_elapsed) => assert!(
                std::time::Instant::now() <= deadline,
                "NetworkLost never arrived after both connectors stopped",
            ),
        }
    }

    Ok(())
}

/// Bounded drain asserting no global network event arrives (the
/// partial-outage half of the D-W7 bar).
async fn drain_no_global(
    events: &mut tokio::sync::broadcast::Receiver<taskboard_domain::SystemEvent>,
    while_: &str,
) -> Result<(), BoxError> {
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    loop {
        match tokio::time::timeout(Duration::from_millis(200), events.recv()).await {
            Ok(Ok(taskboard_domain::SystemEvent::NetworkLost)) => {
                panic!("NetworkLost fired {while_}")
            }
            Ok(Ok(taskboard_domain::SystemEvent::NetworkRestored)) => {
                panic!("NetworkRestored fired {while_}")
            }
            Ok(Ok(_)) => {}
            Ok(Err(tokio::sync::broadcast::error::RecvError::Closed)) => break,
            Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(n))) => {
                eprintln!("drain_no_global: lagged; {n} events skipped");
            }
            Err(_elapsed) => {
                if std::time::Instant::now() > deadline {
                    break;
                }
            }
        }
    }
    Ok(())
}
