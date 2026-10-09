// SPDX-License-Identifier: MIT OR Apache-2.0
//! Connection setup: pragmas, pool shape, and boot-time migrations.

use std::path::Path;
use std::time::Duration;

use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};

use crate::error::OpenError;
use crate::repo::SqliteTaskRepository;

/// Embedded migrations; [`open`]/[`open_memory`] apply them before the
/// repository is usable, so production boots need neither `sqlx-cli` nor
/// access to the migration sources (plan decision 5).
static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// The connection recipe every database gets (plan decision 4): WAL,
/// `synchronous=NORMAL`, FK enforcement, a busy timeout. sqlx logs every
/// statement at debug level by default, which makes a bug-report log
/// localize the failing SQL.
fn apply_pragmas(options: SqliteConnectOptions) -> SqliteConnectOptions {
    options
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(5))
}

/// The single-connection pool shape (plan decision 4): the storage actor is
/// the only writer, so one connection eliminates the `SQLITE_BUSY` class —
/// and for `:memory:` databases it is *required*, because such a database
/// lives per connection.
fn pool_options(memory: bool) -> SqlitePoolOptions {
    let options = SqlitePoolOptions::new().max_connections(1);
    if memory {
        options
            .min_connections(1)
            .idle_timeout(None)
            .max_lifetime(None)
    } else {
        options
    }
}

async fn open_with(
    options: SqliteConnectOptions,
    memory: bool,
) -> Result<SqliteTaskRepository, OpenError> {
    let pool: SqlitePool = pool_options(memory)
        .connect_with(options)
        .await
        .map_err(OpenError::Connect)?;
    MIGRATOR.run(&pool).await.map_err(OpenError::Migrate)?;
    tracing::info!(
        migrations_embedded = MIGRATOR.iter().count(),
        "sqlite database opened; schema ensured"
    );
    Ok(SqliteTaskRepository { pool })
}

/// Opens (creating if absent) the database at `path`, applies pending
/// migrations, and returns the repository.
///
/// The path is handed to sqlite as a filename (never interpolated into a
/// connection URL), so `%`, `?`, and non-UTF-8 path components survive.
///
/// # Errors
///
/// [`OpenError::Connect`] when the file cannot be opened;
/// [`OpenError::Migrate`] when applying pending migrations fails.
pub async fn open(path: impl AsRef<Path>) -> Result<SqliteTaskRepository, OpenError> {
    let path = path.as_ref();
    tracing::debug!(path = %path.display(), "opening sqlite database");
    open_with(
        apply_pragmas(SqliteConnectOptions::new()).filename(path),
        false,
    )
    .await
}

/// In-memory variant for tests. The database is per-connection, so the pool
/// pins its single connection open; WAL is a harmless no-op on `:memory:`.
///
/// # Errors
///
/// [`OpenError::Connect`] if the pool cannot be created;
/// [`OpenError::Migrate`] if the embedded migrations fail.
pub async fn open_memory() -> Result<SqliteTaskRepository, OpenError> {
    let parsed: SqliteConnectOptions = "sqlite::memory:".parse().map_err(OpenError::Connect)?;
    open_with(apply_pragmas(parsed), true).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use taskboard_domain::persistence::{PersistedState, TaskRepository};

    /// A fresh database loads a default-shaped state (derived
    /// `pending_ops` = 0).
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
        let raw = SqlitePool::connect(&format!("sqlite://{}", file.path().display()))
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
            .fetch_one(&repo.pool)
            .await
            .expect("pragma");
        assert_eq!(mode, "wal");
    }

    /// The pool holds exactly one connection (kiosk single-writer stance).
    #[tokio::test]
    async fn pool_is_single_connection() {
        let repo = open_memory().await.expect("open");
        assert_eq!(repo.pool.size(), 1);
    }
}
