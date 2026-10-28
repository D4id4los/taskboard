// SPDX-License-Identifier: MIT OR Apache-2.0
//! The sqlite [`TaskRepository`] adapter: full hydration at boot, one
//! transaction per apply batch.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use sqlx::SqlitePool;
use taskboard_domain::entities::{
    Board, BoardClocks, Color, Label, LabelClocks, Stack, StackClocks, Task, TaskClocks,
};
use taskboard_domain::ids::{
    AdhocAlarmId, BoardId, ConnectorId, LabelId, ReminderRuleId, RemoteBoardBinding, RemoteBoardId,
    RemoteCardId, RemoteCardRef, RemoteLabelId, RemoteLabelRef, RemoteStackId, RemoteStackRef,
    StackId, TaskId,
};
use taskboard_domain::outbox::{OpId, PendingOp};
use taskboard_domain::persistence::{
    BoxFuture, PersistedState, PersistenceAction, RepositoryError, SyncValidators, TaskRepository,
};
use taskboard_domain::reminders::{
    AdhocAlarm, ReminderDelivery, ReminderDismissal, ReminderOffset, ReminderRef, ReminderRule,
    ReminderSnoozeKey, ReminderState,
};
use taskboard_domain::state::{SyncPhase, SyncStates, derive_sync_states};
use uuid::Uuid;

use crate::codec;

/// The local disk-cache repository. Cheap to clone via `Arc`; see
/// [`connect`](crate::connect) for construction.
#[derive(Debug, Clone)]
pub struct SqliteTaskRepository {
    pub(crate) pool: SqlitePool,
    /// The connector every board upsert tags its binding with (the
    /// config-derived default; see `connect::open`). Legacy NULL tags
    /// are read-only relics — decoded by this rule only when a decode
    /// consumer exists (chunk 4); never backfilled at boot.
    pub(crate) default_connector: ConnectorId,
}

/// Raw `boards` row (`query_as!` target; domain structs with nested clocks
/// are not constructible by the macro).
struct BoardRow {
    id: String,
    remote_board_id: Option<i64>,
    connector: Option<String>,
    title: String,
    color: String,
    archived: bool,
    deleted: bool,
    ck_title: DateTime<Utc>,
    ck_color: DateTime<Utc>,
    ck_archived: DateTime<Utc>,
    ck_deleted: DateTime<Utc>,
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
    scheduled: Option<DateTime<Utc>>,
    done: Option<DateTime<Utc>>,
    sort_order: i64,
    effort: Option<i64>,
    importance: Option<String>,
    urgency: Option<String>,
    recur: Option<String>,
    archived: bool,
    deleted: bool,
    ck_title: String,
    ck_description: String,
    ck_duedate: String,
    ck_scheduled: String,
    ck_done: String,
    ck_position: String,
    ck_effort: String,
    ck_importance: String,
    ck_urgency: String,
    ck_recur: String,
    ck_archived: String,
    ck_deleted: String,
    remote_seen: Option<DateTime<Utc>>,
    remote_baseline: Option<String>,
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
    board_id: Option<String>,
    queued_at: DateTime<Utc>,
}

/// Raw `sync_metadata` row. The table stores the validator bundles; the
/// `sync_metadata` name predates the domain's `ValidatorKey` naming and
/// stays for migration immutability (ADR 0005: shipped migrations are
/// immutable).
struct ValidatorRow {
    key: String,
    etag: Option<String>,
    last_modified: Option<String>,
}

/// Raw `sync_status` row (one per board).
struct SyncStatusRow {
    board: String,
    phase: String,
    last_error: Option<String>,
    last_success: Option<DateTime<Utc>>,
}

/// Raw `reminder_rules` row.
struct ReminderRuleRow {
    rule_id: String,
    task_id: String,
    offset_seconds: i64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

/// Raw `adhoc_alarms` row.
struct AdhocAlarmRow {
    alarm_id: String,
    label: String,
    fire_at: DateTime<Utc>,
    created_at: DateTime<Utc>,
}

/// Raw `reminder_deliveries` row.
struct ReminderDeliveryRow {
    reminder: String,
    occurrence: DateTime<Utc>,
    delivered_at: DateTime<Utc>,
}

/// Raw `reminder_snoozes` row.
struct ReminderSnoozeRow {
    reminder: String,
    occurrence: DateTime<Utc>,
    rescheduled_to: DateTime<Utc>,
}

/// Raw `reminder_dismissals` row.
struct ReminderDismissalRow {
    reminder: String,
    occurrence: DateTime<Utc>,
    dismissed_at: DateTime<Utc>,
}

fn decode_adhoc_alarm(row: &AdhocAlarmRow) -> Result<AdhocAlarm, RepositoryError> {
    Ok(AdhocAlarm {
        id: AdhocAlarmId::from(decode::uuid(&row.alarm_id)?),
        label: row.label.clone(),
        fire_at: row.fire_at,
        created_at: row.created_at,
    })
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

    pub(crate) fn importance(
        raw: &str,
    ) -> Result<taskboard_domain::entities::Importance, RepositoryError> {
        codec::importance_from_text(raw).ok_or(RepositoryError::Corrupted)
    }

    pub(crate) fn urgency(
        raw: &str,
    ) -> Result<taskboard_domain::entities::Urgency, RepositoryError> {
        codec::urgency_from_text(raw).ok_or(RepositoryError::Corrupted)
    }

    pub(crate) fn recur(raw: &str) -> Result<taskboard_domain::recur::RecurRule, RepositoryError> {
        codec::recur_from_text(raw).ok_or(RepositoryError::Corrupted)
    }

    pub(crate) fn remote_baseline(
        raw: &str,
    ) -> Result<taskboard_domain::entities::RemoteTaskBaseline, RepositoryError> {
        // A baseline we cannot read must not silently downgrade to the
        // fallback: the next merge would blanket-clobber — the exact bug
        // the record exists to close (ADR 0004's D-7).
        serde_json::from_str(raw).map_err(|_| RepositoryError::Corrupted)
    }
}

/// Maps a failed statement onto the port's failure classes, logging the
/// source error first so a bug report localizes the failing SQL (plan
/// §3.15) instead of seeing only the coarse port variant.
fn stmt_error(err: &sqlx::Error) -> RepositoryError {
    tracing::debug!(error = %err, "sqlite statement failed");
    crate::error::repo_error(err)
}

fn decode_board(row: BoardRow, default_connector: &ConnectorId) -> Result<Board, RepositoryError> {
    let id = decode::uuid(&row.id)?;
    let remote = decode::remote_opt(&[row.remote_board_id])?.map(|nums| RemoteBoardBinding {
        // NULL tag ⇒ the spawn-time default (DF-4's resolution home); a
        // non-NULL tag naming an unconfigured connector decodes as-is
        // (D-3) — the owning actor simply never finds the board.
        connector: row
            .connector
            .map_or_else(|| default_connector.clone(), ConnectorId::from),
        board: RemoteBoardId(nums[0]),
    });
    Ok(Board {
        id: BoardId::from(id),
        remote,
        title: row.title,
        color: Color::new(row.color),
        archived: row.archived,
        deleted: row.deleted,
        clocks: BoardClocks {
            title: row.ck_title,
            color: row.ck_color,
            archived: row.ck_archived,
            deleted: row.ck_deleted,
        },
        remote_seen: row.remote_seen,
    })
}

fn decode_stack(row: StackRow) -> Result<Stack, RepositoryError> {
    let id = decode::uuid(&row.id)?;
    let board = decode::uuid(&row.board)?;
    let remote = decode::remote_opt(&[row.remote_board_id, row.remote_stack_id])?.map(|nums| {
        RemoteStackRef {
            board: RemoteBoardId(nums[0]),
            stack: RemoteStackId(nums[1]),
        }
    });
    Ok(Stack {
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
    })
}

#[allow(clippy::too_many_lines)] // 21 columns; the mapping is linear
fn decode_task(
    row: TaskRow,
    label_sets: &BTreeMap<Uuid, BTreeMap<Uuid, taskboard_domain::entities::MembershipState>>,
) -> Result<Task, RepositoryError> {
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
    let label_memberships: BTreeMap<LabelId, taskboard_domain::entities::MembershipState> =
        label_sets
            .get(&id)
            .map(|set| set.iter().map(|(u, s)| (LabelId::from(*u), *s)).collect())
            .unwrap_or_default();
    Ok(Task {
        id: TaskId::from(id),
        remote,
        title: row.title,
        description: row.description,
        duedate: row.duedate,
        scheduled: row.scheduled,
        done: row.done,
        stack: StackId::from(stack),
        order: row.sort_order,
        label_memberships,
        effort: row
            .effort
            .map(u64::try_from)
            .transpose()
            .map_err(|_| RepositoryError::Corrupted)?
            .map(taskboard_domain::entities::Effort::new),
        importance: row
            .importance
            .as_deref()
            .map(decode::importance)
            .transpose()?,
        urgency: row.urgency.as_deref().map(decode::urgency).transpose()?,
        recur: row.recur.as_deref().map(decode::recur).transpose()?,

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
            scheduled: row
                .ck_scheduled
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
            effort: row
                .ck_effort
                .parse()
                .map_err(|_| RepositoryError::Corrupted)?,
            importance: row
                .ck_importance
                .parse()
                .map_err(|_| RepositoryError::Corrupted)?,
            urgency: row
                .ck_urgency
                .parse()
                .map_err(|_| RepositoryError::Corrupted)?,
            recur: row
                .ck_recur
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
        remote_baseline: row
            .remote_baseline
            .as_deref()
            .map(decode::remote_baseline)
            .transpose()?,
    })
}

fn decode_label(row: LabelRow) -> Result<Label, RepositoryError> {
    let id = decode::uuid(&row.id)?;
    let board = decode::uuid(&row.board)?;
    let remote = decode::remote_opt(&[row.remote_board_id, row.remote_label_id])?.map(|nums| {
        RemoteLabelRef {
            board: RemoteBoardId(nums[0]),
            label: RemoteLabelId(nums[1]),
        }
    });
    Ok(Label {
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
    })
}

fn decode_outbox_row(row: &OutboxRow) -> Result<PendingOp, RepositoryError> {
    let op_id = OpId(decode::uuid(&row.op_id)?);
    let op = codec::op_from_columns(
        &row.op_kind,
        row.task_id.as_deref(),
        row.stack_id.as_deref(),
        row.label_id.as_deref(),
        row.board_id.as_deref(),
    )
    .map_err(|_| RepositoryError::Corrupted)?;
    Ok(PendingOp {
        op_id,
        op,
        queued_at: row.queued_at,
    })
}

/// Decodes a stored reminder reference (`"rule:<uuid>"`/`"default:<uuid>"`).
fn decode_reminder_ref(raw: &str) -> Result<ReminderRef, RepositoryError> {
    codec::reminder_ref_from_text(raw).map_err(|_| RepositoryError::Corrupted)
}

fn decode_reminder_rule(
    row: &ReminderRuleRow,
) -> Result<(ReminderRuleId, ReminderRule), RepositoryError> {
    // The domain constructor is the integrity check: an unrepresentable
    // or positive stored offset is corruption, not a clamped value.
    let duration =
        chrono::TimeDelta::try_seconds(row.offset_seconds).ok_or(RepositoryError::Corrupted)?;
    let offset = ReminderOffset::before(duration).map_err(|_| RepositoryError::Corrupted)?;
    let id = ReminderRuleId::from(decode::uuid(&row.rule_id)?);
    Ok((
        id,
        ReminderRule {
            id,
            task: TaskId::from(decode::uuid(&row.task_id)?),
            offset,
            created_at: row.created_at,
            // 0007's NOT NULL DEFAULT epoch column: pre-chunk-4 rows
            // decode with the epoch, which loses every clock race
            // exactly like `BoardClocks::epoch`'s precedent.
            updated_at: row.updated_at,
        },
    ))
}

fn decode_reminder_delivery(
    row: &ReminderDeliveryRow,
) -> Result<ReminderDelivery, RepositoryError> {
    Ok(ReminderDelivery {
        reminder: decode_reminder_ref(&row.reminder)?,
        occurrence: row.occurrence,
        delivered_at: row.delivered_at,
    })
}

fn decode_reminder_dismissal(
    row: &ReminderDismissalRow,
) -> Result<ReminderDismissal, RepositoryError> {
    Ok(ReminderDismissal {
        reminder: decode_reminder_ref(&row.reminder)?,
        occurrence: row.occurrence,
        dismissed_at: row.dismissed_at,
    })
}

fn decode_reminder_snooze(
    row: &ReminderSnoozeRow,
) -> Result<(ReminderSnoozeKey, DateTime<Utc>), RepositoryError> {
    let key = ReminderSnoozeKey {
        reminder: decode_reminder_ref(&row.reminder)?,
        occurrence: row.occurrence,
    };
    Ok((key, row.rescheduled_to))
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
    /// Disposes the pool deterministically: awaits the closure of every
    /// connection, letting sqlite checkpoint and remove the `-wal`/`-shm`
    /// siblings. `Drop` alone only marks the pool closed (sqlx 0.9), so a
    /// WAL database whose owner is dropped without `close()` leaves the
    /// sidecar files behind — callers that care about teardown (tests,
    /// embedders) must await this before dropping the database handle.
    pub async fn close(&self) {
        self.pool.close().await;
    }

    #[tracing::instrument(skip(self), err)]
    #[allow(clippy::too_many_lines)] // eight SELECTs; the length is the schema's
    async fn load_inner(&self) -> Result<PersistedState, RepositoryError> {
        // One read transaction around all SELECTs: the single pooled
        // connection would otherwise be released between queries, and a
        // concurrent `apply` could interleave — yielding a state mixing
        // pre- and post-batch rows, with `pending_ops` disagreeing with
        // the hydrated outbox.
        let mut tx = self.pool.begin().await.map_err(|e| stmt_error(&e))?;

        let board_rows = sqlx::query_as!(
            BoardRow,
            r#"SELECT id, remote_board_id, connector, title, color,
                      archived as "archived: bool", deleted as "deleted: bool",
                      ck_title as "ck_title: DateTime<Utc>",
                      ck_color as "ck_color: DateTime<Utc>",
                      ck_archived as "ck_archived: DateTime<Utc>",
                      ck_deleted as "ck_deleted: DateTime<Utc>",
                      remote_seen as "remote_seen: DateTime<Utc>"
               FROM boards ORDER BY id"#
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| stmt_error(&e))?;

        let stack_rows = sqlx::query_as!(
            StackRow,
            r#"SELECT id, board, remote_board_id, remote_stack_id, title, sort_order,
                      archived as "archived: bool", deleted as "deleted: bool",
                      ck_title, ck_sort_order, ck_deleted,
                      remote_seen as "remote_seen: DateTime<Utc>"
               FROM stacks ORDER BY id"#
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| stmt_error(&e))?;

        let label_rows = sqlx::query_as!(
            LabelRow,
            r#"SELECT id, board, remote_board_id, remote_label_id, title, color,
                      deleted as "deleted: bool",
                      ck_title, ck_color, ck_deleted,
                      remote_seen as "remote_seen: DateTime<Utc>"
               FROM labels ORDER BY id"#
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| stmt_error(&e))?;

        let task_rows = sqlx::query_as!(
            TaskRow,
            r#"SELECT id, stack, remote_board_id, remote_stack_id, remote_card_id,
                      title, description,
                      duedate as "duedate: DateTime<Utc>",
                      scheduled as "scheduled: DateTime<Utc>",
                      done as "done: DateTime<Utc>",
                      sort_order,
                      effort,
                      importance, urgency, recur,
                      archived as "archived: bool", deleted as "deleted: bool",
                      ck_title, ck_description, ck_duedate, ck_scheduled, ck_done,
                      ck_position, ck_effort, ck_importance, ck_urgency,
                      ck_recur, ck_archived, ck_deleted,
                      remote_seen as "remote_seen: DateTime<Utc>",
                      remote_baseline
               FROM tasks ORDER BY id"#
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| stmt_error(&e))?;

        let task_label_rows = sqlx::query!(
            r#"SELECT task, label, state, ck_membership FROM task_labels ORDER BY task, label"#
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| stmt_error(&e))?;

        let outbox_rows = sqlx::query_as!(
            OutboxRow,
            r#"SELECT op_id, op_kind, task_id, stack_id, label_id, board_id,
                      queued_at as "queued_at: DateTime<Utc>"
               FROM outbox ORDER BY op_seq"#
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| stmt_error(&e))?;

        let validator_rows = sqlx::query_as!(
            ValidatorRow,
            r#"SELECT key, etag, last_modified FROM sync_metadata ORDER BY key"#
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| stmt_error(&e))?;

        let status_rows = sqlx::query_as!(
            SyncStatusRow,
            r#"SELECT board, phase, last_error,
                      last_success as "last_success: DateTime<Utc>"
               FROM sync_status"#
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| stmt_error(&e))?;

        let reminder_rule_rows = sqlx::query_as!(
            ReminderRuleRow,
            r#"SELECT rule_id, task_id, offset_seconds,
                      created_at as "created_at: DateTime<Utc>",
                      updated_at as "updated_at: DateTime<Utc>"
               FROM reminder_rules ORDER BY rule_id"#
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| stmt_error(&e))?;

        let reminder_delivery_rows = sqlx::query_as!(
            ReminderDeliveryRow,
            r#"SELECT reminder,
                      occurrence as "occurrence: DateTime<Utc>",
                      delivered_at as "delivered_at: DateTime<Utc>"
               FROM reminder_deliveries ORDER BY reminder, occurrence"#
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| stmt_error(&e))?;

        let reminder_dismissal_rows = sqlx::query_as!(
            ReminderDismissalRow,
            r#"SELECT reminder,
                      occurrence as "occurrence: DateTime<Utc>",
                      dismissed_at as "dismissed_at: DateTime<Utc>"
               FROM reminder_dismissals ORDER BY reminder, occurrence"#
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| stmt_error(&e))?;

        let reminder_snooze_rows = sqlx::query_as!(
            ReminderSnoozeRow,
            r#"SELECT reminder,
                      occurrence as "occurrence: DateTime<Utc>",
                      rescheduled_to as "rescheduled_to: DateTime<Utc>"
               FROM reminder_snoozes ORDER BY reminder, occurrence"#
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| stmt_error(&e))?;

        let adhoc_alarm_rows = sqlx::query_as!(
            AdhocAlarmRow,
            r#"SELECT alarm_id, label,
                      fire_at as "fire_at: DateTime<Utc>",
                      created_at as "created_at: DateTime<Utc>"
               FROM adhoc_alarms ORDER BY alarm_id"#
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| stmt_error(&e))?;

        let reminder_muted_rows =
            sqlx::query!(r#"SELECT task_id FROM reminder_muted ORDER BY task_id"#)
                .fetch_all(&mut *tx)
                .await
                .map_err(|e| stmt_error(&e))?;

        let mut boards = BTreeMap::new();
        for row in board_rows {
            let board = decode_board(row, &self.default_connector)?;
            boards.insert(board.id, board);
        }

        let mut stacks = BTreeMap::new();
        for row in stack_rows {
            let stack = decode_stack(row)?;
            stacks.insert(stack.id, stack);
        }

        let mut labels = BTreeMap::new();
        for row in label_rows {
            let label = decode_label(row)?;
            labels.insert(label.id, label);
        }

        // Group the join table once; tasks look up their membership map.
        let mut label_sets: BTreeMap<
            Uuid,
            BTreeMap<Uuid, taskboard_domain::entities::MembershipState>,
        > = BTreeMap::new();
        for row in task_label_rows {
            let task = decode::uuid(&row.task)?;
            let label = decode::uuid(&row.label)?;
            let clock: DateTime<Utc> = row
                .ck_membership
                .parse()
                .map_err(|_| RepositoryError::Corrupted)?;
            let state = match row.state.as_str() {
                "added" => taskboard_domain::entities::MembershipState::Added { at: clock },
                "removed" => taskboard_domain::entities::MembershipState::Removed { at: clock },
                _ => return Err(RepositoryError::Corrupted),
            };
            label_sets.entry(task).or_default().insert(label, state);
        }

        let mut tasks = BTreeMap::new();
        for row in task_rows {
            let task = decode_task(row, &label_sets)?;
            tasks.insert(task.id, task);
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

        // Zero rows is success (the empty map — a fresh install or a
        // standalone whose board-less singleton the 0006 rebuild dropped).
        let mut sync = SyncStates::new();
        for status_row in status_rows {
            let phase =
                codec::sync_phase_from_columns(&status_row.phase, status_row.last_error.as_deref())
                    .map_err(|_| RepositoryError::Corrupted)?;
            let board = decode::uuid(&status_row.board)?;
            sync.set_stored(BoardId::from(board), phase, status_row.last_success);
        }

        let mut rules = BTreeMap::new();
        for row in reminder_rule_rows {
            let (id, rule) = decode_reminder_rule(&row)?;
            rules.insert(id, rule);
        }
        let mut deliveries = BTreeSet::new();
        for row in reminder_delivery_rows {
            deliveries.insert(decode_reminder_delivery(&row)?);
        }
        let mut dismissals = BTreeSet::new();
        for row in reminder_dismissal_rows {
            dismissals.insert(decode_reminder_dismissal(&row)?);
        }
        let mut snoozes = BTreeMap::new();
        for row in reminder_snooze_rows {
            let (key, rescheduled_to) = decode_reminder_snooze(&row)?;
            snoozes.insert(key, rescheduled_to);
        }
        let mut muted = BTreeSet::new();
        for row in reminder_muted_rows {
            muted.insert(TaskId::from(decode::uuid(&row.task_id)?));
        }
        let mut alarms = BTreeMap::new();
        for row in adhoc_alarm_rows {
            let alarm = decode_adhoc_alarm(&row)?;
            alarms.insert(alarm.id, alarm);
        }
        let reminders = ReminderState {
            rules,
            deliveries,
            dismissals,
            snoozes,
            muted,
            alarms,
        };

        let mut state = PersistedState {
            boards,
            stacks,
            tasks,
            labels,
            outbox,
            validators,
            sync,
            reminders,
            // Chunk DR-5/2's migration 0013 hydrates this map; the empty
            // default is the honest shape until that codec exists.
            tracking: taskboard_domain::TrackingState::default(),
        };
        // The per-board projection is derived, never stored: stored rows
        // carry phase/last_success, `pending_ops` is attributed from the
        // outbox rows just hydrated — no second query that could drift.
        derive_sync_states(
            &state.boards,
            &state.stacks,
            &state.tasks,
            &state.labels,
            &state.outbox,
            &mut state.sync,
        );
        // Read-only transaction: dropping without commit releases the
        // snapshot.
        drop(tx);
        tracing::trace!(
            boards = state.boards.len(),
            stacks = state.stacks.len(),
            tasks = state.tasks.len(),
            labels = state.labels.len(),
            pending_ops = state.sync.aggregate().pending_ops,
            "load complete"
        );
        Ok(state)
    }

    #[tracing::instrument(skip(self, actions), err, fields(actions = actions.len()))]
    async fn apply_inner(&self, actions: Vec<PersistenceAction>) -> Result<(), RepositoryError> {
        let mut tx = self.pool.begin().await.map_err(|e| stmt_error(&e))?;
        tracing::trace!("transaction begun");

        // Fixed execution order satisfying the FK graph (plan decision 6):
        // boards → stacks → labels → tasks (+ task_labels) → outbox →
        // validators → sync status. Within each partition batch order is
        // preserved, so the last write of a field wins exactly as the
        // engine issued it.
        let result = execute_batch(&mut tx, &actions, &self.default_connector).await;

        match result {
            Ok(()) => {
                tx.commit().await.map_err(|e| stmt_error(&e))?;
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
    default_connector: &ConnectorId,
) -> Result<(), RepositoryError> {
    // No wildcard arm: a new `PersistenceAction` variant must fail this
    // crate's build (E0004) until storage persists it — a silent
    // fall-through here means an action class accepted with `Ok(())` and
    // never written.
    let mut boards = Vec::new();
    let mut stacks = Vec::new();
    let mut labels = Vec::new();
    let mut tasks = Vec::new();
    let mut op_removals = Vec::new();
    let mut op_appends = Vec::new();
    let mut validator_upserts = Vec::new();
    let mut sync_status_upserts = Vec::new();
    let mut sync_status_deletes = Vec::new();
    let mut rule_upserts = Vec::new();
    let mut rule_deletes = Vec::new();
    let mut alarm_upserts = Vec::new();
    let mut alarm_deletes = Vec::new();
    let mut deliveries = Vec::new();
    let mut dismissals = Vec::new();
    let mut snoozes = Vec::new();
    let mut mute_toggles = Vec::new();
    let mut clear_board_content = Vec::new();
    // R-11 (DR-5/1): these two arms are the no-wildcard rule's own tax —
    // the planner cannot emit rows otherwise. The writes below use
    // runtime-checked `sqlx::query` (not the compile-checked macro)
    // because the `time_entries` table arrives with migration 0013 in
    // chunk DR-5/2; no producer emits these actions in this chunk, so
    // the paths are dead until that migration lands.
    let mut entry_upserts = Vec::new();
    let mut entry_deletes = Vec::new();
    for action in actions {
        match action {
            PersistenceAction::UpsertBoard(board) => boards.push(board),
            PersistenceAction::UpsertStack(stack) => stacks.push(stack),
            PersistenceAction::UpsertLabel(label) => labels.push(label),
            PersistenceAction::UpsertTask(task) => tasks.push(task),
            PersistenceAction::CompleteOp(op_id) | PersistenceAction::FailOp(op_id) => {
                op_removals.push(op_id);
            }
            PersistenceAction::EnqueueOp(op) => op_appends.push(op),
            PersistenceAction::UpsertValidators(key, validators) => {
                validator_upserts.push((key, validators));
            }
            PersistenceAction::UpsertSyncStatus(board, status) => {
                sync_status_upserts.push((*board, status.clone()));
            }
            PersistenceAction::DeleteSyncStatus(board) => {
                sync_status_deletes.push(*board);
            }
            PersistenceAction::UpsertReminderRule(rule) => rule_upserts.push(rule),
            PersistenceAction::DeleteReminderRule(rule_id) => rule_deletes.push(rule_id),
            PersistenceAction::UpsertAdhocAlarm(alarm) => alarm_upserts.push(alarm),
            PersistenceAction::DeleteAdhocAlarm(alarm_id) => alarm_deletes.push(alarm_id),
            PersistenceAction::RecordReminderDelivery(delivery) => deliveries.push(delivery),
            PersistenceAction::RecordReminderDismissal(dismissal) => dismissals.push(dismissal),
            PersistenceAction::RecordReminderSnooze(snooze) => snoozes.push(snooze),
            PersistenceAction::SetTaskRemindersMuted(task, muted) => {
                mute_toggles.push((task, muted));
            }
            PersistenceAction::ClearBoardContent(board) => {
                clear_board_content.push(*board);
            }
            PersistenceAction::UpsertTimeEntry(entry) => entry_upserts.push(entry),
            PersistenceAction::DeleteTimeEntry(entry_id) => entry_deletes.push(*entry_id),
        }
    }

    for board in boards {
        let remote = board
            .remote
            .as_ref()
            .map(|b| codec::remote_col(b.board.get()));
        // The tag source is the domain binding (DF-4): a bound row carries
        // its binding's connector, an unbound row the default (non-NULL on
        // every upsert; the tag on an unbound row is inert).
        let connector = board
            .remote
            .as_ref()
            .map_or(default_connector.as_str(), |b| b.connector.as_str());
        sqlx::query!(
            r#"INSERT INTO boards (id, remote_board_id, title, color, archived, deleted, connector, ck_title, ck_color, ck_archived, ck_deleted, remote_seen)
                   VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                   ON CONFLICT(id) DO UPDATE SET
                       remote_board_id = excluded.remote_board_id,
                       title = excluded.title,
                       color = excluded.color,
                       archived = excluded.archived,
                       deleted = excluded.deleted,
                       connector = excluded.connector,
                       ck_title = excluded.ck_title,
                       ck_color = excluded.ck_color,
                       ck_archived = excluded.ck_archived,
                       ck_deleted = excluded.ck_deleted,
                       remote_seen = excluded.remote_seen"#,
            codec::uuid_to_text(board.id.as_uuid()),
            remote,
            board.title,
            board.color.as_str(),
            board.archived,
            board.deleted,
            connector,
            board.clocks.title.to_rfc3339(),
            board.clocks.color.to_rfc3339(),
            board.clocks.archived.to_rfc3339(),
            board.clocks.deleted.to_rfc3339(),
            board.remote_seen,
        )
        .execute(&mut *tx)
        .await.map_err(|e| stmt_error(&e))?;
        tracing::trace!(board = ?board.id, "board upserted");
    }

    for stack in stacks {
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
        .await.map_err(|e| stmt_error(&e))?;
        tracing::trace!(stack = ?stack.id, board = ?stack.board, "stack upserted");
    }

    for label in labels {
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
        .await.map_err(|e| stmt_error(&e))?;
        tracing::trace!(label = ?label.id, board = ?label.board, "label upserted");
    }

    for task in tasks {
        let (rb, rs, rc) = task.remote.map_or((None, None, None), |r| {
            let (b, s, c) = codec::remote_card_to_columns(r);
            (Some(b), Some(s), Some(c))
        });
        // D-F1's typed bound: effort is unsigned in the domain; the
        // storage INTEGER is an i64, so values past i64::MAX are a
        // corrupt-input rejection, never a silent wrap.
        let effort = task
            .effort
            .map(|e| i64::try_from(e.seconds()).map_err(|_| RepositoryError::Corrupted))
            .transpose()?;
        sqlx::query!(
            r#"INSERT INTO tasks (id, stack, remote_board_id, remote_stack_id, remote_card_id, title, description, duedate, scheduled, done, sort_order, effort, importance, urgency, recur, archived, deleted, ck_title, ck_description, ck_duedate, ck_scheduled, ck_done, ck_position, ck_labels, ck_effort, ck_importance, ck_urgency, ck_recur, ck_archived, ck_deleted, remote_seen, remote_baseline)
                   VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                   ON CONFLICT(id) DO UPDATE SET
                       stack = excluded.stack,
                       remote_board_id = excluded.remote_board_id,
                       remote_stack_id = excluded.remote_stack_id,
                       remote_card_id = excluded.remote_card_id,
                       title = excluded.title,
                       description = excluded.description,
                       duedate = excluded.duedate,
                       scheduled = excluded.scheduled,
                       done = excluded.done,
                       sort_order = excluded.sort_order,
                       effort = excluded.effort,
                       importance = excluded.importance,
                       urgency = excluded.urgency,
                       recur = excluded.recur,
                       archived = excluded.archived,
                       deleted = excluded.deleted,
                       ck_title = excluded.ck_title,
                       ck_description = excluded.ck_description,
                       ck_duedate = excluded.ck_duedate,
                       ck_scheduled = excluded.ck_scheduled,
                       ck_done = excluded.ck_done,
                       ck_position = excluded.ck_position,
                       ck_labels = excluded.ck_labels,
                       ck_effort = excluded.ck_effort,
                       ck_importance = excluded.ck_importance,
                       ck_urgency = excluded.ck_urgency,
                       ck_recur = excluded.ck_recur,
                       ck_archived = excluded.ck_archived,
                       ck_deleted = excluded.ck_deleted,
                       remote_seen = excluded.remote_seen,
                       remote_baseline = excluded.remote_baseline"#,
            codec::uuid_to_text(task.id.as_uuid()),
            codec::uuid_to_text(task.stack.as_uuid()),
            rb,
            rs,
            rc,
            task.title,
            task.description,
            task.duedate,
            task.scheduled,
            task.done,
            task.order,
            effort,
            task.importance.map(codec::importance_to_text),
            task.urgency.map(codec::urgency_to_text),
            task.recur.as_ref().map(codec::recur_to_text),
            task.archived,
            task.deleted,
            task.clocks.title.to_rfc3339(),
            task.clocks.description.to_rfc3339(),
            task.clocks.duedate.to_rfc3339(),
            task.clocks.scheduled.to_rfc3339(),
            task.clocks.done.to_rfc3339(),
            task.clocks.position.to_rfc3339(),
            // Vestigial since the OR-set migration (the membership map
            // owns the label clocks): NOT NULL without a DEFAULT keeps
            // the column in every INSERT; it is never read again.
            task.label_memberships
                .values()
                .map(|s| s.at())
                .max()
                .unwrap_or_else(taskboard_domain::entities::BoardClocks::epoch)
                .to_rfc3339(),
            task.clocks.effort.to_rfc3339(),
            task.clocks.importance.to_rfc3339(),
            task.clocks.urgency.to_rfc3339(),
            task.clocks.recur.to_rfc3339(),
            task.clocks.archived.to_rfc3339(),
            task.clocks.deleted.to_rfc3339(),
            task.remote_seen,
            task
                .remote_baseline
                .as_ref()
                .map(codec::remote_baseline_to_text),
        )
        .execute(&mut *tx)
        .await.map_err(|e| stmt_error(&e))?;

        // The memberships are one OR-set map: replace the task's join
        // rows wholesale. Plain upserts only — `INSERT OR REPLACE` would
        // delete-then-insert and fire `ON DELETE CASCADE` (plan T5).
        sqlx::query!(
            r#"DELETE FROM task_labels WHERE task = ?"#,
            codec::uuid_to_text(task.id.as_uuid())
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| stmt_error(&e))?;
        // Every membership entry is written — including `Removed`
        // tombstones: forgetting a removal lets a stale remote re-add
        // flap (the no-flap rule, #129). Tombstoned rows persist while
        // either entity lives; hard row deletion still sweeps via the
        // FK cascades.
        for (label, state) in &task.label_memberships {
            let (state_text, ck) = match state {
                taskboard_domain::entities::MembershipState::Added { at } => {
                    ("added", at.to_rfc3339())
                }
                taskboard_domain::entities::MembershipState::Removed { at } => {
                    ("removed", at.to_rfc3339())
                }
            };
            sqlx::query!(
                r#"INSERT INTO task_labels (task, label, state, ck_membership) VALUES (?, ?, ?, ?)"#,
                codec::uuid_to_text(task.id.as_uuid()),
                codec::uuid_to_text(label.as_uuid()),
                state_text,
                ck,
            )
            .execute(&mut *tx)
            .await
            .map_err(|e| stmt_error(&e))?;
        }
        tracing::trace!(task = ?task.id, stack = ?task.stack, labels = task.live_labels().len(), "task upserted");
    }

    // R-11 (DR-5/1): the time-entry writes, dead until chunk DR-5/2's
    // migration 0013 creates the table (see the partition comment).
    // Written against the plan's D-T9 DDL: hyphenated-UUID TEXT ids,
    // RFC 3339 TEXT stamps, no FKs (entries outlive soft-deleted tasks).
    // Deletes run after upserts so a batch carrying both for one id
    // mirrors `apply_actions`' sequential last-write-wins semantics.
    for entry in entry_upserts {
        sqlx::query(
            r"INSERT INTO time_entries (entry_id, task_id, task_title, start, end, created_at, updated_at)
                   VALUES (?, ?, ?, ?, ?, ?, ?)
                   ON CONFLICT(entry_id) DO UPDATE SET
                       task_id = excluded.task_id,
                       task_title = excluded.task_title,
                       start = excluded.start,
                       end = excluded.end,
                       created_at = excluded.created_at,
                       updated_at = excluded.updated_at",
        )
        .bind(codec::uuid_to_text(entry.id.as_uuid()))
        .bind(codec::uuid_to_text(entry.task.as_uuid()))
        .bind(&entry.task_title)
        .bind(entry.start.to_rfc3339())
        .bind(entry.end.map(|end| end.to_rfc3339()))
        .bind(entry.created_at.to_rfc3339())
        .bind(entry.updated_at.to_rfc3339())
        .execute(&mut *tx)
        .await
        .map_err(|e| stmt_error(&e))?;
        tracing::trace!(entry = ?entry.id, "time entry upserted");
    }
    for entry_id in entry_deletes {
        sqlx::query(r"DELETE FROM time_entries WHERE entry_id = ?")
            .bind(codec::uuid_to_text(entry_id.as_uuid()))
            .execute(&mut *tx)
            .await
            .map_err(|e| stmt_error(&e))?;
        tracing::trace!(entry = ?entry_id, "time entry deleted");
    }

    // Op removals before appends so a batch that completes and re-enqueues
    // the same id keeps the new entry.
    for op_id in op_removals {
        sqlx::query!(
            r#"DELETE FROM outbox WHERE op_id = ?"#,
            codec::uuid_to_text(op_id.0)
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| stmt_error(&e))?;
        tracing::trace!(op = ?op_id.0, "op removed from outbox");
    }

    for op in op_appends {
        let cols = codec::op_to_columns(&op.op);
        let task_id = cols.task_id.map(|t| codec::uuid_to_text(t.as_uuid()));
        let stack_id = cols.stack_id.map(|s| codec::uuid_to_text(s.as_uuid()));
        let label_id = cols.label_id.map(|l| codec::uuid_to_text(l.as_uuid()));
        let board_id = cols.board_id.map(|b| codec::uuid_to_text(b.as_uuid()));
        // Upsert on op_id: re-enqueueing an existing id refreshes the
        // row in place and keeps the original queue position — the
        // `UNIQUE` constraint stays the integrity backstop (plan P4).
        sqlx::query!(
            r#"INSERT INTO outbox (op_id, op_kind, task_id, stack_id, label_id, board_id, queued_at)
                   VALUES (?, ?, ?, ?, ?, ?, ?)
                   ON CONFLICT(op_id) DO UPDATE SET
                       op_kind = excluded.op_kind,
                       task_id = excluded.task_id,
                       stack_id = excluded.stack_id,
                       label_id = excluded.label_id,
                       board_id = excluded.board_id,
                       queued_at = excluded.queued_at"#,
            codec::uuid_to_text(op.op_id.0),
            codec::op_kind_to_text(&op.op),
            task_id,
            stack_id,
            label_id,
            board_id,
            op.queued_at,
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| stmt_error(&e))?;
        tracing::trace!(op = ?op.op_id.0, kind = codec::op_kind_to_text(&op.op), "op enqueued");
    }

    for (key, validators) in validator_upserts {
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
        .map_err(|e| stmt_error(&e))?;
        tracing::trace!(
            key = codec::validator_key_to_text(key),
            "validators upserted"
        );
    }

    for (board, status) in sync_status_upserts {
        let last_error = match status.phase {
            SyncPhase::Failed { last_error } => Some(codec::sync_error_kind_to_text(last_error)),
            _ => None,
        };
        sqlx::query!(
            r#"INSERT INTO sync_status (board, phase, last_error, last_success) VALUES (?, ?, ?, ?)
                   ON CONFLICT(board) DO UPDATE SET
                       phase = excluded.phase,
                       last_error = excluded.last_error,
                       last_success = excluded.last_success"#,
            codec::uuid_to_text(board.as_uuid()),
            codec::sync_phase_to_text(status.phase),
            last_error,
            status.last_success,
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| stmt_error(&e))?;
        tracing::trace!(
            board = ?board,
            phase = codec::sync_phase_to_text(status.phase),
            "sync status upserted"
        );
    }

    // The unbind ceremony's cleanup: drop the board's status row
    // (absent row = no-op, like the shared apply semantics).
    for board in sync_status_deletes {
        sqlx::query!(
            "DELETE FROM sync_status WHERE board = ?",
            codec::uuid_to_text(board.as_uuid())
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| stmt_error(&e))?;
        tracing::trace!(board = ?board, "sync status deleted");
    }

    // Reminder partitions (tasks precede rules — the only FK). Within
    // each partition batch order is preserved, mirroring `apply_actions`:
    // rule upserts replace by id; a rule delete prunes its deliveries,
    // dismissals, and snooze links with it; a delivery or dismissal
    // insert is set-union on the `(reminder, occurrence)` key (first
    // write wins); a snooze link replaces the prior link for the same
    // key; mute toggles membership.
    for rule in rule_upserts {
        // The type guarantees the offset is whole-second and
        // non-positive, so the integer column stores it exactly — no
        // truncation, no divergence between the evaluated offset and
        // the reloaded one.
        sqlx::query!(
            r#"INSERT INTO reminder_rules (rule_id, task_id, offset_seconds, created_at, updated_at)
                   VALUES (?, ?, ?, ?, ?)
                   ON CONFLICT(rule_id) DO UPDATE SET
                       task_id = excluded.task_id,
                       offset_seconds = excluded.offset_seconds,
                       created_at = excluded.created_at,
                       updated_at = excluded.updated_at"#,
            codec::uuid_to_text(rule.id.as_uuid()),
            codec::uuid_to_text(rule.task.as_uuid()),
            rule.offset.as_duration().num_seconds(),
            rule.created_at.to_rfc3339(),
            rule.updated_at.to_rfc3339(),
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| stmt_error(&e))?;
        tracing::trace!(rule = ?rule.id, task = ?rule.task, "reminder rule upserted");
    }

    for rule_id in rule_deletes {
        let reminder = codec::reminder_ref_to_text(ReminderRef::Rule(*rule_id));
        sqlx::query!(
            r#"DELETE FROM reminder_deliveries WHERE reminder = ?"#,
            reminder
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| stmt_error(&e))?;
        sqlx::query!(
            r#"DELETE FROM reminder_dismissals WHERE reminder = ?"#,
            reminder
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| stmt_error(&e))?;
        sqlx::query!(
            r#"DELETE FROM reminder_snoozes WHERE reminder = ?"#,
            reminder
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| stmt_error(&e))?;
        sqlx::query!(
            r#"DELETE FROM reminder_rules WHERE rule_id = ?"#,
            codec::uuid_to_text(rule_id.as_uuid())
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| stmt_error(&e))?;
        tracing::trace!(rule = ?rule_id, "reminder rule pruned with its dependents");
    }

    for alarm in alarm_upserts {
        sqlx::query!(
            r#"INSERT INTO adhoc_alarms (alarm_id, label, fire_at, created_at)
                   VALUES (?, ?, ?, ?)
                   ON CONFLICT(alarm_id) DO UPDATE SET
                       label = excluded.label,
                       fire_at = excluded.fire_at,
                       created_at = excluded.created_at"#,
            codec::uuid_to_text(alarm.id.as_uuid()),
            alarm.label,
            alarm.fire_at.to_rfc3339(),
            alarm.created_at.to_rfc3339(),
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| stmt_error(&e))?;
        tracing::trace!(alarm = ?alarm.id, "adhoc alarm upserted");
    }

    for alarm_id in alarm_deletes {
        let reminder = codec::reminder_ref_to_text(ReminderRef::Adhoc(*alarm_id));
        sqlx::query!(
            r#"DELETE FROM reminder_deliveries WHERE reminder = ?"#,
            reminder
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| stmt_error(&e))?;
        sqlx::query!(
            r#"DELETE FROM reminder_dismissals WHERE reminder = ?"#,
            reminder
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| stmt_error(&e))?;
        sqlx::query!(
            r#"DELETE FROM reminder_snoozes WHERE reminder = ?"#,
            reminder
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| stmt_error(&e))?;
        sqlx::query!(
            r#"DELETE FROM adhoc_alarms WHERE alarm_id = ?"#,
            codec::uuid_to_text(alarm_id.as_uuid())
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| stmt_error(&e))?;
        tracing::trace!(alarm = ?alarm_id, "adhoc alarm pruned with its dependents");
    }

    for delivery in deliveries {
        // First write wins: a replayed delivery is a no-op in SQL too,
        // so the crash-between-apply-and-notify replay stays benign.
        sqlx::query!(
            r#"INSERT INTO reminder_deliveries (reminder, occurrence, delivered_at)
                   VALUES (?, ?, ?)
                   ON CONFLICT(reminder, occurrence) DO NOTHING"#,
            codec::reminder_ref_to_text(delivery.reminder),
            delivery.occurrence.to_rfc3339(),
            delivery.delivered_at.to_rfc3339(),
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| stmt_error(&e))?;
        tracing::trace!(reminder = %codec::reminder_ref_to_text(delivery.reminder), "reminder delivery recorded");
    }

    for dismissal in dismissals {
        // First write wins (the deliveries parity): a replayed dismissal
        // is a no-op in SQL too.
        sqlx::query!(
            r#"INSERT INTO reminder_dismissals (reminder, occurrence, dismissed_at)
                   VALUES (?, ?, ?)
                   ON CONFLICT(reminder, occurrence) DO NOTHING"#,
            codec::reminder_ref_to_text(dismissal.reminder),
            dismissal.occurrence.to_rfc3339(),
            dismissal.dismissed_at.to_rfc3339(),
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| stmt_error(&e))?;
        tracing::trace!(reminder = %codec::reminder_ref_to_text(dismissal.reminder), "reminder dismissal recorded");
    }

    for snooze in snoozes {
        sqlx::query!(
            r#"INSERT INTO reminder_snoozes (reminder, occurrence, rescheduled_to)
                   VALUES (?, ?, ?)
                   ON CONFLICT(reminder, occurrence) DO UPDATE SET
                       rescheduled_to = excluded.rescheduled_to"#,
            codec::reminder_ref_to_text(snooze.reminder),
            snooze.occurrence.to_rfc3339(),
            snooze.rescheduled_to.to_rfc3339(),
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| stmt_error(&e))?;
        tracing::trace!(reminder = %codec::reminder_ref_to_text(snooze.reminder), "reminder snooze recorded");
    }

    // Content clear (the `ResetBoard` repair): one board-scoped bulk
    // delete per table, inside the batch transaction. Order satisfies
    // the FK graph (reminder facts → tasks → task_labels cascade →
    // stacks/labels → outbox → validators). The board row and
    // `sync_status` are never touched.
    for board in clear_board_content {
        let board_uuid = codec::uuid_to_text(board.as_uuid());
        // 1. Outbox rows of the board and its (about-to-vanish) children —
        // the child subqueries must run while the rows still exist.
        sqlx::query!(
            r#"DELETE FROM outbox
               WHERE (op_kind IN ('update_board', 'delete_board') AND board_id = ?1)
                  OR (op_kind IN ('create_task', 'update_task', 'move_task', 'delete_task',
                                  'assign_label', 'unassign_label')
                      AND task_id IN (SELECT t.id FROM tasks t
                                      JOIN stacks s ON t.stack = s.id
                                      WHERE s.board = ?1))
                  OR (op_kind IN ('create_stack', 'rename_stack', 'move_stack', 'delete_stack')
                      AND stack_id IN (SELECT id FROM stacks WHERE board = ?1))
                  OR (op_kind IN ('create_label', 'update_label', 'delete_label')
                      AND label_id IN (SELECT id FROM labels WHERE board = ?1))"#,
            board_uuid
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| stmt_error(&e))?;
        // 2. Reminder facts of the board's tasks (deliveries/snoozes key
        // by `rule:<id>`/`default:<task>` text, so the board's rules are
        // resolved while their rows are still present — scoped to this
        // board's tasks; another board's rule facts are untouched).
        sqlx::query!(
            r#"DELETE FROM reminder_deliveries
               WHERE reminder IN (SELECT 'rule:' || r.rule_id FROM reminder_rules r
                                  WHERE r.task_id IN (SELECT t.id FROM tasks t
                                                      JOIN stacks s ON t.stack = s.id
                                                      WHERE s.board = ?1))
                  OR reminder IN (SELECT 'default:' || t.id FROM tasks t
                                  JOIN stacks s ON t.stack = s.id
                                  WHERE s.board = ?1)"#,
            board_uuid
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| stmt_error(&e))?;
        sqlx::query!(
            r#"DELETE FROM reminder_dismissals
               WHERE reminder IN (SELECT 'rule:' || r.rule_id FROM reminder_rules r
                                  WHERE r.task_id IN (SELECT t.id FROM tasks t
                                                      JOIN stacks s ON t.stack = s.id
                                                      WHERE s.board = ?1))
                  OR reminder IN (SELECT 'default:' || t.id FROM tasks t
                                  JOIN stacks s ON t.stack = s.id
                                  WHERE s.board = ?1)"#,
            board_uuid
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| stmt_error(&e))?;
        sqlx::query!(
            r#"DELETE FROM reminder_snoozes
               WHERE reminder IN (SELECT 'rule:' || r.rule_id FROM reminder_rules r
                                  WHERE r.task_id IN (SELECT t.id FROM tasks t
                                                      JOIN stacks s ON t.stack = s.id
                                                      WHERE s.board = ?1))
                  OR reminder IN (SELECT 'default:' || t.id FROM tasks t
                                  JOIN stacks s ON t.stack = s.id
                                  WHERE s.board = ?1)"#,
            board_uuid
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| stmt_error(&e))?;
        sqlx::query!(
            r#"DELETE FROM reminder_rules
               WHERE task_id IN (SELECT t.id FROM tasks t
                                 JOIN stacks s ON t.stack = s.id
                                 WHERE s.board = ?1)"#,
            board_uuid
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| stmt_error(&e))?;
        sqlx::query!(
            r#"DELETE FROM reminder_muted
               WHERE task_id IN (SELECT t.id FROM tasks t
                                 JOIN stacks s ON t.stack = s.id
                                 WHERE s.board = ?1)"#,
            board_uuid
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| stmt_error(&e))?;
        // 3. The content rows (task_labels cascade with the tasks).
        sqlx::query!(
            r#"DELETE FROM tasks
               WHERE id IN (SELECT t.id FROM tasks t
                            JOIN stacks s ON t.stack = s.id
                            WHERE s.board = ?1)"#,
            board_uuid
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| stmt_error(&e))?;
        sqlx::query!(r#"DELETE FROM stacks WHERE board = ?1"#, board_uuid)
            .execute(&mut *tx)
            .await
            .map_err(|e| stmt_error(&e))?;
        sqlx::query!(r#"DELETE FROM labels WHERE board = ?1"#, board_uuid)
            .execute(&mut *tx)
            .await
            .map_err(|e| stmt_error(&e))?;
        // 4. The sync validators (mandatory: warm validators would let the
        // next conditional pull 304 and never re-materialize the board).
        sqlx::query!(
            r#"DELETE FROM sync_metadata
               WHERE key = 'boards'
                  OR key IN (SELECT 'stacks:' || remote_board_id FROM boards WHERE id = ?1)
                  OR key IN (SELECT 'archived_stacks:' || remote_board_id FROM boards WHERE id = ?1)"#,
            board_uuid
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| stmt_error(&e))?;
        tracing::trace!(board = ?board, "board content cleared");
    }

    for (task, muted) in mute_toggles {
        if *muted {
            sqlx::query!(
                r#"INSERT INTO reminder_muted (task_id) VALUES (?)
                       ON CONFLICT(task_id) DO NOTHING"#,
                codec::uuid_to_text(task.as_uuid()),
            )
            .execute(&mut *tx)
            .await
            .map_err(|e| stmt_error(&e))?;
        } else {
            sqlx::query!(
                r#"DELETE FROM reminder_muted WHERE task_id = ?"#,
                codec::uuid_to_text(task.as_uuid())
            )
            .execute(&mut *tx)
            .await
            .map_err(|e| stmt_error(&e))?;
        }
        tracing::trace!(task = ?task, muted, "task reminders muted toggled");
    }

    Ok(())
}
