// SPDX-License-Identifier: MIT OR Apache-2.0
//! Shared unit-test scaffolding for the sqlite crate (the stage-1
//! plan's D-TS): the legacy-habit fixture writer and the event-table
//! byte-dump oracle. One home, no copy-pasted INSERT blocks across
//! tests (the SonarCloud new-code 3 % duplication gate).
//!
//! Fixture databases are `tempfile` files — isolated per test, never
//! the user's real data dir (the sandbox guard posture; large scratch
//! goes under `/var/tmp` via `TMPDIR`, which the memory gate
//! redirects).

use sqlx::SqlitePool;
use taskboard_domain::ConnectorId;

use crate::SqliteTaskRepository;

/// A file-backed database fixture for in-crate unit tests: owns the
/// temp file and the repository, and closes the pool before the temp
/// handle drops (the `tests/common::FileDb` posture, adapted for
/// `pub(crate)` use — the integration helper is not visible inside
/// the crate).
pub(crate) struct FixtureDb {
    repo: Option<SqliteTaskRepository>,
    file: tempfile::NamedTempFile,
}

impl FixtureDb {
    pub(crate) async fn new() -> Self {
        let file = tempfile::Builder::new()
            .suffix(".db")
            .tempfile()
            .expect("tempfile");
        let repo = crate::open(file.path(), ConnectorId::from("nextcloud"))
            .await
            .expect("open");
        Self {
            repo: Some(repo),
            file,
        }
    }

    /// The raw pool (fixture INSERTs, rewind, byte dumps).
    pub(crate) fn pool(&self) -> &SqlitePool {
        &self.repo.as_ref().expect("database still open").pool
    }

    /// The repository (load/apply through the port).
    pub(crate) fn repo(&self) -> &SqliteTaskRepository {
        self.repo.as_ref().expect("database still open")
    }

    /// Disposes the pool deterministically before the temp file drops.
    pub(crate) async fn close(&mut self) {
        if let Some(repo) = self.repo.take() {
            repo.close().await;
        }
    }

    /// Closes and reopens the same file: the reopen path replays
    /// pending migrations and re-runs the load (the remap's trigger).
    pub(crate) async fn reopen(&mut self) {
        self.close().await;
        self.repo = Some(
            crate::open(self.file.path(), ConnectorId::from("nextcloud"))
                .await
                .expect("reopen"),
        );
    }
}

impl Drop for FixtureDb {
    fn drop(&mut self) {
        // The sidecar-unlink backstop (`tests/common::FileDb` verbatim
        // posture): sqlx 0.9's pool drop leaves `-wal`/`-shm` behind.
        drop(self.repo.take());
        for suffix in ["-wal", "-shm"] {
            let mut name = self.file.path().as_os_str().to_owned();
            name.push(suffix);
            let _ = std::fs::remove_file(std::path::PathBuf::from(name));
        }
    }
}

/// One legacy `habit_log` row as fixture data (plain struct list; the
/// column spellings mirror the 0017 schema).
#[derive(Debug, Clone)]
pub(crate) struct LegacyHabitLogRow {
    pub habit: uuid::Uuid,
    pub window: chrono::NaiveDate,
    pub count: i64,
    pub clock: String,
    pub au_by: Option<String>,
    pub skip: bool,
}

/// Writes the given legacy `habit_log` rows (the remap fixtures' read
/// side: `habit_id, window_start, count, clock, au_by, skip`).
pub(crate) async fn write_legacy_habit_rows(
    pool: &SqlitePool,
    rows: &[LegacyHabitLogRow],
) -> Result<(), sqlx::Error> {
    for row in rows {
        sqlx::query(
            "INSERT INTO habit_log (habit_id, window_start, count, clock, au_by, skip) \
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(row.habit.to_string())
        .bind(row.window.to_string())
        .bind(row.count)
        .bind(&row.clock)
        .bind(&row.au_by)
        .bind(row.skip)
        .execute(pool)
        .await?;
    }
    Ok(())
}

/// One legacy `habit_day_log` overlay row as fixture data.
#[derive(Debug, Clone)]
pub(crate) struct LegacyHabitDayRow {
    pub habit: uuid::Uuid,
    pub day: chrono::NaiveDate,
    pub count: i64,
    pub clock: String,
}

/// Writes the given legacy `habit_day_log` rows.
pub(crate) async fn write_legacy_day_rows(
    pool: &SqlitePool,
    rows: &[LegacyHabitDayRow],
) -> Result<(), sqlx::Error> {
    for row in rows {
        sqlx::query("INSERT INTO habit_day_log (habit_id, day, count, clock) VALUES (?, ?, ?, ?)")
            .bind(row.habit.to_string())
            .bind(row.day.to_string())
            .bind(row.count)
            .bind(&row.clock)
            .execute(pool)
            .await?;
    }
    Ok(())
}

/// One raw `habit_event` row as fixture data — the direct-INSERT path
/// for decode/corruption tests (bypasses the port on purpose: these
/// tests exercise the load decoder, not the flush).
#[derive(Debug, Clone)]
pub(crate) struct RawEventRow {
    pub id: String,
    pub habit: String,
    pub kind: String,
    pub window: Option<String>,
    pub at: String,
    pub au_by: Option<String>,
    pub deleted_at: Option<String>,
    pub deleted_by: Option<String>,
}

/// Writes the given raw `habit_event` rows.
pub(crate) async fn write_event_rows(
    pool: &SqlitePool,
    rows: &[RawEventRow],
) -> Result<(), sqlx::Error> {
    for row in rows {
        sqlx::query(
            "INSERT INTO habit_event (id, habit_id, kind, window, at, au_by, deleted_at, deleted_by) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&row.id)
        .bind(&row.habit)
        .bind(&row.kind)
        .bind(&row.window)
        .bind(&row.at)
        .bind(&row.au_by)
        .bind(&row.deleted_at)
        .bind(&row.deleted_by)
        .execute(pool)
        .await?;
    }
    Ok(())
}

/// The determinism/rewind oracle (the stage-1 plan's D-TS and test
/// catalogue #15): the full `habit_event` table serialized
/// byte-deterministically — rows in `id` order, fields in column
/// order, `\x1f` between fields, `\x1e` between rows, NULL as the
/// empty field. Fixed text grammar, no ambient state: two dumps are
/// byte-comparable across opens.
/// The raw dump row tuple (the `habit_event` column order).
#[allow(clippy::type_complexity)]
type RawEventRowTuple = (
    String,
    String,
    String,
    Option<String>,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
);

pub(crate) async fn event_table_dump(pool: &SqlitePool) -> Vec<u8> {
    let rows: Vec<RawEventRowTuple> = sqlx::query_as(
        "SELECT id, habit_id, kind, window, at, au_by, deleted_at, deleted_by \
             FROM habit_event ORDER BY id",
    )
    .fetch_all(pool)
    .await
    .expect("event table dump");
    let mut out = Vec::new();
    for (id, habit, kind, window, at, au_by, deleted_at, deleted_by) in rows {
        for field in [
            Some(id),
            Some(habit),
            Some(kind),
            window,
            Some(at),
            au_by,
            deleted_at,
            deleted_by,
        ] {
            out.extend_from_slice(field.as_deref().unwrap_or("").as_bytes());
            out.push(0x1f);
        }
        out.push(0x1e);
    }
    out
}

/// Whether the one-shot remap marker row is present.
pub(crate) async fn marker_present(pool: &SqlitePool) -> bool {
    let (present,): (bool,) = sqlx::query_as("SELECT EXISTS(SELECT 1 FROM habit_event_migration)")
        .fetch_one(pool)
        .await
        .expect("marker probe");
    present
}

/// A live fixture habit over the given target (the derive-on-loaded-
/// state tests' config leg). Deterministic clocks; no UI fields set.
#[must_use]
pub(crate) fn fixture_habit(
    id: uuid::Uuid,
    target: taskboard_domain::habit::HabitTarget,
) -> taskboard_domain::Habit {
    taskboard_domain::Habit {
        id: taskboard_domain::ids::HabitId::from(id),
        board: taskboard_domain::ids::BoardId::from(uuid::Uuid::from_u128(0x900)),
        title: "fixture".to_string(),
        description: String::new(),
        target,
        order: 0,
        anchor_time: None,
        anchor_day: None,
        scheduled_days: None,
        reminder: None,
        clocks: taskboard_domain::habit::HabitClocks::all(
            chrono::TimeZone::timestamp_opt(&chrono::Utc, 100, 0)
                .single()
                .expect("valid instant"),
        ),
        continued_from: None,
        deleted: false,
        by: None,
        paused_until: None,
    }
}

/// The UTC day calendar the derive assertions use.
#[must_use]
pub(crate) fn utc_calendar() -> taskboard_domain::days::DayCalendar {
    taskboard_domain::days::DayCalendar {
        day_start_time: chrono::NaiveTime::MIN,
        offset: chrono::FixedOffset::east_opt(0).expect("zero offset"),
    }
}
