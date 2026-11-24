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
    AdhocAlarmId, BoardId, BreakRuleId, ConnectorId, LabelId, ReminderRuleId, RemoteBoardBinding,
    RemoteBoardId, RemoteCardId, RemoteCardRef, RemoteLabelId, RemoteLabelRef, RemoteStackId,
    RemoteStackRef, StackId, TaskId, TimeEntryId,
};
use taskboard_domain::outbox::{OpId, PendingOp};
use taskboard_domain::persistence::{
    BoxFuture, CorruptionDetail, DecodeReason, DecodeStage, LoadDiagnostics, LoadDiagnosticsSource,
    PersistedState, PersistedTable, PersistenceAction, RepositoryError, SyncValidators,
    TaskRepository,
};
use taskboard_domain::reflection::Rating;
use taskboard_domain::reminders::{
    AdhocAlarm, AtInstant, BreakRule, ReminderDelivery, ReminderDismissal, ReminderKind,
    ReminderOffset, ReminderRef, ReminderRule, ReminderSnoozeKey, ReminderState,
};
use taskboard_domain::state::{SyncPhase, SyncStates, derive_sync_states};
use taskboard_domain::tracking::TimeEntry;
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
    /// The degraded-load diagnostics channel (plan D-B2-6, #1137): the
    /// last successful load's journal-quarantine report, taken once by
    /// the engine's boot hydration call site over the
    /// [`LoadDiagnosticsSource`] hop. `None` after a healthy load.
    ///
    /// AGENTS §3 reserves Mutex-free primitives for cross-actor hops:
    /// the signal travels on a `tokio::sync::watch` channel (the
    /// sender side, shared by the `Clone`d repository), not in a
    /// `Mutex<Option<T>>` cell. The `Mutex` around the internal
    /// receiver below guards *receiver state only* (the version mark
    /// read by `borrow_and_update` needs `&mut`), never the payload —
    /// the lock is taken and released synchronously inside
    /// `take_load_diagnostics`, never across an `.await`.
    ///
    /// Take-once leniency documented at the type (A1): the contract is
    /// exactly one consumption per load because there is exactly one
    /// consumer. If a second receiver/consumer ever appears, the
    /// take-once contract must be re-stated on the
    /// [`LoadDiagnosticsSource`] trait.
    pub(crate) diagnostics: std::sync::Arc<tokio::sync::watch::Sender<Option<LoadDiagnostics>>>,
    /// The consumption-side receiver, `Arc`-shared so every `Clone` of
    /// the repository consumes from the ONE receiver state (a per-clone
    /// receiver would fork the seen-version mark and replay signals);
    /// see [`Self::diagnostics`].
    pub(crate) diagnostics_rx:
        std::sync::Arc<std::sync::Mutex<tokio::sync::watch::Receiver<Option<LoadDiagnostics>>>>,
}

/// The typed `Corrupted` construction helper for the decode paths: the
/// helper knows only the stage and reason; table + row key are attached
/// by the row-decode caller via [`RepositoryError::attached`].
fn corrupt(stage: DecodeStage, reason: DecodeReason) -> RepositoryError {
    RepositoryError::corrupted(CorruptionDetail::staged(stage, reason))
}

/// Raw `boards` row (`query_as!` target; domain structs with nested clocks
/// are not constructible by the macro).
struct BoardRow {
    id: String,
    remote_board_id: Option<i64>,
    remote_files_id: Option<String>,
    connector: Option<String>,
    title: String,
    color: String,
    archived: bool,
    deleted: bool,
    ck_title: DateTime<Utc>,
    ck_color: DateTime<Utc>,
    ck_archived: DateTime<Utc>,
    ck_deleted: DateTime<Utc>,
    au_title: Option<String>,
    au_color: Option<String>,
    au_archived: Option<String>,
    au_deleted: Option<String>,
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
    au_title: Option<String>,
    au_sort_order: Option<String>,
    au_deleted: Option<String>,
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
    kind: String,
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
    ck_kind: String,
    ck_archived: String,
    ck_deleted: String,
    au_title: Option<String>,
    au_description: Option<String>,
    au_duedate: Option<String>,
    au_scheduled: Option<String>,
    au_done: Option<String>,
    au_position: Option<String>,
    au_effort: Option<String>,
    au_importance: Option<String>,
    au_urgency: Option<String>,
    au_recur: Option<String>,
    au_kind: Option<String>,
    au_archived: Option<String>,
    au_deleted: Option<String>,
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
    au_title: Option<String>,
    au_color: Option<String>,
    au_deleted: Option<String>,
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
    payload: Option<String>,
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

/// Raw `reminder_rules` row (0020: `kind` + `at_epoch_seconds`).
struct ReminderRuleRow {
    rule_id: String,
    task_id: String,
    offset_seconds: i64,
    kind: String,
    at_epoch_seconds: Option<i64>,
    insistence: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    au_by: Option<String>,
}

/// Raw `adhoc_alarms` row.
struct AdhocAlarmRow {
    alarm_id: String,
    label: String,
    fire_at: DateTime<Utc>,
    insistence: String,
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

/// Raw `time_entries` row.
struct TimeEntryRow {
    entry_id: String,
    task_id: String,
    task_title: String,
    start: DateTime<Utc>,
    end: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    kind: String,
    au_by: Option<String>,
}

/// Raw `break_rules` row (0018/0021; board-scoped roaming reminder
/// rules).
struct BreakRuleRow {
    rule_id: String,
    board_id: String,
    label: String,
    threshold_seconds: i64,
    insistence: String,
    created_at: DateTime<Utc>,
    au_by: Option<String>,
}

/// Raw `reminder_dismissals` row.
struct ReminderDismissalRow {
    reminder: String,
    occurrence: DateTime<Utc>,
    dismissed_at: DateTime<Utc>,
}

/// Raw `reflection_entries` row (0022; the 0008 conventions —
/// hyphenated-UUID TEXT id, RFC 3339 TEXT stamps, JSON-array flag
/// columns, `deleted` as the in-place tombstone).
struct ReflectionEntryRow {
    entry_id: String,
    day: String,
    cadence: String,
    window_start: String,
    window_end: String,
    text: String,
    mood: Option<i64>,
    energy: Option<i64>,
    flags: String,
    extra_flags: String,
    created_at: String,
    updated_at: String,
    deleted: bool,
}

fn decode_adhoc_alarm(row: &AdhocAlarmRow) -> Result<AdhocAlarm, RepositoryError> {
    Ok(AdhocAlarm {
        id: AdhocAlarmId::from(decode::uuid(&row.alarm_id)?),
        label: row.label.clone(),
        fire_at: row.fire_at,
        insistence: decode::insistence(&row.insistence)?,
        created_at: row.created_at,
    })
}

fn decode_break_rule(row: &BreakRuleRow, board: Uuid) -> Result<BreakRule, RepositoryError> {
    let threshold = chrono::TimeDelta::try_seconds(row.threshold_seconds)
        .and_then(|dur| taskboard_domain::tracking::WorkThreshold::try_new(dur).ok())
        .ok_or(corrupt(DecodeStage::RowBuild, DecodeReason::OutOfRange))?;
    Ok(BreakRule {
        id: BreakRuleId::from(decode::uuid(&row.rule_id)?),
        board: BoardId::from(board),
        label: row.label.clone(),
        threshold,
        insistence: decode::insistence(&row.insistence)?,
        created_at: row.created_at,
        by: decode::author(row.au_by.as_deref())?,
    })
}

/// Decode helpers shared by the row mappers; every failure becomes
/// [`RepositoryError::Corrupted`] (plan decision 10/12).
mod decode {
    use super::{
        ConnectorId, DateTime, DecodeReason, DecodeStage, RemoteBoardBinding, RemoteBoardId,
        RemoteCardId, RemoteCardRef, RemoteLabelId, RemoteLabelRef, RemoteStackId, RemoteStackRef,
        RepositoryError, Utc, Uuid, codec, corrupt,
    };

    pub(crate) fn uuid(raw: &str) -> Result<Uuid, RepositoryError> {
        codec::uuid_from_text(raw)
            .map_err(|_| corrupt(DecodeStage::Column, DecodeReason::MalformedUuid))
    }

    /// An RFC 3339 clock column (`ck_*`, journal stamps, habit-log
    /// clocks). `chrono::DateTime<Utc>::from_str` semantics, unchanged
    /// at every folded site. This decoder *refuses* the `#`-suffix
    /// forms: the non-family columns never carry an author, and the
    /// failure is the cheap tripwire that keeps the two vocabularies
    /// from blurring (T-STORE-4).
    pub(crate) fn clock(text: &str) -> Result<DateTime<Utc>, RepositoryError> {
        if text.contains('#') {
            return Err(corrupt(DecodeStage::Column, DecodeReason::MalformedClock));
        }
        text.parse()
            .map_err(|_| corrupt(DecodeStage::Column, DecodeReason::MalformedClock))
    }

    /// The authored clock's grammar (`rfc3339 [# author]`, D-BF2): the
    /// single parse point for every *text-form* serialized clock+author
    /// pair. Bare RFC 3339 → `(clock, None)` — old rows decode
    /// authorless, the tolerant path. Exactly one `#` → the left half
    /// via [`clock`], the right half strict. Any other shape (two `#`,
    /// empty suffix, non-UUID suffix) → `Corrupted` — fail-closed,
    /// because silently de-authoring would change fold rank.
    ///
    /// The storage realization is columnar (`au_*` columns composed as
    /// `[clock]` + `[author]`), so this grammar's first production
    /// consumers are the files-lane wire codecs (chunk 5); the unit
    /// tests carry it until then.
    #[cfg(test)]
    pub(crate) fn clock_with_author(
        text: &str,
    ) -> Result<(DateTime<Utc>, Option<taskboard_domain::ids::AuthorId>), RepositoryError> {
        let (clock_text, author_text) = match text.split_once('#') {
            None => (text, None),
            Some((left, right)) => {
                if right.is_empty() || right.contains('#') {
                    return Err(corrupt(DecodeStage::Column, DecodeReason::MalformedClock));
                }
                (left, Some(right))
            }
        };
        let clock = clock(clock_text)?;
        let author = author(author_text)?;
        Ok((clock, author))
    }

    /// The nullable `au_*` column: NULL → `None` (the backfill), a
    /// parseable UUID → `Some`, anything else → `Corrupted`.
    pub(crate) fn author(
        text: Option<&str>,
    ) -> Result<Option<taskboard_domain::ids::AuthorId>, RepositoryError> {
        match text {
            None => Ok(None),
            Some(raw) => taskboard_domain::ids::AuthorId::parse(raw)
                .map(Some)
                .ok_or(corrupt(
                    DecodeStage::Column,
                    DecodeReason::UnknownEnumVariant,
                )),
        }
    }

    pub(crate) fn remote_opt(cols: &[Option<i64>]) -> Result<Option<Vec<u64>>, RepositoryError> {
        codec::remote_columns_present_or_none(cols)
            .map_err(|_| corrupt(DecodeStage::RowBuild, DecodeReason::UnknownEnumVariant))
    }

    /// The optional remote binding from the board's id columns
    /// (D-BF4): both NULL → unbound; exactly one set → that tag's
    /// payload; both set (or a negative id, or an unparsable UUID) is
    /// corruption.
    pub(crate) fn remote_board(
        deck_col: Option<i64>,
        files_col: Option<&str>,
        connector: Option<String>,
        default_connector: &ConnectorId,
    ) -> Result<Option<RemoteBoardBinding>, RepositoryError> {
        let board = match (deck_col, files_col) {
            (None, None) => return Ok(None),
            (Some(num), None) => RemoteBoardId::Deck(
                u64::try_from(num)
                    .map_err(|_| corrupt(DecodeStage::RowBuild, DecodeReason::OutOfRange))?,
            ),
            (None, Some(raw)) => RemoteBoardId::Files(
                uuid::Uuid::parse_str(raw)
                    .map_err(|_| corrupt(DecodeStage::RowBuild, DecodeReason::MalformedUuid))?,
            ),
            (Some(_), Some(_)) => {
                return Err(corrupt(
                    DecodeStage::RowBuild,
                    DecodeReason::UnknownEnumVariant,
                ));
            }
        };
        Ok(Some(RemoteBoardBinding {
            // NULL tag ⇒ the spawn-time default (DF-4's resolution
            // home); a non-NULL tag naming an unconfigured connector
            // decodes as-is (D-3) — the owning actor simply never
            // finds the board.
            connector: connector.map_or_else(|| default_connector.clone(), ConnectorId::from),
            board,
        }))
    }

    /// See [`remote_board`].
    pub(crate) fn remote_stack(
        cols: &[Option<i64>],
    ) -> Result<Option<RemoteStackRef>, RepositoryError> {
        match remote_opt(cols)?.as_deref() {
            None => Ok(None),
            Some([b, s]) => Ok(Some(RemoteStackRef {
                board: RemoteBoardId::deck(*b),
                stack: RemoteStackId::deck(*s),
            })),
            Some(_) => Err(corrupt(
                DecodeStage::RowBuild,
                DecodeReason::UnknownEnumVariant,
            )),
        }
    }

    /// See [`remote_board`].
    pub(crate) fn remote_card(
        cols: &[Option<i64>],
    ) -> Result<Option<RemoteCardRef>, RepositoryError> {
        match remote_opt(cols)?.as_deref() {
            None => Ok(None),
            Some([b, s, c]) => Ok(Some(RemoteCardRef {
                board: RemoteBoardId::deck(*b),
                stack: RemoteStackId::deck(*s),
                card: RemoteCardId::deck(*c),
            })),
            Some(_) => Err(corrupt(
                DecodeStage::RowBuild,
                DecodeReason::UnknownEnumVariant,
            )),
        }
    }

    /// See [`remote_board`].
    pub(crate) fn remote_label(
        cols: &[Option<i64>],
    ) -> Result<Option<RemoteLabelRef>, RepositoryError> {
        match remote_opt(cols)?.as_deref() {
            None => Ok(None),
            Some([b, l]) => Ok(Some(RemoteLabelRef {
                board: RemoteBoardId::deck(*b),
                label: RemoteLabelId::deck(*l),
            })),
            Some(_) => Err(corrupt(
                DecodeStage::RowBuild,
                DecodeReason::UnknownEnumVariant,
            )),
        }
    }

    pub(crate) fn importance(
        raw: &str,
    ) -> Result<taskboard_domain::entities::Importance, RepositoryError> {
        codec::importance_from_text(raw).ok_or(corrupt(
            DecodeStage::RowBuild,
            DecodeReason::UnknownEnumVariant,
        ))
    }

    pub(crate) fn urgency(
        raw: &str,
    ) -> Result<taskboard_domain::entities::Urgency, RepositoryError> {
        codec::urgency_from_text(raw).ok_or(corrupt(
            DecodeStage::RowBuild,
            DecodeReason::UnknownEnumVariant,
        ))
    }

    pub(crate) fn insistence(
        raw: &str,
    ) -> Result<taskboard_domain::reminders::Insistence, RepositoryError> {
        codec::insistence_from_text(raw).ok_or(corrupt(
            DecodeStage::RowBuild,
            DecodeReason::UnknownEnumVariant,
        ))
    }

    pub(crate) fn time_entry_kind(
        raw: &str,
    ) -> Result<taskboard_domain::tracking::TimeEntryKind, RepositoryError> {
        codec::time_entry_kind_from_text(raw).ok_or(corrupt(
            DecodeStage::RowBuild,
            DecodeReason::UnknownEnumVariant,
        ))
    }

    pub(crate) fn recur(raw: &str) -> Result<taskboard_domain::recur::RecurRule, RepositoryError> {
        codec::recur_from_text(raw).ok_or(corrupt(
            DecodeStage::RowBuild,
            DecodeReason::UnknownEnumVariant,
        ))
    }

    /// Strict decode of the task-kind column (trusted-written rows: an
    /// unknown text is `Corrupted`, never a clamped value).
    pub(crate) fn kind(raw: &str) -> Result<taskboard_domain::entities::TaskKind, RepositoryError> {
        codec::task_kind_from_text(raw).ok_or(corrupt(
            DecodeStage::RowBuild,
            DecodeReason::UnknownEnumVariant,
        ))
    }

    pub(crate) fn remote_baseline(
        raw: &str,
    ) -> Result<taskboard_domain::entities::RemoteTaskBaseline, RepositoryError> {
        // A baseline we cannot read must not silently downgrade to the
        // fallback: the next merge would blanket-clobber — the exact bug
        // the record exists to close (ADR 0004's D-7).
        taskboard_domain::entities::RemoteTaskBaseline::parse_text_form(raw).ok_or(corrupt(
            DecodeStage::RowBuild,
            DecodeReason::UnknownEnumVariant,
        ))
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
    let remote = decode::remote_board(
        row.remote_board_id,
        row.remote_files_id.as_deref(),
        row.connector,
        default_connector,
    )?;
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
        authors: taskboard_domain::entities::BoardAuthors {
            title: decode::author(row.au_title.as_deref())?,
            color: decode::author(row.au_color.as_deref())?,
            archived: decode::author(row.au_archived.as_deref())?,
            deleted: decode::author(row.au_deleted.as_deref())?,
        },
        remote_seen: row.remote_seen,
    })
}

fn decode_stack(row: StackRow) -> Result<Stack, RepositoryError> {
    let id = decode::uuid(&row.id)?;
    let board = decode::uuid(&row.board)?;
    let remote = decode::remote_stack(&[row.remote_board_id, row.remote_stack_id])?;
    Ok(Stack {
        id: StackId::from(id),
        remote,
        board: BoardId::from(board),
        title: row.title,
        order: row.sort_order,
        archived: row.archived,
        deleted: row.deleted,
        clocks: StackClocks {
            title: decode::clock(&row.ck_title)?,
            order: decode::clock(&row.ck_sort_order)?,
            deleted: decode::clock(&row.ck_deleted)?,
        },
        authors: taskboard_domain::entities::StackAuthors {
            title: decode::author(row.au_title.as_deref())?,
            order: decode::author(row.au_sort_order.as_deref())?,
            deleted: decode::author(row.au_deleted.as_deref())?,
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
        decode::remote_card(&[row.remote_board_id, row.remote_stack_id, row.remote_card_id])?;
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
            .map_err(|_| corrupt(DecodeStage::RowBuild, DecodeReason::OutOfRange))?
            .map(taskboard_domain::entities::Effort::new),
        importance: row
            .importance
            .as_deref()
            .map(decode::importance)
            .transpose()?,
        urgency: row.urgency.as_deref().map(decode::urgency).transpose()?,
        recur: row.recur.as_deref().map(decode::recur).transpose()?,
        kind: decode::kind(&row.kind)?,

        archived: row.archived,
        deleted: row.deleted,
        clocks: TaskClocks {
            title: decode::clock(&row.ck_title)?,
            description: decode::clock(&row.ck_description)?,
            duedate: decode::clock(&row.ck_duedate)?,
            scheduled: decode::clock(&row.ck_scheduled)?,
            done: decode::clock(&row.ck_done)?,
            position: decode::clock(&row.ck_position)?,
            effort: decode::clock(&row.ck_effort)?,
            importance: decode::clock(&row.ck_importance)?,
            urgency: decode::clock(&row.ck_urgency)?,
            recur: decode::clock(&row.ck_recur)?,
            kind: decode::clock(&row.ck_kind)?,
            archived: decode::clock(&row.ck_archived)?,
            deleted: decode::clock(&row.ck_deleted)?,
        },
        authors: taskboard_domain::entities::TaskAuthors {
            title: decode::author(row.au_title.as_deref())?,
            description: decode::author(row.au_description.as_deref())?,
            duedate: decode::author(row.au_duedate.as_deref())?,
            scheduled: decode::author(row.au_scheduled.as_deref())?,
            done: decode::author(row.au_done.as_deref())?,
            position: decode::author(row.au_position.as_deref())?,
            effort: decode::author(row.au_effort.as_deref())?,
            importance: decode::author(row.au_importance.as_deref())?,
            urgency: decode::author(row.au_urgency.as_deref())?,
            recur: decode::author(row.au_recur.as_deref())?,
            kind: decode::author(row.au_kind.as_deref())?,
            archived: decode::author(row.au_archived.as_deref())?,
            deleted: decode::author(row.au_deleted.as_deref())?,
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
    let remote = decode::remote_label(&[row.remote_board_id, row.remote_label_id])?;
    Ok(Label {
        id: LabelId::from(id),
        remote,
        board: BoardId::from(board),
        title: row.title,
        color: Color::new(row.color),
        deleted: row.deleted,
        clocks: LabelClocks {
            title: decode::clock(&row.ck_title)?,
            color: decode::clock(&row.ck_color)?,
            deleted: decode::clock(&row.ck_deleted)?,
        },
        authors: taskboard_domain::entities::LabelAuthors {
            title: decode::author(row.au_title.as_deref())?,
            color: decode::author(row.au_color.as_deref())?,
            deleted: decode::author(row.au_deleted.as_deref())?,
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
        row.payload.as_deref(),
    )
    .map_err(|_| corrupt(DecodeStage::RowBuild, DecodeReason::UnknownEnumVariant))?;
    Ok(PendingOp {
        op_id,
        op,
        queued_at: row.queued_at,
    })
}

/// Decodes a stored reminder reference (`"rule:<uuid>"`/`"default:<uuid>"`).
fn decode_reminder_ref(raw: &str) -> Result<ReminderRef, RepositoryError> {
    codec::reminder_ref_from_text(raw)
        .map_err(|_| corrupt(DecodeStage::RowBuild, DecodeReason::UnknownEnumVariant))
}

fn decode_reminder_rule(
    row: &ReminderRuleRow,
) -> Result<(ReminderRuleId, ReminderRule), RepositoryError> {
    // The kind columns decide the arm, each with its integrity check
    // (trusted-write discipline: a self-contradictory row is corruption,
    // not a clamped value):
    // - `due_offset`: the offset column decodes through the domain
    //   mint (an unrepresentable or positive stored offset is
    //   corruption); a stray non-NULL epoch is a self-contradictory row.
    // - `at`: the epoch column is required and must mint a whole-second
    //   `AtInstant`; NULL is corruption.
    let kind = match codec::reminder_kind_is_at(row.kind.as_str()) {
        Some(false) => {
            if row.at_epoch_seconds.is_some() {
                return Err(corrupt(
                    DecodeStage::RowBuild,
                    DecodeReason::ConstraintViolation,
                ));
            }
            let duration = chrono::TimeDelta::try_seconds(row.offset_seconds)
                .ok_or(corrupt(DecodeStage::RowBuild, DecodeReason::OutOfRange))?;
            ReminderKind::DueOffset(
                ReminderOffset::before(duration)
                    .map_err(|_| corrupt(DecodeStage::RowBuild, DecodeReason::OutOfRange))?,
            )
        }
        Some(true) => {
            let secs = row
                .at_epoch_seconds
                .ok_or(corrupt(DecodeStage::RowBuild, DecodeReason::MissingColumn))?;
            let dt = chrono::DateTime::from_timestamp(secs, 0)
                .ok_or(corrupt(DecodeStage::RowBuild, DecodeReason::OutOfRange))?;
            ReminderKind::At(
                AtInstant::whole_second(dt)
                    .map_err(|_| corrupt(DecodeStage::RowBuild, DecodeReason::OutOfRange))?,
            )
        }
        None => {
            return Err(corrupt(
                DecodeStage::RowBuild,
                DecodeReason::UnknownEnumVariant,
            ));
        }
    };
    let id = ReminderRuleId::from(decode::uuid(&row.rule_id)?);
    Ok((
        id,
        ReminderRule {
            id,
            task: TaskId::from(decode::uuid(&row.task_id)?),
            kind,
            insistence: decode::insistence(&row.insistence)?,
            created_at: row.created_at,
            // 0007's NOT NULL DEFAULT epoch column: pre-chunk-4 rows
            // decode with the epoch, which loses every clock race
            // exactly like `BoardClocks::epoch`'s precedent.
            updated_at: row.updated_at,
            by: decode::author(row.au_by.as_deref())?,
        },
    ))
}

/// Raw `habits` row (`query_as!` target; the runtime-checked tuple
/// precedent hit sqlx's 16-element `FromRow` limit once the 0027 field
/// family grew the row past it). `target` decodes later through the
/// mint's strict serde; `anchor_day` is the `AnchorDay` serde JSON
/// form, `NULL` = unset; every clock is RFC 3339 TEXT.
struct HabitRow {
    habit_id: String,
    board_id: String,
    title: String,
    description: String,
    target: String,
    sort_order: i64,
    anchor_time: Option<String>,
    anchor_day: Option<String>,
    paused_until: Option<String>,

    scheduled_days: Option<String>,
    reminder: Option<String>,
    ck_title: String,
    ck_target: String,
    ck_order: String,
    ck_anchor_time: String,
    ck_description: String,
    ck_anchor_day: String,
    ck_paused_until: String,

    ck_scheduled_days: Option<String>,
    ck_reminder: Option<String>,
    ck_deleted: String,
    deleted: bool,
    continued_from: Option<String>,
    au_by: Option<String>,
}

/// The reminder payload's strict serde decode with the #1271
/// normalize-on-decode law applied: a zero-duration snooze is degenerate
/// (the snooze chain walk moves the occurrence by zero) and unmintable
/// today, but rows minted before the #1211 law may still carry one.
/// Rewriting here — post-serde, pre-use, the single sqlite decode choke
/// point — converges the stored grammar lazily to the mint default
/// without a schema migration or a second codec.
fn decode_reminder(
    row: Option<&str>,
) -> Result<Option<taskboard_domain::habit::HabitReminderOverride>, RepositoryError> {
    let Some(text) = row else {
        return Ok(None);
    };
    let mut ovr: taskboard_domain::habit::HabitReminderOverride = serde_json::from_str(text)
        .map_err(|_| corrupt(DecodeStage::Payload, DecodeReason::SerdePayload))?;
    ovr.snooze = match ovr.snooze {
        taskboard_domain::habit::HabitSnooze::Hours(0)
        | taskboard_domain::habit::HabitSnooze::Days(0) => {
            taskboard_domain::habit::HabitSnooze::Hours(1)
        }
        other => other,
    };
    Ok(Some(ovr))
}

fn decode_habit(
    row: &HabitRow,
) -> Result<(taskboard_domain::ids::HabitId, taskboard_domain::Habit), RepositoryError> {
    let id = taskboard_domain::ids::HabitId::from(
        codec::uuid_from_text(&row.habit_id)
            .map_err(|_| corrupt(DecodeStage::RowBuild, DecodeReason::MalformedUuid))?,
    );
    let board = taskboard_domain::BoardId::from(
        codec::uuid_from_text(&row.board_id)
            .map_err(|_| corrupt(DecodeStage::RowBuild, DecodeReason::MalformedUuid))?,
    );
    let target: taskboard_domain::HabitTarget = serde_json::from_str(&row.target)
        .map_err(|_| corrupt(DecodeStage::Payload, DecodeReason::SerdePayload))?;
    let order = row.sort_order;
    let anchor_time = match &row.anchor_time {
        None => None,
        Some(text) => Some(
            chrono::NaiveTime::parse_from_str(text, "%H:%M:%S")
                .map_err(|_| corrupt(DecodeStage::RowBuild, DecodeReason::MalformedClock))?,
        ),
    };
    let anchor_day = match &row.anchor_day {
        None => None,
        Some(text) => {
            let day: taskboard_domain::habit::AnchorDay = serde_json::from_str(text)
                .map_err(|_| corrupt(DecodeStage::Payload, DecodeReason::SerdePayload))?;
            Some(day)
        }
    };
    let paused_until = match &row.paused_until {
        None => None,
        Some(text) => Some(
            chrono::NaiveDate::parse_from_str(text, "%Y-%m-%d")
                .map_err(|_| corrupt(DecodeStage::RowBuild, DecodeReason::MalformedDate))?,
        ),
    };
    // The 0028/0029 payload columns decode strictly through their serde
    // mints (the `anchor_day` posture): NULL = unset, a malformed
    // payload is corruption, never a defaulted value. The clock slots
    // ride the 0027 COALESCE (pre-HF4 rows decode to the epoch).
    let scheduled_days = match &row.scheduled_days {
        None => None,
        Some(text) => {
            let set: taskboard_domain::habit::WeekdaySet = serde_json::from_str(text)
                .map_err(|_| corrupt(DecodeStage::Payload, DecodeReason::SerdePayload))?;
            Some(set)
        }
    };
    let reminder = decode_reminder(row.reminder.as_deref())?;
    let clocks = taskboard_domain::HabitClocks {
        title: decode::clock(&row.ck_title)?,
        description: decode::clock(&row.ck_description)?,
        target: decode::clock(&row.ck_target)?,
        order: decode::clock(&row.ck_order)?,
        anchor_time: decode::clock(&row.ck_anchor_time)?,
        anchor_day: decode::clock(&row.ck_anchor_day)?,
        paused_until: decode::clock(&row.ck_paused_until)?,

        // The COALESCE'd column: absent = the epoch (never written).
        scheduled_days: decode::clock(
            row.ck_scheduled_days
                .as_deref()
                .unwrap_or("1970-01-01T00:00:00+00:00"),
        )?,
        reminder: decode::clock(
            row.ck_reminder
                .as_deref()
                .unwrap_or("1970-01-01T00:00:00+00:00"),
        )?,
        deleted: decode::clock(&row.ck_deleted)?,
    };
    let continued_from = row
        .continued_from
        .as_deref()
        .map(codec::uuid_from_text)
        .transpose()
        .map_err(|_| corrupt(DecodeStage::RowBuild, DecodeReason::MalformedUuid))?
        .map(taskboard_domain::ids::SeriesId::from);
    let by = decode::author(row.au_by.as_deref())?;
    Ok((
        id,
        taskboard_domain::Habit {
            id,
            board,
            title: row.title.clone(),
            description: row.description.clone(),
            target,
            order,
            anchor_time,
            anchor_day,
            paused_until,

            scheduled_days,
            reminder,
            clocks,
            continued_from,
            deleted: row.deleted,
            by,
        },
    ))
}

/// Decodes one `reflection_entries` row (0022, the `decode_habit`
/// shape). Strict on load, tolerant on the flag *names* only
/// (D-J13/D-H12): a curated name mints into `flags`, an unknown name
/// parks verbatim into `extra_flags` — the decode seam is where the
/// tolerance lives. Everything else is trusted-written: a corrupt id,
/// date, stamp, JSON array, or out-of-range rating is a corrupted
/// database, never a clamped or defaulted value (P-3's strict-decode
/// posture).
fn decode_reflection_entry(
    row: &ReflectionEntryRow,
) -> Result<
    (
        taskboard_domain::ids::ReflectionId,
        taskboard_domain::ReflectionEntry,
    ),
    RepositoryError,
> {
    let id = taskboard_domain::ids::ReflectionId::from(decode::uuid(&row.entry_id)?);
    let cadence =
        taskboard_domain::reflection::ReviewCadence::parse_text_form(&row.cadence).ok_or(
            corrupt(DecodeStage::RowBuild, DecodeReason::UnknownEnumVariant),
        )?;
    let day = chrono::NaiveDate::parse_from_str(&row.day, "%Y-%m-%d")
        .map_err(|_| corrupt(DecodeStage::RowBuild, DecodeReason::MalformedDate))?;
    let window_start: chrono::DateTime<chrono::Utc> = decode::clock(&row.window_start)?;
    let window_end: chrono::DateTime<chrono::Utc> = decode::clock(&row.window_end)?;
    let created_at: chrono::DateTime<chrono::Utc> = decode::clock(&row.created_at)?;
    let updated_at: chrono::DateTime<chrono::Utc> = decode::clock(&row.updated_at)?;
    // The strict rating decode: a stored 0/6+ is a data error, never a
    // clamp (the `Rating` mint's documented posture).
    let rating = |raw: Option<i64>| -> Result<Option<Rating>, RepositoryError> {
        raw.map(u8::try_from)
            .transpose()
            .map_err(|_| corrupt(DecodeStage::RowBuild, DecodeReason::OutOfRange))?
            .map(Rating::try_new)
            .transpose()
            .map_err(|_| corrupt(DecodeStage::RowBuild, DecodeReason::OutOfRange))
    };
    let names = |raw: &str| -> Result<Vec<String>, RepositoryError> {
        serde_json::from_str(raw)
            .map_err(|_| corrupt(DecodeStage::Payload, DecodeReason::SerdePayload))
    };
    let mut flags = std::collections::BTreeSet::new();
    let mut extra_flags = std::collections::BTreeSet::new();
    for name in names(&row.flags)? {
        match taskboard_domain::reflection::HealthFlag::parse_text_form(&name) {
            Some(flag) => {
                flags.insert(flag);
            }
            None => {
                extra_flags.insert(name);
            }
        }
    }
    extra_flags.extend(names(&row.extra_flags)?);
    Ok((
        id,
        taskboard_domain::ReflectionEntry {
            id,
            cadence,
            day,
            window_start,
            window_end,
            text: row.text.clone(),
            attributes: taskboard_domain::ReflectionAttributes {
                mood: rating(row.mood)?,
                energy: rating(row.energy)?,
                flags,
                extra_flags,
            },
            created_at,
            updated_at,
            deleted: row.deleted,
        },
    ))
}

fn decode_time_entry(row: &TimeEntryRow) -> Result<(TimeEntryId, TimeEntry), RepositoryError> {
    // Strict on load, tolerant on the wire: SQLite rows are
    // trusted-written (validated at their originating planner, ADR
    // 0017's leniency boundary); a corrupt row is a corrupted database.
    // Span validity is not re-checked here — the type documents decoded
    // rows as trusted-written; read-side projections clamp.
    let id = TimeEntryId::from(decode::uuid(&row.entry_id)?);
    Ok((
        id,
        TimeEntry {
            id,
            task: TaskId::from(decode::uuid(&row.task_id)?),
            task_title: row.task_title.clone(),
            start: row.start,
            end: row.end,
            created_at: row.created_at,
            updated_at: row.updated_at,
            // Strict decode (the 0013 rule): a corrupt enum is a
            // corrupted database, never a defaulted value. Migration
            // 0018's `DEFAULT 'work'` backfill is what makes every
            // pre-0018 row read `Work` (the local default-off decode).
            kind: decode::time_entry_kind(&row.kind)?,
            by: decode::author(row.au_by.as_deref())?,
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

    fn load_diagnostics(&self) -> Option<&dyn LoadDiagnosticsSource> {
        Some(self)
    }
}

impl LoadDiagnosticsSource for SqliteTaskRepository {
    fn take_load_diagnostics(&self) -> Option<LoadDiagnostics> {
        // (#1137) the domain trait's take-once contract (persistence.rs:
        // "one consumption per load, never a replayed signal") maps onto
        // the watch channel as `has_changed()` gate + `borrow_and_update()`:
        // consuming marks the version read, so the immediately repeated
        // call observes no change and returns `None`.
        let mut rx = self
            .diagnostics_rx
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !rx.has_changed().unwrap_or(false) {
            return None;
        }
        // `borrow_and_update()`'s `Ref` derefs to the channel value, so
        // the `.clone()` here clones the inner `Option<LoadDiagnostics>`
        // by value (the `Clone` bound plan D-1137-a relies on).
        rx.borrow_and_update().clone()
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
    #[allow(clippy::too_many_lines)] // one SELECT per table; the length is the schema's
    async fn load_inner(&self) -> Result<PersistedState, RepositoryError> {
        // Clear any stale diagnostics up front: only THIS load's
        // quarantine pass may publish a signal (one degraded event per
        // degraded load, never a replay of an older load's report).
        // (#1137) on the watch channel this is a `None` send — clearing
        // a stale pending value one step earlier than the Mutex cell's
        // take-once did, and marking the receiver changed so a take
        // between the two sends reads `None` (not the stale `Some`).
        let _ = self.diagnostics.send(None);
        // One read transaction around all SELECTs: the single pooled
        // connection would otherwise be released between queries, and a
        // concurrent `apply` could interleave — yielding a state mixing
        // pre- and post-batch rows, with `pending_ops` disagreeing with
        // the hydrated outbox.
        let mut tx = self.pool.begin().await.map_err(|e| stmt_error(&e))?;

        let board_rows = sqlx::query_as!(
            BoardRow,
            r#"SELECT id, remote_board_id, remote_files_id, connector, title, color,
                      archived as "archived: bool", deleted as "deleted: bool",
                      ck_title as "ck_title: DateTime<Utc>",
                      ck_color as "ck_color: DateTime<Utc>",
                      ck_archived as "ck_archived: DateTime<Utc>",
                      ck_deleted as "ck_deleted: DateTime<Utc>",
                      au_title, au_color, au_archived, au_deleted,
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
                      au_title, au_sort_order, au_deleted,
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
                      au_title, au_color, au_deleted,
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
                      importance, urgency, recur, kind,
                      archived as "archived: bool", deleted as "deleted: bool",
                      ck_title, ck_description, ck_duedate, ck_scheduled, ck_done,
                      ck_position, ck_effort, ck_importance, ck_urgency,
                      ck_recur, ck_kind, ck_archived, ck_deleted,
                      au_title, au_description, au_duedate, au_scheduled,
                      au_done, au_position, au_effort, au_importance,
                      au_urgency, au_recur, au_kind, au_archived, au_deleted,
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
            r#"SELECT op_id, op_kind, task_id, stack_id, label_id, board_id, payload,
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
            r#"SELECT rule_id, task_id, offset_seconds, kind, at_epoch_seconds, insistence,
                      created_at as "created_at: DateTime<Utc>",
                      updated_at as "updated_at: DateTime<Utc>",
                      au_by
               FROM reminder_rules ORDER BY rule_id"#
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| stmt_error(&e))?;

        let break_rule_rows = sqlx::query_as!(
            BreakRuleRow,
            r#"SELECT rule_id, board_id, label, threshold_seconds, insistence,
                      created_at as "created_at: DateTime<Utc>",
                      au_by
               FROM break_rules ORDER BY rule_id"#
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
                      insistence,
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

        // Runtime-checked queries, the habit-tables precedent: the
        // prepared-cache macros cannot see a migration this chunk itself
        // ships until `cargo sqlx prepare` runs against it.
        // (id, parent, created_at, pushed, board, payload) — the lean
        // hydration row; a `query!` macro cannot see this chunk's own
        // migration until `cargo sqlx prepare` runs against it.
        type JournalRow = (String, Option<String>, String, bool, Option<String>, String);
        let journal_rows: Vec<JournalRow> = sqlx::query_as(
            r"SELECT id, parent, created_at, pushed, board, payload
               FROM command_journal ORDER BY rowid",
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| stmt_error(&e))?;

        let journal_head_row: Vec<(Option<String>,)> =
            sqlx::query_as(r"SELECT id FROM command_journal_head")
                .fetch_all(&mut *tx)
                .await
                .map_err(|e| stmt_error(&e))?;
        let journal_head: Option<Option<String>> = journal_head_row.into_iter().next().map(|r| r.0);
        let Some(journal_head) = journal_head else {
            // The head singleton row is missing entirely — the
            // fail-closed invariant (plan matrix case 9); the journal is
            // pointerless and the line is undecidable.
            return Err(RepositoryError::corrupted(CorruptionDetail {
                table: Some(PersistedTable::CommandJournalHead),
                row: None,
                stage: Some(DecodeStage::Invariant),
                reason: Some(DecodeReason::MissingColumn),
            }));
        };

        let time_entry_rows = sqlx::query_as!(
            TimeEntryRow,
            r#"SELECT entry_id, task_id, task_title,
                      start as "start: DateTime<Utc>",
                      end as "end: DateTime<Utc>",
                      created_at as "created_at: DateTime<Utc>",
                      updated_at as "updated_at: DateTime<Utc>",
                      kind,
                      au_by
               FROM time_entries ORDER BY entry_id"#
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| stmt_error(&e))?;

        let habit_rows: Vec<HabitRow> = sqlx::query_as!(
            HabitRow,
            r#"SELECT habit_id, board_id, title, description, target,
                      sort_order, anchor_time, anchor_day,
                      COALESCE(paused_until, NULL) AS paused_until,
                      scheduled_days, reminder,
                      ck_title, ck_target, ck_order, ck_anchor_time,
                      COALESCE(ck_description, '1970-01-01T00:00:00+00:00') AS ck_description,
                      COALESCE(ck_anchor_day, '1970-01-01T00:00:00+00:00') AS ck_anchor_day,
                      COALESCE(ck_paused_until, '1970-01-01T00:00:00+00:00') AS ck_paused_until,

                      COALESCE(ck_scheduled_days, '1970-01-01T00:00:00+00:00') AS ck_scheduled_days,
                      COALESCE(ck_reminder, '1970-01-01T00:00:00+00:00') AS ck_reminder,
                      ck_deleted,
                      deleted as "deleted: bool", continued_from, au_by
               FROM habits ORDER BY habit_id"#
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| stmt_error(&e))?;

        type HabitLogRow = (String, String, i64, String, Option<String>, bool);
        let habit_log_rows: Vec<HabitLogRow> = sqlx::query_as(
            r"SELECT habit_id, window_start, count, clock, au_by, skip FROM habit_log
               ORDER BY habit_id, window_start",
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| stmt_error(&e))?;

        // The per-day overlay (US-8, #1144): unchecked queries — a
        // fresh table need not re-seed the sqlx offline metadata (the
        // `roaming_settings` precedent).
        type HabitDayLogRow = (String, String, i64, String);
        let habit_day_log_rows: Vec<HabitDayLogRow> = sqlx::query_as(
            r"SELECT habit_id, day, count, clock FROM habit_day_log
               ORDER BY habit_id, day",
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| stmt_error(&e))?;

        // The habit time entries (#1305): unchecked queries — the
        // `habit_day_log` precedent (a fresh table need not re-seed the
        // sqlx offline metadata).
        type HabitTimeEntryRow = (
            String,
            String,
            String,
            String,
            Option<String>,
            String,
            String,
            Option<String>,
        );
        let habit_time_entry_rows: Vec<HabitTimeEntryRow> = sqlx::query_as(
            r"SELECT entry_id, habit_id, habit_title, start, end, created_at, updated_at, au_by
               FROM habit_time_entries ORDER BY entry_id",
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| stmt_error(&e))?;

        let reflection_rows = sqlx::query_as!(
            ReflectionEntryRow,
            r#"SELECT entry_id, day, cadence, window_start, window_end, text,
                      mood, energy, flags, extra_flags, created_at, updated_at,
                      deleted as "deleted: bool"
               FROM reflection_entries ORDER BY entry_id"#
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| stmt_error(&e))?;

        // DR-9 D-BF3 (#685): the roaming settings home. The
        // `habit_log` precedent's unchecked queries — a fresh table
        // need not re-seed the sqlx offline metadata.
        let roaming_rows: Vec<(String, String, String)> =
            sqlx::query_as(r"SELECT name, value, clock FROM roaming_settings ORDER BY name")
                .fetch_all(&mut *tx)
                .await
                .map_err(|e| stmt_error(&e))?;

        let mut boards = BTreeMap::new();
        for row in board_rows {
            let row_key = Uuid::parse_str(&row.id).ok();
            let board = decode_board(row, &self.default_connector)
                .map_err(|e| e.attached(PersistedTable::Boards, row_key))?;
            boards.insert(board.id, board);
        }

        let mut stacks = BTreeMap::new();
        for row in stack_rows {
            let row_key = Uuid::parse_str(&row.id).ok();
            let stack =
                decode_stack(row).map_err(|e| e.attached(PersistedTable::Stacks, row_key))?;
            stacks.insert(stack.id, stack);
        }

        let mut labels = BTreeMap::new();
        for row in label_rows {
            let row_key = Uuid::parse_str(&row.id).ok();
            let label =
                decode_label(row).map_err(|e| e.attached(PersistedTable::Labels, row_key))?;
            labels.insert(label.id, label);
        }

        // Group the join table once; tasks look up their membership map.
        let mut label_sets: BTreeMap<
            Uuid,
            BTreeMap<Uuid, taskboard_domain::entities::MembershipState>,
        > = BTreeMap::new();
        for row in task_label_rows {
            let row_key = Uuid::parse_str(&row.task).ok();
            let decoded = (|| {
                Result::<
                        (Uuid, Uuid, taskboard_domain::entities::MembershipState),
                        RepositoryError,
                    >::Ok({
                        let task = decode::uuid(&row.task)?;
                        let label = decode::uuid(&row.label)?;
                        let clock: DateTime<Utc> = decode::clock(&row.ck_membership)?;
                        let state = match row.state.as_str() {
                            "added" => {
                                taskboard_domain::entities::MembershipState::Added { at: clock }
                            }
                            "removed" => {
                                taskboard_domain::entities::MembershipState::Removed { at: clock }
                            }
                            _ => {
                                return Err(corrupt(
                                    DecodeStage::RowBuild,
                                    DecodeReason::UnknownEnumVariant,
                                ));
                            }
                        };
                        (task, label, state)
                    })
            })()
            .map_err(|e| e.attached(PersistedTable::Tasks, row_key))?;
            label_sets
                .entry(decoded.0)
                .or_default()
                .insert(decoded.1, decoded.2);
        }

        let mut tasks = BTreeMap::new();
        for row in task_rows {
            let row_key = Uuid::parse_str(&row.id).ok();
            let task = decode_task(row, &label_sets)
                .map_err(|e| e.attached(PersistedTable::Tasks, row_key))?;
            tasks.insert(task.id, task);
        }

        let mut outbox = Vec::with_capacity(outbox_rows.len());
        for row in outbox_rows {
            let row_key = Uuid::parse_str(&row.op_id).ok();
            let op =
                decode_outbox_row(&row).map_err(|e| e.attached(PersistedTable::Outbox, row_key))?;
            outbox.push(op);
        }

        let mut validators = BTreeMap::new();
        for row in validator_rows {
            let key = codec::validator_key_from_text(&row.key).map_err(|_| {
                corrupt(DecodeStage::RowBuild, DecodeReason::UnknownEnumVariant)
                    .attached(PersistedTable::Validators, None)
            })?;
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
            let row_key = Uuid::parse_str(&status_row.board).ok();
            let phase =
                codec::sync_phase_from_columns(&status_row.phase, status_row.last_error.as_deref())
                    .map_err(|_| {
                        corrupt(DecodeStage::RowBuild, DecodeReason::UnknownEnumVariant)
                            .attached(PersistedTable::SyncStatus, row_key)
                    })?;
            let board = decode::uuid(&status_row.board)?;
            sync.set_stored(BoardId::from(board), phase, status_row.last_success);
        }

        let mut rules = BTreeMap::new();
        for row in reminder_rule_rows {
            let row_key = Uuid::parse_str(&row.rule_id).ok();
            let (id, rule) = decode_reminder_rule(&row)
                .map_err(|e| e.attached(PersistedTable::Reminders, row_key))?;
            rules.insert(id, rule);
        }
        let mut deliveries = BTreeSet::new();
        for row in reminder_delivery_rows {
            let delivery = decode_reminder_delivery(&row)
                .map_err(|e| e.attached(PersistedTable::Reminders, None))?;
            deliveries.insert(delivery);
        }
        let mut dismissals = BTreeSet::new();
        for row in reminder_dismissal_rows {
            let dismissal = decode_reminder_dismissal(&row)
                .map_err(|e| e.attached(PersistedTable::Reminders, None))?;
            dismissals.insert(dismissal);
        }
        let mut snoozes = BTreeMap::new();
        for row in reminder_snooze_rows {
            let (key, rescheduled_to) = decode_reminder_snooze(&row)
                .map_err(|e| e.attached(PersistedTable::Reminders, None))?;
            snoozes.insert(key, rescheduled_to);
        }
        let mut muted = BTreeSet::new();
        for row in reminder_muted_rows {
            let row_key = Uuid::parse_str(&row.task_id).ok();
            let task = decode::uuid(&row.task_id)
                .map_err(|e| e.attached(PersistedTable::Reminders, row_key))?;
            muted.insert(TaskId::from(task));
        }
        let mut alarms = BTreeMap::new();
        for row in adhoc_alarm_rows {
            let row_key = Uuid::parse_str(&row.alarm_id).ok();
            let alarm = decode_adhoc_alarm(&row)
                .map_err(|e| e.attached(PersistedTable::Reminders, row_key))?;
            alarms.insert(alarm.id, alarm);
        }
        let mut tracking = taskboard_domain::TrackingState::default();
        for row in time_entry_rows {
            let row_key = Uuid::parse_str(&row.entry_id).ok();
            let (id, entry) = decode_time_entry(&row)
                .map_err(|e| e.attached(PersistedTable::Tracking, row_key))?;
            tracking.entries.insert(id, entry);
        }
        let mut break_rules = std::collections::BTreeMap::new();
        for row in &break_rule_rows {
            // The 0021 recorded leniency (D-14): a row backfilled with ''
            // (a device that never bound a board — which can never have
            // flushed it) skips at load instead of corrupting the
            // database. Trusted-written rows are strict everywhere else
            // (the 0008 rule).
            let Ok(board) = decode::uuid(&row.board_id) else {
                tracing::warn!(rule = %row.rule_id, "break rule skipped: no backfilled board");
                continue;
            };
            let row_key = Uuid::parse_str(&row.rule_id).ok();
            let rule = decode_break_rule(row, board)
                .map_err(|e| e.attached(PersistedTable::BreakRules, row_key))?;
            break_rules.insert(rule.id, rule);
        }
        let reminders = ReminderState {
            rules,
            deliveries,
            dismissals,
            snoozes,
            break_rules,
            muted,
            alarms,
        };

        // The journal hydrates in insertion order (rowid = chronological;
        // the convention forbids ORDER BY on TEXT stamps) plus the
        // persisted head (R-U10). Empty rows decode to the default —
        // the upgrade-pinned shape.
        //
        // Quarantine-with-signal (plan §4, #1039): this table is undo
        // metadata only (ADR 0019) — the authoritative task state lives
        // in the entity tables, which abort above. An undecodable row is
        // dropped with a typed detail; its parent-chained descendants
        // are dropped as orphaned (the chain invariant forces the
        // minimal sub-forest); a later lineage rooted before the gap
        // still hydrates; the head is re-pinned (D-B2-7); and one typed
        // diagnostics report is published for the engine to broadcast
        // (D-B2-6). A healthy journal behaves exactly as before.
        let mut journal_entries = Vec::with_capacity(journal_rows.len());
        let mut quarantine_details: Vec<CorruptionDetail> = Vec::new();
        let mut dropped: u32 = 0;
        let mut dropped_ids: BTreeSet<Uuid> = BTreeSet::new();
        for (id, parent, created_at, pushed, board, payload) in &journal_rows {
            let row_key = Uuid::parse_str(id).ok();
            let decoded = (|| -> Result<taskboard_domain::JournalEntry, RepositoryError> {
                let id = taskboard_domain::JournalId::from(decode::uuid(id)?);
                let parent = parent
                    .as_deref()
                    .map(decode::uuid)
                    .transpose()?
                    .map(taskboard_domain::JournalId::from);
                let board = board
                    .as_deref()
                    .map(decode::uuid)
                    .transpose()?
                    .map(taskboard_domain::BoardId::from);
                let created_at: DateTime<Utc> = decode::clock(created_at)?;
                // The payload serde arm keeps serde's field-level message
                // out of the typed context (untrusted text) — the warn
                // below carries the typed detail plus serde's field-level
                // triage text (plan §3.2, matrix case 12).
                let payload = codec::journal_payload_from_json(payload).map_err(|serde_err| {
                    tracing::warn!(
                        row = ?row_key,
                        serde_detail = %serde_err,
                        "journal payload serde decode failed"
                    );
                    corrupt(DecodeStage::Payload, DecodeReason::SerdePayload)
                })?;
                Ok(taskboard_domain::JournalEntry {
                    id,
                    parent,
                    command: payload.command,
                    facts: payload.facts,
                    ops: payload.ops,
                    pushed: *pushed,
                    board,
                    created_at,
                })
            })()
            .map_err(|e| e.attached(PersistedTable::CommandJournal, row_key));
            match decoded {
                Ok(entry) => journal_entries.push(entry),
                Err(err) => {
                    // The attached detail (typed facts only); the rowid
                    // sequence is the triage hint.
                    tracing::warn!(
                        row = ?row_key,
                        detail = ?err.corruption_detail(),
                        "command journal row quarantined at boot load"
                    );
                    if let Some(key) = row_key {
                        dropped_ids.insert(key);
                    }
                    dropped += 1;
                    if let Some(detail) = err.corruption_detail() {
                        quarantine_details.push(detail);
                    }
                }
            }
        }
        // The chain invariant forces the descendant drop (plan D-B2-5
        // step 2): a surviving entry whose parent chain crosses the gap
        // is orphaned — dropped with its own typed `Invariant` detail,
        // to a fixpoint (the gap may sit mid-line).
        loop {
            let mut orphaned = 0u32;
            journal_entries.retain(|entry| {
                let orphan = entry
                    .parent
                    .as_ref()
                    .is_some_and(|p| dropped_ids.contains(&p.as_uuid()));
                if orphan {
                    orphaned += 1;
                    dropped_ids.insert(entry.id.as_uuid());
                    tracing::warn!(
                        row = ?entry.id.as_uuid(),
                        "command journal entry orphaned by a quarantined ancestor"
                    );
                    quarantine_details.push(CorruptionDetail {
                        table: Some(PersistedTable::CommandJournal),
                        row: Some(entry.id.as_uuid()),
                        stage: Some(DecodeStage::Invariant),
                        reason: None,
                    });
                }
                !orphan
            });
            if orphaned == 0 {
                break;
            }
            dropped += orphaned;
        }

        let head = journal_head
            .as_deref()
            .map(decode::uuid)
            .transpose()
            .map_err(|e| e.attached(PersistedTable::CommandJournalHead, None))?
            .map(taskboard_domain::JournalId::from);
        // D-B2-7: keep a head that still names a surviving entry (an
        // undo-moved mid-line head is legitimate state); otherwise
        // re-pin to the highest-rowid survivor — or the emptied line's
        // `None`. The persisted pin only moves on a degraded load.
        let journal_quarantined = !quarantine_details.is_empty();
        let head = if journal_quarantined {
            match head {
                Some(h) if journal_entries.iter().any(|e| e.id == h) => Some(h),
                _ => journal_entries.last().map(|e| e.id),
            }
        } else {
            head
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
            tracking,
            habits: {
                let mut habits = taskboard_domain::HabitState::default();
                for row in &habit_rows {
                    let row_key = Uuid::parse_str(&row.habit_id).ok();
                    let (id, habit) = decode_habit(row)
                        .map_err(|e| e.attached(PersistedTable::Habits, row_key))?;
                    habits.habits.insert(id, habit);
                }
                for (habit_id, window_start, count, clock, au_by, skip) in &habit_log_rows {
                    let row_key = Uuid::parse_str(habit_id).ok();
                    let decoded = (|| {
                        Result::<
                            (
                                taskboard_domain::HabitLogKey,
                                taskboard_domain::HabitLogEntry,
                            ),
                            RepositoryError,
                        >::Ok({
                            let habit = taskboard_domain::ids::HabitId::from(
                                codec::uuid_from_text(habit_id).map_err(|_| {
                                    corrupt(DecodeStage::RowBuild, DecodeReason::MalformedUuid)
                                })?,
                            );
                            // D-H12's decode seam: the stored date decodes
                            // as-is, never floored (former-kind synced rows).
                            let date = chrono::NaiveDate::parse_from_str(window_start, "%Y-%m-%d")
                                .map_err(|_| {
                                    corrupt(DecodeStage::RowBuild, DecodeReason::MalformedDate)
                                })?;
                            let key = taskboard_domain::HabitLogKey {
                                habit,
                                window: taskboard_domain::WindowStart::from_stored(date),
                            };
                            let count = u32::try_from(*count).map_err(|_| {
                                corrupt(DecodeStage::RowBuild, DecodeReason::OutOfRange)
                            })?;
                            let clock: DateTime<Utc> = decode::clock(clock)?;
                            let by = decode::author(au_by.as_deref())?;
                            (
                                key,
                                taskboard_domain::HabitLogEntry {
                                    count,
                                    clock,
                                    by,
                                    skip: *skip,
                                },
                            )
                        })
                    })()
                    .map_err(|e| e.attached(PersistedTable::HabitLog, row_key))?;
                    habits.log.insert(decoded.0, decoded.1);
                }
                for (habit_id, day, count, clock) in &habit_day_log_rows {
                    let row_key = Uuid::parse_str(habit_id).ok();
                    let decoded = (|| {
                        Result::<
                            (
                                taskboard_domain::HabitDayKey,
                                taskboard_domain::HabitDayLogEntry,
                            ),
                            RepositoryError,
                        >::Ok({
                            let habit = taskboard_domain::ids::HabitId::from(
                                codec::uuid_from_text(habit_id).map_err(|_| {
                                    corrupt(DecodeStage::RowBuild, DecodeReason::MalformedUuid)
                                })?,
                            );
                            let date = chrono::NaiveDate::parse_from_str(day, "%Y-%m-%d").map_err(
                                |_| corrupt(DecodeStage::RowBuild, DecodeReason::MalformedDate),
                            )?;
                            let count = u32::try_from(*count).map_err(|_| {
                                corrupt(DecodeStage::RowBuild, DecodeReason::OutOfRange)
                            })?;
                            let clock: DateTime<Utc> = decode::clock(clock)?;
                            (
                                taskboard_domain::HabitDayKey { habit, day: date },
                                taskboard_domain::HabitDayLogEntry { count, clock },
                            )
                        })
                    })()
                    .map_err(|e| e.attached(PersistedTable::HabitDayLog, row_key))?;
                    habits.day_log.insert(decoded.0, decoded.1);
                }
                for (entry_id, habit_id, habit_title, start, end, created_at, updated_at, au_by) in
                    &habit_time_entry_rows
                {
                    let row_key = Uuid::parse_str(entry_id).ok();
                    let decoded = (|| {
                        Result::<taskboard_domain::habit_time::HabitTimeEntry, RepositoryError>::Ok(
                            {
                                let id = taskboard_domain::ids::HabitTimeEntryId::from(
                                    codec::uuid_from_text(entry_id).map_err(|_| {
                                        corrupt(DecodeStage::RowBuild, DecodeReason::MalformedUuid)
                                    })?,
                                );
                                let habit = taskboard_domain::ids::HabitId::from(
                                    codec::uuid_from_text(habit_id).map_err(|_| {
                                        corrupt(DecodeStage::RowBuild, DecodeReason::MalformedUuid)
                                    })?,
                                );
                                let parse = |raw: &str| -> Result<DateTime<Utc>, RepositoryError> {
                                    DateTime::parse_from_rfc3339(raw)
                                        .map(|t| t.with_timezone(&Utc))
                                        .map_err(|_| {
                                            corrupt(
                                                DecodeStage::Column,
                                                DecodeReason::MalformedClock,
                                            )
                                        })
                                };
                                let start = parse(start)?;
                                let end = match end {
                                    Some(raw) => Some(parse(raw)?),
                                    None => None,
                                };
                                let created_at = parse(created_at)?;
                                let updated_at = parse(updated_at)?;
                                let by = decode::author(au_by.as_deref())?;
                                taskboard_domain::habit_time::HabitTimeEntry {
                                    id,
                                    habit,
                                    habit_title: habit_title.clone(),
                                    start,
                                    end,
                                    created_at,
                                    updated_at,
                                    by,
                                }
                            },
                        )
                    })()
                    .map_err(|e| e.attached(PersistedTable::HabitTimeEntries, row_key))?;
                    habits.time_log.insert(decoded.id, decoded);
                }
                habits
            },
            reflection: {
                let mut reflection = taskboard_domain::ReflectionState::default();
                for row in &reflection_rows {
                    let row_key = Uuid::parse_str(&row.entry_id).ok();
                    let (id, entry) = decode_reflection_entry(row)
                        .map_err(|e| e.attached(PersistedTable::Reflection, row_key))?;
                    reflection.entries.insert(id, entry);
                }
                reflection
            },
            roaming: {
                let mut roaming = taskboard_domain::RoamingSettings::default();
                for (name, value, clock) in &roaming_rows {
                    let decoded = (|| {
                        Result::<taskboard_domain::RoamingSettingRow, RepositoryError>::Ok({
                            let value: taskboard_domain::RoamingSettingValue =
                                serde_json::from_str(value).map_err(|_| {
                                    corrupt(DecodeStage::Payload, DecodeReason::SerdePayload)
                                })?;
                            let clock = decode::clock(clock)?;
                            taskboard_domain::RoamingSettingRow { value, clock }
                        })
                    })()
                    .map_err(|e| e.attached(PersistedTable::Roaming, None))?;
                    roaming.0.insert(name.clone(), decoded);
                }
                roaming
            },
            journal: taskboard_domain::JournalState {
                entries: journal_entries,
                head,
            },
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
        if journal_quarantined {
            // D-B2-7: the head re-pin is a best-effort write after the
            // load's transaction boundary — a failed re-pin leaves the
            // stale head and the next load's quarantine pass re-pins
            // (idempotent: the surviving-set invariant is what tests
            // pin). Then the one-per-load typed diagnostics report is
            // published for the engine's broadcast hop (D-B2-6).
            let pinned = head.map(|h| codec::uuid_to_text(h.as_uuid()));
            if let Err(err) = sqlx::query(r"UPDATE command_journal_head SET id = ?")
                .bind(pinned)
                .execute(&self.pool)
                .await
            {
                tracing::warn!(error = %err, "journal head re-pin failed; next load retries");
            }
            let _ = self.diagnostics.send(Some(LoadDiagnostics {
                details: quarantine_details,
                dropped,
                head,
            }));
        }
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

/// Every column holding exactly one local entity id.
const ADOPTION_ID_COLUMNS: &[(&str, &str)] = &[
    ("boards", "id"),
    ("stacks", "id"),
    ("stacks", "board"),
    ("tasks", "id"),
    ("tasks", "stack"),
    ("labels", "id"),
    ("labels", "board"),
    ("task_labels", "task"),
    ("task_labels", "label"),
    ("outbox", "task_id"),
    ("outbox", "stack_id"),
    ("outbox", "label_id"),
    ("outbox", "board_id"),
    ("reminder_rules", "task_id"),
    ("reminder_muted", "task_id"),
    ("time_entries", "task_id"),
    ("habits", "board_id"),
    ("break_rules", "board_id"),
    ("command_journal", "board"),
];

/// Columns holding a `default:<task uuid>` reminder reference.
const ADOPTION_REMINDER_COLUMNS: &[&str] = &[
    "reminder_deliveries",
    "reminder_snoozes",
    "reminder_dismissals",
];

/// Columns holding JSON with ids embedded, as `(table, key, column)`.
const ADOPTION_JSON_COLUMNS: &[(&str, &str, &str)] = &[
    ("outbox", "op_seq", "payload"),
    ("command_journal", "id", "payload"),
];

/// The storage half of `AdoptMovedBoard`. The table and column names
/// are the constants above, never input.
async fn execute_board_adoption(
    tx: &mut sqlx::SqliteConnection,
    rekey: &taskboard_domain::board_move::BoardRekey,
) -> Result<(), RepositoryError> {
    let err = |e: sqlx::Error| stmt_error(&e);
    sqlx::query("PRAGMA defer_foreign_keys = ON")
        .execute(&mut *tx)
        .await
        .map_err(err)?;
    sqlx::query("DELETE FROM sync_status WHERE board = ?")
        .bind(rekey.old_board.as_uuid().to_string())
        .execute(&mut *tx)
        .await
        .map_err(err)?;
    sqlx::query(
        "CREATE TEMP TABLE IF NOT EXISTS adoption_map (old TEXT PRIMARY KEY, new TEXT NOT NULL)",
    )
    .execute(&mut *tx)
    .await
    .map_err(err)?;
    sqlx::query("DELETE FROM adoption_map")
        .execute(&mut *tx)
        .await
        .map_err(err)?;
    let pairs = rekey.pairs();
    for (old, new) in &pairs {
        sqlx::query("INSERT INTO adoption_map (old, new) VALUES (?, ?)")
            .bind(old.to_string())
            .bind(new.to_string())
            .execute(&mut *tx)
            .await
            .map_err(err)?;
    }
    for (table, column) in ADOPTION_ID_COLUMNS {
        let sql = format!(
            "UPDATE {table} SET {column} = (SELECT new FROM adoption_map WHERE old = {column}) \
             WHERE {column} IN (SELECT old FROM adoption_map)"
        );
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .execute(&mut *tx)
            .await
            .map_err(err)?;
    }
    for table in ADOPTION_REMINDER_COLUMNS {
        let sql = format!(
            "UPDATE {table} SET reminder = 'default:' || \
             (SELECT new FROM adoption_map WHERE 'default:' || old = reminder) \
             WHERE reminder IN (SELECT 'default:' || old FROM adoption_map)"
        );
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .execute(&mut *tx)
            .await
            .map_err(err)?;
    }
    let lookup: std::collections::HashMap<uuid::Uuid, uuid::Uuid> = pairs.into_iter().collect();
    for (table, key, column) in ADOPTION_JSON_COLUMNS {
        let rows: Vec<(String, String)> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT CAST({key} AS TEXT), {column} FROM {table} WHERE {column} IS NOT NULL"
        )))
        .fetch_all(&mut *tx)
        .await
        .map_err(err)?;
        let update = format!("UPDATE {table} SET {column} = ? WHERE CAST({key} AS TEXT) = ?");
        for (id, text) in rows {
            let rewritten = taskboard_domain::board_move::rewrite_uuid_text(&text, &lookup);
            if rewritten != text {
                sqlx::query(sqlx::AssertSqlSafe(update.clone()))
                    .bind(rewritten)
                    .bind(id)
                    .execute(&mut *tx)
                    .await
                    .map_err(err)?;
            }
        }
    }
    clear_deck_bindings(tx, rekey).await
}

async fn clear_deck_bindings(
    tx: &mut sqlx::SqliteConnection,
    rekey: &taskboard_domain::board_move::BoardRekey,
) -> Result<(), RepositoryError> {
    let err = |e: sqlx::Error| stmt_error(&e);
    let new_board = rekey.new_board.as_uuid().to_string();
    sqlx::query(
        "UPDATE boards SET remote_board_id = NULL, remote_files_id = ?, connector = ?, \
         remote_seen = NULL WHERE id = ?",
    )
    .bind(rekey.new_board.as_uuid().to_string())
    .bind(rekey.binding.connector.as_str())
    .bind(&new_board)
    .execute(&mut *tx)
    .await
    .map_err(err)?;
    sqlx::query(
        "UPDATE tasks SET remote_board_id = NULL, remote_stack_id = NULL, remote_card_id = NULL, \
         remote_seen = NULL, remote_baseline = NULL \
         WHERE stack IN (SELECT id FROM stacks WHERE board = ?)",
    )
    .bind(&new_board)
    .execute(&mut *tx)
    .await
    .map_err(err)?;
    sqlx::query(
        "UPDATE stacks SET remote_board_id = NULL, remote_stack_id = NULL, remote_seen = NULL \
         WHERE board = ?",
    )
    .bind(&new_board)
    .execute(&mut *tx)
    .await
    .map_err(err)?;
    sqlx::query(
        "UPDATE labels SET remote_board_id = NULL, remote_label_id = NULL, remote_seen = NULL \
         WHERE board = ?",
    )
    .bind(&new_board)
    .execute(&mut *tx)
    .await
    .map_err(err)?;
    Ok(())
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
    for action in actions {
        if let PersistenceAction::AdoptMovedBoard(rekey) = action {
            execute_board_adoption(tx, rekey).await?;
        }
    }
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
    let mut break_rule_upserts = Vec::new();
    let mut break_rule_deletes = Vec::new();
    let mut deliveries = Vec::new();
    let mut delivery_prunes = Vec::new();
    let mut dismissals = Vec::new();
    let mut snoozes = Vec::new();
    let mut mute_toggles = Vec::new();
    let mut clear_board_content = Vec::new();
    // R-11 (DR-5/1): these two arms are the no-wildcard rule's own tax —
    // the planner cannot emit rows otherwise.
    let mut entry_upserts = Vec::new();
    let mut entry_deletes = Vec::new();
    // R-11 (DR-6/5), the tracking precedent: the no-wildcard rule's tax
    // for the habit family. The tables ship with 0017 in this tree, so
    // the queries run through the prepared-cache macros like every
    // other lane.
    let mut habit_upserts = Vec::new();
    let mut habit_log_rows = Vec::new();
    let mut habit_day_log_rows = Vec::new();
    let mut habit_time_upserts = Vec::new();
    let mut habit_time_deletes = Vec::new();
    // DR-7 chunk 2 (R-11 exception, the tracking DR-5/1 precedent):
    // the reflection upserts, written by the compile-checked macro
    // against migration 0022's table.
    let mut reflection_upserts = Vec::new();
    let mut roaming_upserts: Vec<(String, taskboard_domain::RoamingSettingRow)> = Vec::new();
    // ADR 0019 (undo/1): the three journal buckets — appends (each with
    // the D11 eviction mirror), head moves, flag flips. The journal
    // references op ids by value only (no FK constraints; the eviction
    // order owns integrity).
    let mut journal_appends = Vec::new();
    let mut journal_pushed = Vec::new();
    let mut journal_head_moves = Vec::new();
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
            // Ran first, above: the batch's upserts address the new ids.
            PersistenceAction::AdoptMovedBoard(_) => {}
            PersistenceAction::UpsertReminderRule(rule) => rule_upserts.push(rule),
            PersistenceAction::DeleteReminderRule(rule_id) => rule_deletes.push(rule_id),
            PersistenceAction::UpsertAdhocAlarm(alarm) => alarm_upserts.push(alarm),
            PersistenceAction::DeleteAdhocAlarm(alarm_id) => alarm_deletes.push(alarm_id),
            PersistenceAction::UpsertBreakRule(rule) => break_rule_upserts.push(rule),
            PersistenceAction::DeleteBreakRule(rule_id) => break_rule_deletes.push(*rule_id),
            PersistenceAction::RecordReminderDelivery(delivery) => deliveries.push(delivery),
            PersistenceAction::RecordReminderDismissal(dismissal) => dismissals.push(dismissal),
            PersistenceAction::RecordReminderSnooze(snooze) => snoozes.push(snooze),
            PersistenceAction::PruneReminderDeliveries(doomed) => {
                delivery_prunes.push(doomed);
            }
            PersistenceAction::SetTaskRemindersMuted(task, muted) => {
                mute_toggles.push((task, muted));
            }
            PersistenceAction::ClearBoardContent(board) => {
                clear_board_content.push(*board);
            }
            PersistenceAction::UpsertTimeEntry(entry) => entry_upserts.push(entry),
            PersistenceAction::DeleteTimeEntry(entry_id) => entry_deletes.push(*entry_id),
            PersistenceAction::UpsertHabit(habit) => habit_upserts.push(habit),
            PersistenceAction::RecordHabitLog(key, entry) => {
                habit_log_rows.push((key, entry));
            }
            PersistenceAction::RecordHabitDayLog(key, entry) => {
                habit_day_log_rows.push((key, entry));
            }
            PersistenceAction::UpsertHabitTimeEntry(entry) => {
                habit_time_upserts.push(entry);
            }
            PersistenceAction::DeleteHabitTimeEntry(entry_id) => {
                habit_time_deletes.push(*entry_id);
            }
            PersistenceAction::UpsertReflectionEntry(entry) => {
                reflection_upserts.push(entry);
            }
            PersistenceAction::UpsertRoamingSetting(name, setting) => {
                roaming_upserts.push((name.clone(), *setting));
            }
            PersistenceAction::RecordJournal(entry) => journal_appends.push(entry),
            PersistenceAction::MarkJournalPushed(id) => journal_pushed.push(*id),
            PersistenceAction::MoveJournalHead(head) => journal_head_moves.push(*head),
        }
    }

    for board in boards {
        // D-BF4: exactly one id column carries the binding's payload —
        // `Deck(n)` the INTEGER one, `Files(u)` the TEXT one. An unbound
        // board leaves both NULL.
        let (remote_deck, remote_files) =
            board
                .remote
                .as_ref()
                .map_or((None, None), |b| match b.board {
                    taskboard_domain::ids::RemoteBoardId::Deck(num) => {
                        (Some(codec::remote_col(num)), None)
                    }
                    taskboard_domain::ids::RemoteBoardId::Files(id) => (None, Some(id.to_string())),
                });
        // The tag source is the domain binding (DF-4): a bound row carries
        // its binding's connector, an unbound row the default (non-NULL on
        // every upsert; the tag on an unbound row is inert).
        let connector = board
            .remote
            .as_ref()
            .map_or(default_connector.as_str(), |b| b.connector.as_str());
        sqlx::query!(
            r#"INSERT INTO boards (id, remote_board_id, remote_files_id, title, color, archived, deleted, connector, ck_title, ck_color, ck_archived, ck_deleted, au_title, au_color, au_archived, au_deleted, remote_seen)
                   VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                   ON CONFLICT(id) DO UPDATE SET
                       remote_board_id = excluded.remote_board_id,
                       remote_files_id = excluded.remote_files_id,
                       title = excluded.title,
                       color = excluded.color,
                       archived = excluded.archived,
                       deleted = excluded.deleted,
                       connector = excluded.connector,
                       ck_title = excluded.ck_title,
                       ck_color = excluded.ck_color,
                       ck_archived = excluded.ck_archived,
                       ck_deleted = excluded.ck_deleted,
                       au_title = excluded.au_title,
                       au_color = excluded.au_color,
                       au_archived = excluded.au_archived,
                       au_deleted = excluded.au_deleted,
                       remote_seen = excluded.remote_seen"#,
            codec::uuid_to_text(board.id.as_uuid()),
            remote_deck,
            remote_files,
            board.title,
            board.color.as_str(),
            board.archived,
            board.deleted,
            connector,
            board.clocks.title.to_rfc3339(),
            board.clocks.color.to_rfc3339(),
            board.clocks.archived.to_rfc3339(),
            board.clocks.deleted.to_rfc3339(),
            board.authors.title.map(|a| a.to_string()),
            board.authors.color.map(|a| a.to_string()),
            board.authors.archived.map(|a| a.to_string()),
            board.authors.deleted.map(|a| a.to_string()),
            board.remote_seen,
        )
        .execute(&mut *tx)
        .await.map_err(|e| stmt_error(&e))?;
        tracing::trace!(board = ?board.id, "board upserted");
    }

    for stack in stacks {
        let (rb, rs) = match stack.remote {
            Some(r) => {
                let (b, s) = codec::remote_stack_to_columns(r).map_err(|_| {
                    corrupt(DecodeStage::RowBuild, DecodeReason::OutOfRange)
                        .attached(PersistedTable::Stacks, Some(stack.id.as_uuid()))
                })?;
                (Some(b), Some(s))
            }
            None => (None, None),
        };
        sqlx::query!(
            r#"INSERT INTO stacks (id, board, remote_board_id, remote_stack_id, title, sort_order, archived, deleted, ck_title, ck_sort_order, ck_deleted, au_title, au_sort_order, au_deleted, remote_seen)
                   VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
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
                       au_title = excluded.au_title,
                       au_sort_order = excluded.au_sort_order,
                       au_deleted = excluded.au_deleted,
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
            stack.authors.title.map(|a| a.to_string()),
            stack.authors.order.map(|a| a.to_string()),
            stack.authors.deleted.map(|a| a.to_string()),
            stack.remote_seen,
        )
        .execute(&mut *tx)
        .await.map_err(|e| stmt_error(&e))?;
        tracing::trace!(stack = ?stack.id, board = ?stack.board, "stack upserted");
    }

    for label in labels {
        let (rb, rl) = match label.remote {
            Some(r) => {
                let (b, l) = codec::remote_label_to_columns(r).map_err(|_| {
                    corrupt(DecodeStage::RowBuild, DecodeReason::OutOfRange)
                        .attached(PersistedTable::Labels, Some(label.id.as_uuid()))
                })?;
                (Some(b), Some(l))
            }
            None => (None, None),
        };
        sqlx::query!(
            r#"INSERT INTO labels (id, board, remote_board_id, remote_label_id, title, color, deleted, ck_title, ck_color, ck_deleted, au_title, au_color, au_deleted, remote_seen)
                   VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
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
                       au_title = excluded.au_title,
                       au_color = excluded.au_color,
                       au_deleted = excluded.au_deleted,
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
            label.authors.title.map(|a| a.to_string()),
            label.authors.color.map(|a| a.to_string()),
            label.authors.deleted.map(|a| a.to_string()),
            label.remote_seen,
        )
        .execute(&mut *tx)
        .await.map_err(|e| stmt_error(&e))?;
        tracing::trace!(label = ?label.id, board = ?label.board, "label upserted");
    }

    for task in tasks {
        let (rb, rs, rc) = match task.remote {
            Some(r) => {
                let (b, s, c) = codec::remote_card_to_columns(r).map_err(|_| {
                    corrupt(DecodeStage::RowBuild, DecodeReason::OutOfRange)
                        .attached(PersistedTable::Tasks, Some(task.id.as_uuid()))
                })?;
                (Some(b), Some(s), Some(c))
            }
            None => (None, None, None),
        };
        // D-F1's typed bound: effort is unsigned in the domain; the
        // storage INTEGER is an i64, so values past i64::MAX are a
        // corrupt-input rejection, never a silent wrap.
        let effort = task
            .effort
            .map(|e| {
                i64::try_from(e.seconds()).map_err(|_| {
                    corrupt(DecodeStage::RowBuild, DecodeReason::OutOfRange)
                        .attached(PersistedTable::Tasks, Some(task.id.as_uuid()))
                })
            })
            .transpose()?;
        sqlx::query!(
            r#"INSERT INTO tasks (id, stack, remote_board_id, remote_stack_id, remote_card_id, title, description, duedate, scheduled, done, sort_order, effort, importance, urgency, recur, kind, archived, deleted, ck_title, ck_description, ck_duedate, ck_scheduled, ck_done, ck_position, ck_labels, ck_effort, ck_importance, ck_urgency, ck_recur, ck_kind, ck_archived, ck_deleted, au_title, au_description, au_duedate, au_scheduled, au_done, au_position, au_effort, au_importance, au_urgency, au_recur, au_kind, au_archived, au_deleted, remote_seen, remote_baseline)
                   VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
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
                       kind = excluded.kind,
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
                       ck_kind = excluded.ck_kind,
                       ck_archived = excluded.ck_archived,
                       ck_deleted = excluded.ck_deleted,
                       au_title = excluded.au_title,
                       au_description = excluded.au_description,
                       au_duedate = excluded.au_duedate,
                       au_scheduled = excluded.au_scheduled,
                       au_done = excluded.au_done,
                       au_position = excluded.au_position,
                       au_effort = excluded.au_effort,
                       au_importance = excluded.au_importance,
                       au_urgency = excluded.au_urgency,
                       au_recur = excluded.au_recur,
                       au_kind = excluded.au_kind,
                       au_archived = excluded.au_archived,
                       au_deleted = excluded.au_deleted,
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
            codec::task_kind_to_text(task.kind),
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
            task.clocks.kind.to_rfc3339(),
            task.clocks.archived.to_rfc3339(),
            task.clocks.deleted.to_rfc3339(),
            task.authors.title.map(|a| a.to_string()),
            task.authors.description.map(|a| a.to_string()),
            task.authors.duedate.map(|a| a.to_string()),
            task.authors.scheduled.map(|a| a.to_string()),
            task.authors.done.map(|a| a.to_string()),
            task.authors.position.map(|a| a.to_string()),
            task.authors.effort.map(|a| a.to_string()),
            task.authors.importance.map(|a| a.to_string()),
            task.authors.urgency.map(|a| a.to_string()),
            task.authors.recur.map(|a| a.to_string()),
            task.authors.kind.map(|a| a.to_string()),
            task.authors.archived.map(|a| a.to_string()),
            task.authors.deleted.map(|a| a.to_string()),
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

    // R-11 (DR-5/1): the time-entry writes. Hyphenated-UUID TEXT ids,
    // RFC 3339 TEXT stamps, no FKs (entries outlive soft-deleted tasks).
    // Deletes run after upserts so a batch carrying both for one id
    // mirrors `apply_actions`' sequential last-write-wins semantics.
    for entry in entry_upserts {
        sqlx::query!(
            r#"INSERT INTO time_entries (entry_id, task_id, task_title, start, end, created_at, updated_at, kind, au_by)
                   VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
                   ON CONFLICT(entry_id) DO UPDATE SET
                       task_id = excluded.task_id,
                       task_title = excluded.task_title,
                       start = excluded.start,
                       end = excluded.end,
                       created_at = excluded.created_at,
                       updated_at = excluded.updated_at,
                       kind = excluded.kind,
                       au_by = excluded.au_by"#,
            codec::uuid_to_text(entry.id.as_uuid()),
            codec::uuid_to_text(entry.task.as_uuid()),
            entry.task_title,
            entry.start.to_rfc3339(),
            entry.end.map(|end| end.to_rfc3339()),
            entry.created_at.to_rfc3339(),
            entry.updated_at.to_rfc3339(),
            codec::time_entry_kind_to_text(entry.kind),
            entry.by.map(|a| a.to_string()),
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| stmt_error(&e))?;
        tracing::trace!(entry = ?entry.id, "time entry upserted");
    }
    for entry_id in entry_deletes {
        sqlx::query!(
            r#"DELETE FROM time_entries WHERE entry_id = ?"#,
            codec::uuid_to_text(entry_id.as_uuid())
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| stmt_error(&e))?;
        tracing::trace!(entry = ?entry_id, "time entry deleted");
    }

    // R-11's sibling (0018): the break-rule writes, the 0008
    // adhoc-alarms conventions. Deletes run after upserts so a batch
    // carrying both for one id mirrors `apply_actions`' sequential
    // last-write-wins semantics.
    for rule in break_rule_upserts {
        sqlx::query!(
            r#"INSERT INTO break_rules (rule_id, board_id, label, threshold_seconds, insistence, created_at, au_by)
                   VALUES (?, ?, ?, ?, ?, ?, ?)
                   ON CONFLICT(rule_id) DO UPDATE SET
                       board_id = excluded.board_id,
                       label = excluded.label,
                       threshold_seconds = excluded.threshold_seconds,
                       insistence = excluded.insistence,
                       created_at = excluded.created_at,
                       au_by = excluded.au_by"#,
            codec::uuid_to_text(rule.id.as_uuid()),
            codec::uuid_to_text(rule.board.as_uuid()),
            rule.label,
            rule.threshold.as_duration().num_seconds(),
            codec::insistence_to_text(rule.insistence),
            rule.created_at.to_rfc3339(),
            rule.by.map(|a| a.to_string()),
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| stmt_error(&e))?;
        tracing::trace!(rule = ?rule.id, "break rule upserted");
    }
    for rule_id in break_rule_deletes {
        sqlx::query!(
            r#"DELETE FROM break_rules WHERE rule_id = ?"#,
            codec::uuid_to_text(rule_id.as_uuid())
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| stmt_error(&e))?;
        tracing::trace!(rule = ?rule_id, "break rule deleted");
    }

    for habit in habit_upserts {
        sqlx::query!(
            r#"INSERT INTO habits (habit_id, board_id, title, description, target, sort_order,
                         anchor_time, anchor_day, paused_until, scheduled_days, reminder,
                         ck_title, ck_target, ck_order, ck_anchor_time, ck_description,
                         ck_anchor_day, ck_paused_until, ck_scheduled_days, ck_reminder,
                         ck_deleted, deleted, continued_from, au_by)
                   VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                   ON CONFLICT(habit_id) DO UPDATE SET
                       board_id = excluded.board_id,
                       title = excluded.title,
                       description = excluded.description,
                       target = excluded.target,
                       sort_order = excluded.sort_order,
                       anchor_time = excluded.anchor_time,
                       anchor_day = excluded.anchor_day,
                       paused_until = excluded.paused_until,
                       scheduled_days = excluded.scheduled_days,
                       reminder = excluded.reminder,
                       ck_title = excluded.ck_title,
                       ck_target = excluded.ck_target,
                       ck_order = excluded.ck_order,
                       ck_anchor_time = excluded.ck_anchor_time,
                       ck_description = excluded.ck_description,
                       ck_anchor_day = excluded.ck_anchor_day,
                       ck_paused_until = excluded.ck_paused_until,
                       ck_scheduled_days = excluded.ck_scheduled_days,
                       ck_reminder = excluded.ck_reminder,
                       ck_deleted = excluded.ck_deleted,
                       deleted = excluded.deleted,
                       continued_from = excluded.continued_from,
                       au_by = excluded.au_by"#,
            codec::uuid_to_text(habit.id.as_uuid()),
            codec::uuid_to_text(habit.board.as_uuid()),
            habit.title,
            &habit.description,
            serde_json::to_string(&habit.target).expect("target serializes"),
            habit.order,
            habit.anchor_time.map(|t| t.to_string()),
            habit
                .anchor_day
                .map(|d| serde_json::to_string(&d).expect("anchor_day serializes")),
            habit.paused_until.map(|d| d.to_string()),
            habit
                .scheduled_days
                .map(|d| serde_json::to_string(&d).expect("scheduled_days serializes")),
            habit
                .reminder
                .map(|d| serde_json::to_string(&d).expect("reminder serializes")),
            habit.clocks.title.to_rfc3339(),
            habit.clocks.target.to_rfc3339(),
            habit.clocks.order.to_rfc3339(),
            habit.clocks.anchor_time.to_rfc3339(),
            habit.clocks.description.to_rfc3339(),
            habit.clocks.anchor_day.to_rfc3339(),
            habit.clocks.paused_until.to_rfc3339(),
            habit.clocks.scheduled_days.to_rfc3339(),
            habit.clocks.reminder.to_rfc3339(),
            habit.clocks.deleted.to_rfc3339(),
            habit.deleted,
            habit
                .continued_from
                .map(|s| codec::uuid_to_text(s.as_uuid())),
            habit.by.map(|a| a.to_string()),
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| stmt_error(&e))?;
        tracing::trace!(habit = ?habit.id, "habit upserted");
    }
    // DR-7 chunk 2 (D-J13): the compile-checked reflection upserts —
    // the habit-write macro's shape, migration 0022's table. Plain
    // whole-row upserts, no clock predicate (Q-13: engine-ordered
    // batches + the tombstone-in-place ruling leave an LWW guard
    // caller-less today; the import fold may add it at #631).
    for entry in reflection_upserts {
        sqlx::query!(
            r#"INSERT INTO reflection_entries (entry_id, cadence, day, window_start, window_end,
                         text, mood, energy, flags, extra_flags, created_at, updated_at, deleted)
                   VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                   ON CONFLICT(entry_id) DO UPDATE SET
                       cadence = excluded.cadence,
                       day = excluded.day,
                       window_start = excluded.window_start,
                       window_end = excluded.window_end,
                       text = excluded.text,
                       mood = excluded.mood,
                       energy = excluded.energy,
                       flags = excluded.flags,
                       extra_flags = excluded.extra_flags,
                       created_at = excluded.created_at,
                       updated_at = excluded.updated_at,
                       deleted = excluded.deleted"#,
            codec::uuid_to_text(entry.id.as_uuid()),
            entry.cadence.text_form(),
            entry.day.to_string(),
            entry.window_start.to_rfc3339(),
            entry.window_end.to_rfc3339(),
            entry.text,
            entry.attributes.mood.map(Rating::get),
            entry.attributes.energy.map(Rating::get),
            codec::reflection_flags_to_json(&entry.attributes.flags),
            codec::reflection_extra_flags_to_json(&entry.attributes.extra_flags),
            entry.created_at.to_rfc3339(),
            entry.updated_at.to_rfc3339(),
            entry.deleted,
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| stmt_error(&e))?;
        tracing::trace!(entry = ?entry.id, "reflection entry upserted");
    }
    // DR-9 D-BF3 (#685): the roaming-settings upserts — the import
    // fold's adopted winner applied verbatim, no clock predicate (the
    // reflection loop's shape; the guard ran in the fold).
    for (name, setting) in &roaming_upserts {
        sqlx::query(
            r"INSERT INTO roaming_settings (name, value, clock) VALUES (?, ?, ?)
                 ON CONFLICT(name) DO UPDATE SET
                     value = excluded.value,
                     clock = excluded.clock",
        )
        .bind(name)
        .bind(serde_json::to_string(&setting.value).map_err(|_| {
            corrupt(DecodeStage::Payload, DecodeReason::SerdePayload)
                .attached(PersistedTable::Roaming, None)
        })?)
        .bind(setting.clock.to_rfc3339())
        .execute(&mut *tx)
        .await
        .map_err(|e| stmt_error(&e))?;
        tracing::trace!(setting = %name, "roaming setting upserted");
    }
    for (key, entry) in habit_log_rows {
        // The LWW guard lives in the SQL: a strictly-older batch row
        // never overwrites; equal/older keeps local (belt to the fold's
        // braces).
        //
        // Invariant (2026-11-23, #1095): clocks in `habit_log` are
        // whole-second RFC 3339 UTC (`SecondsFormat::Secs`); the
        // guard's lexicographic string comparison is chronological
        // under that grammar. Sub-second precision would break the
        // guard (`…:00.5Z` sorts *before* `…:00Z`) and is refused
        // precision at this write boundary; if a future rule ever
        // wants sub-second clocks (#986 rejected it), this guard must
        // move to an epoch-integer comparison first.
        sqlx::query!(
            r#"INSERT INTO habit_log (habit_id, window_start, count, clock, au_by, skip)
                   VALUES (?, ?, ?, ?, ?, ?)
                   ON CONFLICT(habit_id, window_start) DO UPDATE SET
                       count = excluded.count,
                       clock = excluded.clock,
                       au_by = excluded.au_by,
                       skip = excluded.skip
                   WHERE excluded.clock > habit_log.clock"#,
            codec::uuid_to_text(key.habit.as_uuid()),
            key.window.as_date().to_string(),
            i64::from(entry.count),
            // The whole-second grammar (see the invariant above):
            // sub-second input is normalized away here, so the SQL
            // guard's string comparison stays chronological by
            // construction.
            entry
                .clock
                .to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            entry.by.map(|a| a.to_string()),
            entry.skip,
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| stmt_error(&e))?;
        tracing::trace!(habit = ?key.habit, window = %key.window.as_date(), "habit log recorded");
    }
    for (key, entry) in habit_day_log_rows {
        // The same whole-second LWW guard as `habit_log` (#1095's
        // invariant): the overlay's clock grammar is identical, so the
        // string comparison stays chronological. The twin rides the
        // same transaction as its window row (one repo.apply batch —
        // the transactional-twin law).
        sqlx::query(
            r"INSERT INTO habit_day_log (habit_id, day, count, clock)
                   VALUES (?, ?, ?, ?)
                   ON CONFLICT(habit_id, day) DO UPDATE SET
                       count = excluded.count,
                       clock = excluded.clock
                   WHERE excluded.clock > habit_day_log.clock",
        )
        .bind(codec::uuid_to_text(key.habit.as_uuid()))
        .bind(key.day.to_string())
        .bind(i64::from(entry.count))
        .bind(
            entry
                .clock
                .to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| stmt_error(&e))?;
        tracing::trace!(habit = ?key.habit, day = %key.day, "habit day overlay recorded");
    }
    // The habit time-entry writes (#1305): the 0015 conventions —
    // entity upsert discipline (replace by id), no FKs, history
    // outlives the habit.
    for entry in habit_time_upserts {
        sqlx::query(
            r"INSERT INTO habit_time_entries (entry_id, habit_id, habit_title, start, end, created_at, updated_at, au_by)
                   VALUES (?, ?, ?, ?, ?, ?, ?, ?)
                   ON CONFLICT(entry_id) DO UPDATE SET
                       habit_id = excluded.habit_id,
                       habit_title = excluded.habit_title,
                       start = excluded.start,
                       end = excluded.end,
                       created_at = excluded.created_at,
                       updated_at = excluded.updated_at,
                       au_by = excluded.au_by",
        )
        .bind(codec::uuid_to_text(entry.id.as_uuid()))
        .bind(codec::uuid_to_text(entry.habit.as_uuid()))
        .bind(&entry.habit_title)
        .bind(entry.start.to_rfc3339())
        .bind(entry.end.map(|end| end.to_rfc3339()))
        .bind(entry.created_at.to_rfc3339())
        .bind(entry.updated_at.to_rfc3339())
        .bind(entry.by.map(|a| a.to_string()))
        .execute(&mut *tx)
        .await
        .map_err(|e| stmt_error(&e))?;
        tracing::trace!(entry = ?entry.id, "habit time entry upserted");
    }
    for entry_id in habit_time_deletes {
        sqlx::query(r"DELETE FROM habit_time_entries WHERE entry_id = ?")
            .bind(codec::uuid_to_text(entry_id.as_uuid()))
            .execute(&mut *tx)
            .await
            .map_err(|e| stmt_error(&e))?;
        tracing::trace!(entry = ?entry_id, "habit time entry deleted");
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
        let payload = codec::op_payload_to_json(&op.op);
        // Upsert on op_id: re-enqueueing an existing id refreshes the
        // row in place and keeps the original queue position — the
        // `UNIQUE` constraint stays the integrity backstop (plan P4).
        sqlx::query!(
            r#"INSERT INTO outbox (op_id, op_kind, task_id, stack_id, label_id, board_id, payload, queued_at)
                   VALUES (?, ?, ?, ?, ?, ?, ?, ?)
                   ON CONFLICT(op_id) DO UPDATE SET
                       op_kind = excluded.op_kind,
                       task_id = excluded.task_id,
                       stack_id = excluded.stack_id,
                       label_id = excluded.label_id,
                       board_id = excluded.board_id,
                       payload = excluded.payload,
                       queued_at = excluded.queued_at"#,
            codec::uuid_to_text(op.op_id.0),
            codec::op_kind_to_text(&op.op),
            task_id,
            stack_id,
            label_id,
            board_id,
            payload,
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
        // The kind columns are decided by the kind itself: a due-offset
        // row stores the (whole-second, non-positive) offset exactly —
        // the type guarantees no truncation, no divergence between the
        // evaluated offset and the reloaded one — and a NULL epoch; an
        // `At` row stores the epoch seconds and a placeholder offset
        // (the column is NOT NULL from 0004; the decode never reads it
        // for `At` rows).
        let (offset_seconds_column, at_epoch_seconds_column) = match rule.kind {
            ReminderKind::DueOffset(offset) => (offset.as_duration().num_seconds(), None),
            ReminderKind::At(at) => (0, Some(at.as_datetime().timestamp())),
        };
        sqlx::query!(
            r#"INSERT INTO reminder_rules (rule_id, task_id, offset_seconds, kind, at_epoch_seconds, insistence, created_at, updated_at, au_by)
                   VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
                   ON CONFLICT(rule_id) DO UPDATE SET
                       task_id = excluded.task_id,
                       offset_seconds = excluded.offset_seconds,
                       kind = excluded.kind,
                       at_epoch_seconds = excluded.at_epoch_seconds,
                       insistence = excluded.insistence,
                       created_at = excluded.created_at,
                       updated_at = excluded.updated_at,
                       au_by = excluded.au_by"#,
            codec::uuid_to_text(rule.id.as_uuid()),
            codec::uuid_to_text(rule.task.as_uuid()),
            offset_seconds_column,
            codec::reminder_kind_to_text(rule.kind),
            at_epoch_seconds_column,
            codec::insistence_to_text(rule.insistence),
            rule.created_at.to_rfc3339(),
            rule.updated_at.to_rfc3339(),
            rule.by.map(|a| a.to_string()),
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
            r#"INSERT INTO adhoc_alarms (alarm_id, label, fire_at, insistence, created_at)
                   VALUES (?, ?, ?, ?, ?)
                   ON CONFLICT(alarm_id) DO UPDATE SET
                       label = excluded.label,
                       fire_at = excluded.fire_at,
                       insistence = excluded.insistence,
                       created_at = excluded.created_at"#,
            codec::uuid_to_text(alarm.id.as_uuid()),
            alarm.label,
            alarm.fire_at.to_rfc3339(),
            codec::insistence_to_text(alarm.insistence),
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

    for doomed in delivery_prunes {
        // The engine's sweep twin: per-row prepared DELETEs (the
        // `DeleteAdhocAlarm` cascade shape) sidestep SQLite's bound
        // variable ceiling for the one-time large first sweep. Absent
        // rows are skipped by DELETE semantics — the replay parity the
        // in-memory interpreter's set difference carries (the lockstep
        // contract pins both sides on the same scenario).
        for delivery in doomed {
            sqlx::query!(
                r#"DELETE FROM reminder_deliveries
                   WHERE reminder = ? AND occurrence = ?"#,
                codec::reminder_ref_to_text(delivery.reminder),
                delivery.occurrence.to_rfc3339(),
            )
            .execute(&mut *tx)
            .await
            .map_err(|e| stmt_error(&e))?;
        }
        tracing::trace!(rows = doomed.len(), "stale reminder deliveries pruned");
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
                  OR key IN (SELECT 'archived_stacks:' || remote_board_id FROM boards WHERE id = ?1)
                  OR key IN (
                      SELECT 'stacks:files:' || remote_files_id FROM boards
                      WHERE id = ?1 AND remote_files_id IS NOT NULL
                  )
                  OR key IN (
                      SELECT 'archived_stacks:files:' || remote_files_id FROM boards
                      WHERE id = ?1 AND remote_files_id IS NOT NULL
                  )"#,
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

    // Journal partitions (ADR 0019), in the fixed order after the
    // existing ones: appends (the D11 eviction mirror runs inside the
    // same transaction), head moves, flag flips. The idempotence
    // contract mirrors `apply_actions`: an append replaces by id, a flag
    // flip is an unconditional `SET pushed = 1`, a head move is one
    // single-row `UPDATE`.
    for entry in journal_appends {
        sqlx::query(
            r"INSERT INTO command_journal (id, parent, created_at, pushed, board, payload)
                   VALUES (?, ?, ?, ?, ?, ?)
                   ON CONFLICT(id) DO UPDATE SET
                       parent = excluded.parent,
                       created_at = excluded.created_at,
                       pushed = excluded.pushed,
                       board = excluded.board,
                       payload = excluded.payload",
        )
        .bind(codec::uuid_to_text(entry.id.as_uuid()))
        .bind(entry.parent.map(|p| codec::uuid_to_text(p.as_uuid())))
        .bind(entry.created_at.to_rfc3339())
        .bind(entry.pushed)
        .bind(entry.board.map(|b| codec::uuid_to_text(b.as_uuid())))
        .bind(codec::journal_payload_to_json(
            &entry.command,
            &entry.facts,
            &entry.ops,
        ))
        .execute(&mut *tx)
        .await
        .map_err(|e| stmt_error(&e))?;
        // The record action's head move (apply-arm parity: append +
        // head := entry.id).
        sqlx::query(r"UPDATE command_journal_head SET id = ?")
            .bind(codec::uuid_to_text(entry.id.as_uuid()))
            .execute(&mut *tx)
            .await
            .map_err(|e| stmt_error(&e))?;
        tracing::trace!(entry = ?entry.id, "journal entry recorded");
        // The D11 eviction mirror: the same pure decision the memory
        // apply arm ran, over the table's lean columns, inside this
        // transaction — a pruned journal stays pruned across `load()`.
        let rows: Vec<(String, Option<String>, String)> =
            sqlx::query_as(r"SELECT id, parent, created_at FROM command_journal")
                .fetch_all(&mut *tx)
                .await
                .map_err(|e| stmt_error(&e))?;
        let line: Vec<(
            taskboard_domain::JournalId,
            Option<taskboard_domain::JournalId>,
            DateTime<Utc>,
        )> = rows
            .iter()
            .filter_map(|(id, parent, created_at)| {
                let id = taskboard_domain::JournalId::from(decode::uuid(id).ok()?);
                let parent = parent
                    .as_deref()
                    .map(decode::uuid)
                    .transpose()
                    .ok()?
                    .map(taskboard_domain::JournalId::from);
                let created_at: DateTime<Utc> = created_at.parse().ok()?;
                Some((id, parent, created_at))
            })
            .collect();
        if line.len() == rows.len() {
            let evicted = taskboard_domain::evict_journal_ids(&line, entry.created_at);
            for id in evicted {
                sqlx::query(r"DELETE FROM command_journal WHERE id = ?")
                    .bind(codec::uuid_to_text(id.as_uuid()))
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| stmt_error(&e))?;
                // Apply-arm parity: a head the eviction pruned reports
                // `NothingToUndo` after the next boot, too.
                sqlx::query(r"UPDATE command_journal_head SET id = NULL WHERE id = ?")
                    .bind(codec::uuid_to_text(id.as_uuid()))
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| stmt_error(&e))?;
                tracing::trace!(entry = ?id, "journal entry evicted");
            }
        }
    }

    for id in journal_pushed {
        sqlx::query(r"UPDATE command_journal SET pushed = 1 WHERE id = ?")
            .bind(codec::uuid_to_text(id.as_uuid()))
            .execute(&mut *tx)
            .await
            .map_err(|e| stmt_error(&e))?;
        tracing::trace!(entry = ?id, "journal entry marked pushed");
    }

    for head in journal_head_moves {
        sqlx::query(r"UPDATE command_journal_head SET id = ?")
            .bind(head.map(|h| codec::uuid_to_text(h.as_uuid())))
            .execute(&mut *tx)
            .await
            .map_err(|e| stmt_error(&e))?;
        tracing::trace!(?head, "journal head moved");
    }

    Ok(())
}

#[cfg(test)]
mod remote_decode_tests {
    use super::decode;
    use taskboard_domain::ids::ConnectorId;
    use taskboard_domain::persistence::RepositoryError;

    #[test]
    fn all_null_remote_columns_decode_as_unset() {
        assert!(
            decode::remote_board(None, None, None, &ConnectorId::from("c1"))
                .unwrap()
                .is_none()
        );
        assert!(decode::remote_stack(&[None, None]).unwrap().is_none());
        assert!(decode::remote_card(&[None, None, None]).unwrap().is_none());
        assert!(decode::remote_label(&[None, None]).unwrap().is_none());
    }

    #[test]
    fn exact_arity_remote_columns_decode_the_ref() {
        let board = decode::remote_board(Some(7), None, None, &ConnectorId::from("c1"))
            .unwrap()
            .unwrap();
        assert_eq!(board.board, taskboard_domain::ids::RemoteBoardId::deck(7));
        let stack = decode::remote_stack(&[Some(1), Some(2)]).unwrap().unwrap();
        assert_eq!(stack.stack, taskboard_domain::ids::RemoteStackId::deck(2));
        let card = decode::remote_card(&[Some(1), Some(2), Some(3)])
            .unwrap()
            .unwrap();
        assert_eq!(card.card, taskboard_domain::ids::RemoteCardId::deck(3));
        let label = decode::remote_label(&[Some(1), Some(4)]).unwrap().unwrap();
        assert_eq!(label.label, taskboard_domain::ids::RemoteLabelId::deck(4));
    }

    #[test]
    fn under_arity_remote_columns_are_corrupted_not_a_panic() {
        assert!(matches!(
            decode::remote_stack(&[Some(1)]).unwrap_err(),
            RepositoryError::Corrupted(_)
        ));
        assert!(matches!(
            decode::remote_card(&[Some(1), Some(2)]).unwrap_err(),
            RepositoryError::Corrupted(_)
        ));
        assert!(matches!(
            decode::remote_label(&[Some(1)]).unwrap_err(),
            RepositoryError::Corrupted(_)
        ));
    }

    #[test]
    fn over_arity_remote_columns_are_corrupted_not_ignored() {
        assert!(matches!(
            decode::remote_board(Some(1), Some("not-a-uuid"), None, &ConnectorId::from("c1"))
                .unwrap_err(),
            RepositoryError::Corrupted(_)
        ));
        assert!(matches!(
            decode::remote_card(&[Some(1), Some(2), Some(3), Some(4)]).unwrap_err(),
            RepositoryError::Corrupted(_)
        ));
    }
}

#[cfg(test)]
mod author_decode_tests {
    use super::decode;
    use taskboard_domain::ids::AuthorId;
    use taskboard_domain::persistence::RepositoryError;

    fn author(byte: u8) -> AuthorId {
        let mut bytes = [0u8; 16];
        bytes[15] = byte;
        AuthorId::from_uuid(uuid::Uuid::from_bytes(bytes))
    }

    /// T-STORE-3 (decode grammar table): bare RFC 3339 →
    /// `(clock, None)`; `#<uuid>` → `Some`; `#garbage`, double-`#`,
    /// empty suffix → `Corrupted` (typed, no text assertions).
    #[test]
    fn authored_clock_grammar_decode_table() {
        let a = author(0x71);
        let bare = "2026-11-07T10:00:00+00:00";
        let (clock, by) = decode::clock_with_author(bare).expect("bare form");
        assert_eq!(clock.to_rfc3339(), bare);
        assert_eq!(by, None);

        let (clock, by) = decode::clock_with_author(&format!("{bare}#{a}")).expect("suffix form");
        assert_eq!(clock.to_rfc3339(), bare);
        assert_eq!(by, Some(a));

        for bad in [
            format!("{bare}#garbage"),
            format!("{bare}#{a}#extra"),
            format!("{bare}#"),
            format!("{bare}#{a}#"),
        ] {
            assert!(
                matches!(
                    decode::clock_with_author(&bad),
                    Err(RepositoryError::Corrupted(_))
                ),
                "fail closed on {bad}"
            );
        }
        assert!(matches!(
            decode::clock_with_author("not a clock"),
            Err(RepositoryError::Corrupted(_))
        ));
    }

    /// The `au_*` column decode: NULL → `None`, parseable → `Some`,
    /// anything else → `Corrupted` (strict after the split).
    #[test]
    fn author_column_decode_table() {
        let a = author(0x72);
        assert_eq!(decode::author(None).unwrap(), None);
        assert_eq!(decode::author(Some(&a.to_string())).unwrap(), Some(a));
        assert!(matches!(
            decode::author(Some("not-a-uuid")),
            Err(RepositoryError::Corrupted(_))
        ));
        assert!(matches!(
            decode::author(Some("")),
            Err(RepositoryError::Corrupted(_))
        ));
    }

    /// T-STORE-4 (unit): the non-family clock decoder rejects
    /// `#`-forms as `Corrupted` — the vocabulary tripwire that keeps
    /// the authored grammar out of the device-local columns.
    #[test]
    fn plain_clock_decoder_rejects_suffix_forms() {
        assert!(decode::clock("2026-11-07T10:00:00+00:00").is_ok());
        let a = author(0x73);
        let suffixed = format!("2026-11-07T10:00:00+00:00#{a}");
        assert!(matches!(
            decode::clock(&suffixed),
            Err(RepositoryError::Corrupted(_))
        ));
    }
}
