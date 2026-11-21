// SPDX-License-Identifier: AGPL-3.0-only

//! The status-surface integration leg (OP3, the accepted journey plan
//! `2026-11-21-parity-restore-journeys-status-surface.md` §6's W-4/W-5
//! rows): the #1111 CLI draining line's committed-artifact pins (US-3)
//! and the `StatusQuery` reply's 1:1 `StatusView` mirror (W-4). The
//! GUI-leg journeys live in
//! `crates/taskboard-ui-slint/tests/journeys/status_surface_journeys.rs`.

use chrono::{TimeZone, Utc};
use taskboard_app::commands::status::{StatusView, render_text_at};

/// A bare no-daemon view with only the named diagnostics set — the
/// struct-literal base for the text-rendering pins (the in-file
/// `bare` precedent, `commands/status.rs`).
fn no_daemon_view(holder_pid: Option<u32>, draining: Option<bool>) -> StatusView {
    StatusView {
        daemon: false,
        version: None,
        backend: None,
        pid: None,
        sync: None,
        holder_pid,
        connector_attached: None,
        connectors: None,
        boards: None,
        engine: None,
        files_sync: None,
        draining,
    }
}

/// The frozen render clock shared by the committed-artifact pins (D-4):
/// one arbitrary instant — the artifact text embeds no clock-derived
/// text on these views (no files-sync block), and the pin freezes the
/// instant regardless so a future clock-reading clause cannot silently
/// unpin the line.
fn frozen_now() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 21, 12, 0, 0)
        .single()
        .expect("frozen instant")
}

/// US-3/S1 (OP3 W-5, #1111, journey plan US-3/AC-1 with the `us3_s1`
/// step token) — the draining line, pinned byte-for-byte with the
/// render clock frozen (D-4): `daemon == false ∧ draining == Some(true)`
/// renders "daemon draining" with the artifact-and-lock fact plus the
/// lock holder pid, instead of the plain "no daemon" line.
#[test]
fn us3_s1_draining_view_renders_the_draining_line_committed_artifact() {
    let view = no_daemon_view(Some(4242), Some(true));
    assert_eq!(
        render_text_at(&view, frozen_now()),
        "daemon draining: the sync artifact sits beside a held lock (lock holder pid 4242)"
    );

    // The holder pid is the diagnostic sidecar's best-effort fact: a
    // draining view without one still renders the draining posture.
    let pidless = no_daemon_view(None, Some(true));
    assert_eq!(
        render_text_at(&pidless, frozen_now()),
        "daemon draining: the sync artifact sits beside a held lock"
    );
}

/// US-3/S2 (OP3 W-5, journey plan US-3/AC-2) — the additive law: a
/// `draining: None` no-daemon view renders byte-identically to the
/// pre-#1111 rendering (the plain no-daemon line, with and without the
/// diagnostic holder pid). `Some(false)` never mints (B6's law), and
/// the renderer treats it exactly as absent — the same typed rule.
#[test]
fn us3_s2_draining_none_no_daemon_rendering_is_byte_identical() {
    let frozen = frozen_now();
    assert_eq!(
        render_text_at(&no_daemon_view(None, None), frozen),
        "no daemon serving this database"
    );
    assert_eq!(
        render_text_at(&no_daemon_view(Some(7), None), frozen),
        "no daemon serving this database (lock holder pid 7)"
    );
    assert_eq!(
        render_text_at(&no_daemon_view(Some(7), Some(false)), frozen),
        "no daemon serving this database (lock holder pid 7)",
        "Some(false) is the absent fact (B6: never minted), not a posture"
    );
}

/// US-3/S3 (OP3, journey plan AC-3's payload guard) — the draining
/// line is a text-render fact only: the JSON payload of the draining
/// view carries the byte-additive B6 shape (`daemon`, `draining`,
/// `holder_pid` and nothing else), unchanged by the renderer work.
#[test]
fn us3_s3_the_json_payload_stays_the_byte_additive_b6_shape() {
    let view = no_daemon_view(Some(4242), Some(true));
    let json = serde_json::to_value(&view).expect("json");
    let keys = json
        .as_object()
        .expect("object")
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(keys, ["daemon", "draining", "holder_pid"]);
}

/// OP3 W-4 (journey plan `status_query_reply_mirrors_status_view`) —
/// the `StatusQuery` answer mirrors the CLI-built `StatusView` 1:1: the
/// app half folds the in-session projection (the same constructor the
/// REPL `status` line builds, D-2's one-projection law) and every
/// mirrored field passes through verbatim — no GUI-side fact is minted,
/// renamed, or re-derived.
#[tokio::test]
async fn status_query_reply_mirrors_status_view() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut config = taskboard_app::config::AppConfig::default();
    config.sync.backend = Some(taskboard_app::config::SyncBackend::Standalone);
    config.storage.db_path = Some(dir.path().join("taskboard.db"));
    config.app.shutdown_timeout_secs = 2;
    config.validate().expect("valid");
    let app = taskboard_app::bootstrap::bootstrap_in(
        config,
        std::sync::Arc::new(FixedStore),
        std::sync::Arc::new(taskboard_app::notifications::LogSink),
        taskboard_app::bootstrap::BootIdentity::At(dir.path().to_owned()),
    )
    .await
    .expect("boots");

    // The in-session projection (the CLI verb's constructor) and the
    // panel's fold: 1:1 over the scalar surface.
    let view = taskboard_app::commands::status::StatusView::in_session(&app);
    let snapshot = taskboard_app::ui_requests::status_snapshot(&view);
    assert_eq!(snapshot.daemon, view.daemon);
    assert_eq!(snapshot.version, view.version);
    assert_eq!(snapshot.backend, view.backend);
    assert_eq!(snapshot.pid, view.pid);
    assert_eq!(snapshot.holder_pid, view.holder_pid);
    assert_eq!(snapshot.connector_attached, view.connector_attached);
    assert_eq!(snapshot.draining, view.draining);
    assert_eq!(
        snapshot.sync_phase,
        view.sync.as_ref().map(|sync| sync.phase.clone())
    );
    assert_eq!(
        snapshot.sync_pending_ops,
        view.sync.as_ref().map(|sync| sync.pending_ops)
    );
    assert_eq!(
        snapshot.engine_commands,
        view.engine.as_ref().map(|engine| engine.commands)
    );
    assert_eq!(
        snapshot.engine_rejections,
        view.engine.as_ref().map(|engine| engine.rejections)
    );
    assert_eq!(
        snapshot.engine_signals,
        view.engine.as_ref().map(|engine| engine.signals)
    );
    // In-session honesty: the session IS the core holder.
    assert!(!snapshot.daemon);
    assert_eq!(snapshot.holder_pid, Some(std::process::id()));

    // The no-daemon projection folds 1:1 too (the draining fact rides
    // B6's Option verbatim).
    let absent = taskboard_app::commands::status::StatusView::no_daemon(std::path::Path::new(
        "/nonexistent/taskboard.db",
    ));
    let snapshot = taskboard_app::ui_requests::status_snapshot(&absent);
    assert!(!snapshot.daemon);
    assert_eq!(snapshot.draining, None);
    assert_eq!(snapshot.holder_pid, None);

    let _ = app.shutdown().await;
}

/// A credential store that never answers (standalone never consults
/// it) — the unit tests' `FixedStore` twin, local to this binary.
#[derive(Debug)]
struct FixedStore;

impl taskboard_app::secrets::CredentialStore for FixedStore {
    fn load(
        &self,
        _server: &str,
        _username: &str,
    ) -> Result<taskboard_app::secrets::AppPassword, taskboard_app::secrets::CredentialError> {
        Ok(taskboard_app::secrets::AppPassword::new("pw"))
    }
    fn store(
        &self,
        _: &str,
        _: &str,
        _: taskboard_app::secrets::AppPassword,
    ) -> Result<(), taskboard_app::secrets::CredentialError> {
        Err(taskboard_app::secrets::CredentialError::Unsupported)
    }
    fn delete(&self, _: &str, _: &str) -> Result<(), taskboard_app::secrets::CredentialError> {
        Err(taskboard_app::secrets::CredentialError::Unsupported)
    }
}
