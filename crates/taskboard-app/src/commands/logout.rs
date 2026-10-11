// SPDX-License-Identifier: AGPL-3.0-only
//! `taskboard logout` (ADR 0009): delete the stored secret, then —
//! under the *auto* backend only — strip the config's
//! `nextcloud.server_url`/`username` keys so the install cleanly
//! resolves back to stand-alone. An explicit `backend = "nextcloud"` is
//! never rewritten (ADR 0010: the loud choice stays loud; the next boot
//! fails at credential resolution instead of silently flipping shape).
//! Pre-core: no actor graph, no database, no lock.

use crate::commands::CommandError;
use crate::commands::output::LogoutReceipt;
use crate::config::AppConfig;
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
    })
}

/// The human-readable receipt.
#[must_use]
pub fn render_text(receipt: &LogoutReceipt) -> String {
    if receipt.config_cleared {
        format!(
            "logged out of {} as {} (config cleared — back to stand-alone)",
            receipt.server_url, receipt.username
        )
    } else {
        format!(
            "logged out of {} as {} (config keys kept: explicit backend)",
            receipt.server_url, receipt.username
        )
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
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("taskboard-tmp");
    {
        let mut file = std::fs::File::create(&tmp)?;
        std::io::Write::write_all(&mut file, doc.to_string().as_bytes())?;
        file.sync_all().ok();
    }
    std::fs::rename(&tmp, path)?;
    Ok(true)
}
