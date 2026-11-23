// SPDX-License-Identifier: MIT OR Apache-2.0

//! The shared export-suite fixture (the `it_exports` / `it_export_preview`
//! twin): a booted standalone core plus the request lane's halves over
//! it, with the export workspace pointed at the given export home (the
//! viewer open permitted — the desktop lifecycle's posture; the pdf
//! tests pass destinations under a tempdir, and an open attempt
//! degrades to the best-effort no-op it already is).
//!
//! Included per test binary via `#[path = "common/export_fixture.rs"]`;
//! the bodies live here once so the engine identity contract's two
//! suites boot identically.

use std::path::PathBuf;
use std::sync::Arc;

use taskboard_app::bootstrap::{self, App};
use taskboard_app::config::AppConfig;
use taskboard_app::notifications::LogSink;
use taskboard_app::secrets::CredentialStore;
use taskboard_app::ui_requests::{ConnectionWorkspace, ExportWorkspace};

/// A booted standalone core plus its config write path.
pub async fn booted(config: AppConfig, store: Arc<dyn CredentialStore>) -> (App, PathBuf) {
    let mut config = config;
    let dir = tempfile::tempdir().expect("tempdir");
    config.storage.db_path = Some(dir.path().join("taskboard.db"));
    if config.sync.backend.is_none() {
        config.sync.backend = Some(taskboard_app::config::SyncBackend::Standalone);
    }
    config.validate().expect("config valid");
    let app = bootstrap::bootstrap_in(
        config,
        store,
        Arc::new(LogSink),
        taskboard_app::bootstrap::BootIdentity::At(dir.path().to_owned()),
    )
    .await
    .expect("standalone boot");
    let write_path = dir.path().join("taskboard.toml");
    let _ = std::fs::write(&write_path, "");
    (app, write_path)
}

/// The connection workspace over the fixture's handles: the in-session
/// status projection, and the inert-but-typed remediation workspace
/// (the executor's effects are seams — neither suite drives the lane).
pub fn workspace(
    app: &App,
    config_write_path: PathBuf,
    store: Arc<dyn CredentialStore>,
) -> ConnectionWorkspace {
    ConnectionWorkspace {
        config_write_path,
        store,
        control: app.connector_control(),
        engine: app.engine.clone(),
        device_json: None,
        connector_owners: app.connector_owners.clone(),
        export: ExportWorkspace::with_real_clipboard(Arc::new(app.config.clone()), false),
        status: Box::new({
            let engine = app.engine.clone();
            let config = app.config.clone();
            let control = app.connector_control();
            let files = app.files_sync_status.clone();
            move || {
                taskboard_app::commands::status::StatusView::in_session_parts(
                    &engine,
                    &config,
                    control.connector_attached(),
                    files.as_ref(),
                )
            }
        }),
        remediation: taskboard_app::status_remediation::RemediationWorkspace::default(),
    }
}
