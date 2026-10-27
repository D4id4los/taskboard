// SPDX-License-Identifier: MIT OR Apache-2.0
//! The persistence port and its data contract.
//!
//! The port is async-shaped without any async-runtime dependency: methods
//! return [`BoxFuture`] (the same hand-rolled pattern the sync crate uses
//! for its `RetrySleep` seam). One batched write side (`apply`) keeps the
//! contract surface small and stable; Phase 2 wraps the batch in a single
//! sqlite transaction.

use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::entities::{Board, Label, Stack};
use crate::ids::ReminderRuleId;
use crate::outbox::{OpId, PendingOp};
use crate::reminders::{ReminderDelivery, ReminderRef, ReminderSnooze, ReminderState};
use crate::state::{AppState, SyncStates, SyncStatus, derive_sync_states};

/// Hand-rolled boxed future alias: the domain may not depend on
/// `futures`, `tokio`, or `async-trait`.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Local disk-cache port (implemented by `taskboard-storage-sqlite` in
/// Phase 2; faked by `taskboard_domain::test_support::InMemoryRepository`).
pub trait TaskRepository: Send + Sync + std::fmt::Debug {
    /// Full hydration for boot. Returns an empty/default state for a fresh
    /// database.
    ///
    /// # Errors
    ///
    /// [`RepositoryError::Unavailable`] while the backend is unreachable;
    /// [`RepositoryError::Corrupted`] when persisted data fails to decode.
    fn load(&self) -> BoxFuture<'_, Result<PersistedState, RepositoryError>>;

    /// Apply a batch atomically: entity upserts and outbox/validator
    /// transitions land together or fail together.
    ///
    /// # Errors
    ///
    /// [`RepositoryError::Unavailable`] while the backend is unreachable.
    fn apply(&self, actions: Vec<PersistenceAction>) -> BoxFuture<'_, Result<(), RepositoryError>>;
}

/// Persistence-layer failure classes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum RepositoryError {
    /// Backend unreachable/locked/busy — transient, caller may retry.
    #[error("repository unavailable")]
    Unavailable,
    /// Persisted data failed integrity/decoding — not retryable.
    #[error("repository data corrupted")]
    Corrupted,
}

/// The sync actor's read-only view of the engine's persisted state —
/// outbox, bindings, and validators included (ADR 0005: sync reads via the
/// engine, never direct SQL). Implemented by the state crate's engine
/// handle; faked by [`crate::test_support::InMemoryRepository`].
///
/// The replied [`PersistedState`] may carry the memory-only `Syncing`
/// phase transient that no repository `load()` can ever return; consumers
/// must therefore never branch on `sync.phase` — read the outbox,
/// bindings, and validators only.
pub trait SyncStateReader: Send + Sync + std::fmt::Debug {
    /// The full persisted shape at the time the (sequential) engine
    /// processes the read — same durability story as a flush barrier.
    ///
    /// # Errors
    ///
    /// [`RepositoryError::Unavailable`] while the engine is unreachable
    /// (stopped or restarting); [`RepositoryError::Corrupted`] is produced
    /// by implementations whose storage backend can fail to decode.
    fn read_state(&self) -> BoxFuture<'_, Result<PersistedState, RepositoryError>>;
}

/// The full persisted dataset, hydrated at boot.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PersistedState {
    /// Boards.
    pub boards: BTreeMap<crate::ids::BoardId, Board>,
    /// Stacks.
    pub stacks: BTreeMap<crate::ids::StackId, Stack>,
    /// Tasks.
    pub tasks: BTreeMap<crate::ids::TaskId, crate::entities::Task>,
    /// Labels.
    pub labels: BTreeMap<crate::ids::LabelId, Label>,
    /// Pending outbox operations, in queue order.
    pub outbox: Vec<PendingOp>,
    /// Opaque conditional-read validators (mirror of the sync crate's
    /// `Validators`), persisted so polls survive restarts.
    pub validators: BTreeMap<ValidatorKey, SyncValidators>,
    /// Last-known sync status, keyed per board.
    pub sync: SyncStates,
    /// Persisted reminder facts (rules, delivery proofs, snooze links,
    /// muted tasks); delivered-ness and snoozed-ness are plain
    /// set-membership checks.
    pub reminders: ReminderState,
    /// Settled time-tracking history and the open entries (ADR 0017).
    pub tracking: crate::tracking::TrackingState,
}

impl From<PersistedState> for AppState {
    fn from(persisted: PersistedState) -> Self {
        Self {
            boards: persisted.boards,
            stacks: persisted.stacks,
            tasks: persisted.tasks,
            labels: persisted.labels,
            reminders: persisted.reminders,
            tracking: persisted.tracking,
            sync: persisted.sync,
            last_updated: None,
        }
    }
}

/// Which remote resource a validator bundle belongs to.
///
/// Serializes as a string (`"boards"`, `"stacks:<remote-board-id>"`) so it
/// can act as a JSON/YAML map key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ValidatorKey {
    /// The boards listing endpoint.
    Boards,
    /// The active-stacks listing endpoint of one board.
    Stacks(crate::ids::RemoteBoardId),
    /// The archived-stacks listing endpoint of one board (archived cards
    /// sync too — otherwise the completeness contract would tombstone them).
    ArchivedStacks(crate::ids::RemoteBoardId),
}

impl Serialize for ValidatorKey {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Boards => serializer.serialize_str("boards"),
            Self::Stacks(board) => serializer.serialize_str(&format!("stacks:{}", board.get())),
            Self::ArchivedStacks(board) => {
                serializer.serialize_str(&format!("archived_stacks:{}", board.get()))
            }
        }
    }
}

impl<'de> Deserialize<'de> for ValidatorKey {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        if raw == "boards" {
            return Ok(Self::Boards);
        }
        if let Some(num) = raw
            .strip_prefix("archived_stacks:")
            .and_then(|n| n.parse().ok())
        {
            return Ok(Self::ArchivedStacks(crate::ids::RemoteBoardId(num)));
        }
        raw.strip_prefix("stacks:")
            .and_then(|num| num.parse::<u64>().ok())
            .map(|num| Self::Stacks(crate::ids::RemoteBoardId(num)))
            .ok_or_else(|| serde::de::Error::custom("invalid validator key"))
    }
}

/// Opaque conditional-read validators for one endpoint.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncValidators {
    /// `ETag` header value.
    pub etag: Option<String>,
    /// `Last-Modified` header value.
    pub last_modified: Option<String>,
}

/// One write the engine asks the repository to persist. Entity upserts and
/// outbox/validator transitions are batched so they can land in one
/// transaction.
#[allow(clippy::large_enum_variant)] // Task just crossed the threshold (recur rule); boxing churns every match site for a local lint
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PersistenceAction {
    /// Insert or replace a board.
    UpsertBoard(Board),
    /// Insert or replace a stack.
    UpsertStack(Stack),
    /// Insert or replace a task.
    UpsertTask(crate::entities::Task),
    /// Insert or replace a label.
    UpsertLabel(Label),
    /// Append an operation to the outbox.
    EnqueueOp(PendingOp),
    /// Remove a completed (successfully pushed) op.
    CompleteOp(OpId),
    /// Remove a dead-lettered / cancelled op.
    FailOp(OpId),
    /// Store validators for one endpoint.
    UpsertValidators(ValidatorKey, SyncValidators),
    /// Persist the stored part of one board's sync status (phase + last
    /// success). `pending_ops` is NOT stored: implementations derive it at
    /// load time from the outbox attribution, so a stored counter can never
    /// drift from the queue it summarizes.
    UpsertSyncStatus(crate::ids::BoardId, SyncStatus),
    /// Drop one board's sync-status row: the `UnbindBoard` ceremony's
    /// cleanup — a stale `Synced`/`Failed` row for an unbound board
    /// would be a lie the badge could show. Absent row = no-op.
    DeleteSyncStatus(crate::ids::BoardId),
    /// Insert or replace a reminder rule.
    UpsertReminderRule(crate::reminders::ReminderRule),
    /// Remove a rule **and prune its deliveries and snooze links**:
    /// bounded growth without a background GC.
    DeleteReminderRule(ReminderRuleId),
    /// Record a delivery proof. Set-union on the `(reminder, occurrence)`
    /// key: a replayed delivery is a no-op — first write wins (the SQL
    /// primary-key parity; the crash-between-apply-and-notify moment
    /// stays benign).
    RecordReminderDelivery(ReminderDelivery),
    /// Record a user dismissal (R3). Set-union on the `(reminder,
    /// occurrence)` key exactly like a delivery: a replayed dismissal is
    /// a no-op — first write wins (a dismissal is permanent for its
    /// occurrence; there is no un-dismiss).
    RecordReminderDismissal(crate::reminders::ReminderDismissal),
    /// Record a snooze link. Replaces any prior link for the same
    /// `(reminder, occurrence)`.
    RecordReminderSnooze(ReminderSnooze),
    /// Mute (`true`) or unmute (`false`) all firings of a task. The
    /// overdue banner is unaffected by construction (it never reads the
    /// muted set).
    SetTaskRemindersMuted(crate::ids::TaskId, bool),
    /// Insert or replace a standalone alarm.
    UpsertAdhocAlarm(crate::reminders::AdhocAlarm),
    /// Remove an alarm **and prune its `Adhoc`-ref deliveries and
    /// snooze links** (the `DeleteReminderRule` cascade shape):
    /// nothing can re-fire from a pruned map row.
    DeleteAdhocAlarm(crate::ids::AdhocAlarmId),
    /// Insert or replace a time entry.
    UpsertTimeEntry(crate::tracking::TimeEntry),
    /// Remove a time entry (the storage/roaming chunk derives the wire
    /// tombstone at flush).
    DeleteTimeEntry(crate::ids::TimeEntryId),
    /// Clear the local content of one board: stacks, tasks, labels,
    /// outbox entries, sync validators, and reminder facts (rules,
    /// deliveries, snooze links, mutes) of its tasks. The board row
    /// itself (identity, binding, clocks, `remote_seen`) and the sync
    /// status survive. The `ResetBoard` repair's single bulk action.
    ClearBoardContent(crate::ids::BoardId),
}

/// Applies a batch of [`PersistenceAction`]s to a [`PersistedState`] —
/// the executable definition of the port's batch contract, shared by the
/// state engine's memory advance and the repository implementations
/// (fakes and sqlite alike). One semantics, enforced once.
///
/// Per-action semantics: entity upserts replace by id; [`PersistenceAction::
/// EnqueueOp`] appends (an in-place replace on id collision mirrors the
/// sqlite upsert on `op_id` — contract-pinned); `CompleteOp`/`FailOp`
/// remove by op id; `UpsertValidators` replaces the key;
/// `UpsertSyncStatus` sets one board's phase + last success only
/// (`pending_ops` is never stored). Reminder actions: rule upserts
/// replace by id; a rule delete prunes the rule's deliveries, dismissals,
/// and snooze links with it; a delivery or dismissal insert is set-union
/// on the `(reminder, occurrence)` key (first write wins); a snooze link
/// replaces any prior link for the same key; mute toggles membership.
/// Time-entry actions: upserts replace by id, deletes remove the row —
/// history rows have no dependents to prune. A final derivation re-computes
/// `sync.pending_ops` from the outbox depth, and fully-dismissed snooze
/// links are compacted — both derivations run unconditionally after
/// every batch (both this function and `load()` derive them). Alarm
/// rows join the same unconditional derivation: an alarm whose
/// effective occurrence is delivered AND dismissed (the R3 settle) is
/// pruned with its dependents, always after the compaction (the order
/// the composition needs). The sync
/// projection is derived the same way: every board materialized,
/// `pending_ops` attributed from the outbox.
#[allow(clippy::too_many_lines)] // one exhaustive semantics table; the arms ARE the contract
pub fn apply_actions(state: &mut PersistedState, actions: &[PersistenceAction]) {
    for action in actions {
        match action {
            PersistenceAction::UpsertBoard(board) => {
                state.boards.insert(board.id, board.clone());
            }
            PersistenceAction::UpsertStack(stack) => {
                state.stacks.insert(stack.id, stack.clone());
            }
            PersistenceAction::UpsertTask(task) => {
                state.tasks.insert(task.id, task.clone());
            }
            PersistenceAction::UpsertLabel(label) => {
                state.labels.insert(label.id, label.clone());
            }
            PersistenceAction::EnqueueOp(op) => {
                // Re-enqueueing an existing id replaces the entry in place
                // and keeps its queue position (parity with the sqlite
                // upsert on `op_id`, contract-pinned).
                match state.outbox.iter_mut().find(|e| e.op_id == op.op_id) {
                    Some(slot) => *slot = op.clone(),
                    None => state.outbox.push(op.clone()),
                }
            }
            PersistenceAction::CompleteOp(op_id) | PersistenceAction::FailOp(op_id) => {
                state.outbox.retain(|entry| entry.op_id != *op_id);
            }
            PersistenceAction::UpsertValidators(key, validators) => {
                state.validators.insert(*key, validators.clone());
            }
            PersistenceAction::UpsertSyncStatus(board, status) => {
                // The stored part is phase + last success; the row's
                // memory-only fields (sidecar health, capability flags)
                // ride the in-memory map too — they are derivations the
                // engine published with this very batch, never persisted.
                state
                    .sync
                    .set_stored(*board, status.phase, status.last_success);
                if let Some(row) = state.sync.get_mut(*board) {
                    row.sidecar = status.sidecar;
                    row.features = status.features;
                }
            }
            PersistenceAction::DeleteSyncStatus(board) => {
                state.sync.remove(*board);
            }
            PersistenceAction::UpsertReminderRule(rule) => {
                state.reminders.rules.insert(rule.id, rule.clone());
            }
            PersistenceAction::DeleteReminderRule(id) => {
                state.reminders.rules.remove(id);
                state
                    .reminders
                    .deliveries
                    .retain(|d| d.reminder != ReminderRef::Rule(*id));
                state
                    .reminders
                    .dismissals
                    .retain(|d| d.reminder != ReminderRef::Rule(*id));
                state
                    .reminders
                    .snoozes
                    .retain(|key, _| key.reminder != ReminderRef::Rule(*id));
            }
            PersistenceAction::UpsertAdhocAlarm(alarm) => {
                state.reminders.alarms.insert(alarm.id, alarm.clone());
            }
            PersistenceAction::DeleteAdhocAlarm(id) => {
                crate::reminders::remove_alarm(&mut state.reminders, *id);
            }
            PersistenceAction::UpsertTimeEntry(entry) => {
                state.tracking.entries.insert(entry.id, entry.clone());
            }
            PersistenceAction::DeleteTimeEntry(id) => {
                state.tracking.entries.remove(id);
            }
            PersistenceAction::RecordReminderDelivery(delivery) => {
                if !state
                    .reminders
                    .is_delivered(&delivery.reminder, &delivery.occurrence)
                {
                    state.reminders.deliveries.insert(*delivery);
                }
            }
            PersistenceAction::RecordReminderDismissal(dismissal) => {
                if !state
                    .reminders
                    .is_dismissed(&dismissal.reminder, &dismissal.occurrence)
                {
                    state.reminders.dismissals.insert(*dismissal);
                }
            }
            PersistenceAction::RecordReminderSnooze(snooze) => {
                state.reminders.snoozes.insert(
                    crate::reminders::ReminderSnoozeKey {
                        reminder: snooze.reminder,
                        occurrence: snooze.occurrence,
                    },
                    snooze.rescheduled_to,
                );
            }
            PersistenceAction::SetTaskRemindersMuted(task, muted) => {
                if *muted {
                    state.reminders.muted.insert(*task);
                } else {
                    state.reminders.muted.remove(task);
                }
            }
            PersistenceAction::ClearBoardContent(board) => {
                // Scoped to the one board: collect its entity ids first,
                // then drop rows by membership. The board row and the sync
                // status are never touched.
                let board_stacks: Vec<crate::ids::StackId> = state
                    .stacks
                    .iter()
                    .filter(|(_, s)| s.board == *board)
                    .map(|(id, _)| *id)
                    .collect();
                let board_tasks: Vec<crate::ids::TaskId> = state
                    .tasks
                    .iter()
                    .filter(|(_, t)| {
                        state
                            .stacks
                            .get(&t.stack)
                            .is_some_and(|s| s.board == *board)
                    })
                    .map(|(id, _)| *id)
                    .collect();
                let board_labels: Vec<crate::ids::LabelId> = state
                    .labels
                    .iter()
                    .filter(|(_, l)| l.board == *board)
                    .map(|(id, _)| *id)
                    .collect();

                // Reminder facts of the board's tasks: rules prune their
                // deliveries, dismissals, and snooze links (the
                // DeleteReminderRule parity); default-reminder facts and
                // mutes go by task.
                let rule_ids: Vec<crate::ids::ReminderRuleId> = state
                    .reminders
                    .rules
                    .values()
                    .filter(|r| board_tasks.contains(&r.task))
                    .map(|r| r.id)
                    .collect();
                for rule_id in rule_ids {
                    state.reminders.rules.remove(&rule_id);
                    let reminder = crate::reminders::ReminderRef::Rule(rule_id);
                    state
                        .reminders
                        .deliveries
                        .retain(|d| d.reminder != reminder);
                    state
                        .reminders
                        .dismissals
                        .retain(|d| d.reminder != reminder);
                    state
                        .reminders
                        .snoozes
                        .retain(|key, _| key.reminder != reminder);
                }
                for task in &board_tasks {
                    state.reminders.muted.remove(task);
                    let reminder = crate::reminders::ReminderRef::Default(*task);
                    state
                        .reminders
                        .deliveries
                        .retain(|d| d.reminder != reminder);
                    state
                        .reminders
                        .dismissals
                        .retain(|d| d.reminder != reminder);
                    state
                        .reminders
                        .snoozes
                        .retain(|key, _| key.reminder != reminder);
                }

                for task in &board_tasks {
                    state.tasks.remove(task);
                }
                for stack in &board_stacks {
                    state.stacks.remove(stack);
                }
                for label in &board_labels {
                    state.labels.remove(label);
                }
                let board_stacks: std::collections::BTreeSet<crate::ids::StackId> =
                    board_stacks.into_iter().collect();
                let board_labels: std::collections::BTreeSet<crate::ids::LabelId> =
                    board_labels.into_iter().collect();
                let board_tasks: std::collections::BTreeSet<crate::ids::TaskId> =
                    board_tasks.into_iter().collect();
                state.outbox.retain(|entry| {
                    let doomed = match entry.op {
                        crate::outbox::LocalOp::UpdateBoard(id)
                        | crate::outbox::LocalOp::DeleteBoard(id) => id == *board,
                        crate::outbox::LocalOp::CreateTask(id)
                        | crate::outbox::LocalOp::UpdateTask(id)
                        | crate::outbox::LocalOp::MoveTask(id)
                        | crate::outbox::LocalOp::DeleteTask(id)
                        | crate::outbox::LocalOp::AssignLabel(id, _)
                        | crate::outbox::LocalOp::UnassignLabel(id, _)
                        | crate::outbox::LocalOp::UpdateAppData(id) => board_tasks.contains(&id),
                        crate::outbox::LocalOp::CreateStack(id)
                        | crate::outbox::LocalOp::RenameStack(id)
                        | crate::outbox::LocalOp::MoveStack(id)
                        | crate::outbox::LocalOp::DeleteStack(id) => board_stacks.contains(&id),
                        crate::outbox::LocalOp::CreateLabel(id)
                        | crate::outbox::LocalOp::UpdateLabel(id)
                        | crate::outbox::LocalOp::DeleteLabel(id) => board_labels.contains(&id),
                    };
                    !doomed
                });
                state.validators.retain(|key, _| match key {
                    ValidatorKey::Boards => false,
                    // Another board's stack validators are none of this
                    // action's business (multi-board safety; the MVP holds
                    // 0..1 boards, so the bound remote board is the one).
                    ValidatorKey::Stacks(b) | ValidatorKey::ArchivedStacks(b) => {
                        Some(*b)
                            != state
                                .boards
                                .get(board)
                                .and_then(|row| row.remote.as_ref())
                                .map(|r| r.board)
                    }
                });
            }
        }
    }
    // A snooze link whose source and target occurrences are both
    // dismissed (the R3 settle) can never influence evaluation again —
    // both endpoints are dead in both readers. Pruning it bounds chain
    // growth with zero behavioral change (the compaction-unobservability
    // property, both readers). Delivered-ness is not a criterion: a
    // delivered-but-undismissed occurrence is the live in-app surface,
    // and pruning its link would resurrect a dismissed notification.
    crate::reminders::compact_snooze_chains(&mut state.reminders);
    // An alarm whose effective occurrence is delivered AND dismissed is
    // settled — prune it (with its deliveries, dismissals, and snooze
    // links). Between fire and dismiss the row lives: the in-app
    // surface's alarm arm. The caller-order is load-bearing: compaction
    // first lets a just-settled chain revert to its base before the
    // check reads it (the composition pinned by the mis-order catcher).
    crate::reminders::prune_settled_alarms(&mut state.reminders);
    // Derived, never tracked: after every batch the per-board pending
    // counts are re-derived from the outbox they summarize.
    derive_sync_states(
        &state.boards,
        &state.stacks,
        &state.tasks,
        &state.labels,
        &state.outbox,
        &mut state.sync,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support;
    use proptest::prelude::*;
    use std::collections::BTreeSet;

    use chrono::{DateTime, TimeZone, Utc};

    fn ts(secs: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(secs, 0).unwrap()
    }

    fn task_with_id(raw: u128) -> crate::entities::Task {
        let id = crate::ids::TaskId::from(uuid::Uuid::from_u128(raw));
        crate::entities::Task {
            id,
            remote: None,
            title: format!("task {raw}"),
            description: String::new(),
            duedate: None,
            done: None,
            scheduled: None,
            stack: crate::ids::StackId::from(uuid::Uuid::from_u128(1)),
            order: 0,
            labels: BTreeSet::default(),
            effort: None,
            importance: None,
            urgency: None,
            recur: None,

            archived: false,
            deleted: false,
            clocks: crate::entities::TaskClocks {
                title: ts(100),
                description: ts(100),
                duedate: ts(100),
                scheduled: ts(100),
                done: ts(100),
                position: ts(100),
                labels: ts(100),
                effort: ts(100),
                importance: ts(100),
                urgency: ts(100),
                recur: ts(100),

                archived: ts(100),
                deleted: ts(100),
            },
            remote_seen: None,
            remote_baseline: None,
        }
    }

    fn op(raw: u128, target: crate::ids::TaskId) -> PendingOp {
        PendingOp {
            op_id: OpId(uuid::Uuid::from_u128(raw)),
            op: crate::outbox::LocalOp::UpdateTask(target),
            queued_at: ts(200),
        }
    }

    #[test]
    fn entity_upserts_replace_by_id() {
        let mut state = PersistedState::default();
        let task = task_with_id(1);
        apply_actions(&mut state, &[PersistenceAction::UpsertTask(task.clone())]);
        let mut renamed = task.clone();
        renamed.title = "renamed".into();
        apply_actions(&mut state, &[PersistenceAction::UpsertTask(renamed)]);
        assert_eq!(state.tasks.len(), 1, "replace, not append");
        assert_eq!(state.tasks[&task.id].title, "renamed");
    }

    #[test]
    fn enqueue_appends_and_reenqueue_keeps_queue_position() {
        let mut state = PersistedState::default();
        let task = task_with_id(1);
        let first = op(10, task.id);
        let second = op(11, task.id);
        apply_actions(
            &mut state,
            &[
                PersistenceAction::EnqueueOp(first.clone()),
                PersistenceAction::EnqueueOp(second.clone()),
            ],
        );
        assert_eq!(state.outbox, vec![first.clone(), second.clone()]);

        // Same id, new payload: replaces in place, keeps its position.
        let mut replaced = first.clone();
        replaced.queued_at = ts(300);
        apply_actions(
            &mut state,
            &[PersistenceAction::EnqueueOp(replaced.clone())],
        );
        assert_eq!(state.outbox, vec![replaced, second]);
    }

    #[test]
    fn complete_and_fail_remove_by_op_id() {
        let mut state = PersistedState::default();
        let task = task_with_id(1);
        let a = op(10, task.id);
        let b = op(11, task.id);
        apply_actions(
            &mut state,
            &[
                PersistenceAction::EnqueueOp(a.clone()),
                PersistenceAction::EnqueueOp(b.clone()),
                PersistenceAction::CompleteOp(a.op_id),
                PersistenceAction::FailOp(b.op_id),
            ],
        );
        assert!(state.outbox.is_empty(), "both removals land");
    }

    #[test]
    fn validators_upsert_and_status_write_partial_fields() {
        let mut state = PersistedState::default();
        let key = ValidatorKey::Boards;
        apply_actions(
            &mut state,
            &[PersistenceAction::UpsertValidators(
                key,
                SyncValidators {
                    etag: Some("v1".into()),
                    last_modified: None,
                },
            )],
        );
        apply_actions(
            &mut state,
            &[PersistenceAction::UpsertValidators(
                key,
                SyncValidators {
                    etag: None,
                    last_modified: Some("lm".into()),
                },
            )],
        );
        assert_eq!(
            state.validators[&key],
            SyncValidators {
                etag: None,
                last_modified: Some("lm".into())
            },
            "upsert replaces the whole bundle"
        );

        let board = crate::ids::BoardId::from(uuid::Uuid::from_u128(71));
        state
            .sync
            .set_stored(board, SyncStatus::default().phase, None);
        apply_actions(
            &mut state,
            &[PersistenceAction::UpsertSyncStatus(
                board,
                SyncStatus {
                    phase: crate::state::SyncPhase::Offline,
                    last_success: Some(ts(400)),
                    pending_ops: 9999, // ignored: derived, never stored
                    counters: crate::state::SyncCounters::default(),
                    ..SyncStatus::default()
                },
            )],
        );
        let stored = state.sync.get(board).expect("board row");
        assert_eq!(stored.phase, crate::state::SyncPhase::Offline);
        assert_eq!(stored.last_success, Some(ts(400)));
        assert_eq!(
            stored.pending_ops, 0,
            "the counter is re-derived from the (empty) outbox after the batch"
        );
    }

    #[test]
    fn delete_sync_status_removes_only_that_row() {
        let mut state = PersistedState::default();
        let known = crate::ids::BoardId::from(uuid::Uuid::from_u128(71));
        let orphan = crate::ids::BoardId::from(uuid::Uuid::from_u128(72));
        // The known board exists in the maps (its row re-materializes as
        // a default after the delete); the orphan has no board row at
        // all (its row is dropped wholesale).
        state.boards.insert(
            known,
            crate::entities::Board {
                id: known,
                remote: None,
                title: "board".into(),
                color: crate::entities::Color::new("0000ff"),
                archived: false,
                deleted: false,
                clocks: crate::entities::BoardClocks {
                    title: crate::entities::BoardClocks::epoch(),
                    color: crate::entities::BoardClocks::epoch(),
                    archived: crate::entities::BoardClocks::epoch(),
                    deleted: crate::entities::BoardClocks::epoch(),
                },
                remote_seen: None,
            },
        );
        for board in [known, orphan] {
            apply_actions(
                &mut state,
                &[PersistenceAction::UpsertSyncStatus(
                    board,
                    SyncStatus {
                        phase: crate::state::SyncPhase::Failed {
                            last_error: crate::state::SyncErrorKind::Network,
                        },
                        last_success: Some(ts(400)),
                        pending_ops: 0,
                        counters: crate::state::SyncCounters::default(),
                        ..SyncStatus::default()
                    },
                )],
            );
        }
        apply_actions(&mut state, &[PersistenceAction::DeleteSyncStatus(known)]);
        // The deleted board is known to the maps, so the derivation
        // re-materializes a default row — the *stored* part (phase,
        // last success) is what the delete clears.
        let known_row = state.sync.get(known).expect("materialized default row");
        assert_eq!(known_row.phase, crate::state::SyncPhase::Idle);
        assert_eq!(known_row.last_success, None);
        // A row of a board absent from the maps survives untouched —
        // the delete removes only the named board's row.
        let orphan_row = state.sync.get(orphan).expect("only the named row goes");
        assert_eq!(
            orphan_row.phase,
            crate::state::SyncPhase::Failed {
                last_error: crate::state::SyncErrorKind::Network
            }
        );
        assert_eq!(orphan_row.last_success, Some(ts(400)));
    }

    #[test]
    fn pending_ops_are_rederived_from_the_outbox_after_every_batch() {
        let mut state = PersistedState::default();
        let task = task_with_id(1);
        // The stack and task the ops target must exist: attribution runs
        // through the entity graph (a torn state cannot attribute).
        let stack = crate::entities::Stack {
            id: task.stack,
            remote: None,
            board: crate::ids::BoardId::from(uuid::Uuid::from_u128(9)),
            title: "stack".into(),
            order: 0,
            archived: false,
            deleted: false,
            clocks: crate::entities::StackClocks {
                title: ts(100),
                order: ts(100),
                deleted: ts(100),
            },
            remote_seen: None,
        };
        state.stacks.insert(stack.id, stack);
        state.tasks.insert(task.id, task.clone());
        apply_actions(
            &mut state,
            &[
                PersistenceAction::EnqueueOp(op(10, task.id)),
                PersistenceAction::EnqueueOp(op(11, task.id)),
            ],
        );
        assert_eq!(state.sync.aggregate().pending_ops, 2);
        apply_actions(
            &mut state,
            &[PersistenceAction::CompleteOp(OpId(uuid::Uuid::from_u128(
                10,
            )))],
        );
        assert_eq!(state.sync.aggregate().pending_ops, 1);
    }

    #[test]
    #[allow(clippy::too_many_lines)] // the fixture IS the scope proof
    fn clear_board_content_clears_exactly_the_board_scope() {
        use crate::entities::{Board, Label, Stack};
        use crate::ids::{BoardId, LabelId, StackId};
        let board = BoardId::from(uuid::Uuid::from_u128(11));
        let other = BoardId::from(uuid::Uuid::from_u128(12));
        let stack = StackId::from(uuid::Uuid::from_u128(21));
        let other_stack = StackId::from(uuid::Uuid::from_u128(22));
        let task = crate::ids::TaskId::from(uuid::Uuid::from_u128(31));
        let other_task = crate::ids::TaskId::from(uuid::Uuid::from_u128(32));
        let label = LabelId::from(uuid::Uuid::from_u128(41));
        let clocks = crate::entities::BoardClocks {
            title: ts(100),
            color: ts(100),
            archived: ts(100),
            deleted: ts(100),
        };
        let board_row = |id, remote| Board {
            id,
            remote,
            title: format!("board {id:?}"),
            color: crate::entities::Color::new("0000ff"),
            archived: false,
            deleted: false,
            clocks,
            remote_seen: None,
        };
        let stack_row = |id, b| Stack {
            id,
            remote: None,
            board: b,
            title: format!("stack {id:?}"),
            order: 0,
            archived: false,
            deleted: false,
            clocks: crate::entities::StackClocks {
                title: ts(100),
                order: ts(100),
                deleted: ts(100),
            },
            remote_seen: None,
        };
        let task_row = |id, s| crate::entities::Task {
            id,
            remote: None,
            title: format!("task {id:?}"),
            description: String::new(),
            duedate: None,
            done: None,
            scheduled: None,
            stack: s,
            order: 0,
            labels: BTreeSet::new(),
            effort: None,
            importance: None,
            urgency: None,
            recur: None,

            archived: false,
            deleted: false,
            clocks: crate::entities::TaskClocks {
                title: ts(100),
                description: ts(100),
                duedate: ts(100),
                scheduled: ts(100),
                done: ts(100),
                position: ts(100),
                labels: ts(100),
                effort: ts(100),
                importance: ts(100),
                urgency: ts(100),
                recur: ts(100),

                archived: ts(100),
                deleted: ts(100),
            },
            remote_seen: None,
            remote_baseline: None,
        };
        let label_row = |id, b| Label {
            id,
            remote: None,
            board: b,
            title: format!("label {id:?}"),
            color: crate::entities::Color::new("00ff00"),
            deleted: false,
            clocks: crate::entities::LabelClocks {
                title: ts(100),
                color: ts(100),
                deleted: ts(100),
            },
            remote_seen: None,
        };
        let rule = crate::reminders::ReminderRule {
            id: crate::ids::ReminderRuleId::from(uuid::Uuid::from_u128(51)),
            task,
            offset: crate::reminders::ReminderOffset::at_due(),
            created_at: ts(100),
            updated_at: ts(100),
        };
        let mut state = PersistedState::default();
        apply_actions(
            &mut state,
            &[
                // The doomed board (bound), a second (untouched) board.
                PersistenceAction::UpsertBoard(board_row(
                    board,
                    crate::test_support::test_binding(crate::ids::RemoteBoardId(7)),
                )),
                PersistenceAction::UpsertBoard(board_row(
                    other,
                    crate::test_support::test_binding(crate::ids::RemoteBoardId(8)),
                )),
                PersistenceAction::UpsertStack(stack_row(stack, board)),
                PersistenceAction::UpsertStack(stack_row(other_stack, other)),
                PersistenceAction::UpsertTask(task_row(task, stack)),
                PersistenceAction::UpsertTask(task_row(other_task, other_stack)),
                PersistenceAction::UpsertLabel(label_row(label, board)),
                PersistenceAction::UpsertReminderRule(rule.clone()),
                PersistenceAction::RecordReminderDelivery(crate::reminders::ReminderDelivery {
                    reminder: crate::reminders::ReminderRef::Rule(rule.id),
                    occurrence: ts(1_000),
                    delivered_at: ts(1_001),
                }),
                PersistenceAction::RecordReminderDelivery(crate::reminders::ReminderDelivery {
                    reminder: crate::reminders::ReminderRef::Default(task),
                    occurrence: ts(2_000),
                    delivered_at: ts(2_001),
                }),
                PersistenceAction::RecordReminderDismissal(crate::reminders::ReminderDismissal {
                    reminder: crate::reminders::ReminderRef::Rule(rule.id),
                    occurrence: ts(1_000),
                    dismissed_at: ts(1_002),
                }),
                PersistenceAction::RecordReminderDismissal(crate::reminders::ReminderDismissal {
                    reminder: crate::reminders::ReminderRef::Default(task),
                    occurrence: ts(2_000),
                    dismissed_at: ts(2_002),
                }),
                PersistenceAction::SetTaskRemindersMuted(task, true),
                // Ops: the board's own, its children's, and the other
                // board's (untouched).
                PersistenceAction::EnqueueOp(crate::outbox::PendingOp {
                    op_id: OpId(uuid::Uuid::from_u128(61)),
                    op: crate::outbox::LocalOp::UpdateBoard(board),
                    queued_at: ts(200),
                }),
                PersistenceAction::EnqueueOp(crate::outbox::PendingOp {
                    op_id: OpId(uuid::Uuid::from_u128(62)),
                    op: crate::outbox::LocalOp::UpdateTask(task),
                    queued_at: ts(200),
                }),
                PersistenceAction::EnqueueOp(crate::outbox::PendingOp {
                    op_id: OpId(uuid::Uuid::from_u128(63)),
                    op: crate::outbox::LocalOp::DeleteStack(stack),
                    queued_at: ts(200),
                }),
                PersistenceAction::EnqueueOp(crate::outbox::PendingOp {
                    op_id: OpId(uuid::Uuid::from_u128(64)),
                    op: crate::outbox::LocalOp::DeleteLabel(label),
                    queued_at: ts(200),
                }),
                PersistenceAction::EnqueueOp(crate::outbox::PendingOp {
                    op_id: OpId(uuid::Uuid::from_u128(65)),
                    op: crate::outbox::LocalOp::UpdateTask(other_task),
                    queued_at: ts(200),
                }),
                PersistenceAction::UpsertValidators(
                    ValidatorKey::Boards,
                    SyncValidators::default(),
                ),
                PersistenceAction::UpsertValidators(
                    ValidatorKey::Stacks(crate::ids::RemoteBoardId(7)),
                    SyncValidators::default(),
                ),
                PersistenceAction::UpsertValidators(
                    ValidatorKey::Stacks(crate::ids::RemoteBoardId(8)),
                    SyncValidators::default(),
                ),
                PersistenceAction::UpsertSyncStatus(
                    board,
                    SyncStatus {
                        phase: crate::state::SyncPhase::Idle,
                        last_success: Some(ts(300)),
                        pending_ops: 0,
                        counters: crate::state::SyncCounters::default(),
                        ..SyncStatus::default()
                    },
                ),
            ],
        );

        apply_actions(&mut state, &[PersistenceAction::ClearBoardContent(board)]);

        assert!(
            state.stacks.contains_key(&other_stack),
            "the other board's stack survives"
        );
        assert!(
            state.tasks.contains_key(&other_task),
            "the other board's task survives"
        );
        assert!(
            !state.stacks.contains_key(&stack)
                && !state.tasks.contains_key(&task)
                && !state.labels.contains_key(&label),
            "the board's content rows are gone"
        );
        assert!(
            state.boards.contains_key(&board),
            "the board row (identity, binding, clocks) survives"
        );
        let remaining_ops: Vec<_> = state.outbox.iter().map(|o| o.op_id.0).collect();
        assert_eq!(
            remaining_ops,
            vec![uuid::Uuid::from_u128(65)],
            "only the other board's op survives"
        );
        assert!(
            !state.validators.contains_key(&ValidatorKey::Boards)
                && !state
                    .validators
                    .contains_key(&ValidatorKey::Stacks(crate::ids::RemoteBoardId(7))),
            "the board's validators are cleared"
        );
        assert!(
            state
                .validators
                .contains_key(&ValidatorKey::Stacks(crate::ids::RemoteBoardId(8))),
            "the other board's validators survive"
        );
        assert!(state.reminders.rules.is_empty());
        assert!(state.reminders.deliveries.is_empty());
        assert!(
            state.reminders.dismissals.is_empty(),
            "the board's dismissal facts clear with its content"
        );
        assert!(!state.reminders.muted.contains(&task));
        assert_eq!(
            state.sync.aggregate().last_success,
            Some(ts(300)),
            "sync status survives"
        );
        assert_eq!(state.sync.aggregate().pending_ops, 1);
    }

    proptest! {
        #![proptest_config(test_support::proptest_config(256))]

        #[test]
        fn empty_batch_is_identity(state in test_support::persisted_state_strategy()) {
            // Identity is claimed on invariant-satisfying states: normalize
            // the derived projection by running the (empty) batch once.
            let mut expected = state.clone();
            apply_actions(&mut expected, &[]);
            let mut got = state;
            apply_actions(&mut got, &[]);
            prop_assert_eq!(got, expected);
        }
    }

    // ---- Reminder actions (chunk-1 semantics, defined once here) ------

    fn time_entry(raw: u128, start: i64, end: Option<i64>) -> crate::tracking::TimeEntry {
        crate::tracking::TimeEntry {
            id: crate::ids::TimeEntryId::from(uuid::Uuid::from_u128(raw)),
            task: crate::ids::TaskId::from(uuid::Uuid::from_u128(900)),
            task_title: "task 900".into(),
            start: ts(start),
            end: end.map(ts),
            created_at: ts(100),
            updated_at: ts(100),
        }
    }

    #[test]
    fn upsert_time_entry_replaces_by_id() {
        let mut state = PersistedState::default();
        let entry = time_entry(1, 1_000, None);
        apply_actions(
            &mut state,
            &[PersistenceAction::UpsertTimeEntry(entry.clone())],
        );
        let mut stamped = entry.clone();
        stamped.end = Some(ts(2_000));
        stamped.updated_at = ts(2_500);
        apply_actions(&mut state, &[PersistenceAction::UpsertTimeEntry(stamped)]);
        assert_eq!(state.tracking.entries.len(), 1, "replace, not append");
        assert_eq!(
            state.tracking.entries[&entry.id].end,
            Some(ts(2_000)),
            "the second write wins"
        );
    }

    #[test]
    fn delete_time_entry_removes_the_row() {
        let mut state = PersistedState::default();
        let entry = time_entry(1, 1_000, Some(2_000));
        apply_actions(&mut state, &[PersistenceAction::UpsertTimeEntry(entry)]);
        apply_actions(
            &mut state,
            &[PersistenceAction::DeleteTimeEntry(
                crate::ids::TimeEntryId::from(uuid::Uuid::from_u128(1)),
            )],
        );
        assert!(state.tracking.entries.is_empty(), "the row is gone");
        // Removing an absent row is fine (the repeat is benign).
        apply_actions(
            &mut state,
            &[PersistenceAction::DeleteTimeEntry(
                crate::ids::TimeEntryId::from(uuid::Uuid::from_u128(1)),
            )],
        );
    }

    fn reminder_rule(raw: u128) -> crate::reminders::ReminderRule {
        crate::reminders::ReminderRule {
            id: crate::ids::ReminderRuleId::from(uuid::Uuid::from_u128(raw)),
            task: crate::ids::TaskId::from(uuid::Uuid::from_u128(raw + 1)),
            offset: crate::reminders::ReminderOffset::at_due(),
            created_at: ts(100),
            updated_at: ts(100),
        }
    }

    #[test]
    fn reminder_rule_upsert_replaces_by_id_and_delete_prunes_dependents() {
        let mut state = PersistedState::default();
        let rule = reminder_rule(1);
        apply_actions(
            &mut state,
            &[PersistenceAction::UpsertReminderRule(rule.clone())],
        );
        let mut renamed = rule.clone();
        renamed.offset =
            crate::reminders::ReminderOffset::before(chrono::TimeDelta::try_minutes(-5).unwrap())
                .unwrap();
        apply_actions(
            &mut state,
            &[PersistenceAction::UpsertReminderRule(renamed)],
        );
        assert_eq!(state.reminders.rules.len(), 1, "replace, not append");

        let r = crate::reminders::ReminderRef::Rule(rule.id);
        let task_ref = crate::reminders::ReminderRef::Default(rule.task);
        apply_actions(
            &mut state,
            &[
                PersistenceAction::RecordReminderDelivery(crate::reminders::ReminderDelivery {
                    reminder: r,
                    occurrence: ts(1_000),
                    delivered_at: ts(1_001),
                }),
                PersistenceAction::RecordReminderSnooze(crate::reminders::ReminderSnooze {
                    reminder: r,
                    occurrence: ts(2_000),
                    rescheduled_to: ts(3_000),
                }),
                PersistenceAction::RecordReminderDismissal(crate::reminders::ReminderDismissal {
                    reminder: r,
                    occurrence: ts(1_000),
                    dismissed_at: ts(1_002),
                }),
                // A default-reminder row must survive the rule delete.
                PersistenceAction::RecordReminderDelivery(crate::reminders::ReminderDelivery {
                    reminder: task_ref,
                    occurrence: ts(1_000),
                    delivered_at: ts(1_001),
                }),
                PersistenceAction::RecordReminderDismissal(crate::reminders::ReminderDismissal {
                    reminder: task_ref,
                    occurrence: ts(1_000),
                    dismissed_at: ts(1_002),
                }),
            ],
        );
        apply_actions(
            &mut state,
            &[PersistenceAction::DeleteReminderRule(rule.id)],
        );
        assert!(state.reminders.rules.is_empty());
        assert_eq!(
            state.reminders.deliveries.len(),
            1,
            "only the default-reminder delivery survives"
        );
        assert_eq!(
            state.reminders.dismissals.len(),
            1,
            "only the default-reminder dismissal survives"
        );
        assert!(state.reminders.snoozes.is_empty());
    }

    #[test]
    fn reminder_dismissal_replay_is_a_no_op_first_write_wins() {
        let mut state = PersistedState::default();
        let r = crate::reminders::ReminderRef::Default(crate::ids::TaskId::from(
            uuid::Uuid::from_u128(5),
        ));
        let first = crate::reminders::ReminderDismissal {
            reminder: r,
            occurrence: ts(1_000),
            dismissed_at: ts(1_002),
        };
        let replay = crate::reminders::ReminderDismissal {
            reminder: r,
            occurrence: ts(1_000),
            dismissed_at: ts(9_999),
        };
        apply_actions(
            &mut state,
            &[
                PersistenceAction::RecordReminderDismissal(first),
                PersistenceAction::RecordReminderDismissal(replay),
            ],
        );
        assert_eq!(
            state
                .reminders
                .dismissals
                .iter()
                .copied()
                .collect::<Vec<_>>(),
            vec![first],
            "the replay must not replace the first dismissal"
        );
    }

    #[test]
    fn reminder_delivery_replay_is_a_no_op_first_write_wins() {
        let mut state = PersistedState::default();
        let r = crate::reminders::ReminderRef::Default(crate::ids::TaskId::from(
            uuid::Uuid::from_u128(5),
        ));
        let first = crate::reminders::ReminderDelivery {
            reminder: r,
            occurrence: ts(1_000),
            delivered_at: ts(1_001),
        };
        let replay = crate::reminders::ReminderDelivery {
            reminder: r,
            occurrence: ts(1_000),
            delivered_at: ts(9_999),
        };
        apply_actions(
            &mut state,
            &[
                PersistenceAction::RecordReminderDelivery(first),
                PersistenceAction::RecordReminderDelivery(replay),
            ],
        );
        assert_eq!(
            state
                .reminders
                .deliveries
                .iter()
                .copied()
                .collect::<Vec<_>>(),
            vec![first],
            "the replay must not replace the first proof"
        );
    }

    #[test]
    fn reminder_snooze_replaces_the_prior_link_for_the_same_key() {
        let mut state = PersistedState::default();
        let r = crate::reminders::ReminderRef::Default(crate::ids::TaskId::from(
            uuid::Uuid::from_u128(5),
        ));
        let first = crate::reminders::ReminderSnooze {
            reminder: r,
            occurrence: ts(1_000),
            rescheduled_to: ts(2_000),
        };
        let second = crate::reminders::ReminderSnooze {
            reminder: r,
            occurrence: ts(1_000),
            rescheduled_to: ts(3_000),
        };
        apply_actions(
            &mut state,
            &[
                PersistenceAction::RecordReminderSnooze(first),
                PersistenceAction::RecordReminderSnooze(second),
            ],
        );
        assert_eq!(
            state
                .reminders
                .snoozes
                .get(&crate::reminders::ReminderSnoozeKey {
                    reminder: r,
                    occurrence: ts(1_000),
                }),
            Some(&ts(3_000))
        );
        assert_eq!(state.reminders.snoozes.len(), 1);
    }

    #[test]
    fn task_mute_toggles_membership_without_touching_the_banner_inputs() {
        let mut state = PersistedState::default();
        let task = crate::ids::TaskId::from(uuid::Uuid::from_u128(5));
        apply_actions(
            &mut state,
            &[
                PersistenceAction::SetTaskRemindersMuted(task, true),
                PersistenceAction::SetTaskRemindersMuted(task, true),
            ],
        );
        assert_eq!(state.reminders.muted.len(), 1, "set is idempotent");
        apply_actions(
            &mut state,
            &[PersistenceAction::SetTaskRemindersMuted(task, false)],
        );
        assert!(state.reminders.muted.is_empty());
    }

    /// Re-timed 2026-10-26 (#474): delivery-keyed compaction was the
    /// pre-R3 contract; DR-1 R3
    /// (`.artifacts/plans/2026-10-25-reminder-rule-sync-design-round-plan.md`
    /// §4-R3) makes dismissal the settle. The persistence half of the
    /// old pin is intact: the compaction still lands in the same
    /// post-pass batch, keyed on both endpoints dismissed.
    #[test]
    fn snooze_compaction_prunes_only_fully_dismissed_links() {
        let mut state = PersistedState::default();
        let r = crate::reminders::ReminderRef::Default(crate::ids::TaskId::from(
            uuid::Uuid::from_u128(5),
        ));
        let snooze = |from: i64, to: i64| crate::reminders::ReminderSnooze {
            reminder: r,
            occurrence: ts(from),
            rescheduled_to: ts(to),
        };
        let delivery = |at: i64| crate::reminders::ReminderDelivery {
            reminder: r,
            occurrence: ts(at),
            delivered_at: ts(at + 1),
        };
        let dismissal = |at: i64| crate::reminders::ReminderDismissal {
            reminder: r,
            occurrence: ts(at),
            dismissed_at: ts(at + 2),
        };
        // Pruned: both endpoints delivered AND dismissed.
        let settled = snooze(1_000, 2_000);
        // Pruned: both endpoints dismissed, deliveries irrelevant now —
        // an undelivered-∧-dismissed link is equally dead in both readers.
        let settled_undelivered = snooze(3_000, 4_000);
        // Kept: both endpoints delivered but UNDISMISSED — the old
        // contract's "settled" link, now the live-surface discriminator.
        let live_delivered = snooze(5_000, 6_000);
        // Kept: only the source dismissed.
        let source_dismissed = snooze(7_000, 8_000);
        // Kept: only the target dismissed.
        let target_dismissed = snooze(9_000, 10_000);
        apply_actions(
            &mut state,
            &[
                PersistenceAction::RecordReminderSnooze(settled),
                PersistenceAction::RecordReminderSnooze(settled_undelivered),
                PersistenceAction::RecordReminderSnooze(live_delivered),
                PersistenceAction::RecordReminderSnooze(source_dismissed),
                PersistenceAction::RecordReminderSnooze(target_dismissed),
                PersistenceAction::RecordReminderDelivery(delivery(1_000)),
                PersistenceAction::RecordReminderDelivery(delivery(2_000)),
                PersistenceAction::RecordReminderDelivery(delivery(5_000)),
                PersistenceAction::RecordReminderDelivery(delivery(6_000)),
                PersistenceAction::RecordReminderDelivery(delivery(8_000)),
                PersistenceAction::RecordReminderDismissal(dismissal(1_000)),
                PersistenceAction::RecordReminderDismissal(dismissal(2_000)),
                PersistenceAction::RecordReminderDismissal(dismissal(3_000)),
                PersistenceAction::RecordReminderDismissal(dismissal(4_000)),
                PersistenceAction::RecordReminderDismissal(dismissal(7_000)),
                PersistenceAction::RecordReminderDismissal(dismissal(10_000)),
            ],
        );
        let key = |at: i64| crate::reminders::ReminderSnoozeKey {
            reminder: r,
            occurrence: ts(at),
        };
        assert!(
            !state.reminders.snoozes.contains_key(&key(1_000))
                && !state.reminders.snoozes.contains_key(&key(3_000)),
            "fully-dismissed links prune in the same post-pass, delivered or not"
        );
        assert!(
            state.reminders.snoozes.contains_key(&key(5_000))
                && state.reminders.snoozes.contains_key(&key(7_000))
                && state.reminders.snoozes.contains_key(&key(9_000)),
            "a link with any undismissed endpoint still influences the walk"
        );
    }

    proptest! {
        #![proptest_config(test_support::proptest_config(256))]

        #[test]
        fn p5_settlement_roundtrip_through_apply_actions(
            (app, reminders) in test_support::app_state_strategy()
                .prop_map(|app| (app.tasks.clone(), app.reminders.clone())),
            offset in test_support::reminder_offset_strategy(),
            now in test_support::timestamp_strategy(),
            snooze_mask in any::<u64>(),
            mute_mask in any::<u64>(),
        ) {
            let defaults = crate::reminders::ReminderDefaults { default_before: offset };
            let target = now + chrono::TimeDelta::try_minutes(10).unwrap();
            let f1 = crate::reminders::due_firings(&app, &reminders, &defaults, now);
            let mut actions = Vec::new();
            let mut batch_snoozed = std::collections::BTreeSet::new();
            for (i, f) in f1.iter().enumerate() {
                actions.push(PersistenceAction::RecordReminderDelivery(
                    crate::reminders::ReminderDelivery {
                        reminder: f.reminder,
                        occurrence: f.occurrence,
                        delivered_at: now,
                    },
                ));
                // Snooze a subset with a strictly future target: the
                // delivery settles the original occurrence, the link
                // moves the next one.
                if (snooze_mask >> i) & 1 == 1 {
                    batch_snoozed.insert(f.reminder);
                    actions.push(PersistenceAction::RecordReminderSnooze(
                        crate::reminders::ReminderSnooze {
                            reminder: f.reminder,
                            occurrence: f.occurrence,
                            rescheduled_to: target,
                        },
                    ));
                }
            }
            for (i, task_id) in app.keys().enumerate() {
                if (mute_mask >> (i % 64)) & 1 == 1 {
                    actions.push(PersistenceAction::SetTaskRemindersMuted(*task_id, true));
                }
            }
            let mut state = PersistedState {
                reminders,
                ..PersistedState::default()
            };
            apply_actions(&mut state, &actions);
            let settled = &state.reminders;

            // (a) nothing fires again at `now`...
            let f2 = crate::reminders::due_firings(&app, settled, &defaults, now);
            prop_assert!(f2.is_empty(), "settled occurrences must not refire");
            // (b) reminders THIS batch snoozed stay quiet below their
            // (now + 10 m) target. Only the batch's own links qualify as
            // the suppressed set: a generated link's target may land
            // inside the probe window, and firing exactly at a link's
            // target is legal (P7) — a reminder-identity proxy over all
            // links would forbid that legal fire.
            let probe = now + chrono::TimeDelta::try_minutes(9).unwrap();
            let at_probe = crate::reminders::due_firings(&app, settled, &defaults, probe);
            prop_assert!(
                at_probe
                    .iter()
                    .all(|f| !batch_snoozed.contains(&f.reminder) || f.occurrence >= target),
                "a batch-snoozed reminder must not fire below its target"
            );
            // (c) muted tasks contribute zero task firings.
            let fired_tasks: std::collections::BTreeSet<_> =
                crate::reminders::due_firings(&app, settled, &defaults, now + chrono::TimeDelta::try_minutes(20).unwrap())
                    .into_iter()
                    .filter_map(|f| match f.anchor {
                        crate::reminders::ReminderAnchor::Task(t) => Some(t),
                        crate::reminders::ReminderAnchor::Alarm(_) => None,
                    })
                    .collect();
            prop_assert!(fired_tasks.iter().all(|t| !settled.muted.contains(t)));
            // (d) persisted invariants: <= 1 delivery and <= 1 dismissal
            // per key; compacted (a surviving link has at least one
            // undismissed endpoint — the dismissal-keyed criterion).
            let keys: std::collections::BTreeSet<_> = settled
                .deliveries
                .iter()
                .map(|d| (d.reminder, d.occurrence))
                .collect();
            prop_assert_eq!(keys.len(), settled.deliveries.len());
            let dismissal_keys: std::collections::BTreeSet<_> = settled
                .dismissals
                .iter()
                .map(|d| (d.reminder, d.occurrence))
                .collect();
            prop_assert_eq!(dismissal_keys.len(), settled.dismissals.len());
            for key in settled.snoozes.keys() {
                prop_assert!(
                    !settled.is_dismissed(&key.reminder, &key.occurrence)
                        || !settled.is_dismissed(&key.reminder, &settled.snoozes[key])
                );
            }
        }

        #[test]
        fn compaction_is_unobservable_in_due_firings(
            (app, reminders) in test_support::app_state_strategy()
                .prop_map(|app| (app.tasks.clone(), app.reminders.clone())),
            offset in test_support::reminder_offset_strategy(),
            now in test_support::timestamp_strategy(),
        ) {
            let defaults = crate::reminders::ReminderDefaults { default_before: offset };
            let mut normalized = PersistedState {
                reminders: reminders.clone(),
                ..PersistedState::default()
            };
            apply_actions(&mut normalized, &[]);
            let got_plain = crate::reminders::due_firings(&app, &reminders, &defaults, now);
            let got_compacted =
                crate::reminders::due_firings(&app, &normalized.reminders, &defaults, now);
            prop_assert_eq!(got_plain, got_compacted);
            // The second reader (the resurrection hole): the in-app
            // surface must be equally untouched by the post-pass.
            prop_assert_eq!(
                crate::reminders::active_notifications(&app, &reminders, &defaults, now),
                crate::reminders::active_notifications(&app, &normalized.reminders, &defaults, now),
                "compaction never changes the active surface",
            );
        }

        #[test]
        fn compaction_is_unobservable_on_hand_uncompacted_states(
            (app, reminders) in test_support::app_state_strategy()
                .prop_map(|app| (app.tasks.clone(), app.reminders.clone())),
            offset in test_support::reminder_offset_strategy(),
            now in test_support::timestamp_strategy(),
        ) {
            let defaults = crate::reminders::ReminderDefaults { default_before: offset };
            // Un-compact: dismiss both endpoints of an existing link, the
            // representable-but-unsanctioned shape the (dismissal-keyed)
            // normalization prunes.
            let mut dirty = reminders.clone();
            if let Some((&key, &target)) = reminders.snoozes.iter().next() {
                for occurrence in [key.occurrence, target] {
                    dirty.dismissals.insert(crate::reminders::ReminderDismissal {
                        reminder: key.reminder,
                        occurrence,
                        dismissed_at: now,
                    });
                }
            }
            let mut normalized = PersistedState {
                reminders: dirty.clone(),
                ..PersistedState::default()
            };
            apply_actions(&mut normalized, &[]);
            prop_assert_eq!(
                crate::reminders::due_firings(&app, &dirty, &defaults, now),
                crate::reminders::due_firings(&app, &normalized.reminders, &defaults, now)
            );
            prop_assert_eq!(
                crate::reminders::active_notifications(&app, &dirty, &defaults, now),
                crate::reminders::active_notifications(&app, &normalized.reminders, &defaults, now),
                "compaction never changes the active surface",
            );
        }

        #[test]
        fn the_post_pass_never_resurrects_a_dismissed_notification(
            (app, reminders) in test_support::app_state_strategy()
                .prop_map(|app| (app.tasks.clone(), app.reminders.clone())),
            offset in test_support::reminder_offset_strategy(),
            now in test_support::timestamp_strategy(),
        ) {
            let defaults = crate::reminders::ReminderDefaults { default_before: offset };
            let due_before: std::collections::BTreeSet<_> =
                crate::reminders::due_firings(&app, &reminders, &defaults, now)
                    .into_iter()
                    .map(|f| (f.reminder, f.occurrence))
                    .collect();
            let active_before: std::collections::BTreeSet<_> =
                crate::reminders::active_notifications(&app, &reminders, &defaults, now)
                    .into_iter()
                    .map(|n| (n.reminder, n.occurrence))
                    .collect();
            let mut after = PersistedState {
                reminders: reminders.clone(),
                ..PersistedState::default()
            };
            apply_actions(&mut after, &[]);
            let due_after: std::collections::BTreeSet<_> =
                crate::reminders::due_firings(&app, &after.reminders, &defaults, now)
                    .into_iter()
                    .map(|f| (f.reminder, f.occurrence))
                    .collect();
            let active_after: std::collections::BTreeSet<_> =
                crate::reminders::active_notifications(&app, &after.reminders, &defaults, now)
                    .into_iter()
                    .map(|n| (n.reminder, n.occurrence))
                    .collect();
            // Directional (arbitrary states): the post-pass never ADDS a
            // firing or an active row. Two-sided equality is the ruled
            // composition's interior-settlement exception (a dismissed
            // walk prefix reclaims its unreachable tail); recorded
            // behavior, pinned deterministically by the mis-order
            // catcher. The active reader is the point: under the old
            // delivery-keyed criteria this subset law fails on the
            // resurrection trace.
            prop_assert!(due_after.is_subset(&due_before));
            prop_assert!(active_after.is_subset(&active_before));
        }
    }

    #[test]
    fn invalid_validator_key_string_is_rejected() {
        assert!(serde_json::from_str::<ValidatorKey>("\"not-a-key\"").is_err());
        assert!(serde_json::from_str::<ValidatorKey>("\"stacks:\"").is_err());
    }

    #[test]
    fn archived_stacks_validator_key_roundtrips_through_serde() {
        let key = ValidatorKey::ArchivedStacks(crate::ids::RemoteBoardId(9));
        let back: ValidatorKey =
            serde_json::from_str(&serde_json::to_string(&key).unwrap()).unwrap();
        assert_eq!(back, key);
        assert_eq!(
            serde_json::to_string(&key).unwrap(),
            serde_json::to_string("archived_stacks:9").unwrap(),
            "the wire tag is the storage codec's text form"
        );
    }

    // ---- Alarm settlement (chunk-7): the prune and its property set --

    fn adhoc_alarm(raw: u128, fire_at_secs: i64) -> crate::reminders::AdhocAlarm {
        crate::reminders::AdhocAlarm {
            id: crate::ids::AdhocAlarmId::from(uuid::Uuid::from_u128(raw)),
            label: "steep the tea".into(),
            fire_at: ts(fire_at_secs),
            created_at: ts(100),
        }
    }

    #[test]
    fn adhoc_alarm_delete_cascades_dependents_and_nothing_elses() {
        let mut state = PersistedState::default();
        let a = adhoc_alarm(30, 1_000);
        let task = crate::ids::TaskId::from(uuid::Uuid::from_u128(5));
        let task_ref = crate::reminders::ReminderRef::Default(task);
        apply_actions(
            &mut state,
            &[
                PersistenceAction::UpsertAdhocAlarm(a.clone()),
                PersistenceAction::RecordReminderDelivery(crate::reminders::ReminderDelivery {
                    reminder: crate::reminders::ReminderRef::Adhoc(a.id),
                    occurrence: ts(1_000),
                    delivered_at: ts(1_001),
                }),
                PersistenceAction::RecordReminderSnooze(crate::reminders::ReminderSnooze {
                    reminder: crate::reminders::ReminderRef::Adhoc(a.id),
                    occurrence: ts(2_000),
                    rescheduled_to: ts(3_000),
                }),
                PersistenceAction::RecordReminderDismissal(crate::reminders::ReminderDismissal {
                    reminder: crate::reminders::ReminderRef::Adhoc(a.id),
                    occurrence: ts(1_000),
                    dismissed_at: ts(1_002),
                }),
                // A task-reminder row must survive the alarm delete.
                PersistenceAction::RecordReminderDelivery(crate::reminders::ReminderDelivery {
                    reminder: task_ref,
                    occurrence: ts(1_000),
                    delivered_at: ts(1_001),
                }),
                PersistenceAction::RecordReminderDismissal(crate::reminders::ReminderDismissal {
                    reminder: task_ref,
                    occurrence: ts(1_000),
                    dismissed_at: ts(1_002),
                }),
            ],
        );
        apply_actions(&mut state, &[PersistenceAction::DeleteAdhocAlarm(a.id)]);
        assert!(state.reminders.alarms.is_empty());
        assert_eq!(
            state.reminders.deliveries.len(),
            1,
            "only the task-reminder delivery survives"
        );
        assert_eq!(
            state.reminders.dismissals.len(),
            1,
            "only the task-reminder dismissal survives"
        );
        assert!(state.reminders.snoozes.is_empty());
    }

    #[test]
    fn cancelled_alarm_is_quiet_at_every_now() {
        let mut state = PersistedState::default();
        let a = adhoc_alarm(31, 500);
        let alarm_id = a.id;
        apply_actions(
            &mut state,
            &[PersistenceAction::UpsertAdhocAlarm(a.clone())],
        );
        // Fires before the cancel...
        let app: AppState = state.clone().into();
        let defaults = crate::reminders::ReminderDefaults::default();
        assert_eq!(
            crate::reminders::due_firings(&app.tasks, &state.reminders, &defaults, ts(500)).len(),
            1
        );
        // ...and never after it: the batch `CancelAdhocAlarm` plans is
        // exactly `DeleteAdhocAlarm`, and nothing can re-fire from a
        // pruned map row (snooze links go with the row).
        apply_actions(
            &mut state,
            &[
                PersistenceAction::RecordReminderSnooze(crate::reminders::ReminderSnooze {
                    reminder: crate::reminders::ReminderRef::Adhoc(alarm_id),
                    occurrence: ts(500),
                    rescheduled_to: ts(9_000),
                }),
                PersistenceAction::DeleteAdhocAlarm(alarm_id),
            ],
        );
        for now in [499, 500, 9_000, 10_000] {
            let app: AppState = state.clone().into();
            let firings =
                crate::reminders::due_firings(&app.tasks, &state.reminders, &defaults, ts(now));
            assert!(
                firings
                    .iter()
                    .all(|f| f.reminder != crate::reminders::ReminderRef::Adhoc(alarm_id)),
                "no re-fire at now={now}"
            );
        }
    }

    #[test]
    fn the_prune_runs_after_compaction_settled_chain_reverts_then_prunes() {
        // Re-timed 2026-10-26 (#474): the mis-order catcher, re-keyed to
        // the dismissal criteria. A two-link chain (base→m→t) with base
        // and m delivered AND dismissed, t undelivered. Compaction
        // first drops the fully-dismissed base→m link, the walk reverts
        // to base, and the prune removes the alarm whose remaining m→t
        // link is unreachable garbage (the ruled interior-settlement
        // composition: the user settled the walk's reachable prefix).
        // Prune-first (or dropped) would read effective = t
        // (undelivered) and keep the zombie through round one.
        let mut state = PersistedState::default();
        let a = adhoc_alarm(32, 1_000);
        let reminder = crate::reminders::ReminderRef::Adhoc(a.id);
        apply_actions(
            &mut state,
            &[
                PersistenceAction::UpsertAdhocAlarm(a),
                PersistenceAction::RecordReminderDelivery(crate::reminders::ReminderDelivery {
                    reminder,
                    occurrence: ts(1_000),
                    delivered_at: ts(1_001),
                }),
                PersistenceAction::RecordReminderDelivery(crate::reminders::ReminderDelivery {
                    reminder,
                    occurrence: ts(2_000),
                    delivered_at: ts(2_001),
                }),
                PersistenceAction::RecordReminderSnooze(crate::reminders::ReminderSnooze {
                    reminder,
                    occurrence: ts(1_000),
                    rescheduled_to: ts(2_000),
                }),
                PersistenceAction::RecordReminderSnooze(crate::reminders::ReminderSnooze {
                    reminder,
                    occurrence: ts(2_000),
                    rescheduled_to: ts(3_000),
                }),
                PersistenceAction::RecordReminderDismissal(crate::reminders::ReminderDismissal {
                    reminder,
                    occurrence: ts(1_000),
                    dismissed_at: ts(1_002),
                }),
                PersistenceAction::RecordReminderDismissal(crate::reminders::ReminderDismissal {
                    reminder,
                    occurrence: ts(2_000),
                    dismissed_at: ts(2_002),
                }),
            ],
        );
        // One post-pass (the empty batch) must run compaction → prune.
        apply_actions(&mut state, &[]);
        assert!(
            state.reminders.alarms.is_empty(),
            "the settled alarm prunes in the same post-pass"
        );
    }

    #[test]
    fn prune_settled_alarms_keys_on_delivered_and_dismissed() {
        let mut state = PersistedState::default();
        // Delivered only: the fired-but-undismissed alarm is the live
        // in-app surface — it survives (the old contract pruned it here).
        let fired = adhoc_alarm(40, 1_000);
        // Delivered AND dismissed at the effective occurrence: settled,
        // pruned with all dependents in the same batch.
        let settled = adhoc_alarm(41, 2_000);
        // Dismissed but unfired (pre-dismissal is legal data): no
        // delivery, so not settled — survives and fires never.
        let predismissed = adhoc_alarm(42, 3_000);
        let fired_ref = crate::reminders::ReminderRef::Adhoc(fired.id);
        let settled_ref = crate::reminders::ReminderRef::Adhoc(settled.id);
        let predismissed_ref = crate::reminders::ReminderRef::Adhoc(predismissed.id);
        let delivery = |at: i64| {
            PersistenceAction::RecordReminderDelivery(crate::reminders::ReminderDelivery {
                reminder: settled_ref,
                occurrence: ts(at),
                delivered_at: ts(at + 1),
            })
        };
        apply_actions(
            &mut state,
            &[
                PersistenceAction::UpsertAdhocAlarm(fired.clone()),
                PersistenceAction::UpsertAdhocAlarm(settled.clone()),
                PersistenceAction::UpsertAdhocAlarm(predismissed.clone()),
                delivery(2_000),
                PersistenceAction::RecordReminderDismissal(crate::reminders::ReminderDismissal {
                    reminder: settled_ref,
                    occurrence: ts(2_000),
                    dismissed_at: ts(2_002),
                }),
                PersistenceAction::RecordReminderDismissal(crate::reminders::ReminderDismissal {
                    reminder: predismissed_ref,
                    occurrence: ts(3_000),
                    dismissed_at: ts(3_002),
                }),
                // A live delivery for the fired alarm: keeps it on the
                // surface, keyed delivered ∧ ¬dismissed.
                PersistenceAction::RecordReminderDelivery(crate::reminders::ReminderDelivery {
                    reminder: fired_ref,
                    occurrence: ts(1_000),
                    delivered_at: ts(1_001),
                }),
            ],
        );
        assert!(
            state.reminders.alarms.contains_key(&fired.id),
            "delivered-but-undismissed is the live surface, not a settle"
        );
        assert!(
            !state.reminders.alarms.contains_key(&settled.id),
            "delivered ∧ dismissed prunes in the same batch"
        );
        assert_eq!(
            state.reminders.deliveries.len(),
            1,
            "the settled alarm's delivery cascades; the fired alarm's survives"
        );
        assert!(state.reminders.is_delivered(&fired_ref, &ts(1_000)));
        assert!(
            state.reminders.alarms.contains_key(&predismissed.id),
            "dismissed-but-unfired has no delivery, so it is not settled"
        );
        let app: AppState = state.clone().into();
        for now in [3_000, 4_000, 10_000] {
            assert!(
                crate::reminders::due_firings(
                    &app.tasks,
                    &state.reminders,
                    &crate::reminders::ReminderDefaults::default(),
                    ts(now)
                )
                .iter()
                .all(|f| f.reminder != predismissed_ref),
                "a pre-dismissed alarm fires never"
            );
        }
        // Settled is a fixed point.
        apply_actions(&mut state, &[]);
        assert!(state.reminders.alarms.contains_key(&fired.id));
        assert!(state.reminders.alarms.contains_key(&predismissed.id));
    }

    #[test]
    #[allow(clippy::too_many_lines)] // the trace IS the scope proof
    fn dismissed_notification_does_not_resurrect_through_compaction() {
        // The design round's §1 resurrection trace, deterministic: fire
        // → snooze → fire → dismiss → the surface stays empty and every
        // dependent row is reclaimed in the dismissal batch's post-pass.
        // Red under the old criteria: the delivery batch pruned the row
        // (delivered-only), and with only the prune re-timed the
        // both-delivered compaction reverted the walk to the
        // undismissed base — the resurrected row this test forbids.
        let mut state = PersistedState::default();
        let a = adhoc_alarm(43, 1_000);
        let reminder = crate::reminders::ReminderRef::Adhoc(a.id);
        let defaults = crate::reminders::ReminderDefaults::default();
        apply_actions(
            &mut state,
            &[PersistenceAction::UpsertAdhocAlarm(a.clone())],
        );
        assert!(state.reminders.alarms.contains_key(&a.id));

        // Fire t1_000: the alarm row AND its delivery survive.
        apply_actions(
            &mut state,
            &[PersistenceAction::RecordReminderDelivery(
                crate::reminders::ReminderDelivery {
                    reminder,
                    occurrence: ts(1_000),
                    delivered_at: ts(1_001),
                },
            )],
        );
        assert!(state.reminders.alarms.contains_key(&a.id));
        assert_eq!(state.reminders.deliveries.len(), 1);
        let app: AppState = state.clone().into();
        assert_eq!(
            crate::reminders::active_notifications(
                &app.tasks,
                &state.reminders,
                &defaults,
                ts(1_000)
            ),
            vec![crate::reminders::Notification {
                reminder,
                anchor: crate::reminders::ReminderAnchor::Alarm(a.id),
                title: "steep the tea".into(),
                occurrence: ts(1_000),
            }],
        );

        // Snooze t1_000 → t2_000: the row leaves the surface by
        // derivation (the effective occurrence moves past `now`).
        apply_actions(
            &mut state,
            &[PersistenceAction::RecordReminderSnooze(
                crate::reminders::ReminderSnooze {
                    reminder,
                    occurrence: ts(1_000),
                    rescheduled_to: ts(2_000),
                },
            )],
        );
        let app: AppState = state.clone().into();
        assert_eq!(
            crate::reminders::active_notifications(
                &app.tasks,
                &state.reminders,
                &defaults,
                ts(1_500)
            ),
            [] as [crate::reminders::Notification; 0],
        );

        // Fire t2_000: alive, one surface row at the new occurrence.
        apply_actions(
            &mut state,
            &[PersistenceAction::RecordReminderDelivery(
                crate::reminders::ReminderDelivery {
                    reminder,
                    occurrence: ts(2_000),
                    delivered_at: ts(2_001),
                },
            )],
        );
        assert!(state.reminders.alarms.contains_key(&a.id));
        let app: AppState = state.clone().into();
        assert_eq!(
            crate::reminders::active_notifications(
                &app.tasks,
                &state.reminders,
                &defaults,
                ts(2_000)
            )
            .iter()
            .map(|n| (n.reminder, n.occurrence))
            .collect::<Vec<_>>(),
            vec![(reminder, ts(2_000))],
        );

        // Dismiss t2_000: the link survives compaction (its source is
        // undismissed), the prune reclaims the settled alarm with all
        // dependents — the surface stays empty at t2_000 and beyond.
        apply_actions(
            &mut state,
            &[PersistenceAction::RecordReminderDismissal(
                crate::reminders::ReminderDismissal {
                    reminder,
                    occurrence: ts(2_000),
                    dismissed_at: ts(2_002),
                },
            )],
        );
        assert!(state.reminders.alarms.is_empty());
        assert!(state.reminders.deliveries.is_empty());
        assert!(state.reminders.snoozes.is_empty());
        assert!(state.reminders.dismissals.is_empty());
        let app: AppState = state.clone().into();
        for now in [2_000, 3_000, 10_000] {
            assert!(
                crate::reminders::active_notifications(
                    &app.tasks,
                    &state.reminders,
                    &defaults,
                    ts(now)
                )
                .is_empty(),
                "no resurrection at now={now}"
            );
        }

        // Empty batch: a fixed point, still empty.
        apply_actions(&mut state, &[]);
        assert!(state.reminders.alarms.is_empty());
    }

    #[test]
    fn the_composition_narrative_through_batches_only() {
        // Re-timed 2026-10-26 (#474): delivery-keyed settlement was the
        // pre-R3 contract; DR-1 R3
        // (`.artifacts/plans/2026-10-25-reminder-rule-sync-design-round-plan.md`
        // §4-R3) makes dismissal the settle. The persistence half of the
        // old pin is intact: the prune still lands in the same post-pass
        // batch as the action that settles the occurrence — now the
        // dismissal batch, not the delivery batch. D-R7's sentence made
        // concrete, on the sanctioned path: create → the user snoozes
        // the pending occurrence (link data is opaque; snoozing before
        // the fire is typed-legal) → the alarm fires at its target →
        // the delivery batch leaves the row AND its delivery AND the
        // link alive (the live in-app surface) → the dismissal batch
        // settles it: the prune removes the alarm and its dependents in
        // the SAME post-pass. All through `apply_actions` batches, no
        // direct mutation. (The plan's literal deliver-base-then-snooze
        // sequence is unrepresentable: an unsnoozed fire's delivery
        // settles its occurrence but no longer prunes the row — the
        // snooze now targets a live row; recorded as a stage-2
        // deviation note, updated for the fired-alarm survival window.)
        let mut state = PersistedState::default();
        let a = adhoc_alarm(33, 1_000);
        let reminder = crate::reminders::ReminderRef::Adhoc(a.id);
        apply_actions(
            &mut state,
            &[PersistenceAction::UpsertAdhocAlarm(a.clone())],
        );
        // Snooze the pending occurrence to 2_000.
        apply_actions(
            &mut state,
            &[PersistenceAction::RecordReminderSnooze(
                crate::reminders::ReminderSnooze {
                    reminder,
                    occurrence: ts(1_000),
                    rescheduled_to: ts(2_000),
                },
            )],
        );
        // Below the target: alive, quiet (the chain moved the fire).
        let app: AppState = state.clone().into();
        assert_eq!(
            crate::reminders::due_firings(
                &app.tasks,
                &state.reminders,
                &crate::reminders::ReminderDefaults::default(),
                ts(1_500),
            ),
            Vec::new()
        );
        // Fire at the target: the evaluator delivers it; the delivery
        // batch's post-pass keeps the row alive — delivery and link
        // intact, the in-app surface carrying the fired occurrence.
        apply_actions(
            &mut state,
            &[PersistenceAction::RecordReminderDelivery(
                crate::reminders::ReminderDelivery {
                    reminder,
                    occurrence: ts(2_000),
                    delivered_at: ts(2_001),
                },
            )],
        );
        assert!(
            state.reminders.alarms.contains_key(&a.id),
            "fired is not settled"
        );
        assert_eq!(state.reminders.deliveries.len(), 1);
        assert_eq!(state.reminders.snoozes.len(), 1);
        let app: AppState = state.clone().into();
        assert_eq!(
            crate::reminders::active_notifications(
                &app.tasks,
                &state.reminders,
                &crate::reminders::ReminderDefaults::default(),
                ts(2_000),
            )
            .len(),
            1,
            "the fired alarm is the live in-app surface"
        );
        // Dismiss at the effective occurrence: the dismissal batch's
        // post-pass prunes the settled alarm with its deliveries,
        // dismissals, and snooze links.
        apply_actions(
            &mut state,
            &[PersistenceAction::RecordReminderDismissal(
                crate::reminders::ReminderDismissal {
                    reminder,
                    occurrence: ts(2_000),
                    dismissed_at: ts(2_002),
                },
            )],
        );
        assert!(state.reminders.alarms.is_empty());
        assert!(state.reminders.snoozes.is_empty());
        assert!(state.reminders.deliveries.is_empty());
        assert!(state.reminders.dismissals.is_empty());
        // One more batch: settled is a fixed point.
        apply_actions(&mut state, &[]);
        assert!(state.reminders.alarms.is_empty());
        assert!(state.reminders.snoozes.is_empty());
    }

    proptest! {
        #![proptest_config(test_support::proptest_config(256))]

        #[test]
        fn alarm_prune_is_unobservable_in_due_firings(
            (app, reminders) in test_support::app_state_strategy()
                .prop_map(|app| (app.tasks.clone(), app.reminders.clone())),
            offset in test_support::reminder_offset_strategy(),
            now in test_support::timestamp_strategy(),
        ) {
            let defaults = crate::reminders::ReminderDefaults { default_before: offset };
            let mut pruned = reminders.clone();
            crate::reminders::prune_settled_alarms(&mut pruned);
            prop_assert_eq!(
                crate::reminders::due_firings(&app, &reminders, &defaults, now),
                crate::reminders::due_firings(&app, &pruned, &defaults, now),
                "the prune never changes what fires",
            );
            // The second reader: a settled (delivered ∧ dismissed)
            // alarm contributes no active row, so removing it cannot
            // change the surface.
            prop_assert_eq!(
                crate::reminders::active_notifications(&app, &reminders, &defaults, now),
                crate::reminders::active_notifications(&app, &pruned, &defaults, now),
                "the prune never changes the active surface",
            );
        }
    }

    proptest! {
        #![proptest_config(test_support::proptest_config(256))]

        #[test]
        fn persisted_state_roundtrips_through_serde(state in test_support::persisted_state_strategy()) {
            let json = serde_json::to_string(&state).unwrap();
            let back: PersistedState = serde_json::from_str(&json).unwrap();
            prop_assert_eq!(back, state);
        }

        #[test]
        fn persisted_state_maps_into_app_state(state in test_support::persisted_state_strategy()) {
            let app: AppState = state.clone().into();
            prop_assert_eq!(app.boards, state.boards);
            prop_assert_eq!(app.stacks, state.stacks);
            prop_assert_eq!(app.tasks, state.tasks);
            prop_assert_eq!(app.labels, state.labels);
            prop_assert_eq!(app.sync, state.sync);
            prop_assert!(app.last_updated.is_none());
        }

        #[test]
        fn board_id_used_as_validator_key_roundtrips(b in any::<u64>()) {
            let key = ValidatorKey::Stacks(crate::ids::RemoteBoardId(b));
            let back: ValidatorKey =
                serde_json::from_str(&serde_json::to_string(&key).unwrap()).unwrap();
            prop_assert_eq!(back, key);
        }
    }
}
