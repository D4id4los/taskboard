// SPDX-License-Identifier: AGPL-3.0-only
//! `taskboard status` (ADR 0012): who serves this database and how is
//! sync. One serde shape answers both cases, the `daemon` bool
//! discriminating: over IPC, the daemon's live engine state (read from
//! the `ArcSwap` — never a sync cycle trigger); without a daemon, the
//! instance-lock sidecar's diagnostic holder PID. Either way the
//! command never boots a core and exits 0 — an observation, not a
//! failure.

use serde::{Deserialize, Serialize};
use std::fmt::Write as _;
use std::path::Path;

use taskboard_domain::{AppState, SyncStatus};

use crate::commands::output::{BoardStatusView, ConnectorStatusView, SyncStatusView};
use crate::connectors::ConnectorSpec;

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
    /// Per-connector status (D6-3): present in core-holding projections
    /// (`server` and `in_session`) — every configured connector,
    /// zero-bound ones included. Absent (not `null`) in `no_daemon`
    /// answers; additivity in both wire directions rides `default` +
    /// `skip_serializing_if` (no protocol-version bump — the
    /// `AttachConnector.name` precedent).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connectors: Option<Vec<ConnectorStatusView>>,
    /// The flat per-board view (same population rule as `connectors`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub boards: Option<Vec<BoardStatusView>>,
}

/// The attribution derivation (D6-4): groups every live board's sync
/// row under its binding's connector tag — derived from the bindings,
/// never stored (DF-9) — and returns the per-connector rows (config
/// order, the default first; zero-bound connectors configured-and-idle)
/// plus the flat board-id-ordered view (local-only boards included,
/// `connector`/`remote` `null`). Archived boards stay in (they remain
/// commandable and sync); tombstoned boards are out.
///
/// # Panics
///
/// Never on caller inputs: the one `expect` guards a grouping
/// invariant of this very function (a grouped board id is bound to the
/// group's connector by construction).
#[must_use]
pub(crate) fn derive_connector_views(
    state: &AppState,
    configured: &[ConnectorSpec],
) -> (Vec<ConnectorStatusView>, Vec<BoardStatusView>) {
    let mut flat: Vec<BoardStatusView> = Vec::new();
    // connector name -> (board ids, raw rows) for the health fold.
    let mut grouped: std::collections::BTreeMap<String, Vec<taskboard_domain::BoardId>> =
        std::collections::BTreeMap::new();
    for board in state.boards.values().filter(|b| !b.deleted) {
        let (connector, remote) = match &board.remote {
            Some(binding) => (Some(binding.connector.to_string()), Some(binding.board.0)),
            None => (None, None),
        };
        if let Some(name) = &connector {
            grouped.entry(name.clone()).or_default().push(board.id);
        }
        let row = board_row(state, board.id);
        flat.push(BoardStatusView {
            board: board.id.as_uuid().to_string(),
            title: board.title.clone(),
            remote,
            connector,
            sync: SyncStatusView::new(&row),
        });
    }
    let connectors = configured
        .iter()
        .map(|spec| {
            let name = spec.name.to_string();
            let member_ids = grouped.get(&name).cloned().unwrap_or_default();
            let members: Vec<BoardStatusView> = member_ids
                .iter()
                .map(|id| {
                    let board = &state.boards[id];
                    BoardStatusView {
                        board: id.as_uuid().to_string(),
                        title: board.title.clone(),
                        remote: Some(board.remote.as_ref().expect("grouped ⇒ bound").board.0),
                        connector: Some(name.clone()),
                        sync: SyncStatusView::new(&board_row(state, *id)),
                    }
                })
                .collect();
            let rows: Vec<SyncStatus> = member_ids.iter().map(|id| board_row(state, *id)).collect();
            ConnectorStatusView {
                name,
                health: SyncStatusView::new(&taskboard_domain::aggregate_rows(rows.iter())),
                boards: members,
            }
        })
        .collect();
    (connectors, flat)
}

/// The board's sync map row, or the default when none exists yet (a
/// just-bound board before its first cycle; an unbound board after
/// unbind dropped its row).
fn board_row(state: &AppState, board: taskboard_domain::BoardId) -> SyncStatus {
    state
        .sync
        .get(board)
        .cloned()
        .unwrap_or_else(SyncStatus::default)
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
        let state = app.engine.shared_state().load_full();
        let configured = crate::connectors::configured_connectors(&app.config);
        let (connectors, boards) = derive_connector_views(&state, &configured);
        Self {
            daemon: true,
            version: Some(crate::VERSION.to_owned()),
            backend: Some(backend.to_owned()),
            pid: Some(std::process::id()),
            // The byte pin (D-W6): the `sync` field's value shape is the
            // all-boards aggregate fold, unchanged — the per-connector
            // detail lives in `connectors[]`.
            sync: Some(SyncStatusView::new(&state.sync.aggregate())),
            holder_pid: None,
            connector_attached,
            connectors: Some(connectors),
            boards: Some(boards),
        }
    }

    /// The client-side answer for a database no daemon serves: the lock
    /// sidecar's diagnostic PID, nothing more. No core is booted, no
    /// lock is taken; both arrays stay `None` (nothing is booted —
    /// there is nothing to derive), which keeps the `{"daemon": false}`
    /// byte pin intact.
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
            connectors: None,
            boards: None,
        }
    }

    /// The in-session answer (REPL `status` line, D6-7): the session
    /// *is* the core holder, so it derives the arrays from its own live
    /// core — the same derivation as `server` — while `sync` stays
    /// `None` (its documented presence rule is "present when `daemon`",
    /// unchanged).
    #[must_use]
    pub fn in_session(app: &crate::App) -> Self {
        let configured = crate::connectors::configured_connectors(&app.config);
        let connector_attached = connector_flag(!configured.is_empty(), app.connector_attached());
        let state = app.engine.shared_state().load_full();
        let (connectors, boards) = derive_connector_views(&state, &configured);
        Self {
            daemon: false,
            version: None,
            backend: None,
            pid: None,
            sync: None,
            holder_pid: Some(std::process::id()),
            connector_attached,
            connectors: Some(connectors),
            boards: Some(boards),
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
        // The arrays render after the existing clauses (the daemon-line
        // pins pass unmodified): one line per connector, one indented
        // line per bound board, local-only boards trailing.
        if let Some(connectors) = &view.connectors {
            for connector in connectors {
                daemon.push('\n');
                let _ = write!(
                    daemon,
                    "connector {}: {}, {} pending",
                    connector.name, connector.health.phase, connector.health.pending_ops
                );
                for board in &connector.boards {
                    daemon.push_str(&board_line(board, "  "));
                }
            }
        }
        if let Some(boards) = &view.boards {
            for board in boards.iter().filter(|b| b.connector.is_none()) {
                daemon.push_str(&board_line(board, ""));
            }
        }
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

/// One bound board's indented render: uuid, title, remote id, phase,
/// pending — the data-survival facts (the flat array itself is
/// machine-only and never rendered twice).
fn board_line(board: &BoardStatusView, indent: &str) -> String {
    let remote = board
        .remote
        .map_or_else(|| "local".to_owned(), |id| format!("remote {id}"));
    format!(
        "\n{indent}board {} \"{}\" ({}) — {}, {} pending",
        board.board, board.title, remote, board.sync.phase, board.sync.pending_ops
    )
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
    use super::{StatusView, connector_flag, derive_connector_views, render_text};
    use crate::commands::output::{BoardStatusView, ConnectorStatusView, SyncStatusView};
    use crate::config::{AppConfig, SyncBackend};
    use crate::connectors::ConnectorSpec;
    use taskboard_domain::ids::BoardId;
    use taskboard_domain::{
        AppState, Board, BoardClocks, Color, RemoteBoardBinding, SyncErrorKind, SyncPhase,
        SyncStates, SyncStatus,
    };

    /// A bare view: only `daemon` set, everything else absent — the
    /// struct-literal base for the projection/serde pins.
    fn bare(daemon: bool) -> StatusView {
        StatusView {
            daemon,
            version: None,
            backend: None,
            pid: None,
            sync: None,
            holder_pid: None,
            connector_attached: None,
            connectors: None,
            boards: None,
        }
    }

    /// The in-session detached fact rides whatever the caller gated —
    /// the pure gate (`connector_flag`) is where the backend rule
    /// lives; the view never re-derives.
    #[test]
    fn the_connector_flag_is_gated_on_configured_connectors() {
        assert_eq!(connector_flag(true, true), Some(true));
        assert_eq!(connector_flag(true, false), Some(false));
        assert_eq!(connector_flag(false, true), None);
        assert_eq!(connector_flag(false, false), None);
    }

    /// The serde contract: `connector_attached` and both arrays are
    /// skipped when absent — a no-daemon answer serializes to exactly
    /// `{"daemon": false}` (the byte pin, unmodified by the array
    /// growth).
    #[test]
    fn absent_optional_fields_are_skipped() {
        let json = serde_json::to_value(StatusView::no_daemon(std::path::Path::new(
            "/nonexistent/taskboard.db",
        )))
        .expect("json");
        assert_eq!(json, serde_json::json!({ "daemon": false }));
    }

    /// The in-session projection carries the gated connector fact and
    /// its own pid (holder = the session process).
    #[tokio::test]
    async fn in_session_carries_the_connector_fact_and_holder_pid() {
        let dir = tempfile::tempdir().expect("tempdir");
        let app = boot_standalone(dir.path()).await;
        // The gate-to-view wiring: a standalone app configures no
        // connectors, so the fact is absent and the arrays are empty
        // but present.
        assert_eq!(connector_flag(false, app.connector_attached()), None);
        let view = StatusView::in_session(&app);
        assert!(!view.daemon);
        assert_eq!(view.holder_pid, Some(std::process::id()));
        assert!(view.sync.is_none(), "sync stays absent in-session");
        assert!(view.connectors.is_some(), "the session derives the arrays");
        assert!(view.boards.is_some());
        // Standalone: zero configured connectors ⇒ empty arrays, present.
        assert_eq!(
            view.connectors.as_ref().expect("some"),
            &Vec::new(),
            "standalone: zero configured connectors"
        );
        assert_eq!(view.boards.as_ref().expect("some"), &Vec::new());
        let _ = app.shutdown().await;
    }

    async fn boot_standalone(dir: &std::path::Path) -> crate::App {
        let mut config = AppConfig::default();
        config.sync.backend = Some(SyncBackend::Standalone);
        config.storage.db_path = Some(dir.join("taskboard.db"));
        config.app.shutdown_timeout_secs = 2;
        config.validate().expect("valid");
        let store: std::sync::Arc<dyn crate::secrets::CredentialStore> =
            std::sync::Arc::new(FixedStore);
        crate::bootstrap::bootstrap(
            config,
            store,
            std::sync::Arc::new(crate::notifications::LogSink),
        )
        .await
        .expect("boots")
    }

    #[derive(Debug)]
    struct FixedStore;

    impl crate::secrets::CredentialStore for FixedStore {
        fn load(
            &self,
            _: &str,
            _: &str,
        ) -> Result<crate::secrets::AppPassword, crate::secrets::CredentialError> {
            Ok(crate::secrets::AppPassword::new("pw"))
        }
        fn store(
            &self,
            _: &str,
            _: &str,
            _: crate::secrets::AppPassword,
        ) -> Result<(), crate::secrets::CredentialError> {
            Err(crate::secrets::CredentialError::Unsupported)
        }
        fn delete(&self, _: &str, _: &str) -> Result<(), crate::secrets::CredentialError> {
            Err(crate::secrets::CredentialError::Unsupported)
        }
    }

    /// The key-skipping rule for the connector fact (the
    /// serialized-when-present half of the contract).
    #[test]
    fn the_connector_key_is_skipped_when_absent() {
        let absent = bare(false);
        let json = serde_json::to_value(&absent).expect("json");
        assert!(json.get("connector_attached").is_none());
        let mut detached = bare(false);
        detached.connector_attached = Some(false);
        let json = serde_json::to_value(&detached).expect("json");
        assert_eq!(json["connector_attached"], serde_json::Value::Bool(false));
    }

    /// The new arrays' serde mechanics (D6-3): absent ⇒ skipped keys;
    /// present ⇒ appended after `connector_attached` (declaration order
    /// is the JSON key order); inside the arrays the `Option` fields
    /// serialize as `null`, never absent.
    #[test]
    fn array_keys_are_appended_and_inner_options_serialize_null() {
        let json = serde_json::to_value(bare(true)).expect("json");
        let keys: Vec<&str> = json
            .as_object()
            .expect("object")
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(keys, ["daemon"], "all-absent daemon view is bare");

        let mut view = bare(true);
        view.connector_attached = Some(true);
        view.connectors = Some(vec![ConnectorStatusView {
            name: "alpha".to_owned(),
            health: SyncStatusView {
                phase: "idle".to_owned(),
                last_success: None,
                pending_ops: 0,
                error_kind: None,
            },
            boards: Vec::new(),
        }]);
        view.boards = Some(vec![BoardStatusView {
            board: "00000000-0000-0000-0000-000000000001".to_owned(),
            title: "solo".to_owned(),
            remote: None,
            connector: None,
            sync: SyncStatusView {
                phase: "idle".to_owned(),
                last_success: None,
                pending_ops: 2,
                error_kind: None,
            },
        }]);
        // Key order is declaration order — pinned through `to_string`
        // (`to_value`'s object is a sorted map, order-blind).
        let line = serde_json::to_string(&view).expect("json line");
        let at = |key: &str| line.find(&format!(r#""{key}":"#)).expect(key);
        assert!(
            at("daemon") < at("connector_attached")
                && at("connector_attached") < at("connectors")
                && at("connectors") < at("boards"),
            "the two new keys append after connector_attached: {line}"
        );
        let json = serde_json::to_value(&view).expect("json");
        let keys: Vec<&str> = json
            .as_object()
            .expect("object")
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(
            keys,
            ["boards", "connector_attached", "connectors", "daemon"],
            "the full key set"
        );
        let board = &json["boards"][0];
        assert!(board.get("remote").is_some() && board["remote"].is_null());
        assert!(board.get("connector").is_some() && board["connector"].is_null());
        assert_eq!(
            board["sync"]
                .as_object()
                .expect("sync object")
                .keys()
                .map(String::as_str)
                .collect::<std::collections::BTreeSet<_>>(),
            ["phase", "last_success", "pending_ops", "error_kind"]
                .into_iter()
                .collect(),
            "the sync object's key set is unchanged"
        );
        // The round-trip is lossless.
        let back: StatusView = serde_json::from_value(json).expect("decode");
        assert_eq!(back, view);
        // An old-shape answer (arrays absent) decodes with `None`s —
        // the both-ways additivity pin.
        let old: StatusView =
            serde_json::from_value(serde_json::json!({ "daemon": true })).expect("old decode");
        assert_eq!(old.connectors, None);
        assert_eq!(old.boards, None);
    }

    // -- the derivation (D6-4) ---------------------------------------------

    fn board(raw: u128, remote: Option<RemoteBoardBinding>, title: &str) -> (BoardId, Board) {
        let id = BoardId::from(uuid::Uuid::from_u128(raw));
        let clocks = BoardClocks {
            title: BoardClocks::epoch(),
            color: BoardClocks::epoch(),
            archived: BoardClocks::epoch(),
            deleted: BoardClocks::epoch(),
        };
        (
            id,
            Board {
                id,
                remote,
                title: title.to_owned(),
                color: Color::new("0082c9"),
                archived: false,
                deleted: false,
                clocks,
                remote_seen: None,
            },
        )
    }

    fn spec(name: &str) -> ConnectorSpec {
        ConnectorSpec {
            name: taskboard_domain::ConnectorId::from(name),
            backend: SyncBackend::Nextcloud,
            server_url: "https://cloud.example.com".to_owned(),
            username: "alice".to_owned(),
        }
    }

    fn phase_view(phase: SyncPhase, pending: u32) -> SyncStatusView {
        SyncStatusView::new(&SyncStatus {
            phase,
            last_success: None,
            pending_ops: pending,
        })
    }

    fn two_connector_config() -> Vec<ConnectorSpec> {
        vec![spec("alpha"), spec("beta")]
    }

    /// Grouping by derived attribution: bindings tagged alpha/beta put
    /// each board's row under its own connector; the flat view is
    /// board-id ordered and includes the local-only board with nulls.
    #[test]
    fn the_derivation_groups_by_binding_tag_and_includes_local_only_boards() {
        let mut state = AppState::default();
        let (alpha_id, alpha) = board(
            0xA1,
            Some(RemoteBoardBinding {
                connector: taskboard_domain::ConnectorId::from("alpha"),
                board: taskboard_domain::RemoteBoardId(7),
            }),
            "alpha board",
        );
        let (beta_id, beta) = board(
            0xB1,
            Some(RemoteBoardBinding {
                connector: taskboard_domain::ConnectorId::from("beta"),
                board: taskboard_domain::RemoteBoardId(7),
            }),
            "beta board",
        );
        let (local_id, local) = board(0xC1, None, "local only");
        let (dead_id, mut dead) = board(0xD1, alpha.remote.clone(), "tombstoned");
        dead.deleted = true;
        for (id, board) in [
            (alpha_id, alpha),
            (beta_id, beta),
            (local_id, local),
            (dead_id, dead),
        ] {
            state.boards.insert(id, board);
        }
        let mut sync = SyncStates::new();
        sync.set_stored(alpha_id, SyncPhase::Offline, None);
        state.sync = sync;

        let (connectors, flat) = derive_connector_views(&state, &two_connector_config());

        assert_eq!(connectors.len(), 2, "every configured connector lists");
        assert_eq!(connectors[0].name, "alpha", "config order, default first");
        assert_eq!(connectors[0].boards.len(), 1);
        assert_eq!(
            connectors[0].boards[0].board,
            alpha_id.as_uuid().to_string()
        );
        assert_eq!(connectors[0].health.phase, "offline", "the row folds");
        assert_eq!(connectors[1].name, "beta");
        assert_eq!(connectors[1].boards.len(), 1);
        assert_eq!(connectors[1].health.phase, "idle", "no row yet ⇒ default");

        assert_eq!(flat.len(), 3, "live boards only — the tombstone is out");
        let local = flat
            .iter()
            .find(|b| b.connector.is_none())
            .expect("local-only");
        assert_eq!(local.board, local_id.as_uuid().to_string());
        assert!(local.remote.is_none());
        assert_eq!(local.title, "local only");
        let alpha_row = &flat[0];
        assert_eq!(alpha_row.connector.as_deref(), Some("alpha"));
        assert_eq!(alpha_row.remote, Some(7));
        assert_eq!(alpha_row.sync.phase, "offline");
        // Board-id order: alpha (0xA1) < beta (0xB1) < local (0xC1).
        assert_eq!(
            flat.iter().map(|b| b.board.as_str()).collect::<Vec<_>>(),
            [
                alpha_id.as_uuid().to_string(),
                beta_id.as_uuid().to_string(),
                local_id.as_uuid().to_string(),
            ]
        );
    }

    /// A zero-bound configured connector lists configured-and-idle: the
    /// default health, an empty boards array — honest, never hidden.
    #[test]
    fn a_zero_bound_connector_is_configured_and_idle() {
        let mut state = AppState::default();
        let (id, board) = board(
            0xA1,
            Some(RemoteBoardBinding {
                connector: taskboard_domain::ConnectorId::from("alpha"),
                board: taskboard_domain::RemoteBoardId(7),
            }),
            "alpha board",
        );
        state.boards.insert(id, board);
        let configured = vec![spec("alpha"), spec("gama"), spec("beta")];
        let (connectors, flat) = derive_connector_views(&state, &configured);
        assert_eq!(connectors.len(), 3);
        let gama = &connectors[1];
        assert_eq!(gama.name, "gama");
        assert_eq!(gama.boards, Vec::new());
        assert_eq!(gama.health.phase, "idle");
        assert_eq!(gama.health.pending_ops, 0);
        assert_eq!(flat.len(), 1);
    }

    /// Per-connector health is the map-derived fold over its own
    /// boards' rows (a failed alpha never bleeds into beta's health).
    #[test]
    fn per_connector_health_folds_only_its_own_rows() {
        let mut state = AppState::default();
        let mut sync = SyncStates::new();
        let mk = |raw: u128, tag: &str, remote: u64| {
            board(
                raw,
                Some(RemoteBoardBinding {
                    connector: taskboard_domain::ConnectorId::from(tag),
                    board: taskboard_domain::RemoteBoardId(remote),
                }),
                "b",
            )
        };
        let (a1, board) = mk(0xA1, "alpha", 1);
        state.boards.insert(a1, board);
        let (a2, board) = mk(0xA2, "alpha", 2);
        state.boards.insert(a2, board);
        let (b1, board) = mk(0xB1, "beta", 1);
        state.boards.insert(b1, board);
        sync.set_stored(
            a1,
            SyncPhase::Failed {
                last_error: SyncErrorKind::Network,
            },
            None,
        );
        sync.set_stored(a2, SyncPhase::Syncing, None);
        sync.set_stored(b1, SyncPhase::Idle, None);
        state.sync = sync;

        let (connectors, _) = derive_connector_views(&state, &two_connector_config());
        assert_eq!(connectors[0].health.phase, "syncing", "alpha's dominance");
        assert_eq!(connectors[1].health.phase, "idle", "beta unaffected");
        assert_eq!(connectors[0].boards.len(), 2);
    }

    /// The human layout (D6-3b): one line per connector, one indented
    /// line per bound board, local-only boards trailing — asserted as
    /// data survival (uuids, names, the remote id present), never prose.
    #[test]
    fn render_text_survives_the_array_facts() {
        let mut view = bare(true);
        view.version = Some("9.9.9".to_owned());
        view.backend = Some("standalone".to_owned());
        view.pid = Some(42);
        view.connectors = Some(vec![
            ConnectorStatusView {
                name: "alpha".to_owned(),
                health: phase_view(SyncPhase::Idle, 2),
                boards: vec![BoardStatusView {
                    board: "00000000-0000-0000-0000-00000000000a".to_owned(),
                    title: "alpha board".to_owned(),
                    remote: Some(7),
                    connector: Some("alpha".to_owned()),
                    sync: phase_view(SyncPhase::Offline, 2),
                }],
            },
            ConnectorStatusView {
                name: "beta".to_owned(),
                health: phase_view(SyncPhase::Idle, 0),
                boards: Vec::new(),
            },
        ]);
        view.boards = Some(vec![BoardStatusView {
            board: "00000000-0000-0000-0000-00000000000c".to_owned(),
            title: "local only".to_owned(),
            remote: None,
            connector: None,
            sync: phase_view(SyncPhase::Idle, 0),
        }]);
        let text = render_text(&view);
        for fact in [
            "9.9.9",
            "42",
            "connector alpha",
            "connector beta",
            "00000000-0000-0000-0000-00000000000a",
            "alpha board",
            "remote 7",
            "00000000-0000-0000-0000-00000000000c",
            "local only",
        ] {
            assert!(text.contains(fact), "{fact} must survive rendering");
        }
        // The zero-bound connector renders too (configured-and-idle),
        // and the daemon line leads (the existing clause order intact).
        assert!(text.find("daemon v9.9.9").is_some());
        assert!(text.find("connector alpha").unwrap() > text.find("daemon v9.9.9").unwrap());
    }

    /// Without arrays (the `no_daemon` shape), the human text is the
    /// daemon/holder line exactly as before — no empty-array noise.
    #[test]
    fn render_text_without_arrays_is_unchanged() {
        let text = render_text(&bare(false));
        assert!(text.contains("no daemon"));
        assert!(!text.contains("connector"));
    }
}
