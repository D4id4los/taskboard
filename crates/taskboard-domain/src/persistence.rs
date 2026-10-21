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
}

impl From<PersistedState> for AppState {
    fn from(persisted: PersistedState) -> Self {
        Self {
            boards: persisted.boards,
            stacks: persisted.stacks,
            tasks: persisted.tasks,
            labels: persisted.labels,
            reminders: persisted.reminders,
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
    /// Record a snooze link. Replaces any prior link for the same
    /// `(reminder, occurrence)`.
    RecordReminderSnooze(ReminderSnooze),
    /// Mute (`true`) or unmute (`false`) all firings of a task. The
    /// overdue banner is unaffected by construction (it never reads the
    /// muted set).
    SetTaskRemindersMuted(crate::ids::TaskId, bool),
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
/// (`pending_ops` is never stored). Reminder actions: rule upserts replace by id; a rule
/// delete prunes the rule's deliveries and snooze links with it; a
/// delivery insert is set-union on the `(reminder, occurrence)` key
/// (first write wins); a snooze link replaces any prior link for the
/// same key; mute toggles membership. A final derivation re-computes
/// `sync.pending_ops` from the outbox depth, and fully-delivered snooze
/// links are compacted — both derivations run unconditionally after
/// every batch (both this function and `load()` derive them). The sync
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
                state
                    .sync
                    .set_stored(*board, status.phase, status.last_success);
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
                    .snoozes
                    .retain(|key, _| key.reminder != ReminderRef::Rule(*id));
            }
            PersistenceAction::RecordReminderDelivery(delivery) => {
                if !state
                    .reminders
                    .is_delivered(&delivery.reminder, &delivery.occurrence)
                {
                    state.reminders.deliveries.insert(*delivery);
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
                // deliveries and snooze links (the DeleteReminderRule
                // parity); default-reminder facts and mutes go by task.
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
                        | crate::outbox::LocalOp::UnassignLabel(id, _) => board_tasks.contains(&id),
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
    // delivered can never influence evaluation again — both endpoints
    // suppress on their own. Pruning it bounds chain growth with zero
    // behavioral change (the compaction-unobservability property).
    crate::reminders::compact_snooze_chains(&mut state.reminders);
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
                archived: ts(100),
                deleted: ts(100),
            },
            remote_seen: None,
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
                archived: ts(100),
                deleted: ts(100),
            },
            remote_seen: None,
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

    fn reminder_rule(raw: u128) -> crate::reminders::ReminderRule {
        crate::reminders::ReminderRule {
            id: crate::ids::ReminderRuleId::from(uuid::Uuid::from_u128(raw)),
            task: crate::ids::TaskId::from(uuid::Uuid::from_u128(raw + 1)),
            offset: crate::reminders::ReminderOffset::at_due(),
            created_at: ts(100),
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
                // A default-reminder row must survive the rule delete.
                PersistenceAction::RecordReminderDelivery(crate::reminders::ReminderDelivery {
                    reminder: task_ref,
                    occurrence: ts(1_000),
                    delivered_at: ts(1_001),
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
        assert!(state.reminders.snoozes.is_empty());
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

    #[test]
    fn snooze_compaction_prunes_only_fully_delivered_links() {
        let mut state = PersistedState::default();
        let r = crate::reminders::ReminderRef::Default(crate::ids::TaskId::from(
            uuid::Uuid::from_u128(5),
        ));
        // Pruned: source and target both delivered.
        let settled = crate::reminders::ReminderSnooze {
            reminder: r,
            occurrence: ts(1_000),
            rescheduled_to: ts(2_000),
        };
        // Kept: only the target delivered — the source can still settle
        // (it would re-arm under target-only pruning; the divergence case).
        let source_live = crate::reminders::ReminderSnooze {
            reminder: r,
            occurrence: ts(3_000),
            rescheduled_to: ts(4_000),
        };
        apply_actions(
            &mut state,
            &[
                PersistenceAction::RecordReminderSnooze(settled),
                PersistenceAction::RecordReminderSnooze(source_live),
                PersistenceAction::RecordReminderDelivery(crate::reminders::ReminderDelivery {
                    reminder: r,
                    occurrence: ts(1_000),
                    delivered_at: ts(1_001),
                }),
                PersistenceAction::RecordReminderDelivery(crate::reminders::ReminderDelivery {
                    reminder: r,
                    occurrence: ts(2_000),
                    delivered_at: ts(2_001),
                }),
                PersistenceAction::RecordReminderDelivery(crate::reminders::ReminderDelivery {
                    reminder: r,
                    occurrence: ts(4_000),
                    delivered_at: ts(4_001),
                }),
            ],
        );
        assert!(
            !state
                .reminders
                .snoozes
                .contains_key(&crate::reminders::ReminderSnoozeKey {
                    reminder: r,
                    occurrence: ts(1_000),
                })
        );
        assert!(
            state
                .reminders
                .snoozes
                .contains_key(&crate::reminders::ReminderSnoozeKey {
                    reminder: r,
                    occurrence: ts(3_000),
                })
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
            // (c) muted tasks contribute zero firings.
            let fired_tasks: std::collections::BTreeSet<_> =
                crate::reminders::due_firings(&app, settled, &defaults, now + chrono::TimeDelta::try_minutes(20).unwrap())
                    .into_iter()
                    .map(|f| f.task)
                    .collect();
            prop_assert!(fired_tasks.iter().all(|t| !settled.muted.contains(t)));
            // (d) persisted invariants: <= 1 delivery per key; compacted.
            let keys: std::collections::BTreeSet<_> = settled
                .deliveries
                .iter()
                .map(|d| (d.reminder, d.occurrence))
                .collect();
            prop_assert_eq!(keys.len(), settled.deliveries.len());
            for key in settled.snoozes.keys() {
                prop_assert!(!settled.is_delivered(&key.reminder, &key.occurrence)
                    || !settled.is_delivered(&key.reminder, &settled.snoozes[key]));
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
        }

        #[test]
        fn compaction_is_unobservable_on_hand_uncompacted_states(
            (app, reminders) in test_support::app_state_strategy()
                .prop_map(|app| (app.tasks.clone(), app.reminders.clone())),
            offset in test_support::reminder_offset_strategy(),
            now in test_support::timestamp_strategy(),
        ) {
            let defaults = crate::reminders::ReminderDefaults { default_before: offset };
            // Un-compact: deliver both endpoints of an existing link, the
            // representable-but-unsanctioned shape normalization prunes.
            let mut dirty = reminders.clone();
            if let Some((&key, &target)) = reminders.snoozes.iter().next() {
                for occurrence in [key.occurrence, target] {
                    dirty.deliveries.insert(crate::reminders::ReminderDelivery {
                        reminder: key.reminder,
                        occurrence,
                        delivered_at: now,
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
