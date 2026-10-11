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
    use taskboard_domain::ids::BoardId;
    use taskboard_domain::outbox::LocalOp;
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
        assert_eq!(applied, 3, "each shipped migration recorded exactly once");
    }

    /// A pre-0002 database (no `ck_title`) upgrades cleanly: the epoch
    /// default keeps the never-locally-written semantics and the row
    /// decodes into the clocked entity.
    #[tokio::test]
    async fn migration_0002_upgrades_a_pre_clock_database() {
        let file = tempfile::Builder::new().suffix(".db").tempfile().unwrap();
        let raw = SqlitePool::connect(&format!("sqlite://{}?mode=rwc", file.path().display()))
            .await
            .expect("raw connect");
        // Simulate a 0001-era database: run the shipped migrations, then
        // strip the 0002 column and its bookkeeping row.
        sqlx::migrate!("./migrations")
            .run(&raw)
            .await
            .expect("migrate");
        sqlx::query("ALTER TABLE boards DROP COLUMN ck_title")
            .execute(&raw)
            .await
            .expect("strip ck_title");
        sqlx::query("DELETE FROM _sqlx_migrations WHERE version = 2")
            .execute(&raw)
            .await
            .expect("strip bookkeeping");
        let _ = sqlx::query(
            "INSERT INTO boards (id, remote_board_id, title, color, archived, deleted, remote_seen) \
             VALUES ('00000000-0000-0000-0000-00000000000b', 42, 'board', '00ff00', 0, 0, NULL)",
        )
        .execute(&raw)
        .await
        .expect("seed row");
        raw.close().await;

        let repo = open(file.path()).await.expect("migrating open");
        let state = repo.load().await.expect("load");
        let board = state.boards.values().next().expect("board row");
        assert_eq!(board.title, "board");
        assert_eq!(
            board.clocks.title,
            taskboard_domain::BoardClocks::epoch(),
            "the epoch default means never-locally-written"
        );
    }

    type LegacyRow = (
        &'static str,
        &'static str,
        Option<uuid::Uuid>,
        Option<uuid::Uuid>,
        Option<uuid::Uuid>,
    );

    /// A pre-0003 database (no outbox `board_id`) upgrades cleanly: the
    /// legacy `rename_board` wire shape (the board id riding `stack_id`)
    /// is backfilled into the board column and `stack_id` is cleared,
    /// while unrelated op rows keep their columns untouched.
    #[tokio::test]
    async fn migration_0003_backfills_legacy_rename_board_rows() {
        let board_uuid = uuid::Uuid::from_u128(0xa1);
        let stack_uuid = uuid::Uuid::from_u128(0xa2);
        let task_uuid = uuid::Uuid::from_u128(0xa3);
        let file = tempfile::Builder::new().suffix(".db").tempfile().unwrap();
        let raw = SqlitePool::connect(&format!("sqlite://{}?mode=rwc", file.path().display()))
            .await
            .expect("raw connect");
        // Simulate a 0002-era database: run the shipped migrations, then
        // strip the 0003 column and its bookkeeping row.
        sqlx::migrate!("./migrations")
            .run(&raw)
            .await
            .expect("migrate");
        sqlx::query("ALTER TABLE outbox DROP COLUMN board_id")
            .execute(&raw)
            .await
            .expect("strip board_id");
        sqlx::query("DELETE FROM _sqlx_migrations WHERE version = 3")
            .execute(&raw)
            .await
            .expect("strip bookkeeping");
        // Legacy shape: the board id rides `stack_id` under the
        // `rename_board` tag; control rows use their ordinary columns.
        let legacy_rows: [LegacyRow; 3] = [
            (
                "0197c0ff-eeee-7ccc-8ddd-0000000000a1",
                "rename_board",
                None,
                Some(board_uuid),
                None,
            ),
            (
                "0197c0ff-eeee-7ccc-8ddd-0000000000a2",
                "rename_stack",
                None,
                Some(stack_uuid),
                None,
            ),
            (
                "0197c0ff-eeee-7ccc-8ddd-0000000000a3",
                "create_task",
                Some(task_uuid),
                None,
                None,
            ),
        ];
        for (op_id, kind, task, stack, label) in legacy_rows {
            let _ = sqlx::query(
                "INSERT INTO outbox (op_id, op_kind, task_id, stack_id, label_id, queued_at) \
                 VALUES (?, ?, ?, ?, ?, '2026-01-01T00:00:00+00:00')",
            )
            .bind(op_id)
            .bind(kind)
            .bind(task.map(|u| u.to_string()))
            .bind(stack.map(|u| u.to_string()))
            .bind(label.map(|u| u.to_string()))
            .execute(&raw)
            .await
            .expect("seed outbox row");
        }
        raw.close().await;

        let repo = open(file.path()).await.expect("migrating open");
        let state = repo.load().await.expect("load");
        assert!(
            state
                .outbox
                .iter()
                .any(|o| o.op == LocalOp::RenameBoard(BoardId::from(board_uuid))),
            "the backfilled rename_board row must still decode to its op"
        );
        drop(repo);

        let raw = SqlitePool::connect(&format!("sqlite://{}", file.path().display()))
            .await
            .expect("raw connect");
        let (board_id, stack_id): (Option<String>, Option<String>) =
            sqlx::query_as("SELECT board_id, stack_id FROM outbox WHERE op_kind = 'rename_board'")
                .fetch_one(&raw)
                .await
                .expect("rename_board row");
        assert_eq!(board_id.as_deref(), Some(board_uuid.to_string().as_str()));
        assert_eq!(stack_id, None, "the legacy stack_id copy must be cleared");
        let (board_id, stack_id): (Option<String>, Option<String>) =
            sqlx::query_as("SELECT board_id, stack_id FROM outbox WHERE op_kind = 'rename_stack'")
                .fetch_one(&raw)
                .await
                .expect("rename_stack row");
        assert_eq!(board_id, None);
        assert_eq!(stack_id.as_deref(), Some(stack_uuid.to_string().as_str()));
        let (task_id, board_id): (Option<String>, Option<String>) =
            sqlx::query_as("SELECT task_id, board_id FROM outbox WHERE op_kind = 'create_task'")
                .fetch_one(&raw)
                .await
                .expect("create_task row");
        assert_eq!(task_id.as_deref(), Some(task_uuid.to_string().as_str()));
        assert_eq!(board_id, None);
        raw.close().await;
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
