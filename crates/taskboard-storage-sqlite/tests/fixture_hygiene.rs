// SPDX-License-Identifier: MIT OR Apache-2.0
//! #313 — teardown guards for the file-database fixture: a cleanly closed
//! sqlite database leaves no `-wal`/`-shm` sidecar siblings behind, and
//! the fixture's `Drop` backstop removes them even on a forgetful path.
//! Both tests check only the exact sibling paths of the fixture's own
//! file — never a `/tmp` scan.

mod common;

use common::{FileDb, sidecar_paths};
use taskboard_domain::persistence::{PersistenceAction, TaskRepository};
use taskboard_domain::state::{SyncErrorKind, SyncPhase, SyncStatus};

/// One write so sqlite actually materializes the WAL sidecars.
async fn write_one(db: &FileDb) {
    db.repo()
        .apply(vec![PersistenceAction::UpsertSyncStatus(SyncStatus {
            phase: SyncPhase::Failed {
                last_error: SyncErrorKind::Network,
            },
            last_success: None,
            pending_ops: 0,
        })])
        .await
        .expect("apply");
}

/// Guard 1 — `close().await` is a *clean* sqlite close: sqlite itself
/// checkpoints and removes the sidecars (not the fixture's unlinks).
#[tokio::test]
async fn closed_file_databases_leave_no_wal_sidecars() {
    let mut db = FileDb::new().await;
    let db_path = db.path().to_path_buf();
    let (wal, shm) = sidecar_paths(&db_path);
    write_one(&db).await;
    // Non-vacuity: the write must have produced at least one sidecar
    // while the database is open (disjunction tolerant of sidecar-creation
    // timing differences across platforms).
    assert!(
        wal.exists() || shm.exists(),
        "precondition: an open written WAL database has sidecars"
    );

    db.close().await;

    assert!(!wal.exists(), "clean close removes the -wal sibling");
    assert!(!shm.exists(), "clean close removes the -shm sibling");
}

/// Guard 2 — even a plain `drop` without `close` leaves no sidecars: the
/// fixture's `Drop` backstop unlinks them synchronously before it returns.
#[tokio::test]
async fn dropping_without_close_still_leaves_no_sidecars() {
    let db = FileDb::new().await;
    let db_path = db.path().to_path_buf();
    let (wal, shm) = sidecar_paths(&db_path);
    write_one(&db).await;

    drop(db);

    assert!(!wal.exists(), "the Drop backstop removes the -wal sibling");
    assert!(!shm.exists(), "the Drop backstop removes the -shm sibling");
    assert!(!db_path.exists(), "NamedTempFile removes the database file");
}
