// SPDX-License-Identifier: AGPL-3.0-only

//! The connection-management ceremony core (round-1 chunk 3, plan D1):
//! the non-interactive halves of the connect ceremony, extracted by
//! *moving bodies verbatim* out of [`crate::commands::connect`] so the
//! REPL one-shot and the UI request forwarder share them (ADR 0014,
//! D25b). Extract, don't rewrite: the unchanged connect/logout E2E
//! suites are the equivalence gate (plan A11).

use std::sync::Arc;

use taskboard_sync_nextcloud::{DeckClient, DeckError};

use crate::commands::{CommandError, map_deck_error};
use crate::secrets::{AppPassword, CredentialError, CredentialStore};

/// Verifies the credentials against the server (`boards()` is the
/// verification call). Moved verbatim from `connect.rs::verify`
/// (plan F-B1), parameterized by fields instead of the one-shot's
/// private `Identity`.
///
/// # Errors
///
/// [`CommandError`] — the mapped [`DeckError`] classes; nothing is
/// persisted by the caller unless this returned `Ok`.
pub(crate) async fn verify_credentials(
    server: &str,
    user: &str,
    password: &str,
) -> Result<(), CommandError> {
    let client = DeckClient::new(server, user, password).map_err(map_deck_error)?;
    match client.boards().await {
        Ok(_) => Ok(()),
        Err(err) => {
            tracing::info!(variant = deck_variant(&err), "connect verification failed");
            Err(map_deck_error(err))
        }
    }
}

/// Stores the new secret; on a re-point, deletes the replaced
/// identity's secret **after** the new store (D7 — a crash between
/// leaves an orphan, never a broken install). The deletion tolerates
/// `NotFound` (idempotent) and `Unsupported` (the env store has no
/// secret to orphan); any other failure is typed. Moved verbatim from
/// `connect.rs::store_secret` (plan F-B1), parameterized by fields.
///
/// # Errors
///
/// [`CommandError::Credential`] on a store failure other than the
/// tolerated rotation arms.
pub(crate) fn store_and_rotate_secret(
    store: &Arc<dyn CredentialStore>,
    server: &str,
    user: &str,
    password: &str,
    old: Option<(&str, &str)>,
) -> Result<String, CommandError> {
    // A read-only store (env) is a *successful* connect with
    // `stored: "env"`; no secret is ever written to any file.
    let stored = match store.store(server, user, AppPassword::new(password.to_owned())) {
        Ok(()) => "keyring",
        Err(CredentialError::Unsupported) => "env",
        Err(err) => return Err(CommandError::Credential(err)),
    };
    if let Some((old_server, old_user)) = old {
        match store.delete(old_server, old_user) {
            Ok(()) | Err(CredentialError::NotFound | CredentialError::Unsupported) => {}
            Err(err) => return Err(CommandError::Credential(err)),
        }
    }
    Ok(stored.to_owned())
}

/// The bare variant name of a `DeckError` (log-only; never the secret,
/// never a wire payload). Moved with [`verify_credentials`].
fn deck_variant(err: &DeckError) -> &'static str {
    match err {
        DeckError::InvalidBaseUrl => "InvalidBaseUrl",
        DeckError::InvalidUpload => "InvalidUpload",
        DeckError::Transport(_) => "Transport",
        DeckError::Envelope(_) => "Envelope",
        DeckError::Unauthorized => "Unauthorized",
        DeckError::Forbidden => "Forbidden",
        DeckError::NotFound => "NotFound",
        DeckError::BadRequest => "BadRequest",
        DeckError::Conflict => "Conflict",
        DeckError::PreconditionFailed => "PreconditionFailed",
        DeckError::RateLimited { .. } => "RateLimited",
        DeckError::Server(_) => "Server",
        DeckError::Unavailable => "Unavailable",
    }
}
