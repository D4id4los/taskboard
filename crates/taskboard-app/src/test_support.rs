// SPDX-License-Identifier: AGPL-3.0-only
//! Crate-internal test-support fixtures (plan
//! `2026-11-18-journey-fixture-seams` §D4): the shared
//! standalone-boot shape — a seeded config file, a tempdir-backed
//! database, and the credential-refusing store — booting a real
//! engine. The tempdir guard lives in [`StandaloneFixture`], so the
//! paths are valid exactly as long as the assertion window — no
//! leaked guard on this path (#914).

#![forbid(unsafe_code)]

use std::sync::Arc;

use crate::config::{AppConfig, SyncBackend};

/// A credential-refusing store (the bootstrap test's shape): the
/// config arm never consults it. Shared by every fixture consumer so
/// the refusal semantics stay one definition.
#[derive(Debug)]
pub(crate) struct RefusingStore;

/// Convenience constructor: one `Arc<dyn CredentialStore>` per call
/// site, killing the repeated `Arc::new(RefusingStore)` spell (#981).
pub(crate) fn refusing_store() -> Arc<dyn crate::secrets::CredentialStore> {
    Arc::new(RefusingStore)
}

impl crate::secrets::CredentialStore for RefusingStore {
    fn load(
        &self,
        _: &str,
        _: &str,
    ) -> Result<crate::secrets::AppPassword, crate::secrets::CredentialError> {
        Err(crate::secrets::CredentialError::NotFound)
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

/// A booted standalone core plus everything it borrows from disk: the
/// tempdir guard is owned here, so the seeded config and the database
/// live exactly as long as the fixture binding does. Plain fields —
/// no `Deref`, no cleverness.
pub(crate) struct StandaloneFixture {
    /// The tempdir backing the config file and the database; drops
    /// with the fixture (the #914 leak's replacement). Never read —
    /// held for its `Drop` side effect, hence the underscore.
    #[allow(dead_code)]
    pub(crate) dir: tempfile::TempDir,
    /// The seeded `taskboard-{tag}.toml` inside [`Self::dir`].
    pub(crate) config_path: std::path::PathBuf,
    /// The booted core.
    pub(crate) app: crate::bootstrap::App,
}

/// The shared standalone-boot fixture (the `ui_requests` harness
/// shape — fakes over mocks; a real engine): seeds
/// `taskboard-{tag}.toml` with the reminders tick, points the storage
/// at the fixture's tempdir, and boots the standalone core against
/// [`RefusingStore`].
pub(crate) async fn standalone_boot_fixture(tag: &str) -> StandaloneFixture {
    let dir = tempfile::tempdir().expect("tempdir");
    let config_path = dir.path().join(format!("taskboard-{tag}.toml"));
    tokio::fs::write(&config_path, "[reminders]\ntick_secs = 30\n")
        .await
        .expect("seed config");
    let mut config = AppConfig::default();
    config.storage.db_path = Some(dir.path().join("taskboard.db"));
    config.sync.backend = Some(SyncBackend::Standalone);
    config.validate().expect("config valid");
    let app = crate::bootstrap::bootstrap_in(
        config,
        refusing_store(),
        Arc::new(crate::notifications::LogSink),
        crate::bootstrap::BootIdentity::At(dir.path().to_owned()),
    )
    .await
    .expect("standalone boot");
    StandaloneFixture {
        dir,
        config_path,
        app,
    }
}
