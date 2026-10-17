// SPDX-License-Identifier: AGPL-3.0-only
//! `taskboard logout` (ADR 0009): delete the stored secret, then —
//! under the *auto* backend only — strip the config's
//! `nextcloud.server_url`/`username` keys so the install cleanly
//! resolves back to stand-alone. An explicit `backend = "nextcloud"` is
//! never rewritten (ADR 0010: the loud choice stays loud; the next boot
//! fails at credential resolution instead of silently flipping shape).
//! Pre-core: no actor graph, no database, no lock.

use crate::commands::CommandError;
use crate::commands::output::DaemonConnectorState;
use crate::commands::output::{LogoutReceipt, SessionConnectorState};
use crate::config::AppConfig;
use crate::ipc::protocol::DetachWireOutcome;
use crate::secrets::{CredentialError, CredentialStore};
use std::io::ErrorKind as IoErrorKind;
use std::path::Path;

/// Runs the logout sequence: require the configured identity → delete
/// the secret (`NotFound` is an idempotent success) → strip the config
/// file iff the backend auto-resolved. The board binding and local db
/// are untouched: a later `login` to the same server resumes sync.
///
/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
pub fn run(
    config_path: &Path,
    raw: &AppConfig,
    store: &dyn CredentialStore,
) -> Result<LogoutReceipt, CommandError> {
    // Nothing to log out from: the typed refusal, not a silent success.
    let server = raw
        .nextcloud
        .server_url
        .clone()
        .ok_or(CommandError::NotConfigured)?;
    let username = raw
        .nextcloud
        .username
        .clone()
        .ok_or(CommandError::NotConfigured)?;

    // (a) The essential act: the secret dies first. A mid-way failure
    // leaves a recoverable state either way (re-running logout is
    // idempotent).
    // `Unsupported` (the env store) is the honest typed refusal: unset
    // TASKBOARD_APP_PASSWORD yourself.
    match store.delete(&server, &username) {
        Ok(()) | Err(CredentialError::NotFound) => {}
        Err(err) => return Err(CommandError::Credential(err)),
    }

    // (b) De-configure iff the backend is auto: strip the two keys from
    // the *file* (env-derived config is operator territory — the env is
    // never edited, and `config_cleared` reflects the file).
    let auto = raw.sync.backend.is_none();
    let config_cleared = if auto {
        strip_config_keys(config_path, &server, &username).map_err(CommandError::ConfigWrite)?
    } else {
        false
    };

    tracing::info!(server = %server, config_cleared, "logged out");
    Ok(LogoutReceipt {
        server_url: server,
        username,
        config_cleared,
        daemon_connector: None,
        session_connector: None,
    })
}

/// The human-readable receipt.
#[must_use]
pub fn render_text(receipt: &LogoutReceipt) -> String {
    let base = if receipt.config_cleared {
        format!(
            "logged out of {} as {} (config cleared — back to stand-alone)",
            receipt.server_url, receipt.username
        )
    } else {
        format!(
            "logged out of {} as {} (config keys kept: explicit backend)",
            receipt.server_url, receipt.username
        )
    };
    if receipt.daemon_connector == Some(DaemonConnectorState::Dropped) {
        return format!("{base} — daemon connector dropped");
    }
    if receipt.session_connector == Some(SessionConnectorState::Detached) {
        return format!("{base} — session connector detached");
    }
    base
}

/// The hermetic input of the degradation mapping (WI-H tier 8): the
/// post-logout daemon nudge's outcome, socket-free.
#[derive(Debug, Clone, PartialEq)]
pub enum NudgeOutcome {
    /// No daemon serves this database — nothing holds the revoked
    /// credential; not a degradation (D5: no warning).
    NoDaemon,
    /// The daemon answered the detach request.
    Answered(DetachWireOutcome),
    /// Transport failure — a dead connection, a pre-`detach-connector`
    /// daemon (it cannot decode the variant and drops the link), or the
    /// client read budget expired (the daemon still completes its
    /// detach in that case).
    Transport,
    /// The daemon speaks a different wire-protocol version.
    Version,
}

/// Maps the nudge outcome onto the receipt's `daemon_connector` field
/// (pure; unit-pinned over every arm).
#[must_use]
pub fn map_nudge(nudge: &NudgeOutcome) -> DaemonConnectorState {
    match nudge {
        // No daemon, or a daemon that holds no connector (standalone,
        // already detached): nothing holds the revoked credential.
        NudgeOutcome::NoDaemon | NudgeOutcome::Answered(DetachWireOutcome::NoConnector) => {
            DaemonConnectorState::None
        }
        NudgeOutcome::Answered(DetachWireOutcome::Detached) => DaemonConnectorState::Dropped,
        NudgeOutcome::Answered(DetachWireOutcome::IdentityMismatch) => {
            DaemonConnectorState::Mismatch
        }
        // The client cannot know what the operator will find when the
        // daemon aborts at its own join budget or answers in a foreign
        // version: the honest bucket is the warning. A transport
        // failure stays the sharper "unreachable" (it covers the
        // pre-detach-connector daemon, which drops the link).
        NudgeOutcome::Answered(DetachWireOutcome::TimedOut) | NudgeOutcome::Version => {
            DaemonConnectorState::Unknown
        }
        NudgeOutcome::Transport => DaemonConnectorState::Unreachable,
    }
}

/// The stderr warning for the degraded outcomes (D5): the local logout
/// *is* done; this tells the operator what a daemon may still hold.
/// `None` for the clean arms.
#[must_use]
pub fn nudge_warning(state: DaemonConnectorState) -> Option<&'static str> {
    match state {
        DaemonConnectorState::Mismatch => Some(
            "warning: a daemon serving this database logged in as a different identity; \
             restart it or run `taskboard daemon stop`",
        ),
        DaemonConnectorState::Unreachable | DaemonConnectorState::Unknown => Some(
            "warning: the daemon could not drop its connector; \
             restart it or run `taskboard daemon stop`",
        ),
        DaemonConnectorState::Dropped | DaemonConnectorState::None => None,
    }
}

/// The best-effort daemon nudge (WI-H): after the local logout
/// succeeded, ask the daemon serving the same database to drop its
/// connector. Never fails the logout; every outcome degrades through
/// [`map_nudge`]/[`nudge_warning`].
pub(crate) async fn nudge_daemon(db_path: &Path, server: &str, username: &str) -> NudgeOutcome {
    let Some(conn) = crate::ipc::client::try_connect(db_path).await else {
        return NudgeOutcome::NoDaemon;
    };
    match conn.detach(server, username).await {
        Ok(outcome) => NudgeOutcome::Answered(outcome),
        Err(CommandError::IpcVersion { .. }) => NudgeOutcome::Version,
        Err(_) => NudgeOutcome::Transport,
    }
}

/// Removes `nextcloud.server_url`/`username` from the config file via a
/// `toml_edit` read-modify-write (the `login` write's mirror: preserves
/// comments, formatting, and unknown tables), writing atomically.
/// Returns whether anything was removed; a missing file is `false`.
///
/// # Errors
///
/// [`std::io::Error`] on read/parse/write failures.
fn strip_config_keys(path: &Path, server: &str, username: &str) -> Result<bool, std::io::Error> {
    if !path.is_file() {
        return Ok(false);
    }
    let text = std::fs::read_to_string(path)?;
    let mut doc = text
        .parse::<toml_edit::DocumentMut>()
        .map_err(|err| std::io::Error::new(IoErrorKind::InvalidData, err))?;
    let mut cleared = false;
    let nextcloud = &mut doc["nextcloud"];
    if let Some(table) = nextcloud.as_table_mut() {
        if let Some(item) = table.get("server_url")
            && item.as_str() == Some(server)
        {
            table.remove("server_url");
            cleared = true;
        }
        if let Some(item) = table.get("username")
            && item.as_str() == Some(username)
        {
            table.remove("username");
            cleared = true;
        }
    }
    if !cleared {
        return Ok(false);
    }
    crate::config::write_owner_only_atomic(path, doc.to_string().as_str())?;
    Ok(true)
}

#[cfg(test)]
mod detach_tests {
    use super::{DaemonConnectorState, NudgeOutcome, map_nudge, nudge_warning};
    use crate::ipc::protocol::DetachWireOutcome;

    /// F12-01 (#297): the logout strip rewrite (the same tmp+rename
    /// mechanism as the connect overlay) keeps the config owner-only
    /// and re-establishes the mode on a loosened file.
    #[cfg(unix)]
    #[test]
    fn the_logout_strip_rewrite_keeps_the_config_owner_only() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().expect("tempdir");
        let config = dir.path().join("taskboard.toml");
        std::fs::write(
            &config,
            "[nextcloud]\nserver_url = \"https://c.example.com\"\nusername = \"alice\"\n",
        )
        .expect("seed config");
        let mut perms = std::fs::metadata(&config).expect("meta").permissions();
        perms.set_mode(0o644);
        std::fs::set_permissions(&config, perms).expect("loosen");

        let stripped =
            super::strip_config_keys(&config, "https://c.example.com", "alice").expect("strip");
        assert!(stripped, "the keys were present and must be removed");

        let mode = std::fs::metadata(&config)
            .expect("config exists")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600, "the strip rewrite must leave the file 0600");
    }

    /// WI-H tier 8: the pure nudge → receipt-field/warning mapping
    /// covers every arm — no socket involved.
    #[test]
    fn the_nudge_mapping_covers_every_arm() {
        use DetachWireOutcome as W;
        use NudgeOutcome::*;

        assert_eq!(map_nudge(&NoDaemon), DaemonConnectorState::None);
        assert_eq!(
            map_nudge(&Answered(W::Detached)),
            DaemonConnectorState::Dropped
        );
        assert_eq!(
            map_nudge(&Answered(W::NoConnector)),
            DaemonConnectorState::None
        );
        assert_eq!(
            map_nudge(&Answered(W::IdentityMismatch)),
            DaemonConnectorState::Mismatch
        );
        assert_eq!(
            map_nudge(&Answered(W::TimedOut)),
            DaemonConnectorState::Unknown
        );
        assert_eq!(map_nudge(&Transport), DaemonConnectorState::Unreachable);
        assert_eq!(map_nudge(&Version), DaemonConnectorState::Unknown);

        // Only the degraded arms warn (D5: no daemon is not degraded).
        assert!(nudge_warning(DaemonConnectorState::Dropped).is_none());
        assert!(nudge_warning(DaemonConnectorState::None).is_none());
        assert!(nudge_warning(DaemonConnectorState::Mismatch).is_some());
        assert!(nudge_warning(DaemonConnectorState::Unreachable).is_some());
        assert!(nudge_warning(DaemonConnectorState::Unknown).is_some());
    }

    /// The in-session detach outcome serializes kebab-case (the WI-H
    /// IPC vocabulary) and is absent for the one-shot shape.
    #[test]
    fn the_session_connector_field_serializes_kebab_case_or_is_absent() {
        use crate::commands::output::{LogoutReceipt, SessionConnectorState};
        let base = |session_connector| LogoutReceipt {
            server_url: "https://c.example.com".to_owned(),
            username: "alice".to_owned(),
            config_cleared: true,
            daemon_connector: None,
            session_connector,
        };
        let json =
            serde_json::to_value(base(Some(SessionConnectorState::Detached))).expect("serialize");
        assert_eq!(json["session_connector"], "detached");
        let json = serde_json::to_value(base(Some(SessionConnectorState::NoConnector)))
            .expect("serialize");
        assert_eq!(json["session_connector"], "no-connector");
        let json =
            serde_json::to_value(base(Some(SessionConnectorState::TimedOut))).expect("serialize");
        assert_eq!(json["session_connector"], "timed-out");
        let json = serde_json::to_value(base(None)).expect("serialize");
        assert!(json.get("session_connector").is_none());
    }

    /// The receipt field serializes kebab-case and is absent when not
    /// consulted (the pure-local shape stays stable).
    #[test]
    fn the_receipt_field_serializes_kebab_case_or_is_absent() {
        use crate::commands::output::LogoutReceipt;
        let base = |state| LogoutReceipt {
            server_url: "https://c.example.com".to_owned(),
            username: "alice".to_owned(),
            config_cleared: true,
            daemon_connector: state,
            session_connector: None,
        };
        let json =
            serde_json::to_value(base(Some(DaemonConnectorState::Dropped))).expect("serialize");
        assert_eq!(json["daemon_connector"], "dropped");
        let json = serde_json::to_value(base(Some(DaemonConnectorState::None))).expect("serialize");
        assert_eq!(json["daemon_connector"], "none");
        let json = serde_json::to_value(base(None)).expect("serialize");
        assert!(json.get("daemon_connector").is_none());
    }
}
