// SPDX-License-Identifier: AGPL-3.0-only

//! The export run's drain executor (the J2 export plan W-6): the
//! `UiRequest::ExportRun` arm over the real request forwarder on a
//! booted standalone core — the SAME export engine the CLI verb
//! drives (one engine, two frontends), the GUI's file write (D-3),
//! and the typed oneshot outcome (D-5). Assertions are typed
//! outcomes and structural file facts; no text oracles.

#[path = "common/mod.rs"]
mod common;

use std::path::PathBuf;
use std::sync::Arc;

use common::FakeCredentialStore;
use taskboard_app::bootstrap::{self, App};
use taskboard_app::config::AppConfig;
use taskboard_app::notifications::LogSink;
use taskboard_app::secrets::CredentialStore;
use taskboard_app::ui_requests::{
    ConnectionWorkspace, ExportWorkspace, spawn_ui_request_forwarder, ui_request_channel,
};
use taskboard_ui_slint::requests::{
    ExportFamily, ExportFileFormat, ExportRefusal, ExportRunOutcome, ExportSpec, UiRequest,
};

/// A booted standalone core plus the request lane's halves over it,
/// with the export workspace pointed at the given export home (the
/// viewer open permitted — the desktop lifecycle's posture; the pdf
/// tests pass destinations under a tempdir, and an open attempt
/// degrades to the best-effort no-op it already is).
async fn booted(config: AppConfig, store: Arc<dyn CredentialStore>) -> (App, PathBuf) {
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

fn workspace(
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
        // The suite never drives the status panel; the source is the
        // in-session projection over the fixture's handles.
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
        // The suite never drives the remediation lane; the workspace
        // is inert-but-typed (the executor's effects are seams).
        remediation: taskboard_app::status_remediation::RemediationWorkspace {
            db_path: None,
            may_control_processes: false,
            effects: taskboard_app::status_remediation::RemediationEffects::real(None),
        },
    }
}

/// W-6 (US-2/AC-2): a tracking csv run with the D-3 default
/// destination — the reply carries the Written fact and the file
/// exists under the export home with the csv extension.
#[tokio::test]
async fn export_run_reply_carries_typed_outcome() {
    let home = tempfile::tempdir().expect("export home");
    let mut config = AppConfig::default();
    config.export.pdf_dir = Some(home.path().to_path_buf());
    let store = Arc::new(FakeCredentialStore::default());
    let (app, write_path) = booted(config, Arc::clone(&store) as Arc<dyn CredentialStore>).await;
    let ws = workspace(&app, write_path, store);
    let (tx, rx) = ui_request_channel();
    let _forwarder = spawn_ui_request_forwarder(rx, app.engine.shared_state(), ws);

    let (reply, reply_rx) = tokio::sync::oneshot::channel();
    tx.send(UiRequest::ExportRun {
        spec: ExportSpec {
            what: ExportFamily::Tracking,
            format: ExportFileFormat::Csv,
            ..ExportSpec::default()
        },
        reply,
    })
    .await
    .expect("the lane accepts the run");

    let outcome = reply_rx.await.expect("the drain answers");
    let ExportRunOutcome::Written { path } = outcome else {
        panic!("the tracking run writes, got {outcome:?}");
    };
    // The D-3 default lands under the export home: the configured
    // `[export] pdf_dir` is the family's home verbatim (the config
    // key's resolution is the identity here).
    assert_eq!(
        path.parent(),
        Some(home.path()),
        "the D-3 default lands under the export home"
    );
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_owned();
    let extension = path.extension().and_then(|ext| ext.to_str());
    assert_eq!(extension, Some("csv"), "the csv leg's default extension");
    assert!(
        name.starts_with("tracking-"),
        "the dated default file name carries the family: {name}"
    );
    let written = std::fs::read(&path).expect("the file exists");
    assert!(
        !written.is_empty(),
        "the csv payload writes (a header-only export is still success)"
    );
}

/// W-6 (US-2/AC-3): the typed refusal — a task selector matching
/// nothing answers `BadTaskFilter`, the dialog's kind (no text).
#[tokio::test]
async fn export_run_refuses_typed_on_a_bad_selector() {
    let home = tempfile::tempdir().expect("export home");
    let mut config = AppConfig::default();
    config.export.pdf_dir = Some(home.path().to_path_buf());
    let store = Arc::new(FakeCredentialStore::default());
    let (app, write_path) = booted(config, Arc::clone(&store) as Arc<dyn CredentialStore>).await;
    let ws = workspace(&app, write_path, store);
    let (tx, rx) = ui_request_channel();
    let _forwarder = spawn_ui_request_forwarder(rx, app.engine.shared_state(), ws);

    let (reply, reply_rx) = tokio::sync::oneshot::channel();
    tx.send(UiRequest::ExportRun {
        spec: ExportSpec {
            what: ExportFamily::Tracking,
            format: ExportFileFormat::Csv,
            task: Some("no-such-task-selector".into()),
            ..ExportSpec::default()
        },
        reply,
    })
    .await
    .expect("the lane accepts the run");

    let outcome = reply_rx.await.expect("the drain answers");
    assert_eq!(
        outcome,
        ExportRunOutcome::Refused(ExportRefusal::BadTaskFilter),
        "the unknown selector refuses typed (the dialog's kind)"
    );
}

/// W-6 (US-3/AC-1): the pdf leg's run on a feature build — the
/// explicit destination override writes the file, the reply carries
/// the written path. A default build compiles this test out (the leg
/// is the pdf-export feature's).
#[cfg(feature = "pdf-export")]
#[tokio::test]
async fn export_run_writes_the_pdf_override() {
    let home = tempfile::tempdir().expect("export home");
    let destination = home.path().join("override.pdf");
    let mut config = AppConfig::default();
    config.export.pdf_dir = Some(home.path().to_path_buf());
    let store = Arc::new(FakeCredentialStore::default());
    let (app, write_path) = booted(config, Arc::clone(&store) as Arc<dyn CredentialStore>).await;
    let ws = workspace(&app, write_path, store);
    let (tx, rx) = ui_request_channel();
    let _forwarder = spawn_ui_request_forwarder(rx, app.engine.shared_state(), ws);

    let (reply, reply_rx) = tokio::sync::oneshot::channel();
    tx.send(UiRequest::ExportRun {
        spec: ExportSpec {
            what: ExportFamily::Pdf,
            out: Some(destination.clone()),
            open_after: false,
            ..ExportSpec::default()
        },
        reply,
    })
    .await
    .expect("the lane accepts the run");

    let outcome = reply_rx.await.expect("the drain answers");
    let ExportRunOutcome::Written { path } = outcome else {
        panic!("the pdf run writes, got {outcome:?}");
    };
    assert_eq!(path, destination, "the override path is the written path");
    let written = std::fs::read(&path).expect("the file exists");
    assert!(
        written.starts_with(b"%PDF"),
        "the pdf leg writes a pdf (the file fact's content)"
    );
}
