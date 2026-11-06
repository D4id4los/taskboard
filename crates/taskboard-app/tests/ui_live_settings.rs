// SPDX-License-Identifier: AGPL-3.0-only
//! The live-settings write path through the public forwarder (GUI
//! review round 5, chunk 5, D-5.3): the env-lock guard and the
//! effective-config mint. Both journeys need a `TASKBOARD_*` override
//! standing in the process environment, which only this tree's
//! `common` helpers may set (the lib forbids unsafe).

mod common;

use std::sync::Arc;

use common::{EnvOverride, env_lock, forbidden_store, standalone_config, tempdir};
use taskboard_app::bootstrap::bootstrap;
use taskboard_app::notifications::LogSink;
use taskboard_app::ui_requests::{ConnectionWorkspace, spawn_ui_request_forwarder};
use taskboard_ui_slint::requests::{Field, SettingsError, UiRequest};

/// The booted standalone core behind a forwarder; the returned
/// `TempDir` owns the db for the test's lifetime.
async fn forwarder(
    config_path: std::path::PathBuf,
) -> (
    tokio::sync::mpsc::Sender<UiRequest>,
    taskboard_state::EngineHandle,
    taskboard_app::bootstrap::App,
    tempfile::TempDir,
) {
    let dir = tempdir();
    let mut config = standalone_config(dir.path(), "live.db");
    config.sync.backend = Some(taskboard_app::config::SyncBackend::Standalone);
    config.validate().expect("validates");
    let app = bootstrap(config, forbidden_store(), Arc::new(LogSink))
        .await
        .expect("standalone boot");
    let workspace = ConnectionWorkspace {
        config_write_path: config_path,
        store: forbidden_store(),
        control: app.connector_control(),
        engine: app.engine.clone(),
    };
    let (tx, rx) = tokio::sync::mpsc::channel(8);
    let _join = spawn_ui_request_forwarder(rx, app.engine.shared_state(), workspace);
    (tx, app.engine.clone(), app, dir)
}

/// An env-pinned field refuses typed before the file is touched: the
/// file stays byte-identical and the engine's published settings do
/// not move.
#[tokio::test]
async fn an_env_locked_field_is_refused_and_nothing_moves() {
    let _env = env_lock();
    let _pin = EnvOverride::set("TASKBOARD_REMINDERS__TICK_SECS", "77");
    let dir = tempdir();
    let path = dir.path().join("taskboard.toml");
    let seed = "[reminders]\ntick_secs = 30\n";
    std::fs::write(&path, seed).expect("seed");
    let (requests, engine, _app, _db) = forwarder(path.clone()).await;
    let before = engine.shared_state().load().settings.clone();

    let (reply, reply_rx) = tokio::sync::oneshot::channel();
    requests
        .send(UiRequest::SetReminderTick { secs: 45, reply })
        .await
        .expect("forwarder alive");
    let outcome = reply_rx.await.expect("reply lane alive");
    assert!(
        matches!(
            outcome,
            Err(SettingsError::EnvLocked {
                field: Field::TickSecs
            })
        ),
        "{outcome:?}"
    );
    assert_eq!(
        std::fs::read_to_string(&path).expect("read back"),
        seed,
        "the file is byte-identical"
    );
    assert_eq!(
        engine.shared_state().load().settings,
        before,
        "the engine's settings did not move"
    );
}

/// A write to an unlocked field mints the engine's snapshot from the
/// *effective* config: the env-pinned sibling keeps its env value in
/// the engine (never clobbered by the file's number), and the reply
/// echoes the engine — not the file.
#[tokio::test]
async fn a_write_beside_an_env_pin_keeps_the_pin_in_the_engine() {
    let _env = env_lock();
    let _pin = EnvOverride::set("TASKBOARD_REMINDERS__TICK_SECS", "77");
    let dir = tempdir();
    let path = dir.path().join("taskboard.toml");
    std::fs::write(
        &path,
        "[reminders]\ntick_secs = 30\n[ui]\ndue_soon_days = 1\n",
    )
    .expect("seed");
    let (requests, engine, _app, _db) = forwarder(path.clone()).await;

    let (reply, reply_rx) = tokio::sync::oneshot::channel();
    requests
        .send(UiRequest::SetDueSoonDays { days: 4, reply })
        .await
        .expect("forwarder alive");
    let snapshot = reply_rx
        .await
        .expect("reply lane alive")
        .expect("the unlocked write lands");
    assert_eq!(snapshot.due_soon_days, 4);
    assert_eq!(
        snapshot.reminder_tick_secs, 77,
        "the reply echoes the engine's effective value, not the file's 30"
    );
    let settings = engine.shared_state().load().settings.clone();
    assert_eq!(settings.due_soon_days.get(), 4);
    assert_eq!(
        settings.tick_secs.get(),
        77,
        "the env pin survived the write"
    );
}
