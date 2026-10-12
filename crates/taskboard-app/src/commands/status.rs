// SPDX-License-Identifier: AGPL-3.0-only
//! `taskboard status` (ADR 0012): who serves this database and how is
//! sync. One serde shape answers both cases, the `daemon` bool
//! discriminating: over IPC, the daemon's live engine state (read from
//! the `ArcSwap` — never a sync cycle trigger); without a daemon, the
//! instance-lock sidecar's diagnostic holder PID. Either way the
//! command never boots a core and exits 0 — an observation, not a
//! failure.

use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::commands::output::SyncStatusView;

/// The status answer shared by the daemon-side (IPC `Status` request)
/// and client-side (no daemon / in-session) projections.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StatusView {
    /// Is a daemon serving this database?
    pub daemon: bool,
    /// The daemon's application version (present when `daemon`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// `"standalone" | "nextcloud"` (present when `daemon`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backend: Option<String>,
    /// The daemon's OS pid (present when `daemon`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    /// Live sync status (present when `daemon`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sync: Option<SyncStatusView>,
    /// The instance-lock sidecar's recorded holder PID (present only in
    /// the client-side answer; diagnostic, best-effort).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub holder_pid: Option<u32>,
    /// Whether the daemon's sync connector is still attached (WI-H):
    /// present only in daemon-side answers under the `nextcloud`
    /// backend — `true` live actor, `false` detached (e.g. a `logout`
    /// nudged the daemon). A detached daemon would otherwise answer
    /// `backend: "nextcloud"` with phase `Idle`: structurally true,
    /// operationally misleading.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connector_attached: Option<bool>,
}

impl StatusView {
    /// The daemon-side answer: live state off the engine's `ArcSwap` —
    /// never boots anything, never triggers a cycle.
    #[must_use]
    pub fn server(app: &crate::App) -> Self {
        let backend = match app.config.sync_backend() {
            crate::config::SyncBackend::Standalone => "standalone",
            crate::config::SyncBackend::Nextcloud => "nextcloud",
        };
        let connector_attached = match app.config.sync_backend() {
            crate::config::SyncBackend::Nextcloud => Some(app.connector_attached()),
            crate::config::SyncBackend::Standalone => None,
        };
        Self {
            daemon: true,
            version: Some(crate::VERSION.to_owned()),
            backend: Some(backend.to_owned()),
            pid: Some(std::process::id()),
            sync: Some(SyncStatusView::new(
                &app.engine.shared_state().load_full().sync,
            )),
            holder_pid: None,
            connector_attached,
        }
    }

    /// The client-side answer for a database no daemon serves: the lock
    /// sidecar's diagnostic PID, nothing more. No core is booted, no
    /// lock is taken.
    #[must_use]
    pub fn no_daemon(db_path: &Path) -> Self {
        Self {
            daemon: false,
            version: None,
            backend: None,
            pid: None,
            sync: None,
            holder_pid: crate::lockfile::holder_pid(db_path),
            connector_attached: None,
        }
    }

    /// The in-session answer (REPL `status` line): the session *is* the
    /// core holder, so its own pid is the honest client-side view.
    #[must_use]
    pub fn in_session() -> Self {
        Self {
            daemon: false,
            version: None,
            backend: None,
            pid: None,
            sync: None,
            holder_pid: Some(std::process::id()),
            connector_attached: None,
        }
    }
}

/// The human-readable status line (presentation text, never asserted).
#[must_use]
pub fn render_text(view: &StatusView) -> String {
    if view.daemon {
        let mut daemon = format!(
            "daemon v{} (backend: {}, pid {})",
            view.version.as_deref().unwrap_or("?"),
            view.backend.as_deref().unwrap_or("?"),
            view.pid.map_or_else(|| "?".to_owned(), |p| p.to_string())
        );
        if let Some(sync) = &view.sync {
            daemon.push_str(" — sync phase: ");
            daemon.push_str(&sync.phase);
            daemon.push_str(", ");
            daemon.push_str(&sync.pending_ops.to_string());
            daemon.push_str(" pending");
        }
        if view.connector_attached == Some(false) {
            daemon.push_str(" — connector detached (logged out)");
        }
        daemon
    } else {
        match view.holder_pid {
            Some(pid) => format!("no daemon serving this database (lock holder pid {pid})"),
            None => "no daemon serving this database".to_owned(),
        }
    }
}

/// The one emission path for the status answer — the client-side
/// `dispatch` arm and the binary's IPC-relayed `status` both print
/// through here, so the two surfaces cannot drift: the JSON payload on
/// `out` under `--json`, the rendered line otherwise.
pub fn emit(io: &mut crate::commands::CommandIo<'_>, view: &StatusView) {
    crate::commands::emit_to(io, view, &render_text(view));
}

#[cfg(test)]
mod tests {
    use super::{StatusView, render_text};

    /// The daemon-side projection: live fields present, holder pid
    /// absent; the client-side projection is its mirror.
    #[test]
    fn the_two_answer_shapes_are_discriminated_by_daemon() {
        // A throwaway db path: the lock-sidecar read is best-effort and
        // side-effect free, and this path does not exist.
        let db = std::env::temp_dir().join("taskboard-status-tests-nonexistent.db");
        let client_view = StatusView::no_daemon(&db);
        assert!(!client_view.daemon);
        assert!(client_view.version.is_none());
        assert!(client_view.pid.is_none());
        assert!(client_view.sync.is_none());

        let session_view = StatusView::in_session();
        assert!(!session_view.daemon);
        assert_eq!(session_view.holder_pid, Some(std::process::id()));
    }

    /// Human text is presentation (never asserted in journeys), but the
    /// discriminating facts must survive rendering.
    #[test]
    fn render_text_names_the_facts() {
        let daemon_view = StatusView {
            daemon: true,
            version: Some("9.9.9".to_owned()),
            backend: Some("standalone".to_owned()),
            pid: Some(42),
            sync: None,
            holder_pid: None,
            connector_attached: None,
        };
        let text = render_text(&daemon_view);
        assert!(text.contains("9.9.9") && text.contains("standalone") && text.contains("42"));

        let no_daemon = render_text(&StatusView::no_daemon(std::path::Path::new(
            "/nonexistent/taskboard.db",
        )));
        assert!(no_daemon.contains("no daemon"));

        let holder = render_text(&StatusView::in_session());
        assert!(holder.contains("no daemon") && holder.contains("pid"));
    }
}
