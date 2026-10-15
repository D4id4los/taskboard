// SPDX-License-Identifier: AGPL-3.0-only
//! `taskboard config print` (ADR 0009): a decode-only diagnostic that
//! *always* prints — validity is reported in the payload, not via the
//! exit code. Pre-core (no actor graph, no database, no lock).

use crate::cli::ConfigCmd;
use crate::commands::output::{ConfigPathView, ConfigView};
use crate::commands::{CommandCtx, CommandError, emit_to, report_to};
use crate::config::{AppConfig, ConfigError, SyncBackend};

/// Prints the effective config. Only an undecodable config fails (exit
/// 1 — nothing honest to print); a decodes-but-invalid config prints
/// `valid: false` plus the typed `invalid_kind` and exits 0.
///
/// # Errors
///
/// [`CommandError`] — the `decode` kind for provider failures; never
/// a validation failure (that travels in the payload).
pub fn print(config_path: Option<std::path::PathBuf>) -> Result<ConfigView, CommandError> {
    let config_path = AppConfig::config_path(config_path);
    // Decode + db-path fill, no validation gate (the `login` precedent).
    let config = AppConfig::extract_raw(config_path.as_deref())?;
    let invalid_kind = config
        .validate()
        .err()
        .map(|err: ConfigError| err.kind().to_owned());
    Ok(ConfigView {
        config_path: config_path.as_ref().map(|p| p.display().to_string()),
        backend: match config.sync_backend() {
            SyncBackend::Standalone => "standalone",
            SyncBackend::Nextcloud => "nextcloud",
        }
        .to_owned(),
        valid: invalid_kind.is_none(),
        invalid_kind,
        config,
    })
}

/// The human-readable config view: deterministic `key: value` lines
/// (path, backend, validity, then the four sections' fields). Plain
/// formatting — the config is secret-free by construction.
#[must_use]
pub fn render_text(view: &ConfigView) -> String {
    let c = &view.config;
    let mut lines = vec![
        format!(
            "config_path: {}",
            view.config_path.as_deref().unwrap_or("(none)")
        ),
        format!("backend: {}", view.backend),
        format!("valid: {}", view.valid),
    ];
    if let Some(kind) = &view.invalid_kind {
        lines.push(format!("invalid_kind: {kind}"));
    }
    lines.push(format!("app.mode: {:?}", c.app.mode).to_lowercase());
    lines.push(format!(
        "app.shutdown_timeout_secs: {}",
        c.app.shutdown_timeout_secs
    ));
    lines.push(format!(
        "nextcloud.server_url: {}",
        c.nextcloud.server_url.as_deref().unwrap_or("(unset)")
    ));
    lines.push(format!(
        "nextcloud.username: {}",
        c.nextcloud.username.as_deref().unwrap_or("(unset)")
    ));
    lines.push(format!(
        "nextcloud.credential_store: {}",
        match c.nextcloud.credential_store {
            crate::config::CredentialStoreKind::Keyring => "keyring",
            crate::config::CredentialStoreKind::Env => "env",
        }
    ));
    lines.push(format!(
        "storage.db_path: {}",
        c.storage
            .db_path
            .as_ref()
            .map_or_else(|| "(unset)".to_owned(), |p| p.display().to_string())
    ));
    lines.push(format!(
        "sync.backend: {}",
        c.sync
            .backend
            .map_or_else(|| "(auto)".to_owned(), |b| format!("{b:?}").to_lowercase())
    ));
    lines.push(format!(
        "sync.poll_interval_secs: {}",
        c.sync.poll_interval_secs
    ));
    lines.push(format!(
        "sync.backoff_initial_secs: {}",
        c.sync.backoff_initial_secs
    ));
    lines.push(format!(
        "sync.backoff_max_secs: {}",
        c.sync.backoff_max_secs
    ));
    lines.join("\n")
}

/// The resolved-path diagnosis: which file reads resolve and where
/// `connect`/`login` writes. Pure path resolution — no decode, no
/// validation, no lock, no boot; infallible. Both resolution calls
/// take the original `explicit` option: `write_path` re-resolves
/// internally, so passing the already-resolved read path would collapse
/// the very divergence this verb exists to show.
#[must_use]
pub fn path(explicit: Option<std::path::PathBuf>) -> ConfigPathView {
    ConfigPathView {
        config_path: AppConfig::config_path(explicit.clone())
            .as_ref()
            .map(|p| p.display().to_string()),
        write_path: AppConfig::write_path(explicit).display().to_string(),
    }
}

/// The human-readable path diagnosis: deterministic `key: value` lines
/// in `print`'s style.
#[must_use]
pub fn render_path_text(view: &ConfigPathView) -> String {
    [
        format!(
            "config_path: {}",
            view.config_path.as_deref().unwrap_or("(none)")
        ),
        format!("write_path: {}", view.write_path),
    ]
    .join("\n")
}

/// Dispatch glue so `dispatch` stays a flat table.
pub(crate) fn handle(cmd: &ConfigCmd, ctx: &mut CommandCtx<'_>) -> u8 {
    match *cmd {
        ConfigCmd::Print => match print(ctx.config_path.clone()) {
            Ok(view) => {
                emit_to(&mut ctx.io, &view, &render_text(&view));
                0
            }
            Err(err) => report_to(&mut ctx.io, &err),
        },
        ConfigCmd::Path => {
            let view = path(ctx.config_path.clone());
            emit_to(&mut ctx.io, &view, &render_path_text(&view));
            0
        }
    }
}
