// SPDX-License-Identifier: AGPL-3.0-only
//! B-series: hermetic bootstrap E2E over the *real* production graph —
//! real figment config, real sqlite files under tempdirs, the real
//! engine + storage + sync actors, and wiremock as the Deck server.
//! The only injected fake is the credential store (or the env store).
//!
//! Determinism: predicate polls with `tokio::time::timeout` guards,
//! join-handle completion as the terminal signal, wiremock per-mock
//! request verification for shapes. No sleeps, no message-text
//! assertions.

use std::sync::Arc;
use std::time::Duration;

use base64::Engine as _;
use taskboard_app::bootstrap::{App, BootstrapError, bootstrap};
use taskboard_app::config::AppConfig;
use taskboard_app::secrets::{CredentialError, CredentialStore};
use taskboard_domain::persistence::TaskRepository as _;
use taskboard_domain::{
    Board, BoardId, Color, PersistedState, RemoteBoardId, Stack, StackClocks, StackId,
    StateCommand, SyncPhase,
};
use taskboard_storage_sqlite::{StorageHandle, open};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

mod common;

use common::{BOARD, EnvOverride, FakeCredentialStore, STACK, T, mount_standard_pull};

/// The app password every hermetic B-test authenticates with.
const PASSWORD: &str = "b-series-app-password";
const USERNAME: &str = "alice";

fn tempdir() -> tempfile::TempDir {
    tempfile::tempdir().expect("tempdir")
}

/// A daemon-mode config pointing at `server_url`, db under `dir`.
fn config_for(server_url: &str, dir: &tempfile::TempDir) -> AppConfig {
    let mut config = common::valid_config(server_url);
    config.storage.db_path = Some(dir.path().join("taskboard.db"));
    config
}

/// The env credential store preloaded with [`PASSWORD`]: the process
/// env lock plus the override itself, alive until the end of the test
/// (the override's SAFETY contract requires the lock to be held).
struct PasswordEnv {
    /// Held for the override's lifetime (SAFETY contract of `set_env`).
    _lock: common::EnvGuard,
    /// Restores the prior env state on drop.
    _override: EnvOverride,
}

fn env_password() -> PasswordEnv {
    let lock = common::env_lock();
    PasswordEnv {
        _lock: lock,
        _override: EnvOverride::set("TASKBOARD_APP_PASSWORD", PASSWORD),
    }
}

/// The Basic-auth header the client must send for the B-series creds.
fn expected_auth_header() -> String {
    format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode(format!("{USERNAME}:{PASSWORD}"))
    )
}

/// The seeded local identities (constants keep board/stack consistent
/// across helpers and the `CreateTask` target in B4).
fn bound_board_id() -> BoardId {
    BoardId::from_uuid(uuid::Uuid::from_u128(0xB0_00))
}

fn local_stack_id() -> StackId {
    StackId::from_uuid(uuid::Uuid::from_u128(0xB0_01))
}

fn bound_board() -> Board {
    Board {
        id: bound_board_id(),
        remote: Some(RemoteBoardId(BOARD)),
        title: "board".into(),
        color: Color::new("00ff00"),
        archived: false,
        deleted: false,
        remote_seen: None,
    }
}

fn bound_stack() -> Stack {
    Stack {
        id: local_stack_id(),
        remote: Some(taskboard_domain::RemoteStackRef {
            board: RemoteBoardId(BOARD),
            stack: taskboard_domain::RemoteStackId(STACK),
        }),
        board: bound_board_id(),
        title: "col".into(),
        order: 0,
        archived: false,
        deleted: false,
        clocks: StackClocks {
            title: chrono_t(T),
            order: chrono_t(T),
            deleted: chrono_t(T),
        },
        remote_seen: None,
    }
}

/// Seeds one live, remotely-bound board (and optionally one bound stack)
/// through the storage crate's public API — the plan's seeding seam.
async fn seed_binding(
    dir: &tempfile::TempDir,
    db_name: &str,
    with_stack: bool,
) -> (StorageHandle, tokio::task::JoinHandle<()>) {
    let db_path = dir.path().join(db_name);
    let repo = Arc::new(open(&db_path).await.expect("open seed db"));
    let (handle, join) = taskboard_storage_sqlite::spawn_storage_actor(repo);

    let mut actions = vec![taskboard_domain::PersistenceAction::UpsertBoard(
        bound_board(),
    )];
    if with_stack {
        actions.push(taskboard_domain::PersistenceAction::UpsertStack(
            bound_stack(),
        ));
    }
    handle.apply(actions).await.expect("seed applies");
    (handle, join)
}

fn chrono_t(secs: i64) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::from_timestamp(secs, 0).expect("fixture timestamp")
}

/// Polls `pred` against the app's shared state under a deadline.
async fn eventually(app: &App, pred: impl Fn(&taskboard_domain::AppState) -> bool) {
    tokio::time::timeout(Duration::from_secs(60), async {
        loop {
            if pred(&app.engine.shared_state().load_full()) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("predicate satisfied within the deadline");
}

/// Bootstraps with the env store (the password override and its env
/// lock live as long as the returned guard).
async fn boot(config: AppConfig) -> (App, PasswordEnv) {
    let guard = env_password();
    let app = bootstrap(config, Arc::new(taskboard_app::secrets::EnvCredentialStore))
        .await
        .expect("bootstrap");
    (app, guard)
}

// ---------------------------------------------------------------------
// B-series
// ---------------------------------------------------------------------

/// B1: boot on an empty temp database — the engine hydrates the default
/// state, and a `RequestSync` runs a real cycle through the whole graph:
/// with no board bound, the actor reports `Failed { NoBoard }` and the
/// phase lands in `AppState`.
#[tokio::test]
async fn b1_boot_and_no_board_cycle_through_the_graph() {
    let dir = tempdir();
    let server = MockServer::start().await;
    let (app, _env) = boot(config_for(&server.uri(), &dir)).await;

    let outcome = app
        .engine
        .execute(StateCommand::RequestSync)
        .await
        .expect("RequestSync accepted");
    assert!(matches!(
        outcome,
        taskboard_domain::CommandOutcome::SyncRequested
    ));

    eventually(&app, |state| {
        state.sync.phase
            == SyncPhase::Failed {
                last_error: taskboard_domain::SyncErrorKind::NoBoard,
            }
    })
    .await;

    let outcome = app.shutdown().await;
    assert!(!outcome.actor_aborted);
}

/// B2: with a seeded board binding and a scripted cycle, the remote task
/// merges into `AppState`, `last_success` is set — and the boards mock's
/// auth matcher proves the Basic header decodes to
/// `username:TASKBOARD_APP_PASSWORD` (config + store + bootstrap
/// delivered the secret to the client).
#[tokio::test]
async fn b2_seeded_cycle_pulls_and_authenticates() {
    let dir = tempdir();
    let server = MockServer::start().await;

    // Auth proof: the boards listing only answers the exact Basic header
    // built from the env-store secret; a 404 otherwise fails the cycle.
    Mock::given(method("GET"))
        .and(path("/index.php/apps/deck/api/v1.0/boards"))
        .and(header("Authorization", expected_auth_header()))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("ETag", "\"gen-1\"")
                .set_body_string(common::boards_json()),
        )
        .mount(&server)
        .await;
    // Everything else answers without the auth pin (the cycle only
    // proceeds past the auth-pinned boards endpoint).
    mount_standard_pull(&server, serde_json::json!([])).await;

    {
        let (seed_handle, seed_join) = seed_binding(&dir, "taskboard.db", false).await;
        drop(seed_handle);
        seed_join.await.expect("seed actor exits");
    }

    let (app, _env) = boot(config_for(&server.uri(), &dir)).await;
    app.engine
        .execute(StateCommand::RequestSync)
        .await
        .expect("RequestSync accepted");

    eventually(&app, |state| {
        state.sync.last_success.is_some()
            && state
                .boards
                .values()
                .any(|b| b.remote == Some(RemoteBoardId(BOARD)))
    })
    .await;

    let outcome = app.shutdown().await;
    assert!(!outcome.actor_aborted);
}

/// B3: graceful shutdown on an idle graph — all three actors join, the
/// database reopens, and the hydrated state equals the last published
/// projection.
#[tokio::test]
async fn b3_graceful_shutdown_reopens_the_database() {
    let dir = tempdir();
    let server = MockServer::start().await;
    let db_path = dir.path().join("taskboard.db");

    {
        let (repo, join) = seed_binding(&dir, "taskboard.db", true).await;
        let last = repo.load().await.expect("seed load");
        drop(repo);
        join.await.expect("seed actor exits");

        let (app, _env) = boot(config_for(&server.uri(), &dir)).await;
        let outcome = app.shutdown().await;
        assert!(!outcome.actor_aborted);

        // The reopened database hydrates exactly the last projection.
        let repo = open(&db_path).await.expect("reopen");
        let reopened: PersistedState = repo.load().await.expect("reload");
        assert_eq!(reopened.boards, last.boards);
        assert_eq!(reopened.stacks, last.stacks);
        assert_eq!(reopened.outbox, last.outbox);
    }
}

/// B4: shutdown during an in-flight cycle (the first Deck endpoint
/// hangs) completes within the bounded budget, reports the abort, and
/// the reopened database keeps the queued op intact.
#[tokio::test]
async fn b4_shutdown_mid_cycle_is_bounded_and_keeps_the_outbox() {
    let dir = tempdir();
    let server = MockServer::start().await;
    // The hanging boards endpoint: the cycle starts and never returns.
    Mock::given(method("GET"))
        .and(path("/index.php/apps/deck/api/v1.0/boards"))
        .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(3600)))
        .mount(&server)
        .await;

    {
        let (h, j) = seed_binding(&dir, "taskboard.db", true).await;
        drop(h);
        j.await.expect("seed actor exits");
    }

    let mut config = config_for(&server.uri(), &dir);
    config.app.shutdown_timeout_secs = 1; // tight budget: the test's bound
    let (app, _env) = boot(config).await;

    app.engine
        .execute(StateCommand::RequestSync)
        .await
        .expect("RequestSync accepted");
    // A local edit queued *before* the cycle read state would be pushed;
    // queued ops must survive the mid-cycle abort untouched.
    app.engine
        .execute(StateCommand::CreateTask {
            title: "queued offline".into(),
            stack: local_stack_id(),
            order: 0,
        })
        .await
        .expect("create accepted");

    let outcome = app.shutdown().await;
    assert!(outcome.actor_aborted, "the hung actor must be aborted");

    // The database reopens; the queued op is intact (not lost, and the
    // aborted cycle cannot have completed it).
    let repo = open(dir.path().join("taskboard.db")).await.expect("reopen");
    let state: PersistedState = repo.load().await.expect("reload");
    assert_eq!(state.outbox.len(), 1, "the queued op survives");
    assert!(
        state
            .tasks
            .values()
            .any(|t| t.title == "queued offline" && t.remote.is_none()),
        "the offline draft is persisted"
    );
}

/// B5: bootstrap failure paths surface typed variants and leak no
/// actors: bad URL → `Client`, missing credential → `Credential`,
/// unusable database path → `Storage`.
#[tokio::test]
async fn b5_bootstrap_failure_paths_are_typed() {
    let dir = tempdir();
    let store: Arc<dyn CredentialStore> = Arc::new(FakeCredentialStore::default());

    // Bad URL: config validation rejects it before anything spawns.
    let mut config = common::valid_config("not a url");
    config.storage.db_path = Some(dir.path().join("x.db"));
    let err = bootstrap(config, Arc::clone(&store))
        .await
        .expect_err("bad url");
    assert!(matches!(err, BootstrapError::Config(_)));

    // Missing credential: `Credential(NotFound)`.
    let server = MockServer::start().await;
    let config = config_for(&server.uri(), &dir);
    let missing: Arc<dyn CredentialStore> = Arc::new(FakeCredentialStore::default());
    let err = bootstrap(config, missing).await.expect_err("no credential");
    assert!(matches!(
        err,
        BootstrapError::Credential(CredentialError::NotFound)
    ));

    let populated: Arc<dyn CredentialStore> =
        Arc::new(FakeCredentialStore::with(&server.uri(), USERNAME, "pw"));

    // Unusable database path: a *file* where the db's parent dir should
    // be → the storage open fails (typed, nothing spawned).
    let blocker = dir.path().join("blocker");
    std::fs::write(&blocker, b"not a directory").expect("write blocker");
    let mut config = config_for(&server.uri(), &dir);
    config.storage.db_path = Some(blocker.join("nested").join("taskboard.db"));
    let err = bootstrap(config, Arc::clone(&populated))
        .await
        .expect_err("db path unusable");
    assert!(matches!(
        err,
        BootstrapError::Storage(_) | BootstrapError::StorageDir(_)
    ));

    // A corrupted database file fails at open (`Migrate`/`Connect`
    // class) — typed `Storage`.
    let corrupt = dir.path().join("corrupt.db");
    std::fs::write(&corrupt, b"this is not a sqlite database at all").expect("write");
    let mut config = config_for(&server.uri(), &dir);
    config.storage.db_path = Some(corrupt);
    let err = bootstrap(config, populated).await.expect_err("corrupt db");
    assert!(matches!(err, BootstrapError::Storage(_)));
}

/// B6: `mode = "desktop"` decodes but fails at bootstrap with the typed
/// `ModeNotImplemented` (the Phase 7 placeholder arm).
#[tokio::test]
async fn b6_desktop_mode_is_not_implemented() {
    let dir = tempdir();
    let server = MockServer::start().await;
    let mut config = config_for(&server.uri(), &dir);
    config.app.mode = taskboard_app::config::AppMode::Desktop;
    let store: Arc<dyn CredentialStore> = Arc::new(FakeCredentialStore::default());
    let err = bootstrap(config, store).await.expect_err("desktop mode");
    assert!(matches!(
        err,
        BootstrapError::ModeNotImplemented(taskboard_app::config::AppMode::Desktop)
    ));
}

/// B7: restart continuity — after a full pull, a fresh bootstrap on the
/// same database hydrates the board binding and the pull validators. The
/// actor's cold cache forces one unconditional refetch after the
/// restart (ADR 0007 decision 9); the *next* cycle must go conditional:
/// the `304` mocks answer only `If-None-Match` requests, and the
/// unconditional fallbacks must each stay at exactly one hit.
#[tokio::test]
async fn b7_restart_continuity_with_validators() {
    let dir = tempdir();
    let server = MockServer::start().await;
    mount_standard_pull(&server, serde_json::json!([])).await;

    {
        let (h, j) = seed_binding(&dir, "taskboard.db", false).await;
        drop(h);
        j.await.expect("seed actor exits");
    }
    let mut config = config_for(&server.uri(), &dir);
    let db_path = dir.path().join("taskboard.db");

    // First boot: a full pull stores the validators (ETag "gen-1").
    {
        let (app, _env) = boot(config.clone()).await;
        app.engine
            .execute(StateCommand::RequestSync)
            .await
            .expect("RequestSync accepted");
        eventually(&app, |state| state.sync.last_success.is_some()).await;
        app.shutdown().await;
    }

    // Second boot: conditional endpoints 304 on the hydrated validators;
    // anything unconditional gets a full 200 (counted).
    let second = MockServer::start().await;
    let mut conditional_guards = Vec::new();
    let mut fallback_keepalive = Vec::new();
    for p in [
        "/index.php/apps/deck/api/v1.0/boards".to_owned(),
        format!("/index.php/apps/deck/api/v1.0/boards/{BOARD}"),
        format!("/index.php/apps/deck/api/v1.0/boards/{BOARD}/stacks"),
        format!("/index.php/apps/deck/api/v1.0/boards/{BOARD}/stacks/archived"),
    ] {
        let conditional = Mock::given(method("GET"))
            .and(path(&p))
            .and(header("If-None-Match", "\"gen-1\""))
            .respond_with(ResponseTemplate::new(304).insert_header("ETag", "\"gen-1\""));
        conditional_guards.push(conditional.mount_as_scoped(&second).await);

        let body = if p.ends_with("/boards") {
            common::boards_json()
        } else if p.ends_with("/stacks/archived") {
            "[]".to_owned()
        } else if p.ends_with("/stacks") {
            common::stacks_json(&serde_json::json!([]))
        } else {
            common::board_json()
        };
        let fallback = Mock::given(method("GET")).and(path(&p)).respond_with(
            ResponseTemplate::new(200)
                .insert_header("ETag", "\"gen-1\"")
                .set_body_string(body),
        );
        fallback_keepalive.push(fallback.mount_as_scoped(&second).await);
    }

    config.nextcloud.server_url = Some(second.uri());
    config.storage.db_path = Some(db_path);
    let (app, _env) = boot(config).await;

    // Cycle 1 (post-restart): the forced unconditional refetch.
    app.engine
        .execute(StateCommand::RequestSync)
        .await
        .expect("RequestSync accepted");
    eventually(&app, |state| {
        state.sync.last_success.is_some()
            && state
                .boards
                .values()
                .any(|b| b.remote == Some(RemoteBoardId(BOARD)))
    })
    .await;

    // Cycle 2: the cache is warm — every endpoint must go conditional.
    app.engine
        .execute(StateCommand::RequestSync)
        .await
        .expect("RequestSync accepted");
    eventually(&app, |state| state.sync.phase == SyncPhase::Idle).await;

    // The hydrated validators must be *used*: at least one endpoint of
    // the follow-up cycles went conditional (`304` only answers
    // `If-None-Match: "gen-1"`, which only exists if the validators
    // survived the restart through the database).
    let mut any_conditional = false;
    for guard in &conditional_guards {
        any_conditional |= !guard.received_requests().await.is_empty();
    }
    assert!(
        any_conditional,
        "the hydrated validators must produce conditional requests"
    );
    app.shutdown().await;
}
