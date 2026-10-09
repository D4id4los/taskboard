// SPDX-License-Identifier: MIT OR Apache-2.0
//! The sqlite [`TaskRepository`] adapter: full hydration at boot, one
//! transaction per apply batch.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use sqlx::SqlitePool;
use taskboard_domain::entities::{
    Board, Color, Label, LabelClocks, Stack, StackClocks, Task, TaskClocks,
};
use taskboard_domain::ids::{
    BoardId, LabelId, RemoteBoardId, RemoteCardId, RemoteCardRef, RemoteLabelId, RemoteLabelRef,
    RemoteStackId, RemoteStackRef, StackId, TaskId,
};
use taskboard_domain::outbox::{OpId, PendingOp};
use taskboard_domain::persistence::{
    BoxFuture, PersistedState, PersistenceAction, RepositoryError, SyncValidators, TaskRepository,
};
use taskboard_domain::state::{SyncPhase, SyncStatus};
use uuid::Uuid;

use crate::codec;

/// The local disk-cache repository. Cheap to clone via `Arc`; see
/// [`connect`](crate::connect) for construction.
#[derive(Debug, Clone)]
pub struct SqliteTaskRepository {
    pub(crate) pool: SqlitePool,
}

/// Raw `boards` row (`query_as!` target; domain structs with nested clocks
/// are not constructible by the macro).
struct BoardRow {
    id: String,
    remote_board_id: Option<i64>,
    title: String,
    color: String,
    archived: bool,
    deleted: bool,
    remote_seen: Option<DateTime<Utc>>,
}

/// Raw `stacks` row.
struct StackRow {
    id: String,
    board: String,
    remote_board_id: Option<i64>,
    remote_stack_id: Option<i64>,
    title: String,
    sort_order: i64,
    archived: bool,
    deleted: bool,
    ck_title: String,
    ck_sort_order: String,
    ck_deleted: String,
    remote_seen: Option<DateTime<Utc>>,
}

/// Raw `tasks` row.
#[allow(clippy::too_many_lines)] // 21 columns; the shape is the schema's
struct TaskRow {
    id: String,
    stack: String,
    remote_board_id: Option<i64>,
    remote_stack_id: Option<i64>,
    remote_card_id: Option<i64>,
    title: String,
    description: String,
    duedate: Option<DateTime<Utc>>,
    done: Option<DateTime<Utc>>,
    sort_order: i64,
    archived: bool,
    deleted: bool,
    ck_title: String,
    ck_description: String,
    ck_duedate: String,
    ck_done: String,
    ck_position: String,
    ck_labels: String,
    ck_archived: String,
    ck_deleted: String,
    remote_seen: Option<DateTime<Utc>>,
}

/// Raw `labels` row.
struct LabelRow {
    id: String,
    board: String,
    remote_board_id: Option<i64>,
    remote_label_id: Option<i64>,
    title: String,
    color: String,
    deleted: bool,
    ck_title: String,
    ck_color: String,
    ck_deleted: String,
    remote_seen: Option<DateTime<Utc>>,
}

/// Raw `outbox` row.
struct OutboxRow {
    op_id: String,
    op_kind: String,
    task_id: Option<String>,
    stack_id: Option<String>,
    label_id: Option<String>,
    queued_at: DateTime<Utc>,
}

/// Raw `sync_metadata` row.
struct ValidatorRow {
    key: String,
    etag: Option<String>,
    last_modified: Option<String>,
}

/// Raw `sync_status` row.
struct SyncStatusRow {
    phase: String,
    last_error: Option<String>,
    last_success: Option<DateTime<Utc>>,
}

/// Decode helpers shared by the row mappers; every failure becomes
/// [`RepositoryError::Corrupted`] (plan decision 10/12).
mod decode {
    use super::{RepositoryError, Uuid, codec};

    pub(crate) fn uuid(raw: &str) -> Result<Uuid, RepositoryError> {
        codec::uuid_from_text(raw).map_err(|_| RepositoryError::Corrupted)
    }

    pub(crate) fn remote_opt(cols: &[Option<i64>]) -> Result<Option<Vec<u64>>, RepositoryError> {
        codec::remote_columns_present_or_none(cols).map_err(|_| RepositoryError::Corrupted)
    }
}

fn decode_board(row: BoardRow) -> Result<(Uuid, Board), RepositoryError> {
    let id = decode::uuid(&row.id)?;
    let remote = decode::remote_opt(&[row.remote_board_id])?.map(|nums| RemoteBoardId(nums[0]));
    Ok((
        id,
        Board {
            id: BoardId::from(id),
            remote,
            title: row.title,
            color: Color::new(row.color),
            archived: row.archived,
            deleted: row.deleted,
            remote_seen: row.remote_seen,
        },
    ))
}

fn decode_stack(row: StackRow) -> Result<(Uuid, Stack), RepositoryError> {
    let id = decode::uuid(&row.id)?;
    let board = decode::uuid(&row.board)?;
    let remote = decode::remote_opt(&[row.remote_board_id, row.remote_stack_id])?.map(|nums| {
        RemoteStackRef {
            board: RemoteBoardId(nums[0]),
            stack: RemoteStackId(nums[1]),
        }
    });
    let _ = board; // FK integrity is the database's job; only the id is needed
    Ok((
        id,
        Stack {
            id: StackId::from(id),
            remote,
            board: BoardId::from(board),
            title: row.title,
            order: row.sort_order,
            archived: row.archived,
            deleted: row.deleted,
            clocks: StackClocks {
                title: row
                    .ck_title
                    .parse()
                    .map_err(|_| RepositoryError::Corrupted)?,
                order: row
                    .ck_sort_order
                    .parse()
                    .map_err(|_| RepositoryError::Corrupted)?,
                deleted: row
                    .ck_deleted
                    .parse()
                    .map_err(|_| RepositoryError::Corrupted)?,
            },
            remote_seen: row.remote_seen,
        },
    ))
}

#[allow(clippy::too_many_lines)] // 21 columns; the mapping is linear
fn decode_task(
    row: TaskRow,
    label_sets: &BTreeMap<Uuid, BTreeSet<Uuid>>,
) -> Result<(Uuid, Task), RepositoryError> {
    let id = decode::uuid(&row.id)?;
    let stack = decode::uuid(&row.stack)?;
    let remote =
        decode::remote_opt(&[row.remote_board_id, row.remote_stack_id, row.remote_card_id])?.map(
            |nums| RemoteCardRef {
                board: RemoteBoardId(nums[0]),
                stack: RemoteStackId(nums[1]),
                card: RemoteCardId(nums[2]),
            },
        );
    let labels: BTreeSet<LabelId> = label_sets
        .get(&id)
        .map(|set| set.iter().copied().map(LabelId::from).collect())
        .unwrap_or_default();
    Ok((
        id,
        Task {
            id: TaskId::from(id),
            remote,
            title: row.title,
            description: row.description,
            duedate: row.duedate,
            done: row.done,
            stack: StackId::from(stack),
            order: row.sort_order,
            labels,
            archived: row.archived,
            deleted: row.deleted,
            clocks: TaskClocks {
                title: row
                    .ck_title
                    .parse()
                    .map_err(|_| RepositoryError::Corrupted)?,
                description: row
                    .ck_description
                    .parse()
                    .map_err(|_| RepositoryError::Corrupted)?,
                duedate: row
                    .ck_duedate
                    .parse()
                    .map_err(|_| RepositoryError::Corrupted)?,
                done: row
                    .ck_done
                    .parse()
                    .map_err(|_| RepositoryError::Corrupted)?,
                position: row
                    .ck_position
                    .parse()
                    .map_err(|_| RepositoryError::Corrupted)?,
                labels: row
                    .ck_labels
                    .parse()
                    .map_err(|_| RepositoryError::Corrupted)?,
                archived: row
                    .ck_archived
                    .parse()
                    .map_err(|_| RepositoryError::Corrupted)?,
                deleted: row
                    .ck_deleted
                    .parse()
                    .map_err(|_| RepositoryError::Corrupted)?,
            },
            remote_seen: row.remote_seen,
        },
    ))
}

fn decode_label(row: LabelRow) -> Result<(Uuid, Label), RepositoryError> {
    let id = decode::uuid(&row.id)?;
    let board = decode::uuid(&row.board)?;
    let remote = decode::remote_opt(&[row.remote_board_id, row.remote_label_id])?.map(|nums| {
        RemoteLabelRef {
            board: RemoteBoardId(nums[0]),
            label: RemoteLabelId(nums[1]),
        }
    });
    Ok((
        id,
        Label {
            id: LabelId::from(id),
            remote,
            board: BoardId::from(board),
            title: row.title,
            color: Color::new(row.color),
            deleted: row.deleted,
            clocks: LabelClocks {
                title: row
                    .ck_title
                    .parse()
                    .map_err(|_| RepositoryError::Corrupted)?,
                color: row
                    .ck_color
                    .parse()
                    .map_err(|_| RepositoryError::Corrupted)?,
                deleted: row
                    .ck_deleted
                    .parse()
                    .map_err(|_| RepositoryError::Corrupted)?,
            },
            remote_seen: row.remote_seen,
        },
    ))
}

fn decode_outbox_row(row: &OutboxRow) -> Result<PendingOp, RepositoryError> {
    let op_id = OpId(decode::uuid(&row.op_id)?);
    let op = codec::op_from_columns(
        &row.op_kind,
        row.task_id.as_deref(),
        row.stack_id.as_deref(),
        row.label_id.as_deref(),
    )
    .map_err(|_| RepositoryError::Corrupted)?;
    Ok(PendingOp {
        op_id,
        op,
        queued_at: row.queued_at,
    })
}

impl TaskRepository for SqliteTaskRepository {
    fn load(&self) -> BoxFuture<'_, Result<PersistedState, RepositoryError>> {
        Box::pin(self.load_inner())
    }

    fn apply(&self, actions: Vec<PersistenceAction>) -> BoxFuture<'_, Result<(), RepositoryError>> {
        Box::pin(self.apply_inner(actions))
    }
}

impl SqliteTaskRepository {
    /// The underlying pool (single connection). Exposed for diagnostics
    /// and tests; production code goes through the port methods or the
    /// actor.
    #[must_use]
    pub fn pool(&self) -> &sqlx::SqlitePool {
        &self.pool
    }

    #[tracing::instrument(skip(self), err)]
    #[allow(clippy::too_many_lines)] // eight SELECTs; the length is the schema's
    async fn load_inner(&self) -> Result<PersistedState, RepositoryError> {
        let board_rows = sqlx::query_as!(
            BoardRow,
            r#"SELECT id, remote_board_id, title, color,
                      archived as "archived: bool", deleted as "deleted: bool",
                      remote_seen as "remote_seen: DateTime<Utc>"
               FROM boards ORDER BY id"#
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| crate::error::repo_error(&e))?;

        let stack_rows = sqlx::query_as!(
            StackRow,
            r#"SELECT id, board, remote_board_id, remote_stack_id, title, sort_order,
                      archived as "archived: bool", deleted as "deleted: bool",
                      ck_title, ck_sort_order, ck_deleted,
                      remote_seen as "remote_seen: DateTime<Utc>"
               FROM stacks ORDER BY id"#
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| crate::error::repo_error(&e))?;

        let label_rows = sqlx::query_as!(
            LabelRow,
            r#"SELECT id, board, remote_board_id, remote_label_id, title, color,
                      deleted as "deleted: bool",
                      ck_title, ck_color, ck_deleted,
                      remote_seen as "remote_seen: DateTime<Utc>"
               FROM labels ORDER BY id"#
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| crate::error::repo_error(&e))?;

        let task_rows = sqlx::query_as!(
            TaskRow,
            r#"SELECT id, stack, remote_board_id, remote_stack_id, remote_card_id,
                      title, description,
                      duedate as "duedate: DateTime<Utc>",
                      done as "done: DateTime<Utc>",
                      sort_order,
                      archived as "archived: bool", deleted as "deleted: bool",
                      ck_title, ck_description, ck_duedate, ck_done, ck_position,
                      ck_labels, ck_archived, ck_deleted,
                      remote_seen as "remote_seen: DateTime<Utc>"
               FROM tasks ORDER BY id"#
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| crate::error::repo_error(&e))?;

        let task_label_rows =
            sqlx::query!(r#"SELECT task, label FROM task_labels ORDER BY task, label"#)
                .fetch_all(&self.pool)
                .await
                .map_err(|e| crate::error::repo_error(&e))?;

        let outbox_rows = sqlx::query_as!(
            OutboxRow,
            r#"SELECT op_id, op_kind, task_id, stack_id, label_id,
                      queued_at as "queued_at: DateTime<Utc>"
               FROM outbox ORDER BY op_seq"#
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| crate::error::repo_error(&e))?;

        let validator_rows = sqlx::query_as!(
            ValidatorRow,
            r#"SELECT key, etag, last_modified FROM sync_metadata ORDER BY key"#
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| crate::error::repo_error(&e))?;

        let status_row = sqlx::query_as!(
            SyncStatusRow,
            r#"SELECT phase, last_error,
                      last_success as "last_success: DateTime<Utc>"
               FROM sync_status WHERE id = 1"#
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| crate::error::repo_error(&e))?
        .ok_or(RepositoryError::Corrupted)?;

        let mut boards = BTreeMap::new();
        for row in board_rows {
            let (id, board) = decode_board(row)?;
            boards.insert(BoardId::from(id), board);
        }

        let mut stacks = BTreeMap::new();
        for row in stack_rows {
            let (id, stack) = decode_stack(row)?;
            stacks.insert(StackId::from(id), stack);
        }

        let mut labels = BTreeMap::new();
        for row in label_rows {
            let (id, label) = decode_label(row)?;
            labels.insert(LabelId::from(id), label);
        }

        // Group the join table once; tasks look up their whole label set.
        let mut label_sets: BTreeMap<Uuid, BTreeSet<Uuid>> = BTreeMap::new();
        for row in task_label_rows {
            let task = decode::uuid(&row.task)?;
            let label = decode::uuid(&row.label)?;
            label_sets.entry(task).or_default().insert(label);
        }

        let mut tasks = BTreeMap::new();
        for row in task_rows {
            let (id, task) = decode_task(row, &label_sets)?;
            tasks.insert(TaskId::from(id), task);
        }

        let mut outbox = Vec::with_capacity(outbox_rows.len());
        for row in outbox_rows {
            outbox.push(decode_outbox_row(&row)?);
        }

        let mut validators = BTreeMap::new();
        for row in validator_rows {
            let key =
                codec::validator_key_from_text(&row.key).map_err(|_| RepositoryError::Corrupted)?;
            validators.insert(
                key,
                SyncValidators {
                    etag: row.etag,
                    last_modified: row.last_modified,
                },
            );
        }

        let phase =
            codec::sync_phase_from_columns(&status_row.phase, status_row.last_error.as_deref())
                .map_err(|_| RepositoryError::Corrupted)?;

        // `pending_ops` is derived, never stored (plan decision 9): the
        // outbox is the single source of truth for queue depth.
        let pending_ops = sqlx::query!(r#"SELECT COUNT(*) AS depth FROM outbox"#)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| crate::error::repo_error(&e))?
            .depth;
        let pending_ops = u32::try_from(pending_ops).unwrap_or(u32::MAX);

        let state = PersistedState {
            boards,
            stacks,
            tasks,
            labels,
            outbox,
            validators,
            sync: SyncStatus {
                phase,
                last_success: status_row.last_success,
                pending_ops,
            },
        };
        tracing::trace!(
            boards = state.boards.len(),
            stacks = state.stacks.len(),
            tasks = state.tasks.len(),
            labels = state.labels.len(),
            pending_ops = state.sync.pending_ops,
            "load complete"
        );
        Ok(state)
    }

    #[tracing::instrument(skip(self, actions), err, fields(actions = actions.len()))]
    async fn apply_inner(&self, actions: Vec<PersistenceAction>) -> Result<(), RepositoryError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| crate::error::repo_error(&e))?;
        tracing::trace!("transaction begun");

        // Fixed execution order satisfying the FK graph (plan decision 6):
        // boards → stacks → labels → tasks (+ task_labels) → outbox →
        // validators → sync status. Within each partition batch order is
        // preserved, so the last write of a field wins exactly as the
        // engine issued it.
        let result = execute_batch(&mut tx, &actions).await;

        match result {
            Ok(()) => {
                tx.commit()
                    .await
                    .map_err(|e| crate::error::repo_error(&e))?;
                tracing::trace!("transaction committed");
                tracing::info!(actions = actions.len(), "batch applied");
                Ok(())
            }
            Err(err) => {
                // Dropping `tx` without commit rolls the whole batch back —
                // the port's atomicity clause.
                tracing::error!(?err, "apply batch rolled back");
                Err(err)
            }
        }
    }
}

#[allow(clippy::too_many_lines)] // one transaction body; the order IS the design
async fn execute_batch(
    tx: &mut sqlx::SqliteConnection,
    actions: &[PersistenceAction],
) -> Result<(), RepositoryError> {
    for action in actions {
        if let PersistenceAction::UpsertBoard(board) = action {
            let remote = board
                .remote
                .map(|b| i64::try_from(b.get()).unwrap_or(i64::MAX));
            sqlx::query!(
                r#"INSERT INTO boards (id, remote_board_id, title, color, archived, deleted, remote_seen)
                   VALUES (?, ?, ?, ?, ?, ?, ?)
                   ON CONFLICT(id) DO UPDATE SET
                       remote_board_id = excluded.remote_board_id,
                       title = excluded.title,
                       color = excluded.color,
                       archived = excluded.archived,
                       deleted = excluded.deleted,
                       remote_seen = excluded.remote_seen"#,
                codec::uuid_to_text(board.id.as_uuid()),
                remote,
                board.title,
                board.color.as_str(),
                board.archived,
                board.deleted,
                board.remote_seen,
            )
            .execute(&mut *tx)
            .await.map_err(|e| crate::error::repo_error(&e))?;
            tracing::trace!(board = ?board.id, "board upserted");
        }
    }

    for action in actions {
        if let PersistenceAction::UpsertStack(stack) = action {
            let (rb, rs) = stack.remote.map_or((None, None), |r| {
                let (b, s) = codec::remote_stack_to_columns(r);
                (Some(b), Some(s))
            });
            sqlx::query!(
                r#"INSERT INTO stacks (id, board, remote_board_id, remote_stack_id, title, sort_order, archived, deleted, ck_title, ck_sort_order, ck_deleted, remote_seen)
                   VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                   ON CONFLICT(id) DO UPDATE SET
                       board = excluded.board,
                       remote_board_id = excluded.remote_board_id,
                       remote_stack_id = excluded.remote_stack_id,
                       title = excluded.title,
                       sort_order = excluded.sort_order,
                       archived = excluded.archived,
                       deleted = excluded.deleted,
                       ck_title = excluded.ck_title,
                       ck_sort_order = excluded.ck_sort_order,
                       ck_deleted = excluded.ck_deleted,
                       remote_seen = excluded.remote_seen"#,
                codec::uuid_to_text(stack.id.as_uuid()),
                codec::uuid_to_text(stack.board.as_uuid()),
                rb,
                rs,
                stack.title,
                stack.order,
                stack.archived,
                stack.deleted,
                stack.clocks.title.to_rfc3339(),
                stack.clocks.order.to_rfc3339(),
                stack.clocks.deleted.to_rfc3339(),
                stack.remote_seen,
            )
            .execute(&mut *tx)
            .await.map_err(|e| crate::error::repo_error(&e))?;
            tracing::trace!(stack = ?stack.id, board = ?stack.board, "stack upserted");
        }
    }

    for action in actions {
        if let PersistenceAction::UpsertLabel(label) = action {
            let (rb, rl) = label.remote.map_or((None, None), |r| {
                let (b, l) = codec::remote_label_to_columns(r);
                (Some(b), Some(l))
            });
            sqlx::query!(
                r#"INSERT INTO labels (id, board, remote_board_id, remote_label_id, title, color, deleted, ck_title, ck_color, ck_deleted, remote_seen)
                   VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                   ON CONFLICT(id) DO UPDATE SET
                       board = excluded.board,
                       remote_board_id = excluded.remote_board_id,
                       remote_label_id = excluded.remote_label_id,
                       title = excluded.title,
                       color = excluded.color,
                       deleted = excluded.deleted,
                       ck_title = excluded.ck_title,
                       ck_color = excluded.ck_color,
                       ck_deleted = excluded.ck_deleted,
                       remote_seen = excluded.remote_seen"#,
                codec::uuid_to_text(label.id.as_uuid()),
                codec::uuid_to_text(label.board.as_uuid()),
                rb,
                rl,
                label.title,
                label.color.as_str(),
                label.deleted,
                label.clocks.title.to_rfc3339(),
                label.clocks.color.to_rfc3339(),
                label.clocks.deleted.to_rfc3339(),
                label.remote_seen,
            )
            .execute(&mut *tx)
            .await.map_err(|e| crate::error::repo_error(&e))?;
            tracing::trace!(label = ?label.id, board = ?label.board, "label upserted");
        }
    }

    for action in actions {
        if let PersistenceAction::UpsertTask(task) = action {
            let (rb, rs, rc) = task.remote.map_or((None, None, None), |r| {
                let (b, s, c) = codec::remote_card_to_columns(r);
                (Some(b), Some(s), Some(c))
            });
            sqlx::query!(
                r#"INSERT INTO tasks (id, stack, remote_board_id, remote_stack_id, remote_card_id, title, description, duedate, done, sort_order, archived, deleted, ck_title, ck_description, ck_duedate, ck_done, ck_position, ck_labels, ck_archived, ck_deleted, remote_seen)
                   VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                   ON CONFLICT(id) DO UPDATE SET
                       stack = excluded.stack,
                       remote_board_id = excluded.remote_board_id,
                       remote_stack_id = excluded.remote_stack_id,
                       remote_card_id = excluded.remote_card_id,
                       title = excluded.title,
                       description = excluded.description,
                       duedate = excluded.duedate,
                       done = excluded.done,
                       sort_order = excluded.sort_order,
                       archived = excluded.archived,
                       deleted = excluded.deleted,
                       ck_title = excluded.ck_title,
                       ck_description = excluded.ck_description,
                       ck_duedate = excluded.ck_duedate,
                       ck_done = excluded.ck_done,
                       ck_position = excluded.ck_position,
                       ck_labels = excluded.ck_labels,
                       ck_archived = excluded.ck_archived,
                       ck_deleted = excluded.ck_deleted,
                       remote_seen = excluded.remote_seen"#,
                codec::uuid_to_text(task.id.as_uuid()),
                codec::uuid_to_text(task.stack.as_uuid()),
                rb,
                rs,
                rc,
                task.title,
                task.description,
                task.duedate,
                task.done,
                task.order,
                task.archived,
                task.deleted,
                task.clocks.title.to_rfc3339(),
                task.clocks.description.to_rfc3339(),
                task.clocks.duedate.to_rfc3339(),
                task.clocks.done.to_rfc3339(),
                task.clocks.position.to_rfc3339(),
                task.clocks.labels.to_rfc3339(),
                task.clocks.archived.to_rfc3339(),
                task.clocks.deleted.to_rfc3339(),
                task.remote_seen,
            )
            .execute(&mut *tx)
            .await.map_err(|e| crate::error::repo_error(&e))?;

            // The label set is one whole-set field: replace its join rows
            // wholesale. Plain upserts only — `INSERT OR REPLACE` would
            // delete-then-insert and fire `ON DELETE CASCADE` (plan T5).
            sqlx::query!(
                r#"DELETE FROM task_labels WHERE task = ?"#,
                codec::uuid_to_text(task.id.as_uuid())
            )
            .execute(&mut *tx)
            .await
            .map_err(|e| crate::error::repo_error(&e))?;
            for label in &task.labels {
                sqlx::query!(
                    r#"INSERT INTO task_labels (task, label) VALUES (?, ?)"#,
                    codec::uuid_to_text(task.id.as_uuid()),
                    codec::uuid_to_text(label.as_uuid()),
                )
                .execute(&mut *tx)
                .await
                .map_err(|e| crate::error::repo_error(&e))?;
            }
            tracing::trace!(task = ?task.id, stack = ?task.stack, labels = task.labels.len(), "task upserted");
        }
    }

    // Op removals before appends so a batch that completes and re-enqueues
    // the same id keeps the new entry.
    for action in actions {
        if let PersistenceAction::CompleteOp(op_id) | PersistenceAction::FailOp(op_id) = action {
            sqlx::query!(
                r#"DELETE FROM outbox WHERE op_id = ?"#,
                codec::uuid_to_text(op_id.0)
            )
            .execute(&mut *tx)
            .await
            .map_err(|e| crate::error::repo_error(&e))?;
            tracing::trace!(op = ?op_id.0, "op removed from outbox");
        }
    }

    for action in actions {
        if let PersistenceAction::EnqueueOp(op) = action {
            let cols = codec::op_to_columns(&op.op);
            let task_id = cols.task_id.map(|t| codec::uuid_to_text(t.as_uuid()));
            let stack_id = cols.stack_id.map(|s| codec::uuid_to_text(s.as_uuid()));
            let label_id = cols.label_id.map(|l| codec::uuid_to_text(l.as_uuid()));
            // Upsert on op_id: re-enqueueing an existing id refreshes the
            // row in place and keeps the original queue position — the
            // `UNIQUE` constraint stays the integrity backstop (plan P4).
            sqlx::query!(
                r#"INSERT INTO outbox (op_id, op_kind, task_id, stack_id, label_id, queued_at)
                   VALUES (?, ?, ?, ?, ?, ?)
                   ON CONFLICT(op_id) DO UPDATE SET
                       op_kind = excluded.op_kind,
                       task_id = excluded.task_id,
                       stack_id = excluded.stack_id,
                       label_id = excluded.label_id,
                       queued_at = excluded.queued_at"#,
                codec::uuid_to_text(op.op_id.0),
                codec::op_kind_to_text(&op.op),
                task_id,
                stack_id,
                label_id,
                op.queued_at,
            )
            .execute(&mut *tx)
            .await
            .map_err(|e| crate::error::repo_error(&e))?;
            tracing::trace!(op = ?op.op_id.0, kind = codec::op_kind_to_text(&op.op), "op enqueued");
        }
    }

    for action in actions {
        if let PersistenceAction::UpsertValidators(key, validators) = action {
            sqlx::query!(
                r#"INSERT INTO sync_metadata (key, etag, last_modified) VALUES (?, ?, ?)
                   ON CONFLICT(key) DO UPDATE SET
                       etag = excluded.etag,
                       last_modified = excluded.last_modified"#,
                codec::validator_key_to_text(key),
                validators.etag,
                validators.last_modified,
            )
            .execute(&mut *tx)
            .await
            .map_err(|e| crate::error::repo_error(&e))?;
            tracing::trace!(
                key = codec::validator_key_to_text(key),
                "validators upserted"
            );
        }
    }

    for action in actions {
        if let PersistenceAction::UpsertSyncStatus(status) = action {
            let last_error = match status.phase {
                SyncPhase::Failed { last_error } => {
                    Some(codec::sync_error_kind_to_text(last_error))
                }
                _ => None,
            };
            sqlx::query!(
                r#"INSERT INTO sync_status (id, phase, last_error, last_success) VALUES (1, ?, ?, ?)
                   ON CONFLICT(id) DO UPDATE SET
                       phase = excluded.phase,
                       last_error = excluded.last_error,
                       last_success = excluded.last_success"#,
                codec::sync_phase_to_text(&status.phase),
                last_error,
                status.last_success,
            )
            .execute(&mut *tx)
            .await
            .map_err(|e| crate::error::repo_error(&e))?;
            tracing::trace!(
                phase = codec::sync_phase_to_text(&status.phase),
                "sync status upserted"
            );
        }
    }

    Ok(())
}
