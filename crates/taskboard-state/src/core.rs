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
    AppState, PendingOp, PersistedState, PersistenceAction, SyncPhase, SyncStatus, SyncValidators,
    ValidatorKey, apply_actions,
};

/// The engine's private working state: the persisted shape plus the one
/// field persistence never knew.
#[derive(Debug, Clone)]
pub struct EngineCore {
    state: PersistedState,
    last_updated: Option<DateTime<Utc>>,
}

impl EngineCore {
    /// Hydrated at boot; `last_updated` starts `None` (unknown-at-boot).
    #[must_use]
    pub fn from_persisted(state: PersistedState) -> Self {
        Self {
            state,
            last_updated: None,
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
        AppState {
            boards: self.state.boards.clone(),
            stacks: self.state.stacks.clone(),
            tasks: self.state.tasks.clone(),
            labels: self.state.labels.clone(),
            sync: self.state.sync.clone(),
            last_updated: self.last_updated,
        }
    }

    /// The current outbox slice (the command planner's and the sync
    /// pipeline's queue input).
    #[must_use]
    pub fn outbox(&self) -> &[PendingOp] {
        &self.state.outbox
    }

    /// The current sync status (phase + last success feed the
    /// status-transition decisions).
    #[must_use]
    pub fn sync_status(&self) -> &SyncStatus {
        &self.state.sync
    }

    /// The stored conditional-read validators (the ingestion compares
    /// incoming report validators against these before persisting).
    #[must_use]
    pub fn validators(&self) -> &BTreeMap<ValidatorKey, SyncValidators> {
        &self.state.validators
    }

    /// Test/assertion view mirroring the persisted shape. One divergence
    /// from what a repository could return: the memory-only `Syncing`
    /// transient set by [`Self::mark_syncing`] appears here even though
    /// persistence never stores it (a repository `load()` never sees it).
    #[must_use]
    pub fn persisted_view(&self) -> PersistedState {
        self.state.clone()
    }

    /// Marks the `Syncing` transient in memory only — it is never
    /// persisted (a reboot mid-sync must not advertise a phantom in-flight
    /// cycle). Returns whether the phase actually changed.
    pub fn mark_syncing(&mut self) -> bool {
        if self.state.sync.phase == SyncPhase::Syncing {
            return false;
        }
        self.state.sync.phase = SyncPhase::Syncing;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use taskboard_domain::idgen::IdGenerator as _;
    use taskboard_domain::test_support::CountingIds;
    use taskboard_domain::{BoardId, PersistenceAction, Stack, StackClocks, StackId, SyncPhase};

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
    fn mark_syncing_is_idempotent_and_stamps_nothing() {
        let (state, _) = seeded();
        let mut core = EngineCore::from_persisted(state);
        assert_eq!(core.sync_status().phase, SyncPhase::Idle);
        assert!(core.mark_syncing());
        assert_eq!(core.app().sync.phase, SyncPhase::Syncing);
        assert!(!core.mark_syncing(), "repeat is a no-op");
        // The transient is memory-only in the persistence sense: the
        // engine emits no `UpsertSyncStatus` for it (actor-level pin) and
        // `last_updated` — the mutation stamp — is not advanced.
        assert!(core.app().last_updated.is_none());
    }
}
