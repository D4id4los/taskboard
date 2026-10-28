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

mod common;

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
fn seed_array_config(dir: &std::path::Path, server: &str) -> PathBuf {
    let path = dir.join("taskboard.toml");
    std::fs::write(
        &path,
        format!(
            "[storage]\ndb_path = '{}/taskboard.db'\n\
             [[sync.connectors]]\nname = \"work\"\nbackend = \"nextcloud\"\n\
             server_url = \"{server}\"\nusername = \"old-user\"\n",
            dir.display()
        ),
    )
    .expect("seed config");
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
    let app = bootstrap::bootstrap(
        config,
        Arc::new(FakeCredentialStore::default()),
        Arc::new(LogSink),
    )
    .await
    .expect("standalone boot");
    let workspace = ConnectionWorkspace {
        config_write_path,
        store,
        control: app.connector_control(),
        engine: app.engine.clone(),
    };
    (app, workspace)
}

/// A detached-receiver variant of `send`: the request goes out and the
/// reply half is dropped before the handler answers (A7).
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
    assert!(!written.contains("home"), "nothing persisted");
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

/// M10 — the Remove choreography on a legacy file migrates first,
/// then removes the sole entry; the no-empty-husk rule leaves a clean
/// array-less file.
#[tokio::test]
async fn remove_on_a_legacy_file_migrates_then_removes_the_sole_entry() {
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

/// The `SetKeymapPreset` flow: the preset persists, the reply carries the
/// rebuilt keymap (vi resolves the colon in the vi-normal context).
#[tokio::test]
async fn the_preset_flow_persists_and_replies_with_the_rebuilt_keymap() {
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
/// the attach path's title-adopt input.
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
            remote: RemoteBoardId(remote),
            connector: taskboard_domain::ConnectorId::from(connector.to_owned()),
        })
        .await
        .expect("the fixture bind is accepted");
}

fn send_attach(
    tx: &tokio::sync::mpsc::Sender<UiRequest>,
    board: taskboard_domain::BoardId,
    connector: &str,
) -> tokio::sync::oneshot::Receiver<
    Result<taskboard_ui_slint::requests::ConnectionsSnapshot, SettingsError>,
> {
    let (reply, reply_rx) = tokio::sync::oneshot::channel();
    tx.try_send(UiRequest::AttachConnector {
        board,
        connector: connector.to_owned(),
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

/// A2 — the attach happy path: title-adopt over a live listing, the
/// engine binding lands, the actor is attached, the config file is
/// untouched (attach is a live-binding op, never a config op).
#[tokio::test]
async fn attach_adopts_the_titled_remote_and_binds_through_the_engine() {
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
    // The remote fixture's board is titled "board" (common::boards_json).
    let board = create_board(&app, "board").await;

    let outcome = send_attach(&tx, board, "work")
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
    assert_eq!(remote, RemoteBoardId(common::BOARD));

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

/// A3 — no live remote title matches: `NoRemoteMatch`, nothing
/// attached, no binding (verify-before-attach order proof).
#[tokio::test]
async fn attach_without_a_matching_remote_title_attaches_nothing() {
    let server = mount_boards(&[(7, "unrelated")]).await;
    let dir = tempfile::tempdir().expect("tempdir");
    let config = seed_array_config(dir.path(), &server.uri());
    let store = Arc::new(FakeCredentialStore::with(&server.uri(), "old-user", "pw"));
    let (app, workspace) = booted_workspace(config.clone(), store.clone()).await;
    let (tx, rx) = ui_request_channel();
    let forwarder = taskboard_app::ui_requests::spawn_ui_request_forwarder(
        rx,
        app.engine.shared_state(),
        workspace,
    );
    let board = create_board(&app, "home").await;

    let outcome = send_attach(&tx, board, "work")
        .await
        .expect("reply lane alive")
        .expect_err("no matching title");
    assert!(
        matches!(outcome, SettingsError::NoRemoteMatch),
        "typed adoption miss, got {outcome:?}"
    );
    assert!(
        app.connector_control()
            .connector_commands(&taskboard_domain::ConnectorId::from("work"))
            .is_none(),
        "nothing was attached"
    );
    assert!(bound_connector_of(&app, board).is_none(), "no binding");

    drop(tx);
    forwarder.await.expect("forwarder joins cleanly");
}

/// A4 — a board already bound to a *different* connector refuses
/// pre-IO: `BindRefused`, and the unreachable target server proves no
/// listing was attempted.
#[tokio::test]
async fn attach_to_a_board_bound_elsewhere_refuses_before_any_io() {
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

    let outcome = send_attach(&tx, board, "b")
        .await
        .expect("reply lane alive")
        .expect_err("bound elsewhere");
    assert!(
        matches!(outcome, SettingsError::BindRefused),
        "typed bind refusal, got {outcome:?}"
    );
    let (connector, remote) = bound_connector_of(&app, board).expect("binding intact");
    assert_eq!(connector.as_str(), "a");
    assert_eq!(remote, RemoteBoardId(1), "the old binding survived");

    drop(tx);
    forwarder.await.expect("forwarder joins cleanly");
}

/// A5 — re-attaching a board already bound to this connector is the
/// idempotent path: no listing (no route is mounted — a listing would
/// surface as unreachable), the binding survives, `Ok`.
#[tokio::test]
async fn attach_reissue_on_the_same_connector_skips_the_listing() {
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

    let outcome = send_attach(&tx, board, "work")
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
    assert_eq!(remote, RemoteBoardId(7), "the binding is unchanged");

    drop(tx);
    forwarder.await.expect("forwarder joins cleanly");
}

/// A6 — the attach refusals: a missing stored secret is typed; a 401
/// listing is a typed verify refusal; nothing persists either way.
#[tokio::test]
async fn attach_refuses_typed_on_missing_secret_and_rejected_listing() {
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

    let outcome = send_attach(&tx, board, "work")
        .await
        .expect("reply lane alive")
        .expect_err("no secret stored");
    assert!(matches!(outcome, SettingsError::NoSecretStored));

    // With the secret present but the server refusing: typed verify.
    store
        .store(
            &server.uri(),
            "old-user",
            taskboard_app::secrets::AppPassword::new("pw"),
        )
        .expect("seed");
    let rejecting = mount_auth_reject(401).await;
    let path2 = dir.path().join("second.toml");
    std::fs::write(
        &path2,
        format!(
            "[storage]\ndb_path = '{}/t.db'\n\
             [[sync.connectors]]\nname = \"work\"\nbackend = \"nextcloud\"\n\
             server_url = \"{}\"\nusername = \"old-user\"\n",
            dir.path().display(),
            rejecting.uri()
        ),
    )
    .expect("seed");
    store
        .store(
            &rejecting.uri(),
            "old-user",
            taskboard_app::secrets::AppPassword::new("pw"),
        )
        .expect("seed");
    let (app2, workspace2) = booted_workspace(path2.clone(), store.clone()).await;
    // The second core gets its own board: the attach path reads the
    // board row from *its* engine.
    let board2 = create_board(&app2, "home").await;
    let (tx2, rx2) = ui_request_channel();
    let forwarder2 = taskboard_app::ui_requests::spawn_ui_request_forwarder(
        rx2,
        app2.engine.shared_state(),
        workspace2,
    );
    let outcome = send_attach(&tx2, board2, "work")
        .await
        .expect("reply lane alive")
        .expect_err("401");
    assert!(
        matches!(outcome, SettingsError::VerifyRejected),
        "typed credential refusal, got {outcome:?}"
    );

    assert_eq!(
        std::fs::read_to_string(&config).expect("read"),
        before,
        "nothing persisted"
    );

    drop(tx);
    drop(tx2);
    forwarder.await.expect("forwarder joins cleanly");
    forwarder2.await.expect("forwarder 2 joins cleanly");
}

/// A7 — the detach choreography: the binding leaves the engine state,
/// and the connector's actor stops only when no other live board
/// remains bound to it (the recorded multi-board guard).
#[tokio::test]
async fn detach_unbinds_and_stops_the_actor_only_when_the_last_binding_leaves() {
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
    assert_eq!(remote, RemoteBoardId(8));
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
    assert_eq!(remote, RemoteBoardId(7));

    drop(tx);
    forwarder.await.expect("forwarder joins cleanly");
}

/// A10b — the engine-command mapping table (plan §4.5's new arms):
/// stale board → `Gone`; attach refusing because the name is in use
/// maps through the existing `AttachBusy` arm (re-pinned).
#[tokio::test]
async fn attach_on_a_stale_board_id_is_gone_typed() {
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
    let outcome = send_attach(&tx, ghost, "work")
        .await
        .expect("reply lane alive")
        .expect_err("stale board id");
    assert!(matches!(outcome, SettingsError::Gone), "got {outcome:?}");

    drop(tx);
    forwarder.await.expect("forwarder joins cleanly");
}
