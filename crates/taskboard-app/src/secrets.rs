// SPDX-License-Identifier: AGPL-3.0-only
//! Credential resolution (ADR 0008): a [`CredentialStore`] port with two
//! config-selected implementations — [`KeyringStore`] (production) and
//! [`EnvCredentialStore`] (the headless/CI escape hatch).
//!
//! There is **no silent fallback**: the store is chosen explicitly by
//! `[nextcloud] credential_store`, and a headless machine without a
//! Secret Service surfaces a typed [`CredentialError::Backend`] (whose
//! `Display` names the env alternative) rather than degrading quietly.
//!
//! Secrets travel in the redacting [`AppPassword`] newtype: `Debug`
//! prints `[redacted]`, there is no `Display`, and the only read access
//! is [`AppPassword::expose`] — consumed exactly once by
//! `DeckClient::new`.

use keyring::Entry;

/// The keyring service name every taskboard entry lives under.
pub const KEYRING_SERVICE: &str = "taskboard";

/// The environment variable [`EnvCredentialStore`] reads.
pub const ENV_PASSWORD_VAR: &str = "TASKBOARD_APP_PASSWORD";

/// The Nextcloud app password. Redacting `Debug`, no `Display`; the only
/// read access is [`expose`](AppPassword::expose). Never logged, never
/// serialized.
#[derive(Clone)]
pub struct AppPassword(String);

impl AppPassword {
    /// Wraps a secret. Callers own keeping the source out of logs.
    #[must_use]
    pub fn new(secret: impl Into<String>) -> Self {
        Self(secret.into())
    }

    /// The only read access; consumed by `DeckClient::new`.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for AppPassword {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("AppPassword").field(&"[redacted]").finish()
    }
}

/// Credential failure classes, matched on variant shape only.
#[derive(Debug, thiserror::Error)]
pub enum CredentialError {
    /// No credential is stored for this (server, username).
    #[error("no credential stored")]
    NotFound,
    /// The OS credential backend is unavailable or failed. On a headless
    /// Linux box this is the signal to opt into `credential_store =
    /// "env"` explicitly.
    #[error(
        "credential backend unavailable (headless? set `credential_store = \"env\"` and export TASKBOARD_APP_PASSWORD)"
    )]
    Backend(#[source] keyring::Error),
    /// This store cannot `store`/`delete` (the env store is read-only).
    #[error("operation not supported by this store")]
    Unsupported,
}

/// OS-side credential storage seam (fakes over mocks: tests use a
/// `HashMap`-backed store; production selects via
/// `[nextcloud] credential_store`).
pub trait CredentialStore: Send + Sync + std::fmt::Debug {
    /// Loads the app password for `(server, username)`.
    ///
    /// # Errors
    ///
    /// [`CredentialError`] — typed, see the variant docs.
    fn load(&self, server: &str, username: &str) -> Result<AppPassword, CredentialError>;

    /// Stores the app password for `(server, username)` (Phase 6
    /// `login`; shipped now so the seam is final).
    ///
    /// # Errors
    ///
    /// [`CredentialError`] — typed, see the variant docs.
    fn store(
        &self,
        server: &str,
        username: &str,
        secret: AppPassword,
    ) -> Result<(), CredentialError>;

    /// Removes the app password for `(server, username)` (Phase 6
    /// `logout`).
    ///
    /// # Errors
    ///
    /// [`CredentialError`] — typed, see the variant docs.
    fn delete(&self, server: &str, username: &str) -> Result<(), CredentialError>;
}

/// The `keyring` adapter: `Entry::new("taskboard", credential_key(..))`.
///
/// Error mapping: `keyring::Error::NoEntry` →
/// [`CredentialError::NotFound`]; everything else →
/// [`CredentialError::Backend`] — matched on variant shape, never
/// message text.
#[derive(Debug, Default)]
pub struct KeyringStore;

impl CredentialStore for KeyringStore {
    fn load(&self, server: &str, username: &str) -> Result<AppPassword, CredentialError> {
        let entry = entry(server, username)?;
        entry
            .get_password()
            .map(AppPassword)
            .map_err(map_keyring_error)
    }

    fn store(
        &self,
        server: &str,
        username: &str,
        secret: AppPassword,
    ) -> Result<(), CredentialError> {
        let entry = entry(server, username)?;
        entry
            .set_password(secret.expose())
            .map_err(CredentialError::Backend)
    }

    fn delete(&self, server: &str, username: &str) -> Result<(), CredentialError> {
        let entry = entry(server, username)?;
        entry.delete_credential().map_err(map_keyring_error)
    }
}

fn entry(server: &str, username: &str) -> Result<Entry, CredentialError> {
    Entry::new(KEYRING_SERVICE, &credential_key(server, username)).map_err(CredentialError::Backend)
}

/// The keyring → typed error mapping: `NoEntry` means "nothing stored"
/// ([`CredentialError::NotFound`]); every other backend failure is a
/// [`CredentialError::Backend`] carrying the source. Matched on variant
/// shape, never message text.
/// The public mapping is part of the error-mapping contract (S5 pins it);
/// exposed for tests and future callers that translate keyring errors.
#[must_use]
pub fn map_keyring_error(err: keyring::Error) -> CredentialError {
    match err {
        keyring::Error::NoEntry => CredentialError::NotFound,
        other => CredentialError::Backend(other),
    }
}

/// Read-only escape hatch for headless machines and CI: loads
/// [`ENV_PASSWORD_VAR`]; `store`/`delete` are
/// [`CredentialError::Unsupported`]. The variable is deliberately *not*
/// `TASKBOARD_`-nested-config shaped: it is a secret read by this
/// module, not config consumed by figment.
#[derive(Debug, Default)]
pub struct EnvCredentialStore;

impl CredentialStore for EnvCredentialStore {
    fn load(&self, _server: &str, _username: &str) -> Result<AppPassword, CredentialError> {
        // Non-secret env names may use std::env; the value itself is
        // never logged or formatted.
        match std::env::var(ENV_PASSWORD_VAR) {
            Ok(secret) if !secret.is_empty() => Ok(AppPassword(secret)),
            _ => Err(CredentialError::NotFound),
        }
    }

    fn store(
        &self,
        _server: &str,
        _username: &str,
        _secret: AppPassword,
    ) -> Result<(), CredentialError> {
        Err(CredentialError::Unsupported)
    }

    fn delete(&self, _server: &str, _username: &str) -> Result<(), CredentialError> {
        Err(CredentialError::Unsupported)
    }
}

/// Entry identity: `{username}@{host-of-server}` — the server host is
/// part of the key, so a config edit pointing at another server cannot
/// silently reuse a stale password. Hostless/garbage inputs still
/// produce a stable key (pure function, never panics): the trimmed input
/// stands in for the host.
#[must_use]
pub fn credential_key(server: &str, username: &str) -> String {
    let rest = server.split_once("://").map_or(server, |(_, r)| r);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    let with_port = authority.rsplit_once('@').map_or(authority, |(_, a)| a);
    let host = with_port.rsplit_once(':').map_or(with_port, |(h, _)| h);
    let host = if host.trim().is_empty() {
        server.trim()
    } else {
        host
    };
    format!("{username}@{host}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_password_debug_redacts_and_has_no_display() {
        let pw = AppPassword::new("s3cret-app-pw");
        let formatted = format!("{pw:?}");
        assert!(!formatted.contains("s3cret-app-pw"));
        assert!(formatted.contains("[redacted]"));
        assert_eq!(pw.expose(), "s3cret-app-pw");
    }

    #[test]
    fn credential_key_is_username_at_host() {
        assert_eq!(
            credential_key("https://cloud.example.com", "alice"),
            "alice@cloud.example.com"
        );
        // Port is part of the address but not of the identity key.
        assert_eq!(
            credential_key("http://localhost:8080/nextcloud", "bob"),
            "bob@localhost"
        );
    }

    #[test]
    fn credential_key_is_stable_for_garbage_inputs() {
        for server in ["", "   ", "not a url", "://", "https://"] {
            let key = credential_key(server, "u");
            assert_eq!(key, credential_key(server, "u"), "stable for {server:?}");
            assert!(key.starts_with("u@"), "shape kept for {server:?}");
        }
    }

    #[test]
    fn env_store_is_read_only() {
        assert!(matches!(
            EnvCredentialStore.store("https://x", "u", AppPassword::new("v")),
            Err(CredentialError::Unsupported)
        ));
        assert!(matches!(
            EnvCredentialStore.delete("https://x", "u"),
            Err(CredentialError::Unsupported)
        ));
    }
}
