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
    let repo = open_with(
        apply_pragmas(SqliteConnectOptions::new()).filename(path),
        false,
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
        assert_eq!(applied, 4, "each shipped migration recorded exactly once");
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
        sqlx::query("DELETE FROM _sqlx_migrations WHERE version = 4")
            .execute(&raw)
            .await
            .expect("strip bookkeeping");
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

        let repo = open(file.path()).await.expect("migrating open");
        let state = repo.load().await.expect("load");
        assert_eq!(state.tasks.len(), 1, "the seeded task survives");
        assert_eq!(
            state.reminders,
            taskboard_domain::reminders::ReminderState::default(),
            "a pre-0004 dataset hydrates as the empty reminder section"
        );
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

        let repo = open(&db).await.expect("open");
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

        let repo = open(&db).await.expect("open");
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

        let repo = open(&db).await.expect("open");
        drop(repo);

        assert!(owner_mode_is(&db), "opening must tighten to 0600");
    }

    /// The pool holds exactly one connection (kiosk single-writer stance).
    #[tokio::test]
    async fn pool_is_single_connection() {
        let repo = open_memory().await.expect("open");
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

        let repo = Arc::new(open_memory().await.expect("open"));
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
