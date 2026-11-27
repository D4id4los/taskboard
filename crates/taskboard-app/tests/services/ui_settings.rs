// SPDX-License-Identifier: AGPL-3.0-only

//! The settings request path (round-1 chunk 3, plan §3 A6–A9): the
//! Save/Remove/Test/SetScale choreographies driven through the real
//! request lane and forwarder over a booted standalone core, wiremock
//! and fake-store hermetic. Assertions are typed outcomes and
//! structural facts; the secret-redaction proofs are substrings of
//! *secrets* (never of any UI/error text).

use std::path::PathBuf;
use std::sync::Arc;

use arc_swap::ArcSwap;
use taskboard_app::bootstrap::{self, App};
use taskboard_app::config::AppConfig;
use taskboard_app::notifications::LogSink;
use taskboard_app::secrets::{AppPassword, CredentialStore};
use taskboard_app::ui_requests::{ConnectionWorkspace, UI_REQUEST_DEBOUNCE, ui_request_channel};
use taskboard_ui_slint::keymap::Preset;
use taskboard_ui_slint::requests::{ConnectionProfile, Secret, SettingsError, UiRequest};

use crate::common;

use common::FakeCredentialStore;
use taskboard_ui_slint::actions::{ActionId, ContextId};
use taskboard_ui_slint::keymap::stroke;

const BOARDS_PATH: &str = "/index.php/apps/deck/api/v1.0/boards";

async fn mount_boards_ok() -> wiremock::MockServer {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(BOARDS_PATH))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_string(common::boards_json()))
        .mount(&server)
        .await;
    server
}

async fn mount_auth_reject(status: u16) -> wiremock::MockServer {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(BOARDS_PATH))
        .respond_with(wiremock::ResponseTemplate::new(status))
        .mount(&server)
        .await;
    server
}

/// Seeds an array-world config naming one connector at `server`.
fn array_config_text(dir: &std::path::Path, server: &str) -> String {
    format!(
        "[storage]\ndb_path = '{}/taskboard.db'\n\
         [[sync.connectors]]\nname = \"work\"\nbackend = \"nextcloud\"\n\
         server_url = \"{server}\"\nusername = \"old-user\"\n",
        dir.display()
    )
}

fn seed_array_config(dir: &std::path::Path, server: &str) -> PathBuf {
    let path = dir.join("taskboard.toml");
    std::fs::write(&path, array_config_text(dir, server)).expect("seed config");
    path
}

/// A booted standalone core plus the workspace over it: the forwarder's
/// real dependencies, no mocks (fakes over mocks).
async fn booted_workspace(
    config_write_path: PathBuf,
    store: Arc<dyn CredentialStore>,
) -> (App, ConnectionWorkspace) {
    let mut config = AppConfig::default();
    let dir = tempfile::tempdir().expect("tempdir");
    config.storage.db_path = Some(dir.path().join("taskboard.db"));
    config.sync.backend = Some(taskboard_app::config::SyncBackend::Standalone);
    config.validate().expect("config valid");
    let app = bootstrap::bootstrap_in(
        config,
        Arc::new(FakeCredentialStore::default()),
        Arc::new(LogSink),
        taskboard_app::bootstrap::BootIdentity::At(dir.path().to_owned()),
    )
    .await
    .expect("standalone boot");
    let workspace = ConnectionWorkspace {
        config_write_path,
        store,
        control: app.connector_control(),
        engine: app.engine.clone(),
        // The suite's own identity home: the ceremony's create arm
        // mints here, never the platform default (#751). `keep` — the
        // ceremony mints after this function returns.
        device_json: Some(
            tempfile::tempdir()
                .expect("tempdir")
                .keep()
                .join("device.json"),
        ),
        connector_owners: app.connector_owners.clone(),
        export: taskboard_app::ui_requests::ExportWorkspace::default(),
        status: crate::ui_settings::in_session_status_source(&app),
        remediation: taskboard_app::status_remediation::RemediationWorkspace::default(),
    };
    (app, workspace)
}

/// A detached-receiver variant of `send`: the request goes out and the
/// reply half is dropped before the handler answers (A7).
/// The shared in-session status source for this binary's workspace
/// fixtures: the plain value requirement over the booted app's
/// handles (the suite never drives the status panel). One
/// projection, two fronts — same rule as production's spawn sites.
pub(super) fn in_session_status_source(
    app: &App,
) -> Box<dyn Fn() -> taskboard_app::commands::status::StatusView + Send + Sync> {
    let engine = app.engine.clone();
    let config = app.config.clone();
    let control = app.connector_control();
    let files = app.files_sync_status.clone();
    Box::new(move || {
        taskboard_app::commands::status::StatusView::in_session_parts(
            &engine,
            &config,
            control.connector_attached(),
            files.as_ref(),
        )
    })
}

fn send_detached_reply(
    tx: &tokio::sync::mpsc::Sender<UiRequest>,
    request: impl FnOnce(
        tokio::sync::oneshot::Sender<
            Result<taskboard_ui_slint::requests::ConnectionsSnapshot, SettingsError>,
        >,
    ) -> UiRequest,
) {
    let (reply, _rx) = tokio::sync::oneshot::channel();
    tx.try_send(request(reply)).expect("capacity holds");
}

/// A6 — the full Save ceremony: verify ok → file gains the entry → the
/// store holds the secret → the reply carries the fresh list; the
/// written file and the request `Debug` never carry the secret; a
/// second Save with the same name is an edit.
#[tokio::test]
async fn the_save_ceremony_persists_stores_and_attaches_without_leaking_the_secret() {
    let _env = crate::common::taskboard_env_lock();
    let server = mount_boards_ok().await;
    let dir = tempfile::tempdir().expect("tempdir");
    let config = seed_array_config(dir.path(), &server.uri());
    let store = Arc::new(FakeCredentialStore::default());
    let (app, workspace) = booted_workspace(config.clone(), store.clone()).await;
    let (tx, rx) = ui_request_channel();
    let forwarder = taskboard_app::ui_requests::spawn_ui_request_forwarder(
        rx,
        Arc::new(ArcSwap::from_pointee(taskboard_domain::AppState::default())),
        workspace,
    );

    let profile = |name: &str| ConnectionProfile {
        name: name.to_owned(),
        server_url: server.uri(),
        username: "alice".into(),
        password: Secret("hunter2-secret".into()),
    };
    let send_save = |tx: &tokio::sync::mpsc::Sender<UiRequest>, name: &str| {
        let (reply, reply_rx) = tokio::sync::oneshot::channel();
        tx.try_send(UiRequest::SaveConnection {
            profile: profile(name),
            reply,
        })
        .expect("capacity holds");
        reply_rx
    };

    // First save: an append (the seeded entry is named `work`, saving
    // `home`).
    let outcome = send_save(&tx, "home")
        .await
        .expect("the reply lane is alive")
        .expect("the save succeeded");
    // The r2 D32 reply carries the full pair; the connector half is
    // the widened form of the old bare list.
    assert_eq!(
        outcome.connectors.len(),
        2,
        "the fresh list carries both entries"
    );
    assert!(outcome.connectors.iter().any(|row| row.name == "home"));
    assert!(
        outcome
            .connectors
            .iter()
            .all(|row| !row.server_url.is_empty()),
        "rows are the secret-free shape"
    );

    // The written file: entry present, secret absent, store holds it.
    let written = std::fs::read_to_string(&config).expect("read back");
    assert!(written.contains("name = \"home\""));
    assert!(
        !written.contains("hunter2-secret"),
        "no secret in the config"
    );
    assert_eq!(
        store.load(&server.uri(), "alice").expect("stored").expose(),
        "hunter2-secret"
    );

    // The attach half: the connector is live under its name.
    assert!(
        app.connector_control()
            .connector_commands(&taskboard_domain::ConnectorId::from("home"))
            .is_some(),
        "the draft connector was attached live"
    );

    // A second save with the same name is an edit (upsert, not a
    // duplicate) — and its Debug never carries the secret.
    let (reply, reply_rx) = tokio::sync::oneshot::channel();
    let request = UiRequest::SaveConnection {
        profile: profile("home"),
        reply,
    };
    assert!(!format!("{request:?}").contains("hunter2-secret"));
    tx.try_send(request).expect("capacity holds");
    let outcome = reply_rx
        .await
        .expect("the reply lane is alive")
        .expect("the edit succeeded");
    assert_eq!(outcome.connectors.len(), 2, "still two entries — an upsert");

    drop(tx);
    forwarder.await.expect("forwarder joins cleanly");
}

/// A6's failure half — a 401 verify persists nothing and replies typed.
#[tokio::test]
async fn a_failing_verify_persists_nothing_and_replies_typed() {
    let _env = crate::common::taskboard_env_lock();
    let server = mount_auth_reject(401).await;
    let dir = tempfile::tempdir().expect("tempdir");
    let config = seed_array_config(dir.path(), "https://unused.example");
    let store = Arc::new(FakeCredentialStore::default());
    let (_app, workspace) = booted_workspace(config.clone(), store.clone()).await;
    let (tx, rx) = ui_request_channel();
    let forwarder = taskboard_app::ui_requests::spawn_ui_request_forwarder(
        rx,
        Arc::new(ArcSwap::from_pointee(taskboard_domain::AppState::default())),
        workspace,
    );

    let (reply, reply_rx) = tokio::sync::oneshot::channel();
    tx.try_send(UiRequest::SaveConnection {
        profile: ConnectionProfile {
            name: "home".into(),
            server_url: server.uri(),
            username: "alice".into(),
            password: Secret("hunter2-secret".into()),
        },
        reply,
    })
    .expect("capacity holds");
    let outcome = reply_rx.await.expect("reply lane alive").expect_err("401");
    assert!(
        matches!(outcome, SettingsError::VerifyRejected),
        "the credential refusal is typed, got {outcome:?}"
    );

    let written = std::fs::read_to_string(&config).expect("read back");
    assert_eq!(
        written,
        array_config_text(dir.path(), "https://unused.example"),
        "a rejected verify persists nothing"
    );
    assert!(
        store.load(&server.uri(), "alice").is_err(),
        "no secret stored"
    );

    drop(tx);
    forwarder.await.expect("forwarder joins cleanly");
}

/// A7 — a dropped reply receiver is survived: no panic, the file write
/// still landed, the forwarder stays alive.
#[tokio::test]
async fn a_dropped_reply_receiver_never_wedges_the_forwarder() {
    let _env = crate::common::taskboard_env_lock();
    let dir = tempfile::tempdir().expect("tempdir");
    let config = seed_array_config(dir.path(), "https://unused.example");
    let (_app, workspace) =
        booted_workspace(config.clone(), Arc::new(FakeCredentialStore::default())).await;
    let (tx, rx) = ui_request_channel();
    let forwarder = taskboard_app::ui_requests::spawn_ui_request_forwarder(
        rx,
        Arc::new(ArcSwap::from_pointee(taskboard_domain::AppState::default())),
        workspace,
    );

    // A failing save (unreachable server) with the receiver dropped:
    // the handler logs, replies into the void, and lives on.
    send_detached_reply(&tx, |reply| UiRequest::SaveConnection {
        profile: ConnectionProfile {
            name: "home".into(),
            server_url: "http://127.0.0.1:9".into(),
            username: "alice".into(),
            password: Secret("x".into()),
        },
        reply,
    });

    // The lane still works: a real SetScale lands.
    let (reply, reply_rx) = tokio::sync::oneshot::channel();
    tx.try_send(UiRequest::SetScale { scale: 1.5, reply })
        .expect("capacity holds");
    reply_rx
        .await
        .expect("reply lane alive")
        .expect("the scale persisted");
    let written = std::fs::read_to_string(&config).expect("read back");
    assert!(written.contains("scale = 1.5"));

    drop(tx);
    forwarder.await.expect("forwarder joins cleanly");
}

/// M9 (A9's replacement, round-3 D21) — the legacy scalar world
/// migrates ahead of the Save ceremony: typed Ok, the file is
/// array-world (identity keys gone, `.bak-legacy` holds the
/// pre-image), the store holds the new secret.
#[tokio::test]
async fn a_legacy_file_migrates_ahead_of_the_save_ceremony() {
    let _env = crate::common::taskboard_env_lock();
    let server = mount_boards_ok().await;
    let dir = tempfile::tempdir().expect("tempdir");
    let config = dir.path().join("taskboard.toml");
    let before = format!(
        "[storage]\ndb_path = '{}/taskboard.db'\n[nextcloud]\n\
         server_url = \"https://legacy.example\"\nusername = \"old-user\"\n",
        dir.path().display()
    );
    std::fs::write(&config, &before).expect("seed config");
    let store = Arc::new(FakeCredentialStore::default());
    let (_app, workspace) = booted_workspace(config.clone(), store.clone()).await;
    let (tx, rx) = ui_request_channel();
    let forwarder = taskboard_app::ui_requests::spawn_ui_request_forwarder(
        rx,
        Arc::new(ArcSwap::from_pointee(taskboard_domain::AppState::default())),
        workspace,
    );

    let (reply, reply_rx) = tokio::sync::oneshot::channel();
    tx.try_send(UiRequest::SaveConnection {
        profile: ConnectionProfile {
            name: "home".into(),
            server_url: server.uri(),
            username: "alice".into(),
            password: Secret("hunter2-secret".into()),
        },
        reply,
    })
    .expect("capacity holds");
    let outcome = reply_rx
        .await
        .expect("the reply lane is alive")
        .expect("the save succeeded after the migration");
    assert!(
        outcome.connectors.iter().any(|row| row.name == "home"),
        "the reply carries the fresh list"
    );

    let written = std::fs::read_to_string(&config).expect("read back");
    assert!(written.contains("[[sync.connectors]]"), "array world");
    assert!(
        !written.contains("[nextcloud]"),
        "the scalar table is gone (the identity moved into the entry)"
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("taskboard.toml.bak-legacy"))
            .expect("the backup exists"),
        before,
        "the backup is the exact pre-image"
    );
    assert_eq!(
        store.load(&server.uri(), "alice").expect("stored").expose(),
        "hunter2-secret"
    );

    drop(tx);
    forwarder.await.expect("forwarder joins cleanly");
}

/// M9's persist-failure twin — an unwritable config replies typed
/// `PersistFailed` and the file stays byte-untouched.
#[cfg(unix)]
#[tokio::test]
async fn a_legacy_file_on_unwritable_media_refuses_persist_failed() {
    use std::os::unix::fs::PermissionsExt as _;

    let _env = crate::common::taskboard_env_lock();
    let server = mount_boards_ok().await;
    let dir = tempfile::tempdir().expect("tempdir");
    let config = dir.path().join("taskboard.toml");
    let before = "[nextcloud]\nserver_url = \"https://legacy.example\"\nusername = \"old-user\"\n";
    std::fs::write(&config, before).expect("seed config");
    let store = Arc::new(FakeCredentialStore::default());
    let (_app, workspace) = booted_workspace(config.clone(), store.clone()).await;
    let (tx, rx) = ui_request_channel();
    let forwarder = taskboard_app::ui_requests::spawn_ui_request_forwarder(
        rx,
        Arc::new(ArcSwap::from_pointee(taskboard_domain::AppState::default())),
        workspace,
    );

    let mut perms = std::fs::metadata(dir.path()).expect("stat").permissions();
    perms.set_mode(0o500);
    std::fs::set_permissions(dir.path(), perms).expect("chmod");

    let (reply, reply_rx) = tokio::sync::oneshot::channel();
    tx.try_send(UiRequest::SaveConnection {
        profile: ConnectionProfile {
            name: "home".into(),
            server_url: server.uri(),
            username: "alice".into(),
            password: Secret("hunter2-secret".into()),
        },
        reply,
    })
    .expect("capacity holds");
    let outcome = reply_rx
        .await
        .expect("the reply lane is alive")
        .expect_err("the persist is refused typed");
    assert!(
        matches!(outcome, SettingsError::PersistFailed),
        "the typed refusal, never silent"
    );

    let mut perms = std::fs::metadata(dir.path()).expect("stat").permissions();
    perms.set_mode(0o700);
    std::fs::set_permissions(dir.path(), perms).expect("chmod back");
    assert_eq!(
        std::fs::read_to_string(&config).expect("read"),
        before,
        "the file is byte-untouched"
    );

    drop(tx);
    forwarder.await.expect("forwarder joins cleanly");
}

/// T1 (round-6 chunk 2, plan §3; #721's exact repro) — a save onto a
/// config carrying neither `[sync]` nor `[nextcloud]` proceeds: the
/// writer creates the `[[sync.connectors]]` subtree, the store holds
/// the secret, the connector is attached live.
#[tokio::test]
async fn a_save_on_a_config_without_a_sync_section_creates_the_subtree_and_attaches() {
    let _env = crate::common::taskboard_env_lock();
    let server = mount_boards_ok().await;
    let dir = tempfile::tempdir().expect("tempdir");
    let config = dir.path().join("taskboard.toml");
    std::fs::write(
        &config,
        format!(
            "[storage]\ndb_path = '{}/taskboard.db'\n[ui]\nscale = 1.5\n",
            dir.path().display()
        ),
    )
    .expect("seed config");
    let store = Arc::new(FakeCredentialStore::default());
    let (app, workspace) = booted_workspace(config.clone(), store.clone()).await;
    let (tx, rx) = ui_request_channel();
    let forwarder = taskboard_app::ui_requests::spawn_ui_request_forwarder(
        rx,
        Arc::new(ArcSwap::from_pointee(taskboard_domain::AppState::default())),
        workspace,
    );

    let (reply, reply_rx) = tokio::sync::oneshot::channel();
    tx.try_send(UiRequest::SaveConnection {
        profile: ConnectionProfile {
            name: "home".into(),
            server_url: server.uri(),
            username: "alice".into(),
            password: Secret("hunter2-secret".into()),
        },
        reply,
    })
    .expect("capacity holds");
    let outcome = reply_rx
        .await
        .expect("the reply lane is alive")
        .expect("the sync-less config is a write target");
    assert!(outcome.connectors.iter().any(|row| row.name == "home"));

    let written = std::fs::read_to_string(&config).expect("read back");
    assert!(written.contains("[[sync.connectors]]"), "subtree created");
    assert!(written.contains("name = \"home\""));
    assert!(written.contains("scale = 1.5"), "unrelated tables survive");
    assert_eq!(
        store.load(&server.uri(), "alice").expect("stored").expose(),
        "hunter2-secret"
    );
    assert!(
        app.connector_control()
            .connector_commands(&taskboard_domain::ConnectorId::from("home"))
            .is_some(),
        "the draft connector was attached live"
    );

    drop(tx);
    forwarder.await.expect("forwarder joins cleanly");
}

/// T2 — the absent-file variant of the #721 repro: the save creates
/// the file, its directory, and the entry end to end.
#[tokio::test]
async fn a_save_with_no_config_file_at_all_creates_file_directory_and_entry() {
    let _env = crate::common::taskboard_env_lock();
    let server = mount_boards_ok().await;
    let dir = tempfile::tempdir().expect("tempdir");
    let config = dir.path().join("conf").join("taskboard.toml");
    let store = Arc::new(FakeCredentialStore::default());
    let (_app, workspace) = booted_workspace(config.clone(), store.clone()).await;
    let (tx, rx) = ui_request_channel();
    let forwarder = taskboard_app::ui_requests::spawn_ui_request_forwarder(
        rx,
        Arc::new(ArcSwap::from_pointee(taskboard_domain::AppState::default())),
        workspace,
    );

    let (reply, reply_rx) = tokio::sync::oneshot::channel();
    tx.try_send(UiRequest::SaveConnection {
        profile: ConnectionProfile {
            name: "home".into(),
            server_url: server.uri(),
            username: "alice".into(),
            password: Secret("hunter2-secret".into()),
        },
        reply,
    })
    .expect("capacity holds");
    reply_rx
        .await
        .expect("the reply lane is alive")
        .expect("the absent config file is a write target");

    let written = std::fs::read_to_string(&config).expect("the file was created");
    assert!(written.contains("[[sync.connectors]]"));
    assert!(written.contains("name = \"home\""));

    drop(tx);
    forwarder.await.expect("forwarder joins cleanly");
}

/// T3 — the corner that stands: an env-injected `[nextcloud]` identity
/// beside a sync-less config is genuinely not a write target — the
/// save replies typed `PersistRejected` and the file stays
/// byte-untouched. Both husk shapes are pinned: the full env identity
/// and a *partial* legacy husk (one identity key alone — the `||` leg
/// of the typed predicate).
#[tokio::test]
async fn an_env_injected_identity_still_refuses_persist_rejected() {
    async fn save_refuses(config: &std::path::Path, before: &str, why: &str) {
        let (_app, workspace) = booted_workspace(
            config.to_path_buf(),
            Arc::new(FakeCredentialStore::default()),
        )
        .await;
        let (tx, rx) = ui_request_channel();
        let forwarder = taskboard_app::ui_requests::spawn_ui_request_forwarder(
            rx,
            Arc::new(ArcSwap::from_pointee(taskboard_domain::AppState::default())),
            workspace,
        );

        let (reply, reply_rx) = tokio::sync::oneshot::channel();
        tx.try_send(UiRequest::SaveConnection {
            profile: ConnectionProfile {
                name: "home".into(),
                server_url: "https://fresh.example".into(),
                username: "alice".into(),
                password: Secret("hunter2-secret".into()),
            },
            reply,
        })
        .expect("capacity holds");
        let outcome = reply_rx
            .await
            .expect("the reply lane is alive")
            .expect_err("the env identity is not a write target");
        assert!(
            matches!(outcome, SettingsError::PersistRejected),
            "the husk corner stands typed ({why}), got {outcome:?}"
        );
        assert_eq!(
            std::fs::read_to_string(config).expect("read"),
            before,
            "the file is byte-untouched ({why})"
        );

        drop(tx);
        forwarder.await.expect("forwarder joins cleanly");
    }

    let _env = common::taskboard_env_lock();
    let server = common::EnvOverride::set("TASKBOARD_NEXTCLOUD__SERVER_URL", "https://env.example");
    let user = common::EnvOverride::set("TASKBOARD_NEXTCLOUD__USERNAME", "env-user");
    let dir = tempfile::tempdir().expect("tempdir");
    let config = dir.path().join("taskboard.toml");
    let before = format!(
        "[storage]\ndb_path = '{}/taskboard.db'\n[ui]\nscale = 1.5\n",
        dir.path().display()
    );
    std::fs::write(&config, &before).expect("seed config");

    save_refuses(&config, &before, "the full env identity").await;
    // The partial husk: one identity key alone is still the scalar
    // world — the predicate's other leg refuses the same way.
    drop(user);
    save_refuses(&config, &before, "the server_url-only husk").await;

    drop(server);
}

/// M10 — the Remove choreography on a legacy file migrates first,
/// then removes the sole entry; the no-empty-husk rule leaves a clean
/// array-less file.
#[tokio::test]
async fn remove_on_a_legacy_file_migrates_then_removes_the_sole_entry() {
    let _env = crate::common::taskboard_env_lock();
    let dir = tempfile::tempdir().expect("tempdir");
    let config = dir.path().join("taskboard.toml");
    std::fs::write(
        &config,
        "[nextcloud]\nserver_url = \"https://legacy.example\"\nusername = \"old-user\"\n",
    )
    .expect("seed config");
    let store = Arc::new(FakeCredentialStore::default());
    let (_app, workspace) = booted_workspace(config.clone(), store.clone()).await;
    let (tx, rx) = ui_request_channel();
    let forwarder = taskboard_app::ui_requests::spawn_ui_request_forwarder(
        rx,
        Arc::new(ArcSwap::from_pointee(taskboard_domain::AppState::default())),
        workspace,
    );

    let (reply, reply_rx) = tokio::sync::oneshot::channel();
    tx.try_send(UiRequest::RemoveConnection {
        name: "nextcloud".into(),
        reply,
    })
    .expect("capacity holds");
    let outcome = reply_rx
        .await
        .expect("the reply lane is alive")
        .expect("the remove succeeded after the migration");
    assert!(
        outcome.connectors.is_empty(),
        "the sole entry is gone from the fresh list"
    );

    let written = std::fs::read_to_string(&config).expect("read back");
    assert!(
        !written.contains("sync.connectors"),
        "the no-empty-husk rule removed the array with its last entry"
    );
    assert!(
        !written.contains("server_url"),
        "the scalar identity was migrated away, then removed"
    );
    assert!(
        dir.path().join("taskboard.toml.bak-legacy").is_file(),
        "the migration's backup exists"
    );

    drop(tx);
    forwarder.await.expect("forwarder joins cleanly");
}

/// A state carrying one live board + stack (the width prune keeps only
/// live entities — the fixture makes the flush observable).
fn fixture_state() -> (
    taskboard_domain::AppState,
    taskboard_domain::BoardId,
    taskboard_domain::StackId,
) {
    use taskboard_domain::{Board, BoardClocks, Stack, StackClocks};
    let board_id = taskboard_domain::BoardId::from_uuid(uuid::Uuid::from_u128(0x1001));
    let stack_id = taskboard_domain::StackId::from_uuid(uuid::Uuid::from_u128(0x2001));
    let mut state = taskboard_domain::AppState::default();
    let epoch = BoardClocks::epoch();
    state.boards.insert(
        board_id,
        Board {
            id: board_id,
            remote: None,
            title: "board".into(),
            color: taskboard_domain::Color::new(""),
            archived: false,
            deleted: false,
            clocks: BoardClocks {
                title: epoch,
                color: epoch,
                archived: epoch,
                deleted: epoch,
            },
            authors: taskboard_domain::entities::BoardAuthors::default(),
            remote_seen: None,
        },
    );
    state.stacks.insert(
        stack_id,
        Stack {
            id: stack_id,
            remote: None,
            board: board_id,
            title: "stack".into(),
            order: 0,
            archived: false,
            deleted: false,
            clocks: StackClocks {
                title: epoch,
                order: epoch,
                deleted: epoch,
            },
            authors: taskboard_domain::entities::StackAuthors::default(),
            remote_seen: None,
        },
    );
    (state, board_id, stack_id)
}

/// A8 — a settings request inside a width debounce window executes
/// immediately; the widths flush lands once, merged, at window end
/// (the chunk-2 debounce semantics byte-for-byte).
#[tokio::test]
async fn a_settings_request_inside_a_quiet_window_is_never_debounced() {
    let _env = crate::common::taskboard_env_lock();
    let dir = tempfile::tempdir().expect("tempdir");
    let config = dir.path().join("taskboard.toml");
    std::fs::write(&config, "[ui]\nscale = 1.0\n").expect("seed");
    let (_app, workspace) =
        booted_workspace(config.clone(), Arc::new(FakeCredentialStore::default())).await;
    let (state, board, stack) = fixture_state();
    let (tx, rx) = ui_request_channel();
    let forwarder = taskboard_app::ui_requests::spawn_ui_request_forwarder(
        rx,
        Arc::new(ArcSwap::from_pointee(state)),
        workspace,
    );

    // A width snapshot opens the quiet window.
    tx.try_send(UiRequest::StackWidthsChanged {
        board,
        widths: vec![(stack, 240)],
    })
    .expect("capacity holds");

    // Inside the window: the scale request is executed on the spot.
    let (reply, reply_rx) = tokio::sync::oneshot::channel();
    tx.try_send(UiRequest::SetScale { scale: 2.0, reply })
        .expect("capacity holds");
    reply_rx
        .await
        .expect("reply lane alive")
        .expect("the scale persisted immediately");
    let written = std::fs::read_to_string(&config).expect("read back");
    assert!(written.contains("scale = 2.0"));
    assert!(
        !written.contains(&stack.as_uuid().to_string()),
        "the widths have not flushed yet"
    );

    // The window elapses: the widths flush lands once.
    let (reply, reply_rx) = tokio::sync::oneshot::channel();
    tx.try_send(UiRequest::SetScale { scale: 2.0, reply })
        .expect("capacity holds");
    let _ = reply_rx.await;
    tokio::time::pause();
    tokio::time::advance(UI_REQUEST_DEBOUNCE + std::time::Duration::from_millis(1)).await;
    for _ in 0..8 {
        tokio::task::yield_now().await;
    }
    let written = std::fs::read_to_string(&config).expect("read back");
    assert!(
        written.contains(&stack.as_uuid().to_string()),
        "the widths flushed"
    );

    drop(tx);
    forwarder.await.expect("forwarder joins cleanly");
}

/// The `SetTheme` flow (#1437 D5): the typed choice persists the
/// `[ui] theme` word immediately through the same writeback lane —
/// and the request carries no unvalidated payload (the closed-set
/// `ThemeName` is minted UI-side), so the reply cannot be a typed
/// refusal: the persist is the whole lane.
#[tokio::test]
async fn the_theme_flow_persists_the_word_immediately() {
    let _env = crate::common::taskboard_env_lock();
    let dir = tempfile::tempdir().expect("tempdir");
    let config = dir.path().join("taskboard.toml");
    std::fs::write(&config, "[ui]\nscale = 1.0\n").expect("seed");
    let (_app, workspace) =
        booted_workspace(config.clone(), Arc::new(FakeCredentialStore::default())).await;
    let (tx, rx) = ui_request_channel();
    let forwarder = taskboard_app::ui_requests::spawn_ui_request_forwarder(
        rx,
        Arc::new(ArcSwap::from_pointee(taskboard_domain::AppState::default())),
        workspace,
    );

    let (reply, reply_rx) = tokio::sync::oneshot::channel();
    tx.try_send(UiRequest::SetTheme {
        theme: taskboard_ui_slint::palette::ThemeName::LIGHT,
        reply,
    })
    .expect("capacity holds");
    reply_rx
        .await
        .expect("reply lane alive")
        .expect("the theme persisted immediately");
    let written = std::fs::read_to_string(&config).expect("read back");
    assert!(written.contains("theme = \"light\""), "the word lands");
    assert!(written.contains("scale = 1.0"), "the sibling key survives");

    drop(tx);
    forwarder.await.expect("forwarder joins cleanly");
}

/// The `SetKeymapPreset` flow: the preset persists, the reply carries the
/// rebuilt keymap (vi resolves the colon in the vi-normal context).
#[tokio::test]
async fn the_preset_flow_persists_and_replies_with_the_rebuilt_keymap() {
    let _env = crate::common::taskboard_env_lock();
    let dir = tempfile::tempdir().expect("tempdir");
    let config = dir.path().join("taskboard.toml");
    std::fs::write(
        &config,
        "[ui]\n[ui.keymap]\npreset = \"standard\"\n[ui.keymap.overrides]\n\
         \"task.complete\" = [\"Ctrl+!\"]\n",
    )
    .expect("seed config");
    let (_app, workspace) =
        booted_workspace(config.clone(), Arc::new(FakeCredentialStore::default())).await;
    let (tx, rx) = ui_request_channel();
    let forwarder = taskboard_app::ui_requests::spawn_ui_request_forwarder(
        rx,
        Arc::new(ArcSwap::from_pointee(taskboard_domain::AppState::default())),
        workspace,
    );

    let (reply, reply_rx) = tokio::sync::oneshot::channel();
    tx.try_send(UiRequest::SetKeymapPreset {
        preset: Preset::Vi,
        reply,
    })
    .expect("capacity holds");
    let update = reply_rx
        .await
        .expect("reply lane alive")
        .expect("the preset persisted");

    // The rebuilt keymap resolves vi's colon opener (the live-apply
    // fact), and the overrides survived the write.
    let colon = stroke::parse(":").expect("parses");
    assert_eq!(
        update.keymap.resolve(&[ContextId::ViNormal], &[colon]),
        Some(ActionId::AppMenu),
        "the reply carries the vi preset resolved"
    );
    let written = std::fs::read_to_string(&config).expect("read back");
    assert!(written.contains("preset = \"vi\""));
    assert!(written.contains("\"task.complete\""), "overrides survive");

    // A hostile preset word is refused typed (the config layer's
    // fail-fast contract) — exercised through SetScale's twin flow
    // instead: an out-of-range scale.
    let (reply, reply_rx) = tokio::sync::oneshot::channel();
    tx.try_send(UiRequest::SetScale { scale: 9.0, reply })
        .expect("capacity holds");
    let outcome = reply_rx
        .await
        .expect("reply lane alive")
        .expect_err("out of range");
    assert!(matches!(
        outcome,
        SettingsError::Invalid {
            field: taskboard_ui_slint::requests::Field::Scale,
            kind: taskboard_ui_slint::requests::InvalidKind::OutOfRange
        }
    ));

    drop(tx);
    forwarder.await.expect("forwarder joins cleanly");
}

/// The Remove choreography: the entry leaves the file, the live actor
/// detaches (idempotent), the stored secret dies, the reply carries the
/// fresh list.
#[tokio::test]
async fn the_remove_choreography_persists_detaches_and_deletes_the_secret() {
    let _env = crate::common::taskboard_env_lock();
    let server = mount_boards_ok().await;
    let dir = tempfile::tempdir().expect("tempdir");
    let config = seed_array_config(dir.path(), &server.uri());
    let store = Arc::new(FakeCredentialStore::default());
    store
        .store(
            "https://unused.example",
            "old-user",
            AppPassword::new("old-secret"),
        )
        .expect("seed secret");
    // The seeded identity's secret (remove deletes it best-effort).
    store
        .store(&server.uri(), "old-user", AppPassword::new("current"))
        .expect("seed secret");
    let (_app, workspace) = booted_workspace(config.clone(), store.clone()).await;
    let (tx, rx) = ui_request_channel();
    let forwarder = taskboard_app::ui_requests::spawn_ui_request_forwarder(
        rx,
        Arc::new(ArcSwap::from_pointee(taskboard_domain::AppState::default())),
        workspace,
    );

    let (reply, reply_rx) = tokio::sync::oneshot::channel();
    tx.try_send(UiRequest::RemoveConnection {
        name: "work".into(),
        reply,
    })
    .expect("capacity holds");
    let outcome = reply_rx
        .await
        .expect("reply lane alive")
        .expect("the remove succeeded");
    assert!(outcome.connectors.is_empty(), "the fresh list is empty");

    let written = std::fs::read_to_string(&config).expect("read back");
    assert!(!written.contains("connectors"), "no empty husk");
    assert!(
        store.load(&server.uri(), "old-user").is_err(),
        "the removed identity's secret died"
    );

    drop(tx);
    forwarder.await.expect("forwarder joins cleanly");
}

// -- r2 D32/D33: the board-scoped connection surface (attach/detach) --

use taskboard_domain::{Color, CommandOutcome, RemoteBoardId, StateCommand};

/// A boards listing carrying the given `(id, title)` live boards —
/// the listing fixture.
fn boards_json_with(entries: &[(u64, &str)]) -> String {
    let boards: Vec<serde_json::Value> = entries
        .iter()
        .map(|(id, title)| {
            serde_json::json!({
                "id": id, "title": title, "color": "00ff00",
                "lastModified": common::T, "deletedAt": 0,
                "archived": false, "labels": []
            })
        })
        .collect();
    serde_json::json!(boards).to_string()
}

async fn mount_boards(entries: &[(u64, &str)]) -> wiremock::MockServer {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(BOARDS_PATH))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_string(boards_json_with(entries)),
        )
        .mount(&server)
        .await;
    server
}

/// Mints a local board through the engine (the real command path —
/// the same route the attach handler's `BindBoard` rides).
async fn create_board(app: &App, title: &str) -> taskboard_domain::BoardId {
    let outcome = app
        .engine
        .execute(StateCommand::CreateBoard {
            title: title.to_owned(),
            color: Color::new("ff8800"),
        })
        .await
        .expect("engine alive");
    match outcome {
        CommandOutcome::CreatedBoard(id) => id,
        other => panic!("unexpected outcome: {other:?}"),
    }
}

/// Binds a board directly through the engine (config-blind — the
/// fixture for already-bound boards; the attach path must cope).
async fn bind_directly(app: &App, board: taskboard_domain::BoardId, connector: &str, remote: u64) {
    app.engine
        .execute(StateCommand::BindBoard {
            board,
            remote: RemoteBoardId::deck(remote),
            connector: taskboard_domain::ConnectorId::from(connector.to_owned()),
        })
        .await
        .expect("the fixture bind is accepted");
}

fn send_attach(
    tx: &tokio::sync::mpsc::Sender<UiRequest>,
    board: taskboard_domain::BoardId,
    connector: &str,
    remote: u64,
) -> tokio::sync::oneshot::Receiver<
    Result<taskboard_ui_slint::requests::ConnectionsSnapshot, SettingsError>,
> {
    let (reply, reply_rx) = tokio::sync::oneshot::channel();
    tx.try_send(UiRequest::AttachConnector {
        board,
        connector: connector.to_owned(),
        remote: RemoteBoardId::deck(remote),
        reply,
    })
    .expect("capacity holds");
    reply_rx
}

fn send_detach(
    tx: &tokio::sync::mpsc::Sender<UiRequest>,
    board: taskboard_domain::BoardId,
    connector: &str,
) -> tokio::sync::oneshot::Receiver<
    Result<taskboard_ui_slint::requests::ConnectionsSnapshot, SettingsError>,
> {
    let (reply, reply_rx) = tokio::sync::oneshot::channel();
    tx.try_send(UiRequest::DetachConnector {
        board,
        connector: connector.to_owned(),
        reply,
    })
    .expect("capacity holds");
    reply_rx
}

fn bound_connector_of(
    app: &App,
    board: taskboard_domain::BoardId,
) -> Option<(taskboard_domain::ConnectorId, RemoteBoardId)> {
    app.engine
        .shared_state()
        .load()
        .boards
        .get(&board)
        .and_then(|b| b.remote.as_ref())
        .map(|binding| (binding.connector.clone(), binding.board))
}

/// A2 — the attach happy path: the explicitly chosen remote binds
/// (round-5 chunk 8 — the local title plays no part), the actor is
/// attached, the config file is untouched (attach is a live-binding
/// op, never a config op).
#[tokio::test]
async fn attach_binds_the_chosen_remote_through_the_engine() {
    let _env = crate::common::taskboard_env_lock();
    // The full standard pull harness (listing + detail + stacks): the
    // post-bind settle must see a servable remote, not a half-mocked
    // one whose 404 detail reads as a remote tombstone.
    let server = wiremock::MockServer::start().await;
    common::mount_standard_pull(&server, serde_json::json!([])).await;
    let dir = tempfile::tempdir().expect("tempdir");
    let config = seed_array_config(dir.path(), &server.uri());
    let before = std::fs::read_to_string(&config).expect("read");
    let store = Arc::new(FakeCredentialStore::with(&server.uri(), "old-user", "pw"));
    let (app, workspace) = booted_workspace(config.clone(), store.clone()).await;
    let (tx, rx) = ui_request_channel();
    let forwarder = taskboard_app::ui_requests::spawn_ui_request_forwarder(
        rx,
        app.engine.shared_state(),
        workspace,
    );
    // A local title unrelated to the remote's: the bind is explicit.
    let board = create_board(&app, "unrelated local title").await;

    let outcome = send_attach(&tx, board, "work", common::BOARD)
        .await
        .expect("reply lane alive")
        .expect("attach succeeded");
    // The reply pair names the fresh binding.
    let row = outcome
        .boards
        .iter()
        .find(|b| b.id == board.as_uuid())
        .expect("the board row is in the reply");
    assert_eq!(row.bound_connector.as_deref(), Some("work"));
    assert!(outcome.connectors.iter().any(|c| c.name == "work"));

    // The engine state carries the binding (the command path ran).
    let (connector, remote) = bound_connector_of(&app, board).expect("bound");
    assert_eq!(connector.as_str(), "work");
    assert_eq!(remote, RemoteBoardId::deck(common::BOARD));

    // The connector actor is live under its name.
    assert!(
        app.connector_control()
            .connector_commands(&taskboard_domain::ConnectorId::from("work"))
            .is_some(),
        "the connector was attached"
    );

    // Attach never writes the config.
    assert_eq!(
        std::fs::read_to_string(&config).expect("read"),
        before,
        "the config file is byte-untouched"
    );

    drop(tx);
    forwarder.await.expect("forwarder joins cleanly");
}

/// A3 — a board bound through this connector to a *different* remote
/// refuses pre-IO: `BindRefused`, the binding intact, nothing attached
/// (the unreachable server proves no IO ran).
#[tokio::test]
async fn attach_to_another_remote_on_the_same_connector_refuses_before_any_io() {
    let _env = crate::common::taskboard_env_lock();
    let dir = tempfile::tempdir().expect("tempdir");
    let config = seed_array_config(dir.path(), "http://127.0.0.1:9");
    let store = Arc::new(FakeCredentialStore::with(
        "http://127.0.0.1:9",
        "old-user",
        "pw",
    ));
    let (app, workspace) = booted_workspace(config.clone(), store).await;
    let (tx, rx) = ui_request_channel();
    let forwarder = taskboard_app::ui_requests::spawn_ui_request_forwarder(
        rx,
        app.engine.shared_state(),
        workspace,
    );
    let board = create_board(&app, "home").await;
    bind_directly(&app, board, "work", 7).await;

    let outcome = send_attach(&tx, board, "work", 8)
        .await
        .expect("reply lane alive")
        .expect_err("bound to another remote");
    assert!(
        matches!(outcome, SettingsError::BindRefused),
        "typed bind refusal, got {outcome:?}"
    );
    assert_eq!(
        bound_connector_of(&app, board).map(|(_, remote)| remote),
        Some(RemoteBoardId::deck(7)),
        "the old binding survived"
    );
    assert!(
        app.connector_control()
            .connector_commands(&taskboard_domain::ConnectorId::from("work"))
            .is_none(),
        "nothing was attached"
    );

    drop(tx);
    forwarder.await.expect("forwarder joins cleanly");
}

/// A4 — a board already bound to a *different* connector refuses
/// pre-IO: `BindRefused`, and the unreachable target server proves no
/// listing was attempted.
#[tokio::test]
async fn attach_to_a_board_bound_elsewhere_refuses_before_any_io() {
    let _env = crate::common::taskboard_env_lock();
    // The target connector's server is unreachable: any listing would
    // fail VerifyUnreachable, not BindRefused — the typed refusal
    // proves the early arm fired.
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("taskboard.toml");
    std::fs::write(
        &path,
        "[storage]\ndb_path = '/tmp/t.db'\n\
         [[sync.connectors]]\nname = \"a\"\nbackend = \"nextcloud\"\n\
         server_url = \"https://a.example\"\nusername = \"ua\"\n\
         [[sync.connectors]]\nname = \"b\"\nbackend = \"nextcloud\"\n\
         server_url = \"http://127.0.0.1:9\"\nusername = \"old-user\"\n",
    )
    .expect("seed config");
    let store = Arc::new(FakeCredentialStore::with(
        "http://127.0.0.1:9",
        "old-user",
        "pw",
    ));
    let (app, workspace) = booted_workspace(path.clone(), store).await;
    let (tx, rx) = ui_request_channel();
    let forwarder = taskboard_app::ui_requests::spawn_ui_request_forwarder(
        rx,
        app.engine.shared_state(),
        workspace,
    );
    let board = create_board(&app, "home").await;
    bind_directly(&app, board, "a", 1).await;

    let outcome = send_attach(&tx, board, "b", 1)
        .await
        .expect("reply lane alive")
        .expect_err("bound elsewhere");
    assert!(
        matches!(outcome, SettingsError::BindRefused),
        "typed bind refusal, got {outcome:?}"
    );
    let (connector, remote) = bound_connector_of(&app, board).expect("binding intact");
    assert_eq!(connector.as_str(), "a");
    assert_eq!(remote, RemoteBoardId::deck(1), "the old binding survived");

    drop(tx);
    forwarder.await.expect("forwarder joins cleanly");
}

/// A5 — re-attaching a board to the `(connector, remote)` it is
/// already bound to is the idempotent re-issue: the binding survives,
/// `Ok`.
#[tokio::test]
async fn attach_reissue_on_the_same_remote_is_idempotent() {
    let _env = crate::common::taskboard_env_lock();
    let dir = tempfile::tempdir().expect("tempdir");
    let config = seed_array_config(dir.path(), "http://127.0.0.1:9");
    let store = Arc::new(FakeCredentialStore::with(
        "http://127.0.0.1:9",
        "old-user",
        "pw",
    ));
    let (app, workspace) = booted_workspace(config.clone(), store).await;
    let (tx, rx) = ui_request_channel();
    let forwarder = taskboard_app::ui_requests::spawn_ui_request_forwarder(
        rx,
        app.engine.shared_state(),
        workspace,
    );
    let board = create_board(&app, "home").await;
    bind_directly(&app, board, "work", 7).await;

    let outcome = send_attach(&tx, board, "work", 7)
        .await
        .expect("reply lane alive")
        .expect("idempotent reissue succeeds");
    let row = outcome
        .boards
        .iter()
        .find(|b| b.id == board.as_uuid())
        .expect("board row");
    assert_eq!(row.bound_connector.as_deref(), Some("work"));
    let (connector, remote) = bound_connector_of(&app, board).expect("bound");
    assert_eq!(connector.as_str(), "work");
    assert_eq!(remote, RemoteBoardId::deck(7), "the binding is unchanged");

    drop(tx);
    forwarder.await.expect("forwarder joins cleanly");
}

/// A6 — the attach refusal before any IO: a missing stored secret is
/// typed, nothing is attached or bound, the config is untouched. (The
/// credential verification moved to the listing, round-5 chunk 8 —
/// its typed refusals are pinned by the listing tests.)
#[tokio::test]
async fn attach_refuses_typed_on_a_missing_secret() {
    let _env = crate::common::taskboard_env_lock();
    let server = mount_boards(&[(7, "home")]).await;
    let dir = tempfile::tempdir().expect("tempdir");
    let config = seed_array_config(dir.path(), &server.uri());
    let before = std::fs::read_to_string(&config).expect("read");
    let store = Arc::new(FakeCredentialStore::default());
    let (app, workspace) = booted_workspace(config.clone(), store.clone()).await;
    let (tx, rx) = ui_request_channel();
    let forwarder = taskboard_app::ui_requests::spawn_ui_request_forwarder(
        rx,
        app.engine.shared_state(),
        workspace,
    );
    let board = create_board(&app, "home").await;

    let outcome = send_attach(&tx, board, "work", 7)
        .await
        .expect("reply lane alive")
        .expect_err("no secret stored");
    assert!(matches!(outcome, SettingsError::NoSecretStored));
    assert!(bound_connector_of(&app, board).is_none(), "no binding");
    assert_eq!(
        std::fs::read_to_string(&config).expect("read"),
        before,
        "nothing persisted"
    );

    drop(tx);
    forwarder.await.expect("forwarder joins cleanly");
}

/// A7 — the detach choreography: the binding leaves the engine state,
/// and the connector's actor stops only when no other live board
/// remains bound to it (the recorded multi-board guard).
#[tokio::test]
async fn detach_unbinds_and_stops_the_actor_only_when_the_last_binding_leaves() {
    let _env = crate::common::taskboard_env_lock();
    let dir = tempfile::tempdir().expect("tempdir");
    let config = seed_array_config(dir.path(), "http://127.0.0.1:9");
    let store = Arc::new(FakeCredentialStore::with(
        "http://127.0.0.1:9",
        "old-user",
        "pw",
    ));
    let (app, workspace) = booted_workspace(config.clone(), store).await;

    // A pre-attached actor: the detach's stop decision is observable.
    let client = taskboard_sync_nextcloud::DeckClient::new("http://127.0.0.1:9", "old-user", "pw")
        .expect("client builds");
    app.connector_control()
        .attach_connector(taskboard_domain::ConnectorId::from("work"), client)
        .await
        .expect("the fixture attach is accepted");

    let (tx, rx) = ui_request_channel();
    let forwarder = taskboard_app::ui_requests::spawn_ui_request_forwarder(
        rx,
        app.engine.shared_state(),
        workspace,
    );
    let first = create_board(&app, "home").await;
    let second = create_board(&app, "away").await;
    bind_directly(&app, first, "work", 7).await;
    bind_directly(&app, second, "work", 8).await;
    let work = taskboard_domain::ConnectorId::from("work");

    // Multi-board guard: detaching one of two boards leaves the actor
    // running and the sibling binding intact.
    let outcome = send_detach(&tx, first, "work")
        .await
        .expect("reply lane alive")
        .expect("detach succeeded");
    let row = outcome
        .boards
        .iter()
        .find(|b| b.id == first.as_uuid())
        .expect("board row");
    assert_eq!(row.bound_connector, None, "the detached board is unbound");
    assert!(bound_connector_of(&app, first).is_none());
    let (connector, remote) = bound_connector_of(&app, second).expect("sibling intact");
    assert_eq!(connector.as_str(), "work");
    assert_eq!(remote, RemoteBoardId::deck(8));
    assert!(
        app.connector_control().connector_commands(&work).is_some(),
        "the shared actor survives a sibling's detach"
    );

    // The last binding: the actor stops (idempotent named detach).
    let outcome = send_detach(&tx, second, "work")
        .await
        .expect("reply lane alive")
        .expect("detach succeeded");
    assert!(
        outcome.boards.iter().all(|b| b.bound_connector.is_none()),
        "the reply pair shows every board unbound"
    );
    assert!(
        app.connector_control().connector_commands(&work).is_none(),
        "the actor stopped when the last binding left"
    );

    drop(tx);
    forwarder.await.expect("forwarder joins cleanly");
}

/// A8 — detaching an unbound board is an accepted no-op (`Ok`, no
/// panic, the actor state untouched).
#[tokio::test]
async fn detach_of_an_unbound_board_is_an_accepted_no_op() {
    let _env = crate::common::taskboard_env_lock();
    let dir = tempfile::tempdir().expect("tempdir");
    let config = seed_array_config(dir.path(), "http://127.0.0.1:9");
    let store = Arc::new(FakeCredentialStore::with(
        "http://127.0.0.1:9",
        "old-user",
        "pw",
    ));
    let (app, workspace) = booted_workspace(config.clone(), store).await;
    let (tx, rx) = ui_request_channel();
    let forwarder = taskboard_app::ui_requests::spawn_ui_request_forwarder(
        rx,
        app.engine.shared_state(),
        workspace,
    );
    let board = create_board(&app, "home").await;

    let outcome = send_detach(&tx, board, "work")
        .await
        .expect("reply lane alive")
        .expect("no-op detach succeeds");
    assert!(
        outcome.boards.iter().all(|b| b.bound_connector.is_none()),
        "nothing was bound"
    );

    drop(tx);
    forwarder.await.expect("forwarder joins cleanly");
}

/// A9 — a dropped reply receiver on attach is survived: the work
/// landed (the binding is in the engine state), the forwarder stays
/// alive for the next request.
#[tokio::test]
async fn a_dropped_attach_reply_receiver_never_wedges_the_forwarder() {
    let _env = crate::common::taskboard_env_lock();
    let server = mount_boards(&[(7, "home")]).await;
    let dir = tempfile::tempdir().expect("tempdir");
    let config = seed_array_config(dir.path(), &server.uri());
    let store = Arc::new(FakeCredentialStore::with(&server.uri(), "old-user", "pw"));
    let (app, workspace) = booted_workspace(config.clone(), store).await;
    let (tx, rx) = ui_request_channel();
    let forwarder = taskboard_app::ui_requests::spawn_ui_request_forwarder(
        rx,
        app.engine.shared_state(),
        workspace,
    );
    let board = create_board(&app, "home").await;

    // Send with the receiver dropped before the handler answers.
    {
        let (reply, _rx) = tokio::sync::oneshot::channel();
        tx.try_send(UiRequest::AttachConnector {
            board,
            connector: "work".into(),
            remote: RemoteBoardId::deck(7),
            reply,
        })
        .expect("capacity holds");
    }

    // The lane still works: a real SetScale lands (the deterministic
    // round-trip barrier), and the binding landed behind it.
    let (reply, reply_rx) = tokio::sync::oneshot::channel();
    tx.try_send(UiRequest::SetScale { scale: 1.5, reply })
        .expect("capacity holds");
    reply_rx
        .await
        .expect("reply lane alive")
        .expect("the scale persisted");
    let (connector, remote) = bound_connector_of(&app, board).expect("the binding landed");
    assert_eq!(connector.as_str(), "work");
    assert_eq!(remote, RemoteBoardId::deck(7));

    drop(tx);
    forwarder.await.expect("forwarder joins cleanly");
}

/// A10b — the engine-command mapping table (plan §4.5's new arms):
/// stale board → `Gone`; attach refusing because the name is in use
/// maps through the existing `AttachBusy` arm (re-pinned).
#[tokio::test]
async fn attach_on_a_stale_board_id_is_gone_typed() {
    let _env = crate::common::taskboard_env_lock();
    let server = mount_boards(&[(7, "home")]).await;
    let dir = tempfile::tempdir().expect("tempdir");
    let config = seed_array_config(dir.path(), &server.uri());
    let store = Arc::new(FakeCredentialStore::with(&server.uri(), "old-user", "pw"));
    let (app, workspace) = booted_workspace(config.clone(), store).await;
    let (tx, rx) = ui_request_channel();
    let forwarder = taskboard_app::ui_requests::spawn_ui_request_forwarder(
        rx,
        app.engine.shared_state(),
        workspace,
    );
    let ghost = taskboard_domain::BoardId::from_uuid(uuid::Uuid::from_u128(0xDEAD));
    let outcome = send_attach(&tx, ghost, "work", 7)
        .await
        .expect("reply lane alive")
        .expect_err("stale board id");
    assert!(matches!(outcome, SettingsError::Gone), "got {outcome:?}");

    drop(tx);
    forwarder.await.expect("forwarder joins cleanly");
}

// -- round-5 chunk 8: the account-first connection surface --

fn send_list(
    tx: &tokio::sync::mpsc::Sender<UiRequest>,
    connector: &str,
) -> tokio::sync::oneshot::Receiver<
    Result<Vec<taskboard_ui_slint::requests::RemoteBoardRow>, SettingsError>,
> {
    let (reply, reply_rx) = tokio::sync::oneshot::channel();
    tx.try_send(UiRequest::ListRemoteBoards {
        connector: connector.to_owned(),
        reply,
    })
    .expect("capacity holds");
    reply_rx
}

/// A boards listing with one deleted and one archived entry beside the
/// live ones (the live filter's input).
async fn mount_mixed_boards() -> wiremock::MockServer {
    let mut boards: Vec<serde_json::Value> =
        serde_json::from_str(&boards_json_with(&[(7, "home"), (8, "away")])).expect("fixture json");
    boards.push(serde_json::json!({
        "id": 9, "title": "gone", "color": "00ff00", "lastModified": common::T,
        "deletedAt": common::T, "archived": false, "labels": []
    }));
    boards.push(serde_json::json!({
        "id": 10, "title": "old", "color": "00ff00", "lastModified": common::T,
        "deletedAt": 0, "archived": true, "labels": []
    }));
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(BOARDS_PATH))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_string(serde_json::Value::Array(boards).to_string()),
        )
        .mount(&server)
        .await;
    server
}

/// L1 — the listing projects the live remotes only and changes
/// nothing, bindings present or not: no attach, no state change, no
/// file write.
#[tokio::test]
async fn the_listing_projects_live_rows_and_changes_nothing() {
    let _env = crate::common::taskboard_env_lock();
    let server = mount_mixed_boards().await;
    let dir = tempfile::tempdir().expect("tempdir");
    let config = seed_array_config(dir.path(), &server.uri());
    let before = std::fs::read_to_string(&config).expect("read");
    let store = Arc::new(FakeCredentialStore::with(&server.uri(), "old-user", "pw"));
    let (app, workspace) = booted_workspace(config.clone(), store).await;
    let (tx, rx) = ui_request_channel();
    let forwarder = taskboard_app::ui_requests::spawn_ui_request_forwarder(
        rx,
        app.engine.shared_state(),
        workspace,
    );
    let mine = create_board(&app, "mine").await;
    let foreign = create_board(&app, "foreign").await;
    bind_directly(&app, mine, "work", 7).await;
    bind_directly(&app, foreign, "other", 8).await;
    let state_before = app.engine.shared_state().load_full();

    let rows = send_list(&tx, "work")
        .await
        .expect("reply lane alive")
        .expect("the listing succeeded");
    let ids: Vec<u64> = rows.iter().filter_map(|r| r.remote_id.as_deck()).collect();
    assert_eq!(ids, vec![7, 8], "deleted and archived remotes are filtered");

    assert!(
        app.connector_control()
            .connector_commands(&taskboard_domain::ConnectorId::from("work"))
            .is_none(),
        "the listing attaches nothing"
    );
    assert_eq!(*app.engine.shared_state().load_full(), *state_before);
    assert_eq!(std::fs::read_to_string(&config).expect("read"), before);

    drop(tx);
    forwarder.await.expect("forwarder joins cleanly");
}

/// L2 — the listing's typed failures leave the state untouched: no
/// stored secret, a vanished connector, an unreachable server.
#[tokio::test]
async fn the_listing_refuses_typed_without_state_change() {
    let _env = crate::common::taskboard_env_lock();
    let dir = tempfile::tempdir().expect("tempdir");
    let config = seed_array_config(dir.path(), "http://127.0.0.1:9");
    let store = Arc::new(FakeCredentialStore::default());
    let (app, workspace) = booted_workspace(config.clone(), store.clone()).await;
    let (tx, rx) = ui_request_channel();
    let forwarder = taskboard_app::ui_requests::spawn_ui_request_forwarder(
        rx,
        app.engine.shared_state(),
        workspace,
    );
    let state_before = app.engine.shared_state().load_full();

    let err = send_list(&tx, "work")
        .await
        .expect("alive")
        .expect_err("no secret");
    assert!(matches!(err, SettingsError::NoSecretStored), "got {err:?}");
    let err = send_list(&tx, "nope")
        .await
        .expect("alive")
        .expect_err("gone");
    assert!(matches!(err, SettingsError::Gone), "got {err:?}");
    store
        .store("http://127.0.0.1:9", "old-user", AppPassword::new("pw"))
        .expect("seed");
    let err = send_list(&tx, "work")
        .await
        .expect("alive")
        .expect_err("down");
    assert!(
        matches!(err, SettingsError::VerifyUnreachable),
        "got {err:?}"
    );
    assert_eq!(*app.engine.shared_state().load_full(), *state_before);

    drop(tx);
    forwarder.await.expect("forwarder joins cleanly");
}

/// S1 — a sign-in (empty name) derives the connector name from the
/// identity; a re-sign-in of the same identity reuses it (no
/// duplicate); a different user on the same host gets its own name.
#[tokio::test]
async fn a_sign_in_derives_the_connector_name_and_a_re_sign_in_reuses_it() {
    let _env = crate::common::taskboard_env_lock();
    let server = mount_boards_ok().await;
    let dir = tempfile::tempdir().expect("tempdir");
    let config = seed_array_config(dir.path(), &server.uri());
    let store = Arc::new(FakeCredentialStore::default());
    let (_app, workspace) = booted_workspace(config.clone(), store).await;
    let (tx, rx) = ui_request_channel();
    let forwarder = taskboard_app::ui_requests::spawn_ui_request_forwarder(
        rx,
        Arc::new(ArcSwap::from_pointee(taskboard_domain::AppState::default())),
        workspace,
    );
    let sign_in = |user: &str| {
        let (reply, reply_rx) = tokio::sync::oneshot::channel();
        tx.try_send(UiRequest::SaveConnection {
            profile: ConnectionProfile {
                name: String::new(),
                server_url: server.uri(),
                username: user.to_owned(),
                password: Secret("pw".into()),
            },
            reply,
        })
        .expect("capacity holds");
        reply_rx
    };
    let host = server.uri().replace("http://", "").replace(['.', ':'], "-");

    let first = sign_in("alice").await.expect("alive").expect("signed in");
    let names: Vec<&str> = first.connectors.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, vec!["work".to_owned(), format!("{host}-alice")]);

    let again = sign_in("alice")
        .await
        .expect("alive")
        .expect("re-signed in");
    assert_eq!(
        again.connectors, first.connectors,
        "an upsert, not a new card"
    );

    let bob = sign_in("bob").await.expect("alive").expect("signed in");
    assert_eq!(bob.connectors.len(), 3);
    assert_eq!(bob.connectors[2].name, format!("{host}-bob"));

    drop(tx);
    forwarder.await.expect("forwarder joins cleanly");
}

fn send_create(
    tx: &tokio::sync::mpsc::Sender<UiRequest>,
    connector: &str,
    remote: u64,
) -> tokio::sync::oneshot::Receiver<
    Result<taskboard_ui_slint::requests::ConnectionsSnapshot, SettingsError>,
> {
    let (reply, reply_rx) = tokio::sync::oneshot::channel();
    tx.try_send(UiRequest::CreateLocalBoard {
        connector: connector.to_owned(),
        remote: RemoteBoardId::deck(remote),
        reply,
    })
    .expect("capacity holds");
    reply_rx
}

/// The live local boards titled `title`.
fn boards_titled(app: &App, title: &str) -> Vec<taskboard_domain::BoardId> {
    app.engine
        .shared_state()
        .load()
        .boards
        .values()
        .filter(|b| !b.deleted && !b.archived && b.title == title)
        .map(|b| b.id)
        .collect()
}

/// C1 — create-local-from-remote: the receipt's minted board carries
/// the remote's title and is bound to that remote through the
/// connector; a remote that is not live is `Gone` with no board made.
#[tokio::test]
async fn create_local_board_mints_and_binds_the_remote() {
    let _env = crate::common::taskboard_env_lock();
    let server = wiremock::MockServer::start().await;
    common::mount_standard_pull(&server, serde_json::json!([])).await;
    let dir = tempfile::tempdir().expect("tempdir");
    let config = seed_array_config(dir.path(), &server.uri());
    let store = Arc::new(FakeCredentialStore::with(&server.uri(), "old-user", "pw"));
    let (app, workspace) = booted_workspace(config.clone(), store).await;
    let (tx, rx) = ui_request_channel();
    let forwarder = taskboard_app::ui_requests::spawn_ui_request_forwarder(
        rx,
        app.engine.shared_state(),
        workspace,
    );

    let err = send_create(&tx, "work", 999)
        .await
        .expect("alive")
        .expect_err("no such remote");
    assert!(matches!(err, SettingsError::Gone), "got {err:?}");
    assert!(
        app.engine.shared_state().load().boards.is_empty(),
        "no board made"
    );

    let outcome = send_create(&tx, "work", common::BOARD)
        .await
        .expect("alive")
        .expect("created and bound");
    // The remote fixture's board is titled "board" (common::boards_json).
    let created = boards_titled(&app, "board");
    assert_eq!(created.len(), 1, "one local board minted");
    let (connector, remote) = bound_connector_of(&app, created[0]).expect("bound");
    assert_eq!(connector.as_str(), "work");
    assert_eq!(remote, RemoteBoardId::deck(common::BOARD));
    assert!(
        outcome
            .boards
            .iter()
            .any(|b| b.id == created[0].as_uuid() && b.bound_connector.as_deref() == Some("work")),
        "the reply pair names the binding"
    );

    drop(tx);
    forwarder.await.expect("forwarder joins cleanly");
}

/// C2 — a refused bind after the create performs no rollback: the
/// remote is already in use through this connector, so the bind is
/// `BindRefused` and the minted local board stays, unbound.
#[tokio::test]
async fn a_refused_bind_after_create_leaves_the_local_board() {
    let _env = crate::common::taskboard_env_lock();
    let server = wiremock::MockServer::start().await;
    common::mount_standard_pull(&server, serde_json::json!([])).await;
    let dir = tempfile::tempdir().expect("tempdir");
    let config = seed_array_config(dir.path(), &server.uri());
    let store = Arc::new(FakeCredentialStore::with(&server.uri(), "old-user", "pw"));
    let (app, workspace) = booted_workspace(config.clone(), store).await;
    let (tx, rx) = ui_request_channel();
    let forwarder = taskboard_app::ui_requests::spawn_ui_request_forwarder(
        rx,
        app.engine.shared_state(),
        workspace,
    );
    let holder = create_board(&app, "holder").await;
    bind_directly(&app, holder, "work", common::BOARD).await;

    let err = send_create(&tx, "work", common::BOARD)
        .await
        .expect("alive")
        .expect_err("the remote is in use");
    assert!(matches!(err, SettingsError::BindRefused), "got {err:?}");
    let created = boards_titled(&app, "board");
    assert_eq!(created.len(), 1, "the minted board stays");
    assert!(bound_connector_of(&app, created[0]).is_none(), "unbound");

    drop(tx);
    forwarder.await.expect("forwarder joins cleanly");
}

/// R1 — account removal: refused typed while a board is bound through
/// any of the identity's connectors (nothing changes); once unbound,
/// every connector sharing the identity leaves together.
#[tokio::test]
async fn account_removal_is_guarded_by_bindings_and_removes_the_identity() {
    let _env = crate::common::taskboard_env_lock();
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("taskboard.toml");
    std::fs::write(
        &path,
        format!(
            "[storage]\ndb_path = '{}/taskboard.db'\n\
             [[sync.connectors]]\nname = \"work\"\nbackend = \"nextcloud\"\n\
             server_url = \"http://127.0.0.1:9\"\nusername = \"alice\"\n\
             [[sync.connectors]]\nname = \"work-2\"\nbackend = \"nextcloud\"\n\
             server_url = \"http://127.0.0.1:9\"\nusername = \"alice\"\n\
             [[sync.connectors]]\nname = \"other\"\nbackend = \"nextcloud\"\n\
             server_url = \"http://127.0.0.1:9\"\nusername = \"bob\"\n",
            dir.path().display()
        ),
    )
    .expect("seed config");
    let store = Arc::new(FakeCredentialStore::with(
        "http://127.0.0.1:9",
        "alice",
        "pw",
    ));
    let (app, workspace) = booted_workspace(path.clone(), store.clone()).await;
    let (tx, rx) = ui_request_channel();
    let forwarder = taskboard_app::ui_requests::spawn_ui_request_forwarder(
        rx,
        app.engine.shared_state(),
        workspace,
    );
    let board = create_board(&app, "home").await;
    bind_directly(&app, board, "work-2", 7).await;
    let before = std::fs::read_to_string(&path).expect("read");
    let remove = |name: &str| {
        let (reply, reply_rx) = tokio::sync::oneshot::channel();
        tx.try_send(UiRequest::RemoveConnection {
            name: name.to_owned(),
            reply,
        })
        .expect("capacity holds");
        reply_rx
    };

    let err = remove("work").await.expect("alive").expect_err("bound");
    assert!(matches!(err, SettingsError::BindRefused), "got {err:?}");
    assert_eq!(std::fs::read_to_string(&path).expect("read"), before);
    assert!(
        store.load("http://127.0.0.1:9", "alice").is_ok(),
        "secret kept"
    );

    app.engine
        .execute(StateCommand::UnbindBoard { board })
        .await
        .expect("unbind");
    let outcome = remove("work").await.expect("alive").expect("removed");
    let names: Vec<&str> = outcome.connectors.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, vec!["other"], "the whole identity left");
    assert!(
        store.load("http://127.0.0.1:9", "alice").is_err(),
        "secret deleted"
    );

    drop(tx);
    forwarder.await.expect("forwarder joins cleanly");
}

/// L3 — a listing the server refuses is a typed verify refusal (the
/// listing is the credential check).
#[tokio::test]
async fn a_rejected_listing_is_a_typed_verify_refusal() {
    let _env = crate::common::taskboard_env_lock();
    let server = mount_auth_reject(401).await;
    let dir = tempfile::tempdir().expect("tempdir");
    let config = seed_array_config(dir.path(), &server.uri());
    let store = Arc::new(FakeCredentialStore::with(&server.uri(), "old-user", "pw"));
    let (app, workspace) = booted_workspace(config.clone(), store).await;
    let (tx, rx) = ui_request_channel();
    let forwarder = taskboard_app::ui_requests::spawn_ui_request_forwarder(
        rx,
        app.engine.shared_state(),
        workspace,
    );
    let err = send_list(&tx, "work")
        .await
        .expect("alive")
        .expect_err("401");
    assert!(matches!(err, SettingsError::VerifyRejected), "got {err:?}");

    drop(tx);
    forwarder.await.expect("forwarder joins cleanly");
}

// ---- the Publish arm (round-6 D-6.9, #699) --------------------------

fn send_publish(
    tx: &tokio::sync::mpsc::Sender<UiRequest>,
    board: taskboard_domain::BoardId,
    connector: &str,
) -> tokio::sync::oneshot::Receiver<
    Result<taskboard_ui_slint::requests::ConnectionsSnapshot, SettingsError>,
> {
    let (reply, reply_rx) = tokio::sync::oneshot::channel();
    tx.try_send(UiRequest::PublishBoard {
        board,
        connector: connector.to_owned(),
        reply,
    })
    .expect("capacity holds");
    reply_rx
}

/// P1 — the publish happy path: the create POST carries the local
/// board's title/color, the reply snapshot names the binding to the
/// minted remote id, the actor is attached, and the engine has
/// settled — binding present, phase `Idle` (the ceremony's reply
/// implies the adoption wait; terminal state, no wall-clock).
#[tokio::test]
async fn publish_creates_the_remote_board_and_binds_the_local_one() {
    let _env = crate::common::taskboard_env_lock();
    let server = wiremock::MockServer::start().await;
    common::mount_standard_pull(&server, serde_json::json!([])).await;
    // The create: the ceremony must derive title/color from the local
    // board (the mock's body match *is* that assertion — an unmatched
    // body 404s and the publish fails loudly).
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(BOARDS_PATH))
        .and(wiremock::matchers::body_partial_json(serde_json::json!({
            "title": "publish me",
            "color": "ff8800"
        })))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_string(common::board_json()))
        .mount(&server)
        .await;
    let dir = tempfile::tempdir().expect("tempdir");
    let config = seed_array_config(dir.path(), &server.uri());
    let store = Arc::new(FakeCredentialStore::with(&server.uri(), "old-user", "pw"));
    let (app, workspace) = booted_workspace(config.clone(), store).await;
    let (tx, rx) = ui_request_channel();
    let forwarder = taskboard_app::ui_requests::spawn_ui_request_forwarder(
        rx,
        app.engine.shared_state(),
        workspace,
    );
    let board = create_board(&app, "publish me").await;

    let outcome = send_publish(&tx, board, "work")
        .await
        .expect("reply lane alive")
        .expect("publish succeeded");
    let row = outcome
        .boards
        .iter()
        .find(|b| b.id == board.as_uuid())
        .expect("the board row is in the reply");
    assert_eq!(row.bound_connector.as_deref(), Some("work"));

    // Terminal engine state: bound to the minted remote, settled Idle.
    let (connector, remote) = bound_connector_of(&app, board).expect("bound");
    assert_eq!(connector.as_str(), "work");
    assert_eq!(remote, RemoteBoardId::deck(common::BOARD));
    let phase = app
        .engine
        .shared_state()
        .load()
        .sync
        .get(board)
        .map(|r| r.phase);
    assert_eq!(phase, Some(taskboard_domain::SyncPhase::Idle));

    // The connector actor is live under its name (the nudge reached it).
    assert!(
        app.connector_control()
            .connector_commands(&taskboard_domain::ConnectorId::from("work"))
            .is_some(),
        "the connector was attached"
    );

    drop(tx);
    forwarder.await.expect("forwarder joins cleanly");
}

/// P2 — a bound board refuses pre-IO: `BindRefused`, the binding
/// intact, and the unreachable server proves the create never ran.
#[tokio::test]
async fn publish_refuses_a_bound_board_before_any_io() {
    let _env = crate::common::taskboard_env_lock();
    let dir = tempfile::tempdir().expect("tempdir");
    let config = seed_array_config(dir.path(), "http://127.0.0.1:9");
    let store = Arc::new(FakeCredentialStore::with(
        "http://127.0.0.1:9",
        "old-user",
        "pw",
    ));
    let (app, workspace) = booted_workspace(config.clone(), store).await;
    let (tx, rx) = ui_request_channel();
    let forwarder = taskboard_app::ui_requests::spawn_ui_request_forwarder(
        rx,
        app.engine.shared_state(),
        workspace,
    );
    let board = create_board(&app, "already bound").await;
    bind_directly(&app, board, "work", 7).await;

    let outcome = send_publish(&tx, board, "work")
        .await
        .expect("reply lane alive")
        .expect_err("bound board");
    assert!(
        matches!(outcome, SettingsError::BindRefused),
        "typed bind refusal, got {outcome:?}"
    );
    assert_eq!(
        bound_connector_of(&app, board).map(|(_, remote)| remote),
        Some(RemoteBoardId::deck(7)),
        "the old binding survived"
    );

    drop(tx);
    forwarder.await.expect("forwarder joins cleanly");
}

/// P3 — an unknown connector name refuses `Gone` before any IO (the
/// unreachable server never being contacted is implied by the typed
/// refusal of a name that resolves to no config entry).
#[tokio::test]
async fn publish_against_an_unknown_connector_refuses_gone() {
    let _env = crate::common::taskboard_env_lock();
    let dir = tempfile::tempdir().expect("tempdir");
    let config = seed_array_config(dir.path(), "http://127.0.0.1:9");
    let store = Arc::new(FakeCredentialStore::with(
        "http://127.0.0.1:9",
        "old-user",
        "pw",
    ));
    let (app, workspace) = booted_workspace(config.clone(), store).await;
    let (tx, rx) = ui_request_channel();
    let forwarder = taskboard_app::ui_requests::spawn_ui_request_forwarder(
        rx,
        app.engine.shared_state(),
        workspace,
    );
    let board = create_board(&app, "wandering").await;

    let outcome = send_publish(&tx, board, "missing")
        .await
        .expect("reply lane alive")
        .expect_err("no such connector");
    assert!(
        matches!(outcome, SettingsError::Gone),
        "typed gone refusal, got {outcome:?}"
    );
    assert!(bound_connector_of(&app, board).is_none(), "nothing bound");

    drop(tx);
    forwarder.await.expect("forwarder joins cleanly");
}

// ---- the Move-to-Files arm (round-6 D-6.8 phase 2, #722) ------------

fn send_move(
    tx: &tokio::sync::mpsc::Sender<UiRequest>,
    board: taskboard_domain::BoardId,
) -> tokio::sync::oneshot::Receiver<
    Result<taskboard_ui_slint::requests::ConnectionsSnapshot, SettingsError>,
> {
    let (reply, reply_rx) = tokio::sync::oneshot::channel();
    tx.try_send(UiRequest::MoveBoardToFiles { board, reply })
        .expect("capacity holds");
    reply_rx
}

/// Binds a board directly to the files lane through the engine (the
/// adoption-posture fixture: the files binding's connector is the
/// synthesized `files-sync` identity, no config connector).
async fn bind_files_directly(app: &App, board: taskboard_domain::BoardId) {
    app.engine
        .execute(StateCommand::BindBoard {
            board,
            remote: RemoteBoardId::files(uuid::Uuid::new_v4()),
            connector: taskboard_domain::ConnectorId::from("files-sync"),
        })
        .await
        .expect("the fixture files bind is accepted");
}

/// Seeds an array-world config with one Nextcloud connector and
/// `[files_sync] enabled` (the lane resolves its anchor from the
/// connector; the mover needs the lane on to reach the binding
/// guards).
fn seed_files_config(dir: &std::path::Path, server: &str) -> PathBuf {
    let path = dir.join("taskboard.toml");
    std::fs::write(
        &path,
        format!(
            "[storage]\ndb_path = '{}/taskboard.db'\n\
             [files_sync]\nenabled = true\n\
             [[sync.connectors]]\nname = \"work\"\nbackend = \"nextcloud\"\n\
             server_url = \"{server}\"\nusername = \"old-user\"\n",
            dir.display()
        ),
    )
    .expect("seed config");
    path
}

/// M2 — a files-bound board refuses before any IO: the ceremony is
/// already `Committed` for it (D-1's structural read of
/// `derive_move_stage`); the binding survives and the unreachable
/// server proves nothing was contacted.
#[tokio::test]
async fn move_refuses_a_files_bound_board_before_any_io() {
    let _env = crate::common::taskboard_env_lock();
    let dir = tempfile::tempdir().expect("tempdir");
    let config = seed_files_config(dir.path(), "http://127.0.0.1:9");
    let store = Arc::new(FakeCredentialStore::with(
        "http://127.0.0.1:9",
        "old-user",
        "pw",
    ));
    let (app, workspace) = booted_workspace(config.clone(), store).await;
    let (tx, rx) = ui_request_channel();
    let forwarder = taskboard_app::ui_requests::spawn_ui_request_forwarder(
        rx,
        app.engine.shared_state(),
        workspace,
    );
    let board = create_board(&app, "already files").await;
    bind_files_directly(&app, board).await;

    let outcome = send_move(&tx, board)
        .await
        .expect("reply lane alive")
        .expect_err("files-bound board");
    assert!(
        matches!(outcome, SettingsError::BindRefused),
        "typed bind refusal, got {outcome:?}"
    );
    assert!(
        bound_connector_of(&app, board).is_some_and(|(_, remote)| remote.as_files().is_some()),
        "the files binding survived"
    );

    drop(tx);
    forwarder.await.expect("forwarder joins cleanly");
}

/// M3 — an unbound board refuses before any IO (`BindRefused`): the
/// picker's option set mirrors the ceremony's guard, so this arrives
/// only via a stale pick; the world refuses typed.
#[tokio::test]
async fn move_refuses_an_unbound_board_before_any_io() {
    let _env = crate::common::taskboard_env_lock();
    let dir = tempfile::tempdir().expect("tempdir");
    let config = seed_files_config(dir.path(), "http://127.0.0.1:9");
    let store = Arc::new(FakeCredentialStore::with(
        "http://127.0.0.1:9",
        "old-user",
        "pw",
    ));
    let (app, workspace) = booted_workspace(config.clone(), store).await;
    let (tx, rx) = ui_request_channel();
    let forwarder = taskboard_app::ui_requests::spawn_ui_request_forwarder(
        rx,
        app.engine.shared_state(),
        workspace,
    );
    let board = create_board(&app, "loose").await;

    let outcome = send_move(&tx, board)
        .await
        .expect("reply lane alive")
        .expect_err("unbound board");
    assert!(
        matches!(outcome, SettingsError::BindRefused),
        "typed bind refusal, got {outcome:?}"
    );
    assert!(bound_connector_of(&app, board).is_none(), "nothing bound");

    drop(tx);
    forwarder.await.expect("forwarder joins cleanly");
}

/// M4 — with `[files_sync]` off there is no lane to move onto: the
/// typed `FilesSyncDisabled` refusal fires before any IO (the
/// unreachable server is never contacted).
#[tokio::test]
async fn move_refuses_when_files_sync_is_disabled() {
    let _env = crate::common::taskboard_env_lock();
    let dir = tempfile::tempdir().expect("tempdir");
    let config = seed_array_config(dir.path(), "http://127.0.0.1:9");
    let store = Arc::new(FakeCredentialStore::with(
        "http://127.0.0.1:9",
        "old-user",
        "pw",
    ));
    let (app, workspace) = booted_workspace(config.clone(), store).await;
    let (tx, rx) = ui_request_channel();
    let forwarder = taskboard_app::ui_requests::spawn_ui_request_forwarder(
        rx,
        app.engine.shared_state(),
        workspace,
    );
    let board = create_board(&app, "deck bound").await;
    bind_directly(&app, board, "work", 5).await;

    let outcome = send_move(&tx, board)
        .await
        .expect("reply lane alive")
        .expect_err("no files lane");
    assert!(
        matches!(outcome, SettingsError::FilesSyncDisabled),
        "typed lane-off refusal, got {outcome:?}"
    );
    assert!(
        bound_connector_of(&app, board).is_some_and(|(_, remote)| remote.as_deck() == Some(5)),
        "the Deck binding survived"
    );

    drop(tx);
    forwarder.await.expect("forwarder joins cleanly");
}

// ---- the hermetic ceremony tests (#751, the recorded r6 fallbacks) ---

/// An in-memory DAV substrate over wiremock: the ceremony's PUTs land
/// in one shared map and its GETs serve exactly what was written — a
/// real round-trip, not canned bodies. Entries are keyed by their
/// substrate-relative path (after the DAV home's `taskboard/` root).
#[derive(Clone, Default)]
struct DavMemory(Arc<std::sync::Mutex<std::collections::BTreeMap<String, Vec<u8>>>>);

fn substrate_of(request: &wiremock::Request) -> String {
    request
        .url
        .path()
        .rsplit_once("taskboard/")
        .map(|(_, rest)| rest.trim_end_matches('/').to_owned())
        .unwrap_or_default()
}

async fn mount_dav_memory(server: &wiremock::MockServer, mem: DavMemory) {
    use wiremock::matchers::{method, path_regex};
    use wiremock::{Mock, ResponseTemplate};

    // The Deck endpoints are absolute (`/index.php/...`), the DAV home
    // sits under `/remote.php/dav/files/` — the prefixes are disjoint.
    const DAV_HOME: &str = "/remote.php/dav/files/";
    Mock::given(method("MKCOL"))
        .and(path_regex(format!("^{DAV_HOME}.*")))
        .respond_with(|_: &wiremock::Request| ResponseTemplate::new(201))
        .mount(server)
        .await;
    let put = mem.clone();
    Mock::given(method("PUT"))
        .and(path_regex(format!("^{DAV_HOME}.*")))
        .respond_with(move |request: &wiremock::Request| {
            put.0
                .lock()
                .expect("dav map")
                .insert(substrate_of(request), request.body.clone());
            ResponseTemplate::new(201).insert_header("ETag", "\"dav-put\"")
        })
        .mount(server)
        .await;
    let get = mem.clone();
    Mock::given(method("GET"))
        .and(path_regex(format!("^{DAV_HOME}.*")))
        .respond_with(move |request: &wiremock::Request| {
            get.0
                .lock()
                .expect("dav map")
                .get(&substrate_of(request))
                .cloned()
                .map_or_else(
                    || ResponseTemplate::new(404),
                    |body| {
                        ResponseTemplate::new(200)
                            .set_body_bytes(body)
                            .insert_header("ETag", "\"dav-put\"")
                    },
                )
        })
        .mount(server)
        .await;
}

/// The ceremony mounts the Deck server (`mount_standard_pull`) and the
/// `WebDAV` mounts for the ceremony's probe/put/confirm steps; the
/// request replies `Ok`, the snapshot row names the `files-sync`
/// binding, the engine state carries the files binding and Idle phase.
/// Terminal-state assertions only — the reply implies settlement.
#[tokio::test]
async fn move_commits_a_deck_board_onto_files() {
    let _env = crate::common::taskboard_env_lock();
    let server = wiremock::MockServer::start().await;
    // The WebDAV substrate: an in-memory filesystem over wiremock so
    // the ceremony's GETs serve exactly what its own PUTs wrote (a
    // real round-trip; identity.json's moved_from is read back off the
    // ceremony's own write). No env, no shared dir — the identity file
    // is the workspace's injected tempdir path.
    let dav = DavMemory::default();
    mount_dav_memory(&server, dav).await;
    // The stacks listing (wiremock serves the first-mounted match; this
    // shadows mount_standard_pull's listing): an empty stack list makes
    // `write_move_marker` a typed no-op — no anchor card to create —
    // and the pull folds an empty subtree, which it reports as a
    // success.
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(format!(
            "/index.php/apps/deck/api/v1.0/boards/{}/stacks",
            common::BOARD
        )))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_string("[]"))
        .mount(&server)
        .await;
    common::mount_standard_pull(&server, serde_json::json!([])).await;
    // The attach arm's create POST (the Publish section's grammar):
    // the mock's body match *is* the title/color assertion.
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(BOARDS_PATH))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_string(common::board_json()))
        .mount(&server)
        .await;
    let dir = tempfile::tempdir().expect("tempdir");
    let config = seed_files_config(dir.path(), &server.uri());
    let store = Arc::new(FakeCredentialStore::with(&server.uri(), "old-user", "pw"));
    let (app, workspace) = booted_workspace(config.clone(), store).await;
    let (tx, rx) = ui_request_channel();
    let forwarder = taskboard_app::ui_requests::spawn_ui_request_forwarder(
        rx,
        app.engine.shared_state(),
        workspace,
    );
    let board = create_board(&app, "move me").await;
    send_attach(&tx, board, "work", common::BOARD)
        .await
        .expect("reply lane alive")
        .expect("attach succeeded");

    let outcome = send_move(&tx, board)
        .await
        .expect("reply lane alive")
        .expect("the move committed");
    let row = outcome
        .boards
        .iter()
        .find(|b| b.id == board.as_uuid())
        .expect("the board row is in the reply");
    assert_eq!(row.bound_connector.as_deref(), Some("files-sync"));
    let (_, remote) = bound_connector_of(&app, board).expect("bound");
    assert!(
        remote.as_files().is_some(),
        "the engine holds the files binding"
    );
    let phase = app
        .engine
        .shared_state()
        .load()
        .sync
        .get(board)
        .map(|r| r.phase);
    assert_eq!(phase, Some(taskboard_domain::SyncPhase::Idle));

    drop(tx);
    forwarder.await.expect("forwarder joins cleanly");
}

/// Deck-bound board with a queued op: the ceremony's quiescence guard
/// fires typed (`OutboxNotQuiescent`) before the floor publish, the
/// binding stays Deck. The queued op rides the board-flush trigger
/// (D-H9, the habit write) whose flush verdict is `Idle` against this
/// remote (no appdata anchor ladder) — the op stays queued and the
/// pull's cycle still completes, deterministically.
#[tokio::test]
async fn move_reports_a_queued_outbox_typed() {
    let _env = crate::common::taskboard_env_lock();
    let server = wiremock::MockServer::start().await;
    // Only the Deck-pull endpoints: the quiescence guard fires before
    // the floor publish, so no WebDAV grammar is mounted.
    common::mount_standard_pull(&server, serde_json::json!([])).await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(BOARDS_PATH))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_string(common::board_json()))
        .mount(&server)
        .await;
    let dir = tempfile::tempdir().expect("tempdir");
    let config = seed_files_config(dir.path(), &server.uri());
    let store = Arc::new(FakeCredentialStore::with(&server.uri(), "old-user", "pw"));
    let (app, workspace) = booted_workspace(config.clone(), store).await;
    let (tx, rx) = ui_request_channel();
    let forwarder = taskboard_app::ui_requests::spawn_ui_request_forwarder(
        rx,
        app.engine.shared_state(),
        workspace,
    );
    let board = create_board(&app, "queued").await;
    send_attach(&tx, board, "work", common::BOARD)
        .await
        .expect("reply lane alive")
        .expect("attach succeeded");
    app.engine
        .execute(taskboard_domain::StateCommand::CreateHabit {
            board,
            title: "drill".into(),
            target: taskboard_domain::HabitTarget::PerDay(
                taskboard_domain::NonZeroCount::new(1).expect("nonzero"),
            ),
        })
        .await
        .expect("the habit write is accepted");

    let outcome = send_move(&tx, board)
        .await
        .expect("reply lane alive")
        .expect_err("the outbox is not quiet");
    assert!(
        matches!(outcome, SettingsError::OutboxNotQuiescent),
        "typed outbox refusal, got {outcome:?}"
    );
    assert_eq!(
        bound_connector_of(&app, board).map(|(_, remote)| remote),
        Some(RemoteBoardId::deck(common::BOARD)),
        "the Deck binding survived"
    );

    drop(tx);
    forwarder.await.expect("forwarder joins cleanly");
}

// -- the #1113 re-homed env-composing legs ---------------------------
//
// The three ui_requests legs that compose the process env in
// production (the guard/persist reads) live here, under the suite's
// `taskboard_env_lock()` — runnable under any ambient, the
// hostile-env oracle by construction (#865/#1113).

/// A config whose files lane is spawnable (`[files_sync] enabled`
/// with a resolvable Nextcloud anchor) at the workspace's write path.
fn write_files_lane_config(path: &std::path::Path) {
    std::fs::write(
        path,
        "[ui]\nscale = 1.0\n\
         [[sync.connectors]]\nname = \"work\"\nbackend = \"nextcloud\"\n\
         server_url = \"https://c.example\"\nusername = \"alice\"\n\
         [files_sync]\nenabled = true\nanchor = \"work\"\n",
    )
    .expect("seed files-lane config");
}

/// US-6 (the files-sync-status plan D-7) — the undo-window
/// Restore arm's guards refuse typed: a gone board is `Gone`;
/// with no files lane configured the ceremony refuses typed
/// (`FilesSyncDisabled`) — the ceremony's own undo leg is the
/// stage gate, never a reimplemented undo.
#[tokio::test]
async fn us6_ac1_restore_guards_refuse_typed() {
    let _env = crate::common::taskboard_env_lock();
    let dir = tempfile::tempdir().expect("tempdir");
    let config = dir.path().join("taskboard.toml");
    std::fs::write(&config, "[ui]\nscale = 1.0\n").expect("seed");
    let (app, workspace) = booted_workspace(config, Arc::new(FakeCredentialStore::default())).await;
    let unbound = create_board(&app, "loose").await;

    let gone = taskboard_app::ui_requests::undo_board_move(
        &workspace,
        taskboard_domain::BoardId::from_uuid(uuid::Uuid::now_v7()),
    )
    .await
    .expect_err("a gone board refuses");
    assert_eq!(gone, SettingsError::Gone);
    let refused = taskboard_app::ui_requests::undo_board_move(&workspace, unbound)
        .await
        .expect_err("no files lane configured refuses typed");
    assert_eq!(refused, SettingsError::FilesSyncDisabled);
}

/// US-4/S2 — the Publish arm's guards refuse typed: deck-bound
/// and files-bound boards are `BindRefused` (a stale row's
/// mis-target), a disabled files lane is `FilesSyncDisabled`, a
/// gone board is `Gone`.
#[tokio::test]
async fn us4_s2_publish_guards_refuse_deck_bound() {
    let _env = crate::common::taskboard_env_lock();
    let dir = tempfile::tempdir().expect("tempdir");
    let config = dir.path().join("taskboard.toml");
    write_files_lane_config(&config);
    let (app, workspace) = booted_workspace(config, Arc::new(FakeCredentialStore::default())).await;

    let deck_bound = create_board(&app, "deck board").await;
    let files_bound = create_board(&app, "files board").await;
    app.engine
        .execute(taskboard_domain::StateCommand::BindBoard {
            board: deck_bound,
            remote: taskboard_domain::RemoteBoardId::deck(7),
            connector: taskboard_domain::ConnectorId::from("work"),
        })
        .await
        .expect("deck bind lands");
    app.engine
        .execute(taskboard_domain::StateCommand::BindBoard {
            board: files_bound,
            remote: taskboard_domain::RemoteBoardId::files(uuid::Uuid::now_v7()),
            connector: taskboard_domain::ConnectorId::from("files-sync"),
        })
        .await
        .expect("files bind lands");

    let deck = taskboard_app::ui_requests::publish_board_to_files(&workspace, deck_bound)
        .await
        .expect_err("a deck-bound board refuses");
    assert_eq!(deck, SettingsError::BindRefused);
    let files = taskboard_app::ui_requests::publish_board_to_files(&workspace, files_bound)
        .await
        .expect_err("a files-bound board refuses");
    assert_eq!(files, SettingsError::BindRefused);
    let gone = taskboard_app::ui_requests::publish_board_to_files(
        &workspace,
        taskboard_domain::BoardId::from_uuid(uuid::Uuid::now_v7()),
    )
    .await
    .expect_err("a gone board refuses");
    assert_eq!(gone, SettingsError::Gone);

    // A lane-off config refuses before the board guards touch the
    // state (D7's guard order — the cheap config check first).
    let dir_off = tempfile::tempdir().expect("tempdir");
    let config_off = dir_off.path().join("taskboard.toml");
    std::fs::write(&config_off, "[ui]\nscale = 1.0\n").expect("seed");
    let (app_off, workspace_off) =
        booted_workspace(config_off, Arc::new(FakeCredentialStore::default())).await;
    let unbound = create_board(&app_off, "loose").await;
    let disabled = taskboard_app::ui_requests::publish_board_to_files(&workspace_off, unbound)
        .await
        .expect_err("a lane-off config refuses");
    assert_eq!(disabled, SettingsError::FilesSyncDisabled);
}

/// The live lane: a hand-to-the-port write lands in the file AND
/// the engine publishes the new snapshot (`Applied`) — the
/// US-1/US-3 choreography in one place.
#[tokio::test]
async fn the_port_applies_live_to_a_real_engine() {
    let _env = crate::common::taskboard_env_lock();
    let dir = tempfile::tempdir().expect("tempdir");
    let config = dir.path().join("taskboard.toml");
    std::fs::write(&config, "[reminders]\ntick_secs = 30\n").expect("seed config");
    let (_app, workspace) =
        booted_workspace(config, Arc::new(FakeCredentialStore::default())).await;
    let locked = std::collections::BTreeSet::new();
    let outcome = taskboard_app::ui_requests::run_settings_write(
        workspace.config_write_path.as_path(),
        &locked,
        &["reminders.tick_secs"],
        |path| taskboard_app::settings_overlay::set_reminder_tick(path, 45),
        Some(&workspace.engine),
    )
    .await
    .expect("the write lands");
    assert_eq!(
        outcome.apply,
        taskboard_app::ui_requests::LiveApply::Applied,
        "the live lane"
    );
    let published = workspace.engine.shared_state().load();
    assert_eq!(u64::from(published.settings.tick_secs), 45);
    let text = std::fs::read_to_string(workspace.config_write_path.as_path()).expect("file");
    assert!(text.contains("tick_secs = 45"), "the file wrote: {text}");
}

// -- the #1157 re-homing: the seven ui_requests legs that composed the
// process env, moved out of the lib's gated `tests` module behind
// `taskboard_env_lock()` — runnable under any ambient, the
// hostile-env oracle by construction. Assertions are verbatim from
// the inline legs; only the fixture spell adapts (the integration
// binary cannot reach the lib's `pub(crate)` test support, so the
// boots use the suite's `FakeCredentialStore` — none of these legs
// touches the credential store).

/// The #982-ownership boot (mirrors the lib tests' `WorkspaceFixture`):
/// the db and the device identity share one tempdir, held by the
/// returned struct. **Field order is the contract** — `workspace` and
/// `app` drop before `dir`, so the app's actor teardown (the engine
/// join in `App`'s drop) always precedes the directory deletion.
struct OwnedBootFixture {
    workspace: ConnectionWorkspace,
    app: App,
    /// The db + device-identity home; held for its `Drop` side
    /// effect as much as for its path.
    dir: tempfile::TempDir,
}

/// [`owned_boot_at`] over a fresh tempdir: the config write path is a
/// plain unused name (the leg drives the handler, not the file lane).
async fn owned_boot(config_write_path: PathBuf) -> OwnedBootFixture {
    let dir = tempfile::tempdir().expect("tempdir");
    owned_boot_at(config_write_path, dir).await
}

/// The standalone boot over a caller-owned tempdir: the seeded config,
/// the db, and the device identity share the one directory (#982).
async fn owned_boot_at(config_write_path: PathBuf, dir: tempfile::TempDir) -> OwnedBootFixture {
    let mut config = AppConfig::default();
    config.storage.db_path = Some(dir.path().join("taskboard.db"));
    config.sync.backend = Some(taskboard_app::config::SyncBackend::Standalone);
    config.validate().expect("config valid");
    let app = bootstrap::bootstrap_in(
        config,
        Arc::new(FakeCredentialStore::default()),
        Arc::new(LogSink),
        bootstrap::BootIdentity::At(dir.path().to_owned()),
    )
    .await
    .expect("standalone boot");
    let workspace = ConnectionWorkspace {
        config_write_path,
        store: Arc::new(FakeCredentialStore::default()),
        control: app.connector_control(),
        engine: app.engine.clone(),
        // The suite's own identity home: the ceremony's create arm
        // mints there, never the platform default (#751).
        device_json: Some(dir.path().join("device.json")),
        connector_owners: app.connector_owners.clone(),
        export: taskboard_app::ui_requests::ExportWorkspace::default(),
        status: crate::ui_settings::in_session_status_source(&app),
        remediation: taskboard_app::status_remediation::RemediationWorkspace::default(),
    };
    OwnedBootFixture {
        workspace,
        app,
        dir,
    }
}

/// The port boot: a seeded config file sharing the db + identity
/// tempdir (the shared live-write port's fixture).
async fn port_boot(seed_text: &str) -> OwnedBootFixture {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("taskboard.toml");
    std::fs::write(&path, seed_text).expect("seed config");
    owned_boot_at(path, dir).await
}

/// The #982 lifetime pin: the directory survives as long as the
/// fixture, and its deletion happens only after the app's
/// teardown — guaranteed by the field order (workspace, app, then
/// `dir` last); a struct reorder must re-run this test.
#[tokio::test]
async fn workspace_fixture_deletes_the_dir_only_after_the_app_teardown() {
    let _env = crate::common::taskboard_env_lock();
    let fixture = owned_boot(PathBuf::from("unused-write-path.toml")).await;
    let dir = fixture.dir.path().to_owned();
    assert!(dir.exists(), "the fixture owns the directory");
    assert!(dir.join("taskboard.db").exists(), "the db lives there");
    // Destructure (the fixture's drop-by-field semantics are what
    // the pin observes): the app's teardown must complete while the
    // directory still exists, and the deletion must follow it.
    let OwnedBootFixture {
        workspace: _,
        app,
        dir: dir_guard,
    } = fixture;
    let outcome = app.shutdown().await;
    assert!(!outcome.actor_aborted, "clean teardown");
    assert!(dir.exists(), "the dir outlives the app's teardown");
    drop(dir_guard);
    assert!(!dir.exists(), "the drop deletes the dir");
}

/// The snapshot's board half carries the binding's channel,
/// detected STRUCTURALLY from the id type (D-BF4) — never from the
/// binding's connector name: a files binding whose connector name
/// is a Deck-shaped card name still reads `Files`.
#[tokio::test]
async fn current_connections_fills_bound_channel_structurally() {
    let _env = crate::common::taskboard_env_lock();
    let dir = tempfile::tempdir().expect("tempdir");
    let config = dir.path().join("taskboard.toml");
    std::fs::write(&config, "[ui]\nscale = 1.0\n").expect("seed");
    let fixture = owned_boot(config).await;
    let app = &fixture.app;
    let workspace = &fixture.workspace;

    let deck_bound = create_board(app, "deck board").await;
    let files_bound = create_board(app, "files board").await;
    let _loose = create_board(app, "loose").await;
    for (board, remote) in [
        (deck_bound, RemoteBoardId::deck(7)),
        (
            files_bound,
            taskboard_domain::RemoteBoardId::files(uuid::Uuid::now_v7()),
        ),
    ] {
        app.engine
            .execute(StateCommand::BindBoard {
                board,
                remote,
                // Deliberately a Deck-card-shaped name for BOTH
                // bindings: the channel must not key on it (D-BF4).
                connector: taskboard_domain::ConnectorId::from("work"),
            })
            .await
            .expect("bind lands");
    }

    let snapshot = taskboard_app::ui_requests::current_connections(workspace);
    let by_name = |name: &str| {
        snapshot
            .boards
            .iter()
            .find(|board| board.name == name)
            .expect("the live board is in the snapshot")
    };
    assert_eq!(
        by_name("deck board").bound_channel,
        Some(taskboard_ui_slint::requests::BoardChannel::Deck),
    );
    assert_eq!(
        by_name("files board").bound_channel,
        Some(taskboard_ui_slint::requests::BoardChannel::Files),
    );
    assert_eq!(by_name("loose").bound_channel, None);
    assert_eq!(
        by_name("files board").bound_connector.as_deref(),
        Some("work"),
        "the name field is untouched — the channel is the id type's"
    );
}

/// US-2/S2 — the Stop arm's guards refuse typed: a gone board is
/// `Gone`; a deck-bound and an unbound board are `BindRefused`
/// (the row grammar can't normally offer those — the arm still
/// refuses typed).
#[tokio::test]
async fn us2_s2_stop_sync_guards_refuse_typed() {
    let _env = crate::common::taskboard_env_lock();
    let dir = tempfile::tempdir().expect("tempdir");
    let config = dir.path().join("taskboard.toml");
    std::fs::write(&config, "[ui]\nscale = 1.0\n").expect("seed");
    let fixture = owned_boot(config).await;
    let app = &fixture.app;
    let workspace = &fixture.workspace;

    let deck_bound = create_board(app, "deck board").await;
    let unbound = create_board(app, "loose").await;
    app.engine
        .execute(StateCommand::BindBoard {
            board: deck_bound,
            remote: RemoteBoardId::deck(7),
            connector: taskboard_domain::ConnectorId::from("work"),
        })
        .await
        .expect("deck bind lands");

    let gone = taskboard_app::ui_requests::stop_board_files_sync(
        workspace,
        taskboard_domain::BoardId::from_uuid(uuid::Uuid::now_v7()),
    )
    .await
    .expect_err("a gone board refuses");
    assert_eq!(gone, SettingsError::Gone);
    let deck = taskboard_app::ui_requests::stop_board_files_sync(workspace, deck_bound)
        .await
        .expect_err("a deck-bound board refuses");
    assert_eq!(deck, SettingsError::BindRefused);
    let loose = taskboard_app::ui_requests::stop_board_files_sync(workspace, unbound)
        .await
        .expect_err("an unbound board refuses");
    assert_eq!(loose, SettingsError::BindRefused);
}

/// The Stop arm's exact effect (US-2/S3): the engine receives the
/// unbind — the files binding is gone, the board itself survives
/// (unbind is not a delete) — and the reply is the fresh
/// snapshot whose board half no longer carries the binding.
#[tokio::test]
async fn stop_board_files_sync_sends_exact_unbind_command() {
    let _env = crate::common::taskboard_env_lock();
    let dir = tempfile::tempdir().expect("tempdir");
    let config = dir.path().join("taskboard.toml");
    std::fs::write(&config, "[ui]\nscale = 1.0\n").expect("seed");
    let fixture = owned_boot(config).await;
    let app = &fixture.app;
    let workspace = &fixture.workspace;
    let board = create_board(app, "files board").await;
    app.engine
        .execute(StateCommand::BindBoard {
            board,
            remote: taskboard_domain::RemoteBoardId::files(uuid::Uuid::now_v7()),
            connector: taskboard_domain::ConnectorId::from("files-sync"),
        })
        .await
        .expect("files bind lands");

    let snapshot = taskboard_app::ui_requests::stop_board_files_sync(workspace, board)
        .await
        .expect("a files-bound board stops");
    let state = workspace.engine.shared_state().load();
    let row = state.boards.get(&board).expect("the board survives");
    assert!(row.remote.is_none(), "the binding is unbound");
    assert!(!row.deleted, "unbind is not a delete");
    let replied = snapshot
        .boards
        .iter()
        .find(|info| info.id == board.as_uuid())
        .expect("the board is in the fresh snapshot");
    assert_eq!(replied.bound_channel, None);
    assert_eq!(replied.bound_connector, None);
}

/// A stopped engine (`EngineGone`) is NOT an error at the port:
/// the file is written, the next start applies it (D7's typed
/// next-start outcome — the fronts decide how to render it).
#[tokio::test]
async fn the_port_engine_gone_reports_the_next_start_lane() {
    let _env = crate::common::taskboard_env_lock();
    let mut fixture = port_boot("[reminders]\ntick_secs = 30\n").await;
    let OwnedBootFixture {
        ref workspace,
        ref mut app,
        ..
    } = fixture;
    // Drain the engine (the D-5.3 journey's pattern): every later
    // `execute` answers `EngineGone`.
    app.system
        .send(taskboard_domain::SystemEvent::Shutdown)
        .expect("the engine subscribes");
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let probe = workspace.engine.execute(StateCommand::RequestSync).await;
            if matches!(probe, Err(taskboard_state::ExecuteError::EngineGone)) {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("the engine drained");
    let locked = std::collections::BTreeSet::new();
    let outcome = taskboard_app::ui_requests::run_settings_write(
        workspace.config_write_path.as_path(),
        &locked,
        &["reminders.tick_secs"],
        |path| taskboard_app::settings_overlay::set_reminder_tick(path, 45),
        Some(&workspace.engine),
    )
    .await
    .expect("the write still lands");
    assert_eq!(
        outcome.apply,
        taskboard_app::ui_requests::LiveApply::EngineGone,
        "the next-start lane"
    );
    let text = std::fs::read_to_string(workspace.config_write_path.as_path()).expect("file");
    assert!(text.contains("tick_secs = 45"), "the file wrote: {text}");
}

/// The no-engine lane (the one-shot CLI front): the write lands,
/// the port reports `NoEngine` — the typed `applied_live: false`.
#[tokio::test]
async fn the_port_no_engine_lane_reports_next_start() {
    let _env = crate::common::taskboard_env_lock();
    let fixture = port_boot("[reminders]\ntick_secs = 30\n").await;
    let workspace = &fixture.workspace;
    let locked = std::collections::BTreeSet::new();
    let outcome = taskboard_app::ui_requests::run_settings_write(
        workspace.config_write_path.as_path(),
        &locked,
        &["reminders.tick_secs"],
        |path| taskboard_app::settings_overlay::set_reminder_tick(path, 45),
        None,
    )
    .await
    .expect("the write lands");
    assert_eq!(
        outcome.apply,
        taskboard_app::ui_requests::LiveApply::NoEngine
    );
    assert_eq!(outcome.effective.reminders.tick_secs, 45);
}

/// A garbage minute write reaching the handler from a non-UI lane
/// client is refused typed with own-blame attribution — the gate's
/// defence in depth, never a silent clamp at any layer.
#[tokio::test]
async fn focus_gate_refusal_attributes_own_blame() {
    let _env = crate::common::taskboard_env_lock();
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("taskboard.toml");
    std::fs::write(&path, "[ui]\nscale = 1.0\n").expect("seed");
    let fixture = owned_boot(path.clone()).await;
    let workspace = &fixture.workspace;

    let (reply, reply_rx) = tokio::sync::oneshot::channel();
    taskboard_app::ui_requests::handle_settings(
        UiRequest::SetFocusWorkMins { mins: 0, reply },
        workspace,
    )
    .await;
    let outcome = reply_rx.await.expect("the reply lane is alive");
    assert!(
        matches!(
            outcome,
            Err(SettingsError::Invalid {
                field: taskboard_ui_slint::requests::Field::FocusWorkMins,
                kind: taskboard_ui_slint::requests::InvalidKind::OutOfRange
            })
        ),
        "the own-blame refusal is typed: {outcome:?}"
    );
    assert!(
        !std::fs::read_to_string(&path)
            .expect("read")
            .contains("work_mins"),
        "the refused write left the file untouched"
    );
}
