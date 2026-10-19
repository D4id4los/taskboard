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

/// The connector-attached fact under the backend gate, for the
/// in-session projection: present iff `nextcloud` — a session's
/// `standalone` config has no connector concept, and `false` there
/// would read as "detached (logged out)". The session refreshes its
/// config view at every ceremony, so its gating tracks reality; the
/// daemon-side projection carries the live flag unconditionally (it can
/// attach from persisted state a standalone boot never named).
#[must_use]
pub(crate) fn connector_flag(connectors_configured: bool, attached: bool) -> Option<bool> {
    // `Some` when any connector is configured — configured-ness, not
    // the backend word, is the gate (the array world's empty array is
    // the standalone fact). Legacy outputs are identical.
    if connectors_configured {
        Some(attached)
    } else {
        None
    }
}

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
    /// Whether the sync connector is still attached (WI-H): present in
    /// every daemon-side answer and in in-session answers under the
    /// `nextcloud` backend — `true` live actor, `false` detached (e.g.
    /// a `logout` nudged the core). Daemon-side it is the *live* fact
    /// regardless of the boot-shaped backend: a standalone-booted
    /// daemon that attached from persisted state answers
    /// `backend: "standalone"` with `connector_attached: true`. A
    /// detached connector would otherwise answer `backend: "nextcloud"`
    /// with phase `Idle`: structurally true, operationally misleading.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connector_attached: Option<bool>,
}

impl StatusView {
    /// The daemon-side answer: live state off the engine's `ArcSwap` —
    /// never boots anything, never triggers a cycle.
    #[must_use]
    pub fn server(app: &crate::App) -> Self {
        let backend = match app.config.resolved_default_backend() {
            crate::config::SyncBackend::Standalone => "standalone",
            crate::config::SyncBackend::Nextcloud => "nextcloud",
        };
        let connector_attached = Some(app.connector_attached());
        Self {
            daemon: true,
            version: Some(crate::VERSION.to_owned()),
            backend: Some(backend.to_owned()),
            pid: Some(std::process::id()),
            sync: Some(SyncStatusView::new(
                &app.engine.shared_state().load_full().sync.aggregate(),
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
    /// core holder, so its own pid is the honest client-side view, and —
    /// under the `nextcloud` backend — the live attach/detach fact the
    /// caller gated through [`connector_flag`].
    #[must_use]
    pub fn in_session(connector_attached: Option<bool>) -> Self {
        Self {
            daemon: false,
            version: None,
            backend: None,
            pid: None,
            sync: None,
            holder_pid: Some(std::process::id()),
            connector_attached,
        }
    }
}

/// The human-readable status line (presentation text, never asserted).
#[must_use]
pub fn render_text(view: &StatusView) -> String {
    // One spelling of the detached marker: every projection whose
    // answer carries the connector fact appends the same suffix.
    let detached = if view.connector_attached == Some(false) {
        " — connector detached (logged out)"
    } else {
        ""
    };
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
        daemon.push_str(detached);
        daemon
    } else {
        let mut line = match view.holder_pid {
            Some(pid) => format!("no daemon serving this database (lock holder pid {pid})"),
            None => "no daemon serving this database".to_owned(),
        };
        line.push_str(detached);
        line
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
    use super::{StatusView, connector_flag, render_text};

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

        let session_view = StatusView::in_session(None);
        assert!(!session_view.daemon);
        assert_eq!(session_view.holder_pid, Some(std::process::id()));
    }

    /// The in-session projection carries whatever connector fact the
    /// caller gated on the backend — the view itself never re-derives.
    #[test]
    fn in_session_carries_the_connector_fact() {
        assert_eq!(
            StatusView::in_session(Some(true)).connector_attached,
            Some(true)
        );
        assert_eq!(
            StatusView::in_session(Some(false)).connector_attached,
            Some(false)
        );
    }

    /// The gate is view vocabulary, held once: present iff any
    /// connector is configured (D5-9 — the configured-ness fact, not
    /// the backend word; legacy outputs identical).
    #[test]
    fn the_connector_flag_is_gated_on_configured_connectors() {
        assert_eq!(connector_flag(true, true), Some(true));
        assert_eq!(connector_flag(true, false), Some(false));
        assert_eq!(connector_flag(false, true), None);
        assert_eq!(connector_flag(false, false), None);
    }

    /// The serde contract for the in-session arms: the key is skipped
    /// when absent, serialized when the fact exists.
    #[test]
    fn the_connector_key_is_skipped_when_absent() {
        let absent = serde_json::to_value(StatusView::in_session(None)).expect("json");
        assert!(absent.get("connector_attached").is_none());
        let detached = serde_json::to_value(StatusView::in_session(Some(false))).expect("json");
        assert_eq!(
            detached["connector_attached"],
            serde_json::Value::Bool(false)
        );
        let attached = serde_json::to_value(StatusView::in_session(Some(true))).expect("json");
        assert_eq!(
            attached["connector_attached"],
            serde_json::Value::Bool(true)
        );
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

        let holder = render_text(&StatusView::in_session(None));
        assert!(holder.contains("no daemon") && holder.contains("pid"));

        // The in-session detached fact reaches the human text too — the
        // module's own presentation smoke, not a journey assertion.
        let detached = render_text(&StatusView::in_session(Some(false)));
        assert!(detached.contains("no daemon") && detached.contains("connector detached"));
        let plain = render_text(&StatusView::in_session(None));
        assert!(!plain.contains("connector detached"));
    }
}
