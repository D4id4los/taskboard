// SPDX-License-Identifier: MIT OR Apache-2.0
//! Shared helpers for the storage crate's integration tests.
#![allow(dead_code)] // each test binary uses a different subset

use std::path::{Path, PathBuf};

use taskboard_domain::persistence::{PersistedState, PersistenceAction};
use taskboard_domain::reminders::ReminderSnooze;
use taskboard_storage_sqlite::{SqliteTaskRepository, open};

/// The one `apply` batch that persists a whole state (the batching
/// discipline Phase 3's engine will use): entity upserts, outbox appends,
/// reminder facts, validator upserts, and the sync status.
#[must_use]
pub fn actions_from_state(state: &PersistedState) -> Vec<PersistenceAction> {
    let mut actions = Vec::new();
    for board in state.boards.values() {
        actions.push(PersistenceAction::UpsertBoard(board.clone()));
    }
    for stack in state.stacks.values() {
        actions.push(PersistenceAction::UpsertStack(stack.clone()));
    }
    for label in state.labels.values() {
        actions.push(PersistenceAction::UpsertLabel(label.clone()));
    }
    for task in state.tasks.values() {
        actions.push(PersistenceAction::UpsertTask(task.clone()));
    }
    for op in &state.outbox {
        actions.push(PersistenceAction::EnqueueOp(op.clone()));
    }
    for rule in state.reminders.rules.values() {
        actions.push(PersistenceAction::UpsertReminderRule(rule.clone()));
    }
    for delivery in &state.reminders.deliveries {
        actions.push(PersistenceAction::RecordReminderDelivery(*delivery));
    }
    for (key, rescheduled_to) in &state.reminders.snoozes {
        actions.push(PersistenceAction::RecordReminderSnooze(ReminderSnooze {
            reminder: key.reminder,
            occurrence: key.occurrence,
            rescheduled_to: *rescheduled_to,
        }));
    }
    for task in &state.reminders.muted {
        actions.push(PersistenceAction::SetTaskRemindersMuted(*task, true));
    }
    for (key, validators) in &state.validators {
        actions.push(PersistenceAction::UpsertValidators(
            *key,
            validators.clone(),
        ));
    }
    actions.push(PersistenceAction::UpsertSyncStatus(state.sync.clone()));
    actions
}

/// The `-wal`/`-shm` sibling paths of a database file (issue #313: these
/// are exactly the files `NamedTempFile`'s single-file cleanup misses).
#[must_use]
pub fn sidecar_paths(db: &Path) -> (PathBuf, PathBuf) {
    (sibling(db, "-wal"), sibling(db, "-shm"))
}

fn sibling(db: &Path, suffix: &str) -> PathBuf {
    let mut name = db.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

/// A file-backed database fixture: owns the temp file and the repository,
/// and closes the pool *before* the temp handle drops. sqlx 0.9's `Pool`
/// drop only marks the pool closed — sqlite's `-wal`/`-shm` siblings
/// survive `NamedTempFile`'s single-file cleanup (issue #313), so
/// disposal must be awaited, never dropped.
pub struct FileDb {
    repo: Option<SqliteTaskRepository>,
    // In a struct: fields drop repo-first, so the pool is gone before the
    // file handle — plus the `Drop` backstop unlinks any sidecars a
    // forgetful path left behind.
    file: tempfile::NamedTempFile,
}

impl FileDb {
    pub async fn new() -> Self {
        let file = tempfile::Builder::new()
            .suffix(".db")
            .tempfile()
            .expect("tempfile");
        let repo = open(file.path()).await.expect("open");
        Self {
            repo: Some(repo),
            file,
        }
    }

    /// The database file's path (for raw second connections).
    #[must_use]
    pub fn path(&self) -> &Path {
        self.file.path()
    }

    /// The repository; panics after [`close`](Self::close).
    #[must_use]
    pub fn repo(&self) -> &SqliteTaskRepository {
        self.repo.as_ref().expect("database still open")
    }

    /// Disposes the pool deterministically before the temp file drops.
    pub async fn close(&mut self) {
        if let Some(repo) = self.repo.take() {
            repo.close().await;
        }
    }

    /// Closes and reopens the same file (close/reopen cycles).
    pub async fn reopen(&mut self) {
        self.close().await;
        self.repo = Some(open(self.file.path()).await.expect("reopen"));
    }
}

impl Drop for FileDb {
    fn drop(&mut self) {
        // Forgetful-path backstop: drop the repo, then unlink the sidecar
        // siblings synchronously — `NamedTempFile` only removes its own
        // named file. Best-effort on every platform.
        drop(self.repo.take());
        for suffix in ["-wal", "-shm"] {
            let _ = std::fs::remove_file(sibling(self.file.path(), suffix));
        }
    }
}
