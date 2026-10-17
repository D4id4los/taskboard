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
        Arc::new(taskboard_app::notifications::LogSink),
    )
    .await
    .expect("first bootstrap");

    let err = bootstrap(
        config_for(&server.uri(), dir.path(), "k1.db"),
        store,
        Arc::new(taskboard_app::notifications::LogSink),
    )
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
        Arc::new(taskboard_app::notifications::LogSink),
    )
    .await
    .expect("first bootstrap");
    app.shutdown().await;

    let second = bootstrap(
        config_for(&server.uri(), dir.path(), "k2.db"),
        store,
        Arc::new(taskboard_app::notifications::LogSink),
    )
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

    let app = bootstrap(
        config_for(&server.uri(), dir.path(), "k4.db"),
        store,
        Arc::new(taskboard_app::notifications::LogSink),
    )
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
        Arc::new(taskboard_app::notifications::LogSink),
    )
    .await
    .expect("first db");
    let two = bootstrap(
        config_for(&server.uri(), dir.path(), "k5b.db"),
        store,
        Arc::new(taskboard_app::notifications::LogSink),
    )
    .await
    .expect("second db");
    one.shutdown().await;
    two.shutdown().await;
}

/// K6: the `status` client-side answer's diagnostic field — the
/// recorded holder PID is readable *while* the lock is held, and it is
/// the holder's own pid (the binary-level REPL row is composition:
/// R-series covers the session boot, K3 the cross-process lock).
#[tokio::test]
async fn k6_holder_pid_reads_the_recorded_holder_while_locked() {
    let dir = tempdir();
    let server = wiremock::MockServer::start().await;
    let store: Arc<dyn CredentialStore> =
        Arc::new(FakeCredentialStore::with(&server.uri(), "alice", "pw"));
    let db = dir.path().join("k6.db");
    let _app = bootstrap(
        config_for(&server.uri(), dir.path(), "k6.db"),
        store,
        Arc::new(taskboard_app::notifications::LogSink),
    )
    .await
    .expect("boots");
    assert_eq!(
        taskboard_app::lockfile::holder_pid(&db),
        Some(std::process::id())
    );
}

// -- owner-only sidecar modes (F12-01) ------------------------------------

/// Asserts the mode bits recorded in the metadata view (never the
/// access-as-another-user view, which root would defeat).
#[cfg(unix)]
fn owner_mode_is(path: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .expect("sidecar exists")
        .permissions()
        .mode()
        & 0o777
        == 0o600
}

/// F12-01 (#297): a freshly created lock sidecar is owner read/write
/// only (ADR 0012 — OS permissions are the whole auth story).
#[cfg(unix)]
#[test]
fn the_lock_sidecar_is_owner_only() {
    let dir = tempdir();
    let db = dir.path().join("modes.db");

    let lock = taskboard_app::lockfile::InstanceLock::acquire(&db).expect("acquire");
    assert!(owner_mode_is(lock.path()), "the fresh sidecar must be 0600");
}

/// F12-01 (#297): acquiring a lock tightens a pre-existing
/// world-readable sidecar (a crashed run's file, or a pre-hardening
/// leftover) — only the holder touches the mode.
#[cfg(unix)]
#[test]
fn acquire_tightens_a_preexisting_world_readable_sidecar() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempdir();
    let db = dir.path().join("loose.db");
    let sidecar = dir.path().join("loose.lock");
    std::fs::write(&sidecar, "pid=1\n").expect("pre-create sidecar");
    let mut perms = std::fs::metadata(&sidecar).expect("meta").permissions();
    perms.set_mode(0o644);
    std::fs::set_permissions(&sidecar, perms).expect("loosen");

    let lock = taskboard_app::lockfile::InstanceLock::acquire(&db).expect("acquire succeeds");
    assert!(owner_mode_is(lock.path()), "acquire must tighten to 0600");
    assert_eq!(
        taskboard_app::lockfile::holder_pid(&db),
        Some(std::process::id()),
        "the holder record survives the tighten"
    );
}
