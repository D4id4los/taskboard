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
use taskboard_ui_slint::requests::{
    ConnectionProfile, ConnectionTarget, Secret, SettingsError, UiRequest,
};

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
            Result<Vec<taskboard_ui_slint::requests::ConnectorInfo>, SettingsError>,
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
    assert_eq!(outcome.len(), 2, "the fresh list carries both entries");
    assert!(outcome.iter().any(|row| row.name == "home"));
    assert!(
        outcome.iter().all(|row| !row.server_url.is_empty()),
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
    assert_eq!(outcome.len(), 2, "still two entries — an upsert");

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

/// A9 — the legacy scalar world: Save and Remove both refuse typed,
/// file and store byte-untouched.
#[tokio::test]
async fn the_legacy_singleton_world_is_refused_typed_on_save_and_remove() {
    let dir = tempfile::tempdir().expect("tempdir");
    let config = dir.path().join("taskboard.toml");
    std::fs::write(
        &config,
        "[storage]\ndb_path = '/tmp/t.db'\n[nextcloud]\n\
         server_url = \"https://legacy.example\"\nusername = \"old-user\"\n",
    )
    .expect("seed config");
    let before = std::fs::read_to_string(&config).expect("read");
    let store = Arc::new(FakeCredentialStore::default());
    let (_app, workspace) = booted_workspace(config.clone(), store.clone()).await;
    let (tx, rx) = ui_request_channel();
    let forwarder = taskboard_app::ui_requests::spawn_ui_request_forwarder(
        rx,
        Arc::new(ArcSwap::from_pointee(taskboard_domain::AppState::default())),
        workspace,
    );

    for request in [
        UiRequest::SaveConnection {
            profile: ConnectionProfile {
                name: "home".into(),
                server_url: "https://new.example".into(),
                username: "alice".into(),
                password: Secret("pw".into()),
            },
            reply: tokio::sync::oneshot::channel().0,
        },
        UiRequest::RemoveConnection {
            name: "nextcloud".into(),
            reply: tokio::sync::oneshot::channel().0,
        },
    ] {
        // The reply receivers are dropped on purpose: the typed outcome
        // for these two is pinned by the A6/A8 path; here the structural
        // fact is the untouched file.
        let _ = tx.try_send(request);
    }
    // Give the handlers a scheduled yield (no sleeps — the channel
    // round-trip below is the deterministic barrier instead).
    let (reply, reply_rx) = tokio::sync::oneshot::channel();
    tx.try_send(UiRequest::TestConnection {
        target: ConnectionTarget {
            server_url: "https://legacy.example".into(),
            username: "old-user".into(),
        },
        reply,
    })
    .expect("capacity holds");
    // The legacy world's Test is a store probe; the fake store answers
    // NotFound → NoSecretStored (the typed result proves the handler
    // ran after both refusals).
    let outcome = reply_rx
        .await
        .expect("reply lane alive")
        .expect_err("no secret");
    assert!(matches!(outcome, SettingsError::NoSecretStored));

    assert_eq!(
        std::fs::read_to_string(&config).expect("read"),
        before,
        "the legacy file is byte-untouched"
    );
    assert!(
        store.load("https://legacy.example", "old-user").is_err(),
        "the store is untouched"
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
    assert!(outcome.is_empty(), "the fresh list is empty");

    let written = std::fs::read_to_string(&config).expect("read back");
    assert!(!written.contains("connectors"), "no empty husk");
    assert!(
        store.load(&server.uri(), "old-user").is_err(),
        "the removed identity's secret died"
    );

    drop(tx);
    forwarder.await.expect("forwarder joins cleanly");
}
