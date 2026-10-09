// SPDX-License-Identifier: MIT OR Apache-2.0
//! Connection setup: pragmas, pool shape, and boot-time migrations.

use std::path::Path;
use std::str::FromStr;
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
fn connect_options(url: &str) -> Result<SqliteConnectOptions, OpenError> {
    Ok(SqliteConnectOptions::from_str(url)
        .map_err(OpenError::Connect)?
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(5)))
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

async fn open_with(url: &str, memory: bool) -> Result<SqliteTaskRepository, OpenError> {
    let pool: SqlitePool = pool_options(memory)
        .connect_with(connect_options(url)?)
        .await
        .map_err(OpenError::Connect)?;
    MIGRATOR.run(&pool).await.map_err(OpenError::Migrate)?;
    tracing::info!(
        migrations = MIGRATOR.iter().count(),
        "sqlite database opened; schema ensured"
    );
    Ok(SqliteTaskRepository { pool })
}

/// Opens (creating if absent) the database at `path`, applies pending
/// migrations, and returns the repository.
///
/// # Errors
///
/// [`OpenError::Connect`] when the file cannot be opened;
/// [`OpenError::Migrate`] when applying pending migrations fails.
pub async fn open(path: impl AsRef<Path>) -> Result<SqliteTaskRepository, OpenError> {
    let path = path.as_ref();
    tracing::debug!(path = %path.display(), "opening sqlite database");
    open_with(&format!("sqlite://{}", path.display()), false).await
}

/// In-memory variant for tests. The database is per-connection, so the pool
/// pins its single connection open; WAL is a harmless no-op on `:memory:`.
///
/// # Errors
///
/// [`OpenError::Connect`] if the pool cannot be created;
/// [`OpenError::Migrate`] if the embedded migrations fail.
pub async fn open_memory() -> Result<SqliteTaskRepository, OpenError> {
    open_with("sqlite::memory:", true).await
}
