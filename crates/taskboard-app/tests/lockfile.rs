// SPDX-License-Identifier: AGPL-3.0-only
//! K-series: the per-database instance lock (plan §6). In-process
//! cases here; the cross-process binary case lives in `cli_lock.rs`.

use std::sync::Arc;

use taskboard_app::bootstrap::{BootstrapError, bootstrap};
use taskboard_app::secrets::CredentialStore;

mod common;

use common::{FakeCredentialStore, tempdir};

use taskboard_app::config::AppConfig;

fn config_for(server_url: &str, dir: &std::path::Path, db: &str) -> AppConfig {
    let mut config = common::valid_config(server_url);
    config.storage.db_path = Some(dir.join(db));
    config
}

/// K1: a second `bootstrap` on the same database while the first `App`
/// lives fails fast with `InstanceLocked` (db path + holder pid).
#[tokio::test]
async fn k1_second_bootstrap_on_a_live_db_is_instance_locked() {
    let dir = tempdir();
    let server = wiremock::MockServer::start().await;
    let store: Arc<dyn CredentialStore> =
        Arc::new(FakeCredentialStore::with(&server.uri(), "alice", "pw"));

    let app = bootstrap(
        config_for(&server.uri(), dir.path(), "k1.db"),
        Arc::clone(&store),
    )
    .await
    .expect("first bootstrap");

    let err = bootstrap(config_for(&server.uri(), dir.path(), "k1.db"), store)
        .await
        .expect_err("second bootstrap on the same db");
    match err {
        BootstrapError::InstanceLocked { db, holder_pid } => {
            assert_eq!(db, dir.path().join("k1.db"));
            assert_eq!(holder_pid, Some(std::process::id()));
        }
        other => panic!("expected InstanceLocked, got {other:?}"),
    }

    app.shutdown().await;
}

/// K2: after the first `App` shut down, the lock is released and a
/// second bootstrap succeeds.
#[tokio::test]
async fn k2_lock_is_released_after_shutdown() {
    let dir = tempdir();
    let server = wiremock::MockServer::start().await;
    let store: Arc<dyn CredentialStore> =
        Arc::new(FakeCredentialStore::with(&server.uri(), "alice", "pw"));

    let app = bootstrap(
        config_for(&server.uri(), dir.path(), "k2.db"),
        Arc::clone(&store),
    )
    .await
    .expect("first bootstrap");
    app.shutdown().await;

    let second = bootstrap(config_for(&server.uri(), dir.path(), "k2.db"), store)
        .await
        .expect("bootstrap after shutdown");
    second.shutdown().await;
}

/// K4: the sidecar is `<db>.lock` next to the database file.
#[tokio::test]
async fn k4_lock_sidecar_is_a_db_sibling() {
    let dir = tempdir();
    let server = wiremock::MockServer::start().await;
    let store: Arc<dyn CredentialStore> =
        Arc::new(FakeCredentialStore::with(&server.uri(), "alice", "pw"));

    let app = bootstrap(config_for(&server.uri(), dir.path(), "k4.db"), store)
        .await
        .expect("bootstrap");
    assert!(dir.path().join("k4.lock").is_file(), "sidecar is <db>.lock");
    app.shutdown().await;
}

/// K5: two bootstraps with *different* db paths coexist (the lock is
/// db-scoped, so worktrees/CI never contend).
#[tokio::test]
async fn k5_different_databases_do_not_contend() {
    let dir = tempdir();
    let server = wiremock::MockServer::start().await;
    let store: Arc<dyn CredentialStore> =
        Arc::new(FakeCredentialStore::with(&server.uri(), "alice", "pw"));

    let one = bootstrap(
        config_for(&server.uri(), dir.path(), "k5a.db"),
        Arc::clone(&store),
    )
    .await
    .expect("first db");
    let two = bootstrap(config_for(&server.uri(), dir.path(), "k5b.db"), store)
        .await
        .expect("second db");
    one.shutdown().await;
    two.shutdown().await;
}
