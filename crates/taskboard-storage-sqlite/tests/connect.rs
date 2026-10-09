// SPDX-License-Identifier: MIT OR Apache-2.0
//! Connection semantics: fresh databases, idempotent migrations, pragmas.

use taskboard_domain::persistence::{PersistedState, TaskRepository};
use taskboard_storage_sqlite::{open, open_memory};

/// A fresh database loads a default-shaped state (derived `pending_ops` = 0).
#[tokio::test]
async fn fresh_database_loads_default_state() {
    let repo = open_memory().await.expect("open");
    let loaded = repo.load().await.expect("load");
    assert_eq!(loaded, PersistedState::default());
}

/// Opening the same file twice applies zero new migrations (idempotence).
#[tokio::test]
async fn reopening_applies_no_new_migrations() {
    let file = tempfile::Builder::new().suffix(".db").tempfile().unwrap();
    let first = open(file.path()).await.expect("first open");
    drop(first);

    let second = open(file.path()).await.expect("second open");
    second.load().await.expect("load");
    let raw = sqlx::sqlite::SqlitePool::connect(&format!("sqlite://{}", file.path().display()))
        .await
        .expect("raw connect");
    let applied: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations")
        .fetch_one(&raw)
        .await
        .expect("migration bookkeeping");
    assert_eq!(applied, 1, "exactly one migration must be recorded");
}

/// The durability pragmas are actually in force: `journal_mode` is WAL.
#[tokio::test]
async fn journal_mode_is_wal() {
    let file = tempfile::Builder::new().suffix(".db").tempfile().unwrap();
    let repo = open(file.path()).await.expect("open");
    let mode: String = sqlx::query_scalar("PRAGMA journal_mode")
        .fetch_one(&repo_pool(&repo))
        .await
        .expect("pragma");
    assert_eq!(mode, "wal");
}

/// The pool holds exactly one connection (kiosk single-writer stance).
#[tokio::test]
async fn pool_is_single_connection() {
    let repo = open_memory().await.expect("open");
    assert_eq!(repo_pool(&repo).size(), 1);
}

use sqlx::SqlitePool;
use taskboard_storage_sqlite::SqliteTaskRepository;

fn repo_pool(repo: &SqliteTaskRepository) -> SqlitePool {
    repo.pool().clone()
}
