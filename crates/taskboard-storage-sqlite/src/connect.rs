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
    default_connector: taskboard_domain::ConnectorId,
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
    Ok(SqliteTaskRepository {
        pool,
        default_connector,
    })
}

/// Owner-only modes for the database file and its `-wal`/`-shm`
/// siblings (F12-01; ADR 0012 — OS object permissions are the whole v1
/// auth story). sqlx owns the create (`create_if_missing`), so the
/// tighten happens post-open, on the same synchronous path; a missing
/// sibling is skipped (sqlite creates them at its own discretion) and
/// any other failure warns without being fatal — the ipc socket's
/// `harden` stance (kiosk boot resilience).
#[cfg(unix)]
fn harden_sibling_modes(db: &Path) {
    use std::os::unix::fs::PermissionsExt;

    let mut paths = vec![db.to_owned()];
    for suffix in ["-wal", "-shm"] {
        let mut name = db.as_os_str().to_owned();
        name.push(suffix);
        paths.push(std::path::PathBuf::from(name));
    }
    for path in paths {
        let mut perms = match std::fs::metadata(&path) {
            Ok(meta) => meta.permissions(),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
            Err(err) => {
                tracing::warn!(
                    path = %path.display(),
                    error = %err,
                    "could not stat database file for mode tighten"
                );
                continue;
            }
        };
        perms.set_mode(0o600);
        if let Err(err) = std::fs::set_permissions(&path, perms) {
            tracing::warn!(
                path = %path.display(),
                error = %err,
                "could not tighten database file mode to owner-only"
            );
        }
    }
}

/// Non-unix platforms fall back to their platform-default ACL story
/// (no ACL work — documented non-goal).
#[cfg(not(unix))]
fn harden_sibling_modes(db: &Path) {
    let _ = db;
}

/// Opens (creating if absent) the database at `path`, applies pending
/// migrations, and returns the repository. `default_connector` is the
/// config-derived tag every board upsert writes into the binding — the
/// single resolution home for the decode rule (NULL = this default,
/// resolved at read; no boot-time backfill).
///
/// The path is handed to sqlite as a filename (never interpolated into a
/// connection URL), so `%`, `?`, and non-UTF-8 path components survive.
///
/// # Errors
///
/// [`OpenError::Connect`] when the file cannot be opened;
/// [`OpenError::Migrate`] when applying pending migrations fails.
pub async fn open(
    path: impl AsRef<Path>,
    default_connector: taskboard_domain::ConnectorId,
) -> Result<SqliteTaskRepository, OpenError> {
    let path = path.as_ref();
    tracing::debug!(path = %path.display(), "opening sqlite database");
    let repo = open_with(
        apply_pragmas(SqliteConnectOptions::new()).filename(path),
        false,
        default_connector,
    )
    .await?;
    harden_sibling_modes(path);
    Ok(repo)
}

/// In-memory variant for tests. The database is per-connection, so the pool
/// pins its single connection open; WAL is a harmless no-op on `:memory:`.
///
/// # Errors
///
/// [`OpenError::Connect`] if the pool cannot be created;
/// [`OpenError::Migrate`] if the embedded migrations fail.
pub async fn open_memory(
    default_connector: taskboard_domain::ConnectorId,
) -> Result<SqliteTaskRepository, OpenError> {
    let parsed: SqliteConnectOptions = "sqlite::memory:".parse().map_err(OpenError::Connect)?;
    open_with(apply_pragmas(parsed), true, default_connector).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use taskboard_domain::ConnectorId;

    /// The fixed default connector the tests open repositories with
    /// (bootstrap derives the same value; see C5 of the chunk plan).
    pub(super) fn test_connector() -> ConnectorId {
        ConnectorId::from("nextcloud")
    }

    use taskboard_domain::ids::BoardId;
    use taskboard_domain::outbox::LocalOp;
    use taskboard_domain::persistence::{PersistedState, TaskRepository};

    /// A fresh database loads a default-shaped state (derived
    /// `pending_ops` = 0).
    #[tokio::test]
    async fn fresh_database_loads_default_state() {
        let repo = open_memory(test_connector()).await.expect("open");
        let loaded = repo.load().await.expect("load");
        assert_eq!(loaded, PersistedState::default());
    }

    /// Opening the same file twice applies zero new migrations (idempotence).
    #[tokio::test]
    async fn reopening_applies_no_new_migrations() {
        let file = tempfile::Builder::new().suffix(".db").tempfile().unwrap();
        let first = open(file.path(), test_connector())
            .await
            .expect("first open");
        first.close().await;

        let second = open(file.path(), test_connector())
            .await
            .expect("second open");
        second.load().await.expect("load");
        let raw = SqlitePool::connect(&format!("sqlite://{}", file.path().display()))
            .await
            .expect("raw connect");
        let applied: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations")
            .fetch_one(&raw)
            .await
            .expect("migration bookkeeping");
        assert_eq!(applied, 14, "each shipped migration recorded exactly once");
        raw.close().await;
        second.close().await;
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

        let repo = open(file.path(), test_connector())
            .await
            .expect("migrating open");
        let state = repo.load().await.expect("load");
        let board = state.boards.values().next().expect("board row");
        assert_eq!(board.title, "board");
        assert_eq!(
            board.clocks.title,
            taskboard_domain::BoardClocks::epoch(),
            "the epoch default means never-locally-written"
        );
        repo.close().await;
    }

    /// A pre-0005 database (no board edit-verb clocks, `rename_board`
    /// outbox tags) upgrades cleanly: the epoch defaults keep the
    /// never-locally-written merge semantics, and the legacy tag is
    /// backfilled to `update_board` so the row decodes.
    #[tokio::test]
    async fn migration_0005_upgrades_a_pre_edit_verb_database() {
        let board_uuid = uuid::Uuid::from_u128(0xb1);
        let file = tempfile::Builder::new().suffix(".db").tempfile().unwrap();
        let raw = SqlitePool::connect(&format!("sqlite://{}?mode=rwc", file.path().display()))
            .await
            .expect("raw connect");
        // Simulate a 0004-era database: run the shipped migrations, then
        // strip the 0005 effects (the clock columns and the tag rename)
        // and the bookkeeping row.
        sqlx::migrate!("./migrations")
            .run(&raw)
            .await
            .expect("migrate");
        sqlx::query("ALTER TABLE boards DROP COLUMN ck_color")
            .execute(&raw)
            .await
            .expect("strip ck_color");
        sqlx::query("ALTER TABLE boards DROP COLUMN ck_archived")
            .execute(&raw)
            .await
            .expect("strip ck_archived");
        sqlx::query("ALTER TABLE boards DROP COLUMN ck_deleted")
            .execute(&raw)
            .await
            .expect("strip ck_deleted");
        sqlx::query("UPDATE outbox SET op_kind = 'rename_board'")
            .execute(&raw)
            .await
            .expect("restore legacy tag");
        sqlx::query("DELETE FROM _sqlx_migrations WHERE version = 5")
            .execute(&raw)
            .await
            .expect("strip bookkeeping");
        let _ = sqlx::query(
            "INSERT INTO boards (id, remote_board_id, title, color, archived, deleted, ck_title, remote_seen) \
             VALUES (?, 42, 'board', '00ff00', 0, 0, '0001-01-01T00:00:00+00:00', NULL)",
        )
        .bind(board_uuid.to_string())
        .execute(&raw)
        .await
        .expect("seed row");
        let _ = sqlx::query(
            "INSERT INTO outbox (op_id, op_kind, board_id, queued_at) \
             VALUES ('0197c0ff-eeee-7ccc-8ddd-0000000000b1', 'rename_board', ?, '2026-01-01T00:00:00+00:00')",
        )
        .bind(board_uuid.to_string())
        .execute(&raw)
        .await
        .expect("seed outbox row");
        raw.close().await;

        let repo = open(file.path(), test_connector())
            .await
            .expect("migrating open");
        let state = repo.load().await.expect("load");
        let board = &state.boards[&taskboard_domain::BoardId::from(board_uuid)];
        assert_eq!(
            board.clocks.color,
            taskboard_domain::BoardClocks::epoch(),
            "the epoch default means never-locally-written"
        );
        assert_eq!(
            board.clocks.archived,
            taskboard_domain::BoardClocks::epoch()
        );
        assert_eq!(board.clocks.deleted, taskboard_domain::BoardClocks::epoch());
        assert!(
            state.outbox.iter().any(|o| o.op
                == taskboard_domain::LocalOp::UpdateBoard(taskboard_domain::BoardId::from(
                    board_uuid
                ))),
            "the backfilled rename_board row decodes as update_board"
        );
        repo.close().await;
    }

    // -- 0006: the per-board sync_status rebuild + the binding connector
    //    tag (F16-22 / connect-flow §4.5). Same construction recipe as the
    //    0005 pin: shipped migrations, then strip 0006's effects and its
    //    bookkeeping row so the reopening open() replays the real
    //    upgrade path.

    /// Builds a pre-0006 database: strips the `connector` column and the
    /// per-board `sync_status` rebuild, restores the legacy singleton
    /// (with its seed row), and deletes 0006's bookkeeping entry.
    async fn pre_0006_database() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("legacy.db");
        let raw = SqlitePool::connect(&format!("sqlite://{}?mode=rwc", db.display()))
            .await
            .expect("raw connect");
        sqlx::migrate!("./migrations")
            .run(&raw)
            .await
            .expect("migrate");
        sqlx::query("ALTER TABLE boards DROP COLUMN connector")
            .execute(&raw)
            .await
            .expect("strip connector");
        sqlx::query("DROP TABLE sync_status")
            .execute(&raw)
            .await
            .expect("strip per-board status");
        sqlx::query(
            "CREATE TABLE sync_status (
                id           INTEGER PRIMARY KEY CHECK (id = 1),
                phase        TEXT NOT NULL,
                last_error   TEXT NULL,
                last_success TEXT NULL
            )",
        )
        .execute(&raw)
        .await
        .expect("rebuild legacy singleton");
        sqlx::query("INSERT INTO sync_status (id, phase) VALUES (1, 'idle')")
            .execute(&raw)
            .await
            .expect("re-seed legacy singleton");
        sqlx::query("DELETE FROM _sqlx_migrations WHERE version = 6")
            .execute(&raw)
            .await
            .expect("strip bookkeeping");
        raw.close().await;
        (dir, db)
    }

    /// C10-1 + C10-2: the legacy singleton's status backfills the one
    /// bound live board's row; the legacy table shape is gone.
    #[tokio::test]
    async fn migration_0006_backfills_the_bound_boards_row_and_drops_the_singleton() {
        let (_dir, db) = pre_0006_database().await;
        let board_uuid = uuid::Uuid::from_u128(0xc1);
        let raw = SqlitePool::connect(&format!("sqlite://{}", db.display()))
            .await
            .expect("raw connect");
        sqlx::query(
            "INSERT INTO boards (id, remote_board_id, title, color, archived, deleted, ck_title, remote_seen) \
             VALUES (?, 42, 'board', '00ff00', 0, 0, '2026-01-01T00:00:00+00:00', NULL)",
        )
        .bind(board_uuid.to_string())
        .execute(&raw)
        .await
        .expect("seed bound board");
        sqlx::query(
            "UPDATE sync_status SET phase = 'offline', \
             last_success = '2026-01-01T00:00:00+00:00' WHERE id = 1",
        )
        .execute(&raw)
        .await
        .expect("seed singleton");
        raw.close().await;

        let repo = open(&db, test_connector()).await.expect("migrating open");
        let state = repo.load().await.expect("load");
        let row = state
            .sync
            .get(taskboard_domain::BoardId::from(board_uuid))
            .expect("the bound board's row backfilled");
        assert_eq!(
            row.phase,
            taskboard_domain::SyncPhase::Offline,
            "the singleton's phase backfills the board's row"
        );
        assert_eq!(
            row.last_success,
            chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00+00:00")
                .ok()
                .map(chrono::DateTime::<chrono::Utc>::from),
            "the singleton's last_success backfills with it"
        );
        assert_eq!(
            state.sync.iter().count(),
            1,
            "the stored map holds exactly the backfilled row"
        );

        // C10-2: the legacy singleton shape is structurally gone.
        let raw = SqlitePool::connect(&format!("sqlite://{}", db.display()))
            .await
            .expect("raw connect");
        let columns: Vec<String> =
            sqlx::query_scalar("SELECT name FROM pragma_table_info('sync_status')")
                .fetch_all(&raw)
                .await
                .expect("table info");
        assert!(
            !columns.iter().any(|c| c == "id"),
            "the legacy singleton id column is gone"
        );
        assert!(
            columns.iter().any(|c| c == "board"),
            "the per-board key is present"
        );
        raw.close().await;
        repo.close().await;
    }

    /// C10-3: a board-less singleton (standalone installs) is dropped —
    /// `open` + `load` succeed with an empty stored map.
    #[tokio::test]
    async fn migration_0006_drops_a_board_less_singleton() {
        let (_dir, db) = pre_0006_database().await;
        let raw = SqlitePool::connect(&format!("sqlite://{}", db.display()))
            .await
            .expect("raw connect");
        sqlx::query("UPDATE sync_status SET phase = 'failed', last_error = 'network'")
            .execute(&raw)
            .await
            .expect("singleton content no board can claim");
        raw.close().await;

        let repo = open(&db, test_connector()).await.expect("migrating open");
        let state = repo.load().await.expect("load");
        assert_eq!(
            state.sync.iter().count(),
            0,
            "the board-less singleton's content is dropped"
        );
        repo.close().await;
    }

    /// C10-4: the legacy NULL tag is a read-only relic; the first board
    /// upsert re-tags the binding with the repo's default connector
    /// (the DF-4 write rule, pinned end-to-end).
    #[tokio::test]
    async fn migration_0006_leaves_the_null_tag_and_upserts_write_the_default() {
        let (_dir, db) = pre_0006_database().await;
        let board_uuid = uuid::Uuid::from_u128(0xc2);
        let raw = SqlitePool::connect(&format!("sqlite://{}", db.display()))
            .await
            .expect("raw connect");
        sqlx::query(
            "INSERT INTO boards (id, remote_board_id, title, color, archived, deleted, ck_title, remote_seen) \
             VALUES (?, 42, 'board', '00ff00', 0, 0, '2026-01-01T00:00:00+00:00', NULL)",
        )
        .bind(board_uuid.to_string())
        .execute(&raw)
        .await
        .expect("seed bound board");
        raw.close().await;

        let repo = open(&db, test_connector()).await.expect("migrating open");
        let connector_column = || async {
            let raw = SqlitePool::connect(&format!("sqlite://{}", db.display()))
                .await
                .expect("raw connect");
            let tag: Option<String> =
                sqlx::query_scalar("SELECT connector FROM boards WHERE id = ?")
                    .bind(board_uuid.to_string())
                    .fetch_one(&raw)
                    .await
                    .expect("board row");
            raw.close().await;
            tag
        };
        assert_eq!(
            connector_column().await,
            None,
            "the migrated NULL tag is a read-only relic (no boot-time backfill)"
        );

        let board = taskboard_domain::Board {
            id: taskboard_domain::BoardId::from(board_uuid),
            remote: taskboard_domain::test_support::test_binding(taskboard_domain::RemoteBoardId(
                42,
            )),
            title: "board".into(),
            color: taskboard_domain::Color::new("00ff00"),
            archived: false,
            deleted: false,
            clocks: taskboard_domain::BoardClocks {
                title: taskboard_domain::BoardClocks::epoch(),
                color: taskboard_domain::BoardClocks::epoch(),
                archived: taskboard_domain::BoardClocks::epoch(),
                deleted: taskboard_domain::BoardClocks::epoch(),
            },
            remote_seen: None,
        };
        repo.apply(vec![taskboard_domain::PersistenceAction::UpsertBoard(
            board,
        )])
        .await
        .expect("apply");
        assert_eq!(
            connector_column().await.as_deref(),
            Some(test_connector().as_str()),
            "the upsert re-tags the binding with the repo's default connector"
        );
        repo.close().await;
    }

    /// D-3: a non-NULL tag naming a connector nobody configured decodes
    /// as-is — no codec-level existence validation; the owning actor
    /// simply never finds the board (its set filter misses). The chunk-4
    /// world cannot mint such a row (one name exists), so the fixture
    /// writes it raw.
    #[tokio::test]
    async fn a_non_null_foreign_tag_decodes_as_is() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("foreign.db");
        open(&db, test_connector())
            .await
            .expect("migrating open")
            .close()
            .await;
        let raw = SqlitePool::connect(&format!("sqlite://{}", db.display()))
            .await
            .expect("raw connect");
        sqlx::query(
            "INSERT INTO boards (id, remote_board_id, connector, title, color, archived, deleted, ck_title, remote_seen) \
             VALUES (?, 42, 'work', 'board', '00ff00', 0, 0, '2026-01-01T00:00:00+00:00', NULL)",
        )
        .bind(uuid::Uuid::from_u128(0xd3).to_string())
        .execute(&raw)
        .await
        .expect("seed foreign-tagged board");
        raw.close().await;

        let repo = open(&db, test_connector()).await.expect("open");
        let state = repo.load().await.expect("load");
        let board = state
            .boards
            .get(&taskboard_domain::BoardId::from(uuid::Uuid::from_u128(
                0xd3,
            )))
            .expect("the foreign-tagged board loads");
        assert_eq!(
            board.remote,
            Some(taskboard_domain::RemoteBoardBinding {
                connector: taskboard_domain::ConnectorId::from("work"),
                board: taskboard_domain::RemoteBoardId(42),
            }),
            "the row's tag decodes as-is, never rewritten to the default"
        );
        repo.close().await;
    }

    /// DF-4's flip, with the distinguishing case the default-tag fixtures
    /// cannot express: the binding's tag is the write source — a board
    /// bound to a non-default tag stores *that* tag, and the round trip
    /// returns it (an implementation writing the default unconditionally
    /// would fail both reads).
    #[tokio::test]
    async fn upserts_write_the_binding_s_tag_and_it_round_trips() {
        let repo = open_memory(test_connector()).await.expect("open");
        let board = taskboard_domain::Board {
            id: taskboard_domain::BoardId::from(uuid::Uuid::from_u128(0xd4)),
            remote: Some(taskboard_domain::RemoteBoardBinding {
                connector: taskboard_domain::ConnectorId::from("work"),
                board: taskboard_domain::RemoteBoardId(42),
            }),
            title: "board".into(),
            color: taskboard_domain::Color::new("00ff00"),
            archived: false,
            deleted: false,
            clocks: taskboard_domain::BoardClocks {
                title: taskboard_domain::BoardClocks::epoch(),
                color: taskboard_domain::BoardClocks::epoch(),
                archived: taskboard_domain::BoardClocks::epoch(),
                deleted: taskboard_domain::BoardClocks::epoch(),
            },
            remote_seen: None,
        };
        repo.apply(vec![taskboard_domain::PersistenceAction::UpsertBoard(
            board.clone(),
        )])
        .await
        .expect("apply");

        let state = repo.load().await.expect("load");
        assert_eq!(
            state.boards[&board.id].remote, board.remote,
            "the stored binding round-trips with its own tag"
        );
        repo.close().await;
    }

    /// C10-5: a fresh install loads the empty map — the old
    /// `Corrupted`-on-missing-singleton path is dead.
    #[tokio::test]
    async fn a_fresh_database_loads_an_empty_sync_map() {
        let repo = open_memory(test_connector()).await.expect("open");
        let loaded = repo.load().await.expect("load");
        assert_eq!(loaded.sync, taskboard_domain::SyncStates::new());
        repo.close().await;
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
    /// while unrelated op rows keep their columns untouched. The 0005
    /// tag rename lands on top, so the row decodes (and is stored) as
    /// `update_board`.
    #[tokio::test]
    #[allow(clippy::too_many_lines)] // the era simulation needs its strips
    async fn migration_0003_backfills_legacy_rename_board_rows() {
        let board_uuid = uuid::Uuid::from_u128(0xa1);
        let stack_uuid = uuid::Uuid::from_u128(0xa2);
        let task_uuid = uuid::Uuid::from_u128(0xa3);
        let file = tempfile::Builder::new().suffix(".db").tempfile().unwrap();
        let raw = SqlitePool::connect(&format!("sqlite://{}?mode=rwc", file.path().display()))
            .await
            .expect("raw connect");
        // Simulate a 0002-era database: run the shipped migrations, then
        // strip the 0003 column and the 0005 effects (its columns and its
        // tag rename), plus their bookkeeping rows, so the reopening
        // open() replays the real 0003 → 0005 upgrade path.
        sqlx::migrate!("./migrations")
            .run(&raw)
            .await
            .expect("migrate");
        sqlx::query("ALTER TABLE outbox DROP COLUMN board_id")
            .execute(&raw)
            .await
            .expect("strip board_id");
        sqlx::query("ALTER TABLE boards DROP COLUMN ck_color")
            .execute(&raw)
            .await
            .expect("strip ck_color");
        sqlx::query("ALTER TABLE boards DROP COLUMN ck_archived")
            .execute(&raw)
            .await
            .expect("strip ck_archived");
        sqlx::query("ALTER TABLE boards DROP COLUMN ck_deleted")
            .execute(&raw)
            .await
            .expect("strip ck_deleted");
        sqlx::query("DELETE FROM _sqlx_migrations WHERE version IN (3, 5)")
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

        let repo = open(file.path(), test_connector())
            .await
            .expect("migrating open");
        let state = repo.load().await.expect("load");
        assert!(
            state
                .outbox
                .iter()
                .any(|o| o.op == LocalOp::UpdateBoard(BoardId::from(board_uuid))),
            "the backfilled rename_board row must still decode to its op"
        );
        repo.close().await;

        let raw = SqlitePool::connect(&format!("sqlite://{}", file.path().display()))
            .await
            .expect("raw connect");
        let (board_id, stack_id): (Option<String>, Option<String>) =
            sqlx::query_as("SELECT board_id, stack_id FROM outbox WHERE op_kind = 'update_board'")
                .fetch_one(&raw)
                .await
                .expect("update_board row");
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

    /// A pre-0004 database (no reminder tables) upgrades cleanly: the four
    /// tables re-apply on a seeded 0003-era board/stack/task, and `load()`
    /// returns the intact entities with a default-shaped `reminders`
    /// section (an empty reminder dataset is exactly the hydrated shape).
    #[tokio::test]
    async fn migration_0004_upgrades_a_pre_0004_database() {
        let file = tempfile::Builder::new().suffix(".db").tempfile().unwrap();
        let raw = SqlitePool::connect(&format!("sqlite://{}?mode=rwc", file.path().display()))
            .await
            .expect("raw connect");
        // Simulate a 0003-era database: run the shipped migrations, then
        // strip the 0004 artifacts and their bookkeeping row.
        sqlx::migrate!("./migrations")
            .run(&raw)
            .await
            .expect("migrate");
        // Literal drops (sqlx 0.9 rejects dynamic SQL strings).
        sqlx::query("DROP TABLE reminder_rules")
            .execute(&raw)
            .await
            .expect("strip reminder_rules");
        sqlx::query("DROP TABLE reminder_deliveries")
            .execute(&raw)
            .await
            .expect("strip reminder_deliveries");
        sqlx::query("DROP TABLE reminder_snoozes")
            .execute(&raw)
            .await
            .expect("strip reminder_snoozes");
        sqlx::query("DROP TABLE reminder_muted")
            .execute(&raw)
            .await
            .expect("strip reminder_muted");
        // 0007 rides the same strip: its reminder_rules ALTER targets the
        // table 0004 re-creates, so the bookkeeping row must go with it or
        // the re-created table would lack the column (SQLite has no
        // dependency tracking; the real pre-0004 upgrade runs 4, …, 7 in
        // order and lands with the column in place). 0013 is the same
        // shape: its insistence ALTER targets the re-created table, so it
        // replays with the strip too.
        sqlx::query("DELETE FROM _sqlx_migrations WHERE version IN (4, 7, 13)")
            .execute(&raw)
            .await
            .expect("strip bookkeeping");
        // 0013's adhoc_alarms half must strip too: its table was not
        // dropped, so the replayed ALTER would collide with the
        // surviving column (the real pre-0004 upgrade runs 8 and 13
        // fresh, in order).
        sqlx::query("ALTER TABLE adhoc_alarms DROP COLUMN insistence")
            .execute(&raw)
            .await
            .expect("strip insistence col");
        // 0007's task columns ride the same strip (they re-run with it);
        // literal statements only (sqlx 0.9 audits dynamic SQL).
        sqlx::query("ALTER TABLE tasks DROP COLUMN scheduled")
            .execute(&raw)
            .await
            .expect("strip ck col");
        sqlx::query("ALTER TABLE tasks DROP COLUMN effort")
            .execute(&raw)
            .await
            .expect("strip ck col");
        sqlx::query("ALTER TABLE tasks DROP COLUMN importance")
            .execute(&raw)
            .await
            .expect("strip ck col");
        sqlx::query("ALTER TABLE tasks DROP COLUMN urgency")
            .execute(&raw)
            .await
            .expect("strip ck col");
        sqlx::query("ALTER TABLE tasks DROP COLUMN ck_scheduled")
            .execute(&raw)
            .await
            .expect("strip ck col");
        sqlx::query("ALTER TABLE tasks DROP COLUMN ck_effort")
            .execute(&raw)
            .await
            .expect("strip ck col");
        sqlx::query("ALTER TABLE tasks DROP COLUMN ck_importance")
            .execute(&raw)
            .await
            .expect("strip ck col");
        sqlx::query("ALTER TABLE tasks DROP COLUMN ck_urgency")
            .execute(&raw)
            .await
            .expect("strip ck col");
        // Seed the pre-existing entities the upgrade must not disturb
        // (FK order: board → stack → task; clocks are the NOT NULL texts).
        sqlx::query(
            "INSERT INTO boards (id, remote_board_id, title, color, archived, deleted, ck_title, remote_seen) \
             VALUES ('00000000-0000-0000-0000-00000000000b', NULL, 'board', '00ff00', 0, 0, '2026-01-01T00:00:00+00:00', NULL)",
        )
        .execute(&raw)
        .await
        .expect("seed board");
        sqlx::query(
            "INSERT INTO stacks (id, board, remote_board_id, remote_stack_id, title, sort_order, archived, deleted, ck_title, ck_sort_order, ck_deleted, remote_seen) \
             VALUES ('00000000-0000-0000-0000-00000000000c', '00000000-0000-0000-0000-00000000000b', NULL, NULL, 'stack', 0, 0, 0, '2026-01-01T00:00:00+00:00', '2026-01-01T00:00:00+00:00', '2026-01-01T00:00:00+00:00', NULL)",
        )
        .execute(&raw)
        .await
        .expect("seed stack");
        sqlx::query(
            "INSERT INTO tasks (id, stack, remote_board_id, remote_stack_id, remote_card_id, title, description, duedate, done, sort_order, archived, deleted, ck_title, ck_description, ck_duedate, ck_done, ck_position, ck_labels, ck_archived, ck_deleted, remote_seen) \
             VALUES ('00000000-0000-0000-0000-00000000000d', '00000000-0000-0000-0000-00000000000c', NULL, NULL, NULL, 'task', '', NULL, NULL, 0, 0, 0, '2026-01-01T00:00:00+00:00', '2026-01-01T00:00:00+00:00', '2026-01-01T00:00:00+00:00', '2026-01-01T00:00:00+00:00', '2026-01-01T00:00:00+00:00', '2026-01-01T00:00:00+00:00', '2026-01-01T00:00:00+00:00', '2026-01-01T00:00:00+00:00', NULL)",
        )
        .execute(&raw)
        .await
        .expect("seed task");
        raw.close().await;

        let repo = open(file.path(), test_connector())
            .await
            .expect("migrating open");
        let state = repo.load().await.expect("load");
        assert_eq!(state.tasks.len(), 1, "the seeded task survives");
        assert_eq!(
            state.reminders,
            taskboard_domain::reminders::ReminderState::default(),
            "a pre-0004 dataset hydrates as the empty reminder section"
        );
        repo.close().await;
    }

    /// The 0009 upgrade backfill (design D-10): every pre-round
    /// `reminder_deliveries` row becomes a `reminder_dismissals` row
    /// one-for-one with `dismissed_at = delivered_at` — old behavior
    /// ≡ everything already dismissed, so no already-fired occurrence
    /// resurfaces as an active in-app notification after upgrade.
    #[tokio::test]
    #[allow(clippy::too_many_lines)] // the fixture IS the scope proof
    async fn migration_0009_backfills_dismissals_from_deliveries() {
        let file = tempfile::Builder::new().suffix(".db").tempfile().unwrap();
        let raw = SqlitePool::connect(&format!("sqlite://{}?mode=rwc", file.path().display()))
            .await
            .expect("raw connect");
        // Simulate a pre-0009 database: run the shipped migrations, strip
        // the 0009 artifacts and their bookkeeping row, then seed the
        // delivery rows the backfill must convert.
        sqlx::migrate!("./migrations")
            .run(&raw)
            .await
            .expect("migrate");
        sqlx::query("DROP TABLE reminder_dismissals")
            .execute(&raw)
            .await
            .expect("strip reminder_dismissals");
        sqlx::query("DELETE FROM _sqlx_migrations WHERE version = 9")
            .execute(&raw)
            .await
            .expect("strip bookkeeping");
        for (reminder, occurrence) in [
            (
                "rule:00000000-0000-0000-0000-0000000000aa",
                "2026-01-01T00:00:00+00:00",
            ),
            (
                "default:00000000-0000-0000-0000-0000000000bb",
                "2026-01-02T00:00:00+00:00",
            ),
            (
                "alarm:00000000-0000-0000-0000-0000000000cc",
                "2026-01-03T00:00:00+00:00",
            ),
        ] {
            sqlx::query("INSERT INTO reminder_deliveries (reminder, occurrence, delivered_at) VALUES (?, ?, ?)")
                .bind(reminder)
                .bind(occurrence)
                .bind(occurrence)
                .execute(&raw)
                .await
                .expect("seed delivery");
        }
        raw.close().await;

        // The migrating open re-runs 0009 (table + backfill); `load`
        // hydrates the reminder section with the converted rows.
        let repo = open(file.path(), test_connector())
            .await
            .expect("migrating open");
        let state = repo.load().await.expect("load");
        let dismissals: Vec<_> = state
            .reminders
            .dismissals
            .iter()
            .map(|d| {
                (
                    crate::codec::reminder_ref_to_text(d.reminder),
                    d.occurrence.to_rfc3339(),
                    d.dismissed_at.to_rfc3339(),
                )
            })
            .collect();
        assert_eq!(dismissals.len(), 3, "one dismissal per delivery row");
        for (reminder, occurrence) in [
            (
                "rule:00000000-0000-0000-0000-0000000000aa",
                "2026-01-01T00:00:00+00:00",
            ),
            (
                "default:00000000-0000-0000-0000-0000000000bb",
                "2026-01-02T00:00:00+00:00",
            ),
            (
                "alarm:00000000-0000-0000-0000-0000000000cc",
                "2026-01-03T00:00:00+00:00",
            ),
        ] {
            assert!(
                dismissals
                    .iter()
                    .any(|(r, o, d)| r == reminder && o == occurrence && d == occurrence),
                "the delivery for {reminder} must backfill to a dismissal stamped at delivery time"
            );
        }
        // A replayed apply keeps the first-write-wins shape (the SQL
        // parity): a dismissal re-record is a no-op.
        repo.apply(vec![
            taskboard_domain::PersistenceAction::RecordReminderDismissal(
                taskboard_domain::ReminderDismissal {
                    reminder: taskboard_domain::ReminderRef::Rule(
                        taskboard_domain::ReminderRuleId::from(uuid::Uuid::from_u128(0xaa)),
                    ),
                    occurrence: chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00+00:00")
                        .unwrap()
                        .with_timezone(&chrono::Utc),
                    dismissed_at: chrono::DateTime::parse_from_rfc3339("2027-01-01T00:00:00+00:00")
                        .unwrap()
                        .with_timezone(&chrono::Utc),
                },
            ),
        ])
        .await
        .expect("apply");
        let state = repo.load().await.expect("reload");
        assert_eq!(
            state.reminders.dismissals.len(),
            3,
            "the replayed dismissal must not replace the backfilled row"
        );
        repo.close().await;
    }

    /// The durability pragmas are actually in force: `journal_mode` is WAL.
    #[tokio::test]
    async fn journal_mode_is_wal() {
        let file = tempfile::Builder::new().suffix(".db").tempfile().unwrap();
        let repo = open(file.path(), test_connector()).await.expect("open");
        let mode: String = sqlx::query_scalar("PRAGMA journal_mode")
            .fetch_one(&repo.pool)
            .await
            .expect("pragma");
        assert_eq!(mode, "wal");
        repo.close().await;
    }

    // -- owner-only file modes (F12-01) --------------------------------------

    /// Asserts the mode bits recorded in the metadata view (never the
    /// access-as-another-user view, which root would defeat).
    #[cfg(unix)]
    fn owner_mode_is(path: &std::path::Path) -> bool {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path)
            .expect("file exists")
            .permissions()
            .mode()
            & 0o777
            == 0o600
    }

    /// F12-01 (#297): a database created by `open` is owner read/write
    /// only — OS permissions are the whole auth story (ADR 0012).
    #[cfg(unix)]
    #[tokio::test]
    async fn a_fresh_database_file_is_owner_read_write_only() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("fresh.db");

        let repo = open(&db, test_connector()).await.expect("open");
        drop(repo);

        assert!(owner_mode_is(&db), "the fresh database must be 0600");
    }

    /// F12-01 (#297): the `-wal`/`-shm` siblings carry board content
    /// too while the pool holds the connection; they must be owner-only
    /// as well.
    #[cfg(unix)]
    #[tokio::test]
    async fn wal_and_shm_siblings_are_owner_only_while_the_database_is_open() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("fresh.db");

        let repo = open(&db, test_connector()).await.expect("open");
        let _ = repo.load().await.expect("load");

        let wal = dir.path().join("fresh.db-wal");
        let shm = dir.path().join("fresh.db-shm");
        assert!(wal.exists(), "wal exists while the pool connection lives");
        assert!(shm.exists(), "shm exists while the pool connection lives");
        assert!(owner_mode_is(&wal), "the wal sibling must be 0600");
        assert!(owner_mode_is(&shm), "the shm sibling must be 0600");
    }

    /// F12-01 (#297): opening tightens a pre-existing world-readable
    /// database (upgrade self-heal).
    #[cfg(unix)]
    #[tokio::test]
    async fn opening_tightens_a_preexisting_world_readable_database() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("loose.db");
        std::fs::File::create(&db).expect("pre-create empty db");
        let mut perms = std::fs::metadata(&db).expect("meta").permissions();
        perms.set_mode(0o644);
        std::fs::set_permissions(&db, perms).expect("loosen");
        assert!(!owner_mode_is(&db), "fixture must start permissive");

        let repo = open(&db, test_connector()).await.expect("open");
        drop(repo);

        assert!(owner_mode_is(&db), "opening must tighten to 0600");
    }

    /// The pool holds exactly one connection (kiosk single-writer stance).
    #[tokio::test]
    async fn pool_is_single_connection() {
        let repo = open_memory(test_connector()).await.expect("open");
        assert_eq!(repo.pool.size(), 1);
    }

    /// A live board entity with a deterministic id (ids burned from one
    /// shared counter so the fixture boards can never collide).
    fn board(id: BoardId, title: &str) -> taskboard_domain::Board {
        taskboard_domain::Board {
            id,
            remote: None,
            title: title.into(),
            color: taskboard_domain::Color::new("0000ff"),
            archived: false,
            deleted: false,
            clocks: taskboard_domain::BoardClocks {
                title: taskboard_domain::BoardClocks::epoch(),
                color: taskboard_domain::BoardClocks::epoch(),
                archived: taskboard_domain::BoardClocks::epoch(),
                deleted: taskboard_domain::BoardClocks::epoch(),
            },
            remote_seen: None,
        }
    }

    /// F5-02 (#241): concurrent operations share one `:memory:` dataset —
    /// the single-connection pool stance. A held connection pins the
    /// seeded database; a non-pinning pool would be forced to open fresh
    /// (empty) `:memory:` databases for the probes, and no probe can then
    /// ever observe base + its own marker together. The size poll fires
    /// only in that mutant world; expiry is the pinned world's normal
    /// path (bounded, not a fixed sleep).
    #[tokio::test]
    async fn concurrent_operations_share_one_memory_dataset() {
        use std::collections::BTreeSet;
        use std::sync::Arc;
        use std::time::Duration;

        use taskboard_domain::idgen::IdGenerator as _;
        use taskboard_domain::persistence::PersistenceAction;
        use taskboard_domain::test_support::CountingIds;

        let repo = Arc::new(open_memory(test_connector()).await.expect("open"));
        let ids = CountingIds::new();
        let base = board(ids.new_board_id(), "base");
        let base_id = base.id;
        repo.apply(vec![PersistenceAction::UpsertBoard(base)])
            .await
            .expect("base seeded");
        assert_eq!(
            repo.load().await.expect("load").boards.len(),
            1,
            "sanity: the base board is loaded back"
        );
        assert_eq!(repo.pool.size(), 1, "sanity: exactly one connection");

        // Holder: pins the seeded connection for the whole probe phase.
        let held = repo.pool.acquire().await.expect("acquire");

        let markers: Vec<taskboard_domain::Board> = (1..=4)
            .map(|n| board(ids.new_board_id(), &format!("marker {n}")))
            .collect();
        let marker_ids: BTreeSet<BoardId> = markers.iter().map(|b| b.id).collect();
        let mut probes = Vec::new();
        for marker in markers {
            let repo = Arc::clone(&repo);
            let marker_id = marker.id;
            probes.push(tokio::spawn(async move {
                repo.apply(vec![PersistenceAction::UpsertBoard(marker)])
                    .await
                    .expect("marker applied");
                let loaded = repo.load().await.expect("load");
                loaded.boards.contains_key(&base_id) && loaded.boards.contains_key(&marker_id)
            }));
        }

        // Fires only if extra connections were opened (the mutant world).
        let _ = tokio::time::timeout(Duration::from_secs(2), async {
            while repo.pool.size() <= 1 {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await;
        drop(held); // release: queued probes proceed serially

        for probe in probes {
            let saw_base_and_marker = tokio::time::timeout(Duration::from_secs(60), probe)
                .await
                .expect("probe joins within the deadline")
                .expect("probe task survives");
            assert!(
                saw_base_and_marker,
                "every operation must observe the one shared dataset"
            );
        }

        let terminal = repo.load().await.expect("final load");
        let expected: BTreeSet<BoardId> = marker_ids
            .into_iter()
            .chain(std::iter::once(base_id))
            .collect();
        assert_eq!(
            terminal
                .boards
                .keys()
                .copied()
                .collect::<BTreeSet<BoardId>>(),
            expected,
            "the terminal dataset holds the base and every marker exactly once"
        );
    }
}
