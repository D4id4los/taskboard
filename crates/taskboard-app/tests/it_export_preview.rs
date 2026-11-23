// SPDX-License-Identifier: MIT OR Apache-2.0

//! The export preview/clipboard executors (the D2 preview plan W-5):
//! one generator, two sinks (D-1) — the preview payload equals the
//! engine export payload byte-for-byte on the same spec (the trap-c
//! seam test), the presentation cap rides the typed `truncated` fact
//! (D-2; no size-ceiling assertions), and the clipboard copy
//! regenerates app-side over the seam (D-3; no system clipboard is
//! ever touched). Assertions are typed outcomes; no text oracles.

#[path = "common/mod.rs"]
mod common;

#[path = "common/export_fixture.rs"]
mod export_fixture;

use std::sync::Arc;

use common::FakeCredentialStore;
use taskboard_app::config::AppConfig;
use taskboard_app::secrets::CredentialStore;
use taskboard_app::ui_requests::{ExportWorkspace, spawn_ui_request_forwarder, ui_request_channel};
use taskboard_ui_slint::requests::{
    ExportClipboardOutcome, ExportFamily, ExportFileFormat, ExportPreviewOutcome, ExportRefusal,
    ExportSpec, UiRequest,
};

/// W-5 (US-1/AC-2, the trap-c seam test): the preview payload equals
/// the engine export payload BYTE-FOR-BYTE on the same spec — one
/// generator, two sinks; no file is written by the preview.
#[tokio::test]
async fn preview_payload_equals_the_engine_export_payload() {
    let home = tempfile::tempdir().expect("export home");
    let mut config = AppConfig::default();
    config.export.pdf_dir = Some(home.path().to_path_buf());
    let store = Arc::new(FakeCredentialStore::default());
    let (app, write_path) =
        export_fixture::booted(config, Arc::clone(&store) as Arc<dyn CredentialStore>).await;
    let ws = export_fixture::workspace(&app, write_path, store);
    let (tx, rx) = ui_request_channel();
    let _forwarder = spawn_ui_request_forwarder(rx, app.engine.shared_state(), ws);

    let spec = ExportSpec {
        what: ExportFamily::Tracking,
        format: ExportFileFormat::Csv,
        ..ExportSpec::default()
    };

    // The preview sink.
    let (reply, reply_rx) = tokio::sync::oneshot::channel();
    tx.send(UiRequest::ExportPreview {
        spec: spec.clone(),
        reply,
    })
    .await
    .expect("the lane accepts the preview");
    let ExportPreviewOutcome::Payload {
        text,
        format,
        truncated,
    } = reply_rx.await.expect("the drain answers")
    else {
        panic!("the preview generates, got a refusal");
    };
    assert_eq!(format, ExportFileFormat::Csv);
    assert!(!truncated, "a fresh core's payload is under the cap");

    // The file sink: the Run over the same spec writes the file; the
    // bytes must equal the preview's payload (the identity law).
    let (reply, reply_rx) = tokio::sync::oneshot::channel();
    tx.send(UiRequest::ExportRun {
        spec: ExportSpec {
            out: Some(home.path().join("engine.csv")),
            ..spec
        },
        reply,
    })
    .await
    .expect("the lane accepts the run");
    let outcome = reply_rx.await.expect("the drain answers");
    let taskboard_ui_slint::requests::ExportRunOutcome::Written { path } = outcome else {
        panic!("the run writes, got {outcome:?}");
    };
    let written = String::from_utf8(std::fs::read(&path).expect("the file exists"))
        .expect("the csv payload is text");
    assert_eq!(
        text, written,
        "the preview payload equals the engine export payload byte-for-byte (D-1)"
    );
    assert!(
        !home.path().join("tracking").exists(),
        "the preview wrote no file of its own (D-4)"
    );
}

/// W-5 (US-1/AC-3): an over-cap payload rides the typed `truncated`
/// fact (the cap is a presentation budget — the fold asserts the
/// flag, never sizes; no ceiling on success). The payload is engine
/// generated over a seeded tracking log — the same generator the
/// preview verb drives.
#[tokio::test]
async fn us1_s3_over_cap_payload_folds_the_truncated_fact() {
    use taskboard_domain::StateCommand;
    use taskboard_domain::command::CommandOutcome;

    let home = tempfile::tempdir().expect("export home");
    let mut config = AppConfig::default();
    config.export.pdf_dir = Some(home.path().to_path_buf());
    let store = Arc::new(FakeCredentialStore::default());
    let (app, write_path) =
        export_fixture::booted(config, Arc::clone(&store) as Arc<dyn CredentialStore>).await;
    let ws = export_fixture::workspace(&app, write_path, store);

    // Seed a tracking log over the presentation cap: per entry the
    // csv/json row carries the task title, so a long-enough seed
    // title plus enough entries crosses the cap.
    let taskboard_domain::command::CommandOutcome::CreatedBoard(board) = app
        .engine
        .execute(StateCommand::CreateBoard {
            title: "cap seed".into(),
            color: taskboard_domain::Color::new("2a9d8f"),
        })
        .await
        .expect("accepted")
    else {
        panic!("wrong receipt");
    };
    let CommandOutcome::CreatedStack(stack) = app
        .engine
        .execute(StateCommand::CreateStack {
            board,
            title: "column".into(),
            order: 1,
        })
        .await
        .expect("accepted")
    else {
        panic!("wrong receipt");
    };
    // Fewer, fatter rows: a long seed title keeps the crossing
    // deterministic while the engine-execute count (each a sqlite
    // transaction) stays modest.
    let seed_title = "the quick brown fox jumps over the lazy dog padding ".repeat(4);
    for i in 0..600 {
        let CommandOutcome::CreatedTask(task) = app
            .engine
            .execute(StateCommand::CreateTask {
                title: format!("{seed_title}{i}"),
                stack,
                order: i,
            })
            .await
            .expect("accepted")
        else {
            panic!("wrong receipt");
        };
        let _ = app
            .engine
            .execute(StateCommand::StartTracking {
                task,
                kind: taskboard_domain::tracking::TimeEntryKind::Work,
            })
            .await
            .expect("accepted");
        let _ = app
            .engine
            .execute(StateCommand::StopTracking)
            .await
            .expect("accepted");
    }

    let state = app.engine.shared_state().load_full();
    let spec = ExportSpec {
        what: ExportFamily::Tracking,
        format: ExportFileFormat::Json,
        ..ExportSpec::default()
    };
    let outcome = taskboard_app::ui_requests::run_preview(&state, &ws.export, &spec);
    let ExportPreviewOutcome::Payload {
        truncated, text, ..
    } = outcome
    else {
        panic!("the preview generates");
    };
    assert!(truncated, "the over-cap payload carries the typed fact");
    assert!(
        text.len() <= taskboard_app::ui_requests::PREVIEW_CAP_BYTES,
        "the pane's text is the capped head (the presentation budget)"
    );
}

/// W-5 (US-2/AC-2/AC-3): the copy REGENERATES app-side (never the
/// pane's text) and reports the typed outcome; an unavailable
/// clipboard refuses typed (the seam fake — no system clipboard).
#[tokio::test]
async fn us2_s2_copy_regenerates_and_reports_typed_outcome() {
    let home = tempfile::tempdir().expect("export home");
    let mut config = AppConfig::default();
    config.export.pdf_dir = Some(home.path().to_path_buf());
    let store = Arc::new(FakeCredentialStore::default());
    let (app, _write_path) =
        export_fixture::booted(config, Arc::clone(&store) as Arc<dyn CredentialStore>).await;
    let spec = ExportSpec {
        what: ExportFamily::Tracking,
        format: ExportFileFormat::Csv,
        ..ExportSpec::default()
    };

    // The unavailable clipboard: the typed environment refusal.
    let unavailable = ExportWorkspace {
        clipboard_write: Arc::new(|_| Err(())),
        ..ExportWorkspace::with_real_clipboard(Arc::new(app.config.clone()), false)
    };
    let state = app.engine.shared_state().load_full();
    assert_eq!(
        taskboard_app::ui_requests::run_clipboard_copy(&state, &unavailable, &spec),
        ExportClipboardOutcome::Refused(ExportRefusal::ClipboardUnavailable),
        "the unavailable clipboard refuses typed (D-3)"
    );

    // The available clipboard: the copied-byte fact rides the
    // regenerated payload (the sink records what it received).
    let received = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let sink = Arc::clone(&received);
    let available = ExportWorkspace {
        clipboard_write: Arc::new(move |text| {
            sink.lock().expect("lock").push(text.to_owned());
            Ok(())
        }),
        ..ExportWorkspace::with_real_clipboard(Arc::new(app.config.clone()), false)
    };
    let outcome = taskboard_app::ui_requests::run_clipboard_copy(&state, &available, &spec);
    let ExportClipboardOutcome::Copied { bytes } = outcome else {
        panic!("the copy succeeds over the available seam");
    };
    let copied = received.lock().expect("lock");
    assert_eq!(
        copied.len(),
        1,
        "the copy regenerated exactly one payload (D-3's law)"
    );
    assert_eq!(
        bytes,
        copied[0].len() as u64,
        "the copied-byte fact is the payload's length"
    );

    // The pdf family is a typed refusal on both verbs (D-5's
    // defense-in-depth arm).
    let pdf_spec = ExportSpec {
        what: ExportFamily::Pdf,
        ..ExportSpec::default()
    };
    assert_eq!(
        taskboard_app::ui_requests::run_preview(&state, &available, &pdf_spec),
        ExportPreviewOutcome::Refused(ExportRefusal::OptionsNotApplicable),
        "the pdf family refuses the preview typed"
    );
}
