// SPDX-License-Identifier: MIT OR Apache-2.0
//! The engine's private working state and its single-advance rule.
//!
//! Memory advances only by interpreting the very action batch that was
//! persisted ([`EngineCore::interpret`], which delegates to the domain's
//! shared [`apply_actions`]). The engine never mutates its working state
//! any other way — memory ≡ disk by construction, and a failed `apply`
//! needs no rollback because `interpret` never runs (ADR 0006).

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};

use taskboard_domain::{
    AppState, BoardId, PendingOp, PersistedState, PersistenceAction, SyncCounters, SyncValidators,
    ValidatorKey, apply_actions,
};

/// The engine's private working state: the persisted shape plus the one
/// field persistence never knew.
#[derive(Debug, Clone)]
pub struct EngineCore {
    state: PersistedState,
    last_updated: Option<DateTime<Utc>>,
    /// Process-lifetime sync counters, keyed by local board. Memory-only
    /// observability: never part of `PersistedState`, never persisted,
    /// never entering `interpret`'s batches. `app()` overlays them onto
    /// the published rows; a restart re-hydrates rows without counters —
    /// cumulative-since-boot by construction.
    sync_counters: BTreeMap<BoardId, SyncCounters>,
}

impl EngineCore {
    /// Hydrated at boot; `last_updated` starts `None` (unknown-at-boot).
    /// The derived sync projection runs here exactly like every
    /// repository `load()` runs it — memory ≡ disk by construction from
    /// the first published snapshot, not only from the first batch.
    #[must_use]
    pub fn from_persisted(mut state: PersistedState) -> Self {
        taskboard_domain::derive_sync_states(
            &state.boards,
            &state.stacks,
            &state.tasks,
            &state.labels,
            &state.outbox,
            &mut state.sync,
        );
        Self {
            state,
            last_updated: None,
            sync_counters: BTreeMap::new(),
        }
    }

    /// Advance memory by exactly the batch that was persisted, stamp
    /// `last_updated`, and return whether the published projection
    /// changed. Called **only** after `repo.apply` returned `Ok`.
    pub fn interpret(&mut self, actions: &[PersistenceAction], now: DateTime<Utc>) -> bool {
        let previous = self.app();
        apply_actions(&mut self.state, actions);
        self.last_updated = Some(now);
        self.app() != previous
    }

    /// The UI projection: entity maps, derived `pending_ops`, and the
    /// engine's `last_updated` stamp.
    #[must_use]
    pub fn app(&self) -> AppState {
        let mut sync = self.state.sync.clone();
        // Overlay the counters onto the rows that already exist. A
        // counter entry for a row-less board (its row was deleted) writes
        // nothing — no phantom row re-materializes.
        for (board, counters) in &self.sync_counters {
            if let Some(row) = sync.get_mut(*board) {
                row.counters = counters.clone();
            }
        }
        AppState {
            boards: self.state.boards.clone(),
            stacks: self.state.stacks.clone(),
            tasks: self.state.tasks.clone(),
            labels: self.state.labels.clone(),
            reminders: self.state.reminders.clone(),
            tracking: self.state.tracking.clone(),
            habits: self.state.habits.clone(),
            sync,
            undo: taskboard_domain::UndoView::derive(&self.state.journal),
            last_updated: self.last_updated,
        }
    }

    /// The current outbox slice (the command planner's and the sync
    /// pipeline's queue input).
    #[must_use]
    pub fn outbox(&self) -> &[PendingOp] {
        &self.state.outbox
    }

    /// The persisted undo journal (the journal wrapper's line input and
    /// the R-U9 post-pass's entry map).
    #[must_use]
    pub fn journal(&self) -> &taskboard_domain::JournalState {
        &self.state.journal
    }

    /// The stored sync status row of one board (phase + last success feed
    /// the status-transition decisions); `None` = no stored row.
    #[must_use]
    pub fn sync_status(&self, board: BoardId) -> Option<&taskboard_domain::SyncStatus> {
        self.state.sync.get(board)
    }

    /// The stored conditional-read validators (the ingestion compares
    /// incoming report validators against these before persisting).
    #[must_use]
    pub fn validators(&self) -> &BTreeMap<ValidatorKey, SyncValidators> {
        &self.state.validators
    }

    /// The reminders due at `now` — a zero-clone borrow delegated to the
    /// domain evaluator (the brain stays in the domain; the core stays a
    /// state capsule).
    #[must_use]
    pub fn due_reminder_firings(
        &self,
        defaults: &taskboard_domain::ReminderDefaults,
        now: DateTime<Utc>,
    ) -> Vec<taskboard_domain::ReminderFiring> {
        taskboard_domain::due_firings(&self.state.tasks, &self.state.reminders, defaults, now)
    }

    /// The task map (the notification title lookup).
    #[must_use]
    pub fn tasks(&self) -> &BTreeMap<taskboard_domain::TaskId, taskboard_domain::Task> {
        &self.state.tasks
    }

    /// The reminder state (the alarm-label lookup for notification
    /// payload assembly — the `tasks()` sibling).
    #[must_use]
    pub fn reminders(&self) -> &taskboard_domain::ReminderState {
        &self.state.reminders
    }

    /// Test/assertion view mirroring the persisted shape. One divergence
    /// from what a repository could return: the memory-only `Syncing`
    /// transient set by [`Self::mark_syncing`] appears here even though
    /// persistence never stores it (a repository `load()` never sees it).
    #[must_use]
    pub fn persisted_view(&self) -> PersistedState {
        self.state.clone()
    }

    /// Marks the `Syncing` transient of one board in memory only — it is
    /// never persisted (a reboot mid-sync must not advertise a phantom
    /// in-flight cycle). Returns whether the phase actually changed
    /// (per-board idempotence). The row's derived `pending_ops` stays
    /// visible while the cycle runs.
    pub fn mark_syncing(&mut self, board: BoardId) -> bool {
        self.state.sync.mark_syncing(board)
    }

    /// Saturating-accumulates one report's counter delta into a board's
    /// process-lifetime facts (default-entry on first sight). Strictly
    /// outside the persistence pipeline: no batch, no `last_updated`
    /// stamp — observability, not accounting.
    pub fn record_report(&mut self, board: BoardId, delta: &SyncCounters) {
        let entry = self.sync_counters.entry(board).or_default();
        entry.fold_into(delta);
    }

    /// Records one offline-episode edge for a board (the caller owns the
    /// edge semantics; this is the accumulation only).
    pub fn record_offline_episode(&mut self, board: BoardId) {
        let entry = self.sync_counters.entry(board).or_default();
        entry.offline_episodes = entry.offline_episodes.saturating_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use taskboard_domain::idgen::IdGenerator as _;
    use taskboard_domain::test_support::CountingIds;
    use taskboard_domain::{
        Board, BoardId, PersistenceAction, Stack, StackClocks, StackId, SyncCounters, SyncPhase,
    };

    fn ts(secs: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(secs, 0).unwrap()
    }

    /// Deterministic ids from the domain's counting fake: the board gets
    /// the first id, the stack the second.
    fn seeded() -> (PersistedState, StackId) {
        let ids = CountingIds::new();
        let board_id: BoardId = ids.new_board_id();
        let stack = Stack {
            id: ids.new_stack_id(),
            remote: None,
            board: board_id,
            title: "stack".into(),
            order: 1,
            archived: false,
            deleted: false,
            clocks: StackClocks {
                title: ts(100),
                order: ts(100),
                deleted: ts(100),
            },
            remote_seen: None,
        };
        let stack_id = stack.id;
        let mut state = PersistedState::default();
        state.stacks.insert(stack.id, stack);
        (state, stack_id)
    }

    #[test]
    fn from_persisted_starts_with_unknown_last_updated() {
        let (state, _) = seeded();
        let core = EngineCore::from_persisted(state.clone());
        assert!(core.app().last_updated.is_none());
        assert_eq!(core.persisted_view(), state);
    }

    #[test]
    fn interpret_applies_the_batch_and_stamps_last_updated() {
        let (state, stack_id) = seeded();
        let mut core = EngineCore::from_persisted(state);
        let mut renamed = core.persisted_view().stacks[&stack_id].clone();
        renamed.title = "renamed".into();
        let actions = vec![PersistenceAction::UpsertStack(renamed)];
        assert!(core.interpret(&actions, ts(200)));
        assert_eq!(core.app().last_updated, Some(ts(200)));
        assert_eq!(core.persisted_view().stacks.len(), 1);
        assert_eq!(core.persisted_view().stacks[&stack_id].title, "renamed");
    }

    #[test]
    fn interpret_under_a_fixed_clock_reports_unchanged_for_empty_batches() {
        let (state, _) = seeded();
        let mut core = EngineCore::from_persisted(state);
        // First interpret stamps; the second, empty batch at the same
        // instant cannot change the projection.
        assert!(core.interpret(&[], ts(200)));
        assert!(!core.interpret(&[], ts(200)));
    }

    #[test]
    fn the_overlay_writes_counters_only_onto_existing_rows() {
        let (mut state, _) = seeded();
        // The overlay is row-keyed, and rows materialize per board: seed
        // one board (the stack fixture alone has no sync row).
        let ids = CountingIds::new();
        let board_id = ids.new_board_id();
        state.boards.insert(
            board_id,
            Board {
                id: board_id,
                remote: None,
                title: "board".into(),
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
            },
        );
        let mut core = EngineCore::from_persisted(state);
        let known = board_id;
        core.record_report(
            known,
            &SyncCounters {
                cycles: 3,
                pulls: 7,
                ..SyncCounters::default()
            },
        );
        // An existing row receives its counters.
        assert_eq!(core.app().sync.get(known).unwrap().counters.cycles, 3);
        assert_eq!(core.app().sync.get(known).unwrap().counters.pulls, 7);
        // A counter entry for a row-less board materializes no row: the
        // delta drops out of the projection with the deleted board.
        let stranger = BoardId::from(uuid::Uuid::from_u128(999));
        core.record_report(
            stranger,
            &SyncCounters {
                cycles: 9,
                ..SyncCounters::default()
            },
        );
        assert!(
            core.app().sync.get(stranger).is_none(),
            "no phantom row for a board whose row is gone"
        );
        // Counters are invisible to the persisted shape.
        assert!(
            core.persisted_view()
                .sync
                .get(known)
                .unwrap()
                .counters
                .cycles
                == 0,
            "persisted rows stay counter-free (restart-from-zero)"
        );
    }

    #[test]
    fn mark_syncing_is_idempotent_and_stamps_nothing() {
        let (state, _) = seeded();
        let mut core = EngineCore::from_persisted(state);
        let board = taskboard_domain::BoardId::from(uuid::Uuid::from_u128(1));
        assert_eq!(core.sync_status(board).map(|row| row.phase), None);
        assert!(core.mark_syncing(board));
        assert_eq!(
            core.app().sync.get(board).map(|row| row.phase),
            Some(SyncPhase::Syncing)
        );
        assert!(!core.mark_syncing(board), "repeat is a no-op");
        // The transient is memory-only in the persistence sense: the
        // engine emits no `UpsertSyncStatus` for it (actor-level pin) and
        // `last_updated` — the mutation stamp — is not advanced.
        assert!(core.app().last_updated.is_none());
    }
}
