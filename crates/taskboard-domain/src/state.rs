// SPDX-License-Identifier: MIT OR Apache-2.0
//! The canonical application state and its derived views.
//!
//! [`AppState`] is the single source of truth the state engine owns and
//! publishes through `ArcSwap<AppState>`. All public shapes use
//! `BTreeMap` so serialization and merge results are deterministic for
//! equal inputs (convergence property, insta snapshots).

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::entities::{Board, Label, Stack, Task};
use crate::ids::{BoardId, ConnectorId, LabelId, StackId, TaskId};
use crate::outbox::{LocalOp, PendingOp};

/// The complete, immutable UI-facing application state.
///
/// The state engine publishes snapshots of this type through an
/// `arc_swap::ArcSwap<AppState>`; the UI reads them lock-free. Treat any
/// change to this type's shape as a snapshot-breaking change: `insta`
/// tests will surface the diff.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct AppState {
    /// Boards. The MVP holds 0..1 entries; the map shape keeps multi-board
    /// support additive instead of a rewrite.
    pub boards: BTreeMap<BoardId, Board>,
    /// Stacks across all boards.
    pub stacks: BTreeMap<StackId, Stack>,
    /// Tasks across all stacks.
    pub tasks: BTreeMap<TaskId, Task>,
    /// Labels across all boards.
    pub labels: BTreeMap<LabelId, Label>,
    /// Persisted reminder facts (rules, delivery proofs, snooze links,
    /// muted tasks); delivered-ness and snoozed-ness are plain
    /// set-membership checks.
    pub reminders: crate::reminders::ReminderState,
    /// Current sync status, keyed per board (the surfaces read the
    /// `aggregate()` fold).
    pub sync: SyncStates,
    /// Time of the last engine mutation from ANY source (local command or
    /// sync ingestion), stamped via the injected `Clock`.
    pub last_updated: Option<DateTime<Utc>>,
}

/// Sync status summary for the UI badge and CLI output.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncStatus {
    /// Current sync phase.
    pub phase: SyncPhase,
    /// Last successful full sync.
    pub last_success: Option<DateTime<Utc>>,
    /// Outbox depth (pending operations).
    pub pending_ops: u32,
    /// Process-lifetime sync activity facts for this board. Never
    /// persisted: the storage codec and `set_stored` never touch it, so
    /// the serde `default` is what every hydrated row carries.
    #[serde(default)]
    pub counters: SyncCounters,
    /// The board's sidecar-document health, derived from the last
    /// completed cycle's evidence (chunk 4's D14). Memory-only like
    /// `counters`: a restart re-derives from the first cycle's fresh
    /// evidence, so memory-only is honest.
    #[serde(default)]
    pub sidecar: SidecarHealth,
    /// The reporting connector's probed capability flags, recorded by
    /// the last report for this board (chunk 4's D6). Memory-only: a
    /// restart re-learns from the first report (actors probe at
    /// construction).
    #[serde(default)]
    pub features: crate::capabilities::DeckFeatures,
}

/// The board's sidecar-document health (chunk 4's D14): `Absent` and
/// `Document` evidence are both healthy — absent is the ruled *silence*;
/// only the typed unreadable reasons warn.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SidecarHealth {
    /// The document was read, or there is nothing to read (the normal
    /// silent state).
    #[default]
    Healthy,
    /// There was something to read and reading it failed for a typed
    /// reason — the loud fence's subject.
    Unreadable(crate::messages::AppDataUnreadable),
}

/// Derives a board's [`SidecarHealth`] from its cycle evidence (a total
/// match; the pipeline's one derivation site).
#[must_use]
pub fn derive_sidecar_health(evidence: &crate::messages::AppDataEvidence) -> SidecarHealth {
    match evidence {
        // Absent is the ruled silence, a read document is success.
        crate::messages::AppDataEvidence::Document(_)
        | crate::messages::AppDataEvidence::Absent => SidecarHealth::Healthy,
        crate::messages::AppDataEvidence::Unreadable(reason) => SidecarHealth::Unreadable(*reason),
    }
}

/// Process-lifetime sync activity facts for one board.
///
/// Counters are cumulative per process run: they start at zero at every
/// boot, live only in the state engine's memory, and are never
/// persisted — a restart resets them by construction. Cross-restart
/// accumulation would need durable per-board rows (a storage migration)
/// for numbers no kiosk dashboard has asked for; the live process is
/// what the dashboard reads. `last_cycle` is the last cycle's wall
/// duration for that board, however the cycle ended.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncCounters {
    /// Sync cycles that produced a report for this board.
    pub cycles: u64,
    /// Ops successfully pushed for this board.
    pub pushes: u64,
    /// Remote entities pulled (merged) for this board.
    pub pulls: u64,
    /// Cycles that ended in a failure attributed to this board.
    pub failures: u64,
    /// Offline transitions this board lived through (one per
    /// `NetworkLost`/`ConnectorLost` edge, not per repeated event).
    pub offline_episodes: u64,
    /// Wall duration of the board's last cycle, however it ended.
    pub last_cycle: Option<std::time::Duration>,
}

impl SyncCounters {
    /// The aggregate fold step: saturating sums (counters are monotone;
    /// a wrap would be a lie) and the max last-cycle duration
    /// (order-independent, the same rule style as the fold's
    /// `last_success` max).
    pub fn fold_into(&mut self, other: &Self) {
        self.cycles = self.cycles.saturating_add(other.cycles);
        self.pushes = self.pushes.saturating_add(other.pushes);
        self.pulls = self.pulls.saturating_add(other.pulls);
        self.failures = self.failures.saturating_add(other.failures);
        self.offline_episodes = self.offline_episodes.saturating_add(other.offline_episodes);
        self.last_cycle = std::cmp::max(self.last_cycle, other.last_cycle);
    }
}

/// Phase of the sync lifecycle.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SyncPhase {
    /// Nothing running; last cycle succeeded (or none ran yet).
    #[default]
    Idle,
    /// A sync cycle is in flight.
    Syncing,
    /// Offline; retries resume when the network returns.
    Offline,
    /// The last cycle failed with the given error class.
    Failed {
        /// Deck-agnostic failure classification.
        last_error: SyncErrorKind,
    },
}

/// Deck-agnostic sync failure classification. The sync actor maps
/// `DeckError` into this (Phase 4); no text payloads (AGENTS §5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SyncErrorKind {
    /// Transport failure, rate limit, or service unavailable — retryable.
    Network,
    /// Unauthorized.
    Auth,
    /// Forbidden.
    Forbidden,
    /// Server-side failure or unresolvable conflict.
    Server,
    /// Our write was rejected as malformed — dead-letters the op.
    BadRequest,
    /// Local data problem (e.g. repository unavailable during sync).
    LocalData,
    /// No sync target is bound yet — a cycle fired against an empty
    /// derived board set.
    NoBoard,
}

/// Per-board sync status, keyed by local board id. Rows exist for every
/// board the state knows (the derivation materializes defaults); the
/// connector attribution is derived from bindings, never stored here.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SyncStates(BTreeMap<BoardId, SyncStatus>);

impl SyncStates {
    /// An empty map (the fresh-install shape).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The stored row of one board, if any.
    #[must_use]
    pub fn get(&self, board: BoardId) -> Option<&SyncStatus> {
        self.0.get(&board)
    }

    /// Mutable access to one stored row (the engine's counter overlay
    /// writes only existing rows — never materializes one).
    pub fn get_mut(&mut self, board: BoardId) -> Option<&mut SyncStatus> {
        self.0.get_mut(&board)
    }

    /// Every stored row, in deterministic board-id order.
    pub fn iter(&self) -> impl Iterator<Item = (&BoardId, &SyncStatus)> {
        self.0.iter()
    }

    /// The back-compat fold the single-board surfaces read. Phase
    /// precedence is `Syncing` > `Failed` > `Offline` > `Idle` (an
    /// in-flight cycle is the operationally dominant fact; a `Failed`
    /// tie breaks by lowest board id — `BTreeMap` order). `last_success`
    /// is the max over rows, `pending_ops` the saturating sum. The empty
    /// fold is `SyncStatus::default()` — exactly the singleton display a
    /// fresh install shows today.
    #[must_use]
    pub fn aggregate(&self) -> SyncStatus {
        aggregate_rows(self.0.values())
    }

    /// The engine's write seam for the stored part of one row (the
    /// `UpsertSyncStatus` arm and the memory-only mark path).
    /// `pending_ops` is never settable — it is derived from the outbox,
    /// never stored.
    pub fn set_stored(
        &mut self,
        board: BoardId,
        phase: SyncPhase,
        last_success: Option<DateTime<Utc>>,
    ) {
        let row = self.0.entry(board).or_default();
        row.phase = phase;
        row.last_success = last_success;
        row.pending_ops = 0;
    }

    /// The map's own write face for tests and seeding: one row in.
    #[cfg(any(test, feature = "test-support"))]
    #[cfg_attr(not(test), allow(dead_code))]
    #[must_use]
    pub fn with_row(board: BoardId, status: SyncStatus) -> Self {
        Self([(board, status)].into_iter().collect())
    }

    /// Drops one board's row entirely — the [`PersistenceAction::
    /// DeleteSyncStatus`](crate::PersistenceAction::DeleteSyncStatus)
    /// arm. For a board still present in the entity maps, the batch-end
    /// derivation re-materializes a default row; only the *stored* part
    /// (phase, last success) is what the delete clears there.
    pub fn remove(&mut self, board: BoardId) {
        self.0.remove(&board);
    }

    /// The memory-only `Syncing` transient: flips one row's phase in
    /// place, preserving the derived `pending_ops` and the stored
    /// `last_success` (the displayed queue must stay honest while a
    /// cycle runs). Returns whether the phase changed. Distinct from
    /// [`Self::set_stored`], which writes the *stored* part and is
    /// always followed by a fresh derivation.
    pub fn mark_syncing(&mut self, board: BoardId) -> bool {
        let row = self.0.entry(board).or_default();
        if row.phase == SyncPhase::Syncing {
            return false;
        }
        row.phase = SyncPhase::Syncing;
        true
    }
}

/// Operational dominance of a phase inside the aggregate fold.
fn phase_rank(phase: SyncPhase) -> u8 {
    match phase {
        SyncPhase::Idle => 0,
        SyncPhase::Offline => 1,
        SyncPhase::Failed { .. } => 2,
        SyncPhase::Syncing => 3,
    }
}

/// The aggregate fold over an arbitrary *subset* of rows (D6-4): the
/// same precedence/max/saturating-sum rules as
/// [`SyncStates::aggregate`], exposed pure so the surfaces can fold a
/// connector's boards (attribution derived app-side) without
/// duplicating the fold logic. The empty fold is
/// [`SyncStatus::default`].
#[must_use]
pub fn aggregate_rows<'a>(rows: impl Iterator<Item = &'a SyncStatus>) -> SyncStatus {
    let mut aggregated = SyncStatus::default();
    for status in rows {
        // A strictly higher rank replaces; an equal rank keeps the
        // first (a `Failed` tie therefore keeps the lowest board id,
        // since callers iterating a `BTreeMap` feed sorted rows).
        if phase_rank(status.phase) > phase_rank(aggregated.phase) {
            aggregated.phase = status.phase;
        }
        aggregated.last_success = std::cmp::max(aggregated.last_success, status.last_success);
        aggregated.pending_ops = aggregated.pending_ops.saturating_add(status.pending_ops);
        aggregated.counters.fold_into(&status.counters);
    }
    aggregated
}

/// Recomputes the per-board sync projection: materializes a default row
/// for every board, then sets each row's `pending_ops` to the count of
/// outbox ops attributed to that board through the entity graph (see
/// [`attributed_board`]). Stored phase/`last_success` are preserved;
/// `pending_ops` is never stored — the outbox is the single source of
/// truth. A display derivation, like the global depth it replaces: the
/// materialized rows are never written to storage.
pub fn derive_sync_states(
    boards: &BTreeMap<BoardId, Board>,
    stacks: &BTreeMap<StackId, Stack>,
    tasks: &BTreeMap<TaskId, Task>,
    labels: &BTreeMap<LabelId, Label>,
    outbox: &[PendingOp],
    sync: &mut SyncStates,
) {
    for board in boards.keys() {
        sync.0.entry(*board).or_default();
    }
    let mut counts: BTreeMap<BoardId, u32> = BTreeMap::new();
    for op in outbox {
        if let Some(board) = attributed_board(&op.op, stacks, tasks, labels) {
            let count = counts.entry(board).or_default();
            *count = count.saturating_add(1);
        }
    }
    for row in sync.0.values_mut() {
        row.pending_ops = 0;
    }
    for (board, count) in counts {
        sync.0.entry(board).or_default().pending_ops = count;
    }
}

/// The board one outbox op's effects belong to, resolved through the
/// entity graph (board ops → themselves; stack ops → their board; task
/// ops → their stack's board; label ops → their board). The same filter
/// `read_scope` implements app-side; the derivation lives in the domain,
/// so the attribution is defined here once. `None` = the op's entity is
/// absent from the maps (a torn state cannot attribute it).
#[must_use]
pub fn attributed_board(
    op: &LocalOp,
    stacks: &BTreeMap<StackId, Stack>,
    tasks: &BTreeMap<TaskId, Task>,
    labels: &BTreeMap<LabelId, Label>,
) -> Option<BoardId> {
    match op {
        LocalOp::UpdateBoard(board) | LocalOp::DeleteBoard(board) => Some(*board),
        LocalOp::CreateStack(stack)
        | LocalOp::RenameStack(stack)
        | LocalOp::MoveStack(stack)
        | LocalOp::DeleteStack(stack) => stacks.get(stack).map(|s| s.board),
        LocalOp::CreateTask(task)
        | LocalOp::UpdateTask(task)
        | LocalOp::MoveTask(task)
        | LocalOp::DeleteTask(task)
        | LocalOp::AssignLabel(task, _)
        | LocalOp::UpdateAppData(task)
        | LocalOp::UnassignLabel(task, _) => tasks
            .get(task)
            .and_then(|t| stacks.get(&t.stack))
            .map(|s| s.board),
        LocalOp::CreateLabel(label) | LocalOp::UpdateLabel(label) | LocalOp::DeleteLabel(label) => {
            labels.get(label).map(|l| l.board)
        }
    }
}

/// The app-global Inbox board's title (#347, D-V5 v2): the exact-title
/// adoption key for [`AppState::inbox_board`] and the created board's
/// title.
pub const INBOX_BOARD_TITLE: &str = "Inbox";

/// The board-directed capture landing stack's title: the exact-title
/// key for [`AppState::backlog_stack`] and the created stack's title.
pub const BACKLOG_STACK_TITLE: &str = "Backlog";

impl AppState {
    /// The app-global Inbox: the canonical-first (smallest
    /// [`BoardId`] — boards carry no `order` field, so the canonical
    /// board order *is* id order) live board titled exactly
    /// [`INBOX_BOARD_TITLE`]. "Live" = `!deleted ∧ !archived` (the
    /// one canonical live-board set the board bar, walk, ring, and
    /// find all use); an archived `Inbox`-titled board is not adopted.
    #[must_use]
    pub fn inbox_board(&self) -> Option<BoardId> {
        self.boards
            .values()
            .find(|b| !b.deleted && !b.archived && b.title == INBOX_BOARD_TITLE)
            .map(|b| b.id)
    }

    /// The board's canonical-first stack: smallest `(order, id)` among
    /// the board's non-tombstoned stacks (`stacks_in_board`'s order).
    #[must_use]
    pub fn canonical_first_stack(&self, board: BoardId) -> Option<StackId> {
        self.stacks
            .values()
            .filter(|s| s.board == board && !s.deleted)
            .map(|s| (s.order, s.id))
            .min()
            .map(|(_, id)| id)
    }

    /// The board's Backlog: the canonical-first stack titled exactly
    /// [`BACKLOG_STACK_TITLE`] among the board's non-tombstoned
    /// stacks; `None` when no such stack exists (created on demand by
    /// the capture handler).
    #[must_use]
    pub fn backlog_stack(&self, board: BoardId) -> Option<StackId> {
        self.stacks
            .values()
            .filter(|s| s.board == board && !s.deleted && s.title == BACKLOG_STACK_TITLE)
            .map(|s| (s.order, s.id))
            .min()
            .map(|(_, id)| id)
    }

    /// Every board carrying a remote binding — live and tombstoned
    /// alike (a tombstoned board's finalizing push is a sync fact), in
    /// deterministic board-id order.
    #[must_use]
    pub fn bound_boards(&self) -> Vec<BoardId> {
        self.boards
            .values()
            .filter(|b| b.remote.is_some())
            .map(|b| b.id)
            .collect()
    }

    /// Boards bound to one specific connector — live and tombstoned
    /// alike, in deterministic board-id order (the per-connector
    /// analogue of [`Self::bound_boards`]; the engine's scoped
    /// offline write derives its target set from this).
    #[must_use]
    pub fn bound_boards_of(&self, connector: &ConnectorId) -> Vec<BoardId> {
        self.boards
            .values()
            .filter(|b| {
                b.remote
                    .as_ref()
                    .is_some_and(|binding| &binding.connector == connector)
            })
            .map(|b| b.id)
            .collect()
    }

    /// Live (non-tombstoned) tasks, in canonical order: `order` ascending,
    /// ties broken by task id ascending — the single ordering every UI and
    /// test uses.
    #[must_use]
    pub fn live_tasks(&self) -> Vec<&Task> {
        Self::sorted_tasks(self.tasks.values().filter(|t| t.is_live()))
    }

    /// Live tasks of one stack, in canonical order.
    #[must_use]
    pub fn tasks_in_stack(&self, stack: StackId) -> Vec<&Task> {
        Self::sorted_tasks(
            self.tasks
                .values()
                .filter(|t| t.is_live() && t.stack == stack),
        )
    }

    /// Canonical ordering: `order` asc, tie-break id asc.
    fn sorted_tasks<'a, I>(tasks: I) -> Vec<&'a Task>
    where
        I: IntoIterator<Item = &'a Task>,
    {
        let mut tasks: Vec<&Task> = tasks.into_iter().collect();
        tasks.sort_by_key(|t| (t.order, t.id));
        tasks
    }
}

#[cfg(test)]
mod sync_states_tests {
    use std::collections::BTreeSet;

    use super::*;
    use crate::entities::{BoardClocks, Color, LabelClocks, StackClocks, TaskClocks};
    use crate::ids::ConnectorId;
    use crate::outbox::{OpId, PendingOp};
    use chrono::TimeZone;
    use proptest::prelude::*;
    use uuid::Uuid;

    fn ts(secs: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(secs, 0).unwrap()
    }

    fn board_id(raw: u128) -> BoardId {
        BoardId::from(Uuid::from_u128(raw))
    }

    fn status(phase: SyncPhase, last_success: Option<DateTime<Utc>>, pending: u32) -> SyncStatus {
        SyncStatus {
            phase,
            last_success,
            pending_ops: pending,
            counters: SyncCounters::default(),
            ..SyncStatus::default()
        }
    }

    fn failed(kind: SyncErrorKind) -> SyncPhase {
        SyncPhase::Failed { last_error: kind }
    }

    fn map_with(rows: Vec<(u128, SyncStatus)>) -> SyncStates {
        let mut sync = SyncStates::new();
        for (raw, status) in rows {
            sync.0.insert(board_id(raw), status);
        }
        sync
    }

    #[test]
    fn empty_map_aggregates_to_the_default_status() {
        assert_eq!(SyncStates::new().aggregate(), SyncStatus::default());
    }

    #[test]
    fn aggregate_rows_folds_a_subset_with_the_same_rules() {
        // Dominance, max, and saturating sum over an arbitrary subset —
        // the per-connector fold the surfaces derive with (D6-4).
        let rows = [
            status(SyncPhase::Idle, Some(ts(100)), 3),
            status(failed(SyncErrorKind::Auth), Some(ts(200)), 4),
            status(SyncPhase::Offline, None, u32::MAX),
        ];
        let folded = super::aggregate_rows(rows.iter());
        assert_eq!(folded.phase, failed(SyncErrorKind::Auth));
        assert_eq!(folded.last_success, Some(ts(200)));
        assert_eq!(folded.pending_ops, u32::MAX, "saturating");

        // A two-row subset folds to exactly the two-row aggregate.
        let folded = super::aggregate_rows(rows[..2].iter());
        assert_eq!(
            folded,
            status(failed(SyncErrorKind::Auth), Some(ts(200)), 7)
        );

        // The empty fold is the default.
        assert_eq!(
            super::aggregate_rows(std::iter::empty()),
            SyncStatus::default()
        );
    }

    #[test]
    fn sync_counters_default_is_zeroed() {
        assert_eq!(
            SyncCounters::default(),
            SyncCounters {
                cycles: 0,
                pushes: 0,
                pulls: 0,
                failures: 0,
                offline_episodes: 0,
                last_cycle: None,
            }
        );
        assert_eq!(SyncStatus::default().counters, SyncCounters::default());
    }

    #[test]
    fn aggregate_rows_folds_counters_saturating_and_takes_max_last_cycle() {
        let mut big = status(SyncPhase::Idle, None, 0);
        big.counters = SyncCounters {
            cycles: u64::MAX,
            pushes: 1,
            pulls: 2,
            failures: 3,
            offline_episodes: 4,
            last_cycle: Some(std::time::Duration::from_millis(100)),
        };
        let mut small = status(SyncPhase::Idle, None, 0);
        small.counters = SyncCounters {
            cycles: 10,
            pushes: 1,
            pulls: 0,
            failures: 0,
            offline_episodes: 4,
            last_cycle: Some(std::time::Duration::from_millis(250)),
        };
        let folded = super::aggregate_rows([big, small].iter());
        assert_eq!(folded.counters.cycles, u64::MAX, "saturates, never wraps");
        assert_eq!(folded.counters.pushes, 2);
        assert_eq!(folded.counters.pulls, 2);
        assert_eq!(folded.counters.failures, 3);
        assert_eq!(folded.counters.offline_episodes, 8);
        assert_eq!(
            folded.counters.last_cycle,
            Some(std::time::Duration::from_millis(250)),
            "the aggregate's last cycle is the longest of the boards'"
        );
    }

    #[test]
    fn aggregate_delegates_to_aggregate_rows() {
        // One fold logic, one home: the map method is the free function
        // over all rows (equality pin over mixed inputs).
        let sync = map_with(vec![
            (1, status(SyncPhase::Syncing, Some(ts(50)), 2)),
            (2, status(failed(SyncErrorKind::Network), Some(ts(90)), 5)),
        ]);
        assert_eq!(sync.aggregate(), super::aggregate_rows(sync.0.values()));
    }

    #[test]
    fn a_single_row_folds_to_itself_bit_for_bit() {
        let sync = map_with(vec![(1, status(SyncPhase::Offline, Some(ts(100)), 7))]);
        assert_eq!(
            sync.aggregate(),
            status(SyncPhase::Offline, Some(ts(100)), 7)
        );
    }

    #[test]
    fn mark_syncing_flips_only_the_phase_and_is_idempotent() {
        let mut sync = map_with(vec![(
            1,
            status(SyncPhase::Idle, Some(ts(100)), 7), // pending: derived value in memory
        )]);
        assert!(sync.mark_syncing(board_id(1)));
        assert_eq!(
            sync.get(board_id(1)),
            Some(&status(SyncPhase::Syncing, Some(ts(100)), 7)),
            "the transient must not erase the derived pending count or the stamp"
        );
        assert!(!sync.mark_syncing(board_id(1)), "repeat is a no-op");
    }

    #[test]
    fn aggregate_phase_precedence_is_syncing_failed_offline_idle() {
        let cases: Vec<(SyncPhase, SyncPhase, SyncPhase)> = vec![
            (SyncPhase::Syncing, SyncPhase::Idle, SyncPhase::Syncing),
            (SyncPhase::Idle, SyncPhase::Syncing, SyncPhase::Syncing),
            (
                failed(SyncErrorKind::Network),
                SyncPhase::Offline,
                failed(SyncErrorKind::Network),
            ),
            (
                SyncPhase::Offline,
                failed(SyncErrorKind::Network),
                failed(SyncErrorKind::Network),
            ),
            (
                SyncPhase::Syncing,
                failed(SyncErrorKind::Auth),
                SyncPhase::Syncing,
            ),
            (
                failed(SyncErrorKind::Auth),
                SyncPhase::Syncing,
                SyncPhase::Syncing,
            ),
            (SyncPhase::Idle, SyncPhase::Offline, SyncPhase::Offline),
        ];
        for (a, b, expected) in cases {
            let sync = map_with(vec![(1, status(a, None, 0)), (2, status(b, None, 0))]);
            assert_eq!(sync.aggregate().phase, expected, "{a:?} vs {b:?}");
        }
    }

    #[test]
    fn a_failed_tie_keeps_the_lowest_board_id() {
        let sync = map_with(vec![
            (9, status(failed(SyncErrorKind::Auth), None, 0)),
            (3, status(failed(SyncErrorKind::Network), None, 0)),
        ]);
        assert_eq!(
            sync.aggregate().phase,
            failed(SyncErrorKind::Network),
            "board 3 sorts below board 9"
        );
    }

    #[test]
    fn aggregate_takes_max_last_success_and_saturating_sum_pending() {
        let sync = map_with(vec![
            (1, status(SyncPhase::Idle, Some(ts(300)), u32::MAX)),
            (2, status(SyncPhase::Idle, Some(ts(100)), 2)),
            (3, status(SyncPhase::Idle, None, 0)),
        ]);
        let aggregated = sync.aggregate();
        assert_eq!(aggregated.last_success, Some(ts(300)));
        assert_eq!(aggregated.pending_ops, u32::MAX, "saturates, never wraps");
    }

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(256))]

        #[test]
        fn aggregate_sum_is_conserved_and_deterministic(
            rows in prop::collection::vec(
                (0_u128..8, any::<u32>(), prop::option::of(Just(()))),
                0..12,
            ),
            phases in prop::collection::vec(
                prop_oneof![
                    Just(SyncPhase::Idle),
                    Just(SyncPhase::Syncing),
                    Just(SyncPhase::Offline),
                    Just(failed(SyncErrorKind::Network)),
                    Just(failed(SyncErrorKind::Server)),
                ],
                0..12,
            ),
        ) {
            let stamped: Vec<(u128, SyncStatus)> = rows
                .into_iter()
                .zip(phases)
                .map(|((raw, pending, _), phase)| {
                    (raw, status(phase, None, pending))
                })
                .collect();
            let sync = map_with(stamped);
            let first = sync.aggregate();
            let second = sync.aggregate();
            prop_assert_eq!(&first, &second, "deterministic fold");
            let total: u32 = sync
                .iter()
                .map(|(_, row)| row.pending_ops)
                .fold(0_u32, u32::saturating_add);
            prop_assert_eq!(first.pending_ops, total, "sum conservation");
        }

        #[test]
        fn aggregate_counter_sums_are_conserved(
            rows in prop::collection::vec(
                (
                    0_u128..8,
                    any::<u64>(),
                    any::<u64>(),
                    any::<u64>(),
                    any::<u64>(),
                    any::<u64>(),
                    prop::option::of(Just(())),
                ),
                0..12,
            ),
            phases in prop::collection::vec(
                prop_oneof![Just(SyncPhase::Idle), Just(SyncPhase::Offline)],
                0..12,
            ),
        ) {
            let stamped: Vec<(u128, SyncStatus)> = rows
                .into_iter()
                .zip(phases)
                .map(
                    |((raw, cycles, pushes, pulls, failures, episodes, _), phase)| {
                        let mut status = status(phase, None, 0);
                        status.counters = SyncCounters {
                            cycles,
                            pushes,
                            pulls,
                            failures,
                            offline_episodes: episodes,
                            last_cycle: Some(std::time::Duration::from_secs(
                                cycles % 1_000,
                            )),
                        };
                        (raw, status)
                    },
                )
                .collect();
            let sync = map_with(stamped);
            let folded = sync.aggregate();
            let sum = |pick: fn(&SyncCounters) -> u64| {
                sync.iter()
                    .map(|(_, row)| pick(&row.counters))
                    .fold(0_u64, u64::saturating_add)
            };
            prop_assert_eq!(folded.counters.cycles, sum(|c| c.cycles));
            prop_assert_eq!(folded.counters.pushes, sum(|c| c.pushes));
            prop_assert_eq!(folded.counters.pulls, sum(|c| c.pulls));
            prop_assert_eq!(folded.counters.failures, sum(|c| c.failures));
            prop_assert_eq!(
                folded.counters.offline_episodes,
                sum(|c| c.offline_episodes)
            );
            let max_last = sync
                .iter()
                .map(|(_, row)| row.counters.last_cycle)
                .fold(None, std::cmp::max);
            prop_assert_eq!(folded.counters.last_cycle, max_last, "max law");
        }

        #[test]
        fn connector_id_roundtrips_through_serde(raw in ".*") {
            let connector = ConnectorId::from(raw.as_str());
            let json = serde_json::to_string(&connector).unwrap();
            let back: ConnectorId = serde_json::from_str(&json).unwrap();
            prop_assert_eq!(&back, &connector);
            prop_assert_eq!(back.as_str(), raw.as_str());
            prop_assert_eq!(connector.to_string(), raw.clone());
            prop_assert_eq!(ConnectorId::from(raw.as_str()), connector);
        }
    }

    /// The entity fixture: two boards (bound first), one stack per board,
    /// one task per stack, one label on the first board.
    type EntityMaps = (
        BTreeMap<BoardId, Board>,
        BTreeMap<StackId, Stack>,
        BTreeMap<TaskId, Task>,
        BTreeMap<LabelId, Label>,
    );

    fn fixture() -> EntityMaps {
        let clocks = BoardClocks {
            title: ts(100),
            color: ts(100),
            archived: ts(100),
            deleted: ts(100),
        };
        let board = |raw: u128| Board {
            id: board_id(raw),
            remote: crate::test_support::test_binding(crate::ids::RemoteBoardId(
                u64::try_from(raw).unwrap(),
            )),
            title: format!("board {raw}"),
            color: Color::new("0000ff"),
            archived: false,
            deleted: false,
            clocks,
            remote_seen: None,
        };
        let stack = |raw: u128, b: u128| Stack {
            id: StackId::from(Uuid::from_u128(raw)),
            remote: None,
            board: board_id(b),
            title: "stack".into(),
            order: 0,
            archived: false,
            deleted: false,
            clocks: StackClocks {
                title: ts(100),
                order: ts(100),
                deleted: ts(100),
            },
            remote_seen: None,
        };
        let task = |raw: u128, s: u128| Task {
            id: TaskId::from(Uuid::from_u128(raw)),
            remote: None,
            title: "task".into(),
            description: String::new(),
            duedate: None,
            scheduled: None,
            done: None,
            stack: StackId::from(Uuid::from_u128(s)),
            order: 0,
            labels: BTreeSet::default(),
            effort: None,
            importance: None,
            urgency: None,
            archived: false,
            deleted: false,
            clocks: TaskClocks {
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
        let label = Label {
            id: LabelId::from(Uuid::from_u128(41)),
            remote: None,
            board: board_id(1),
            title: "label".into(),
            color: Color::new("00ff00"),
            deleted: false,
            clocks: LabelClocks {
                title: ts(100),
                color: ts(100),
                deleted: ts(100),
            },
            remote_seen: None,
        };
        let mut boards = BTreeMap::new();
        boards.insert(board_id(1), board(1));
        boards.insert(board_id(2), board(2));
        let mut stacks = BTreeMap::new();
        stacks.insert(StackId::from(Uuid::from_u128(11)), stack(11, 1));
        stacks.insert(StackId::from(Uuid::from_u128(21)), stack(21, 2));
        let mut tasks = BTreeMap::new();
        tasks.insert(TaskId::from(Uuid::from_u128(31)), task(31, 11));
        tasks.insert(TaskId::from(Uuid::from_u128(32)), task(32, 21));
        let mut labels = BTreeMap::new();
        labels.insert(LabelId::from(Uuid::from_u128(41)), label);
        (boards, stacks, tasks, labels)
    }

    fn op(raw: u128, op: LocalOp) -> PendingOp {
        PendingOp {
            op_id: OpId(Uuid::from_u128(raw)),
            op,
            queued_at: ts(200),
        }
    }

    #[test]
    fn derivation_materializes_every_board_and_recomputes_pending_per_board() {
        let (boards, stacks, tasks, labels) = fixture();
        let outbox = vec![
            op(1, LocalOp::UpdateTask(TaskId::from(Uuid::from_u128(31)))),
            op(2, LocalOp::DeleteStack(StackId::from(Uuid::from_u128(21)))),
            op(3, LocalOp::CreateLabel(LabelId::from(Uuid::from_u128(41)))),
            op(4, LocalOp::UpdateBoard(board_id(2))),
        ];
        let mut sync = SyncStates::new();
        sync.set_stored(board_id(2), SyncPhase::Offline, Some(ts(50)));
        derive_sync_states(&boards, &stacks, &tasks, &labels, &outbox, &mut sync);

        // Every board materialized; stored phase/last_success preserved.
        assert_eq!(sync.iter().count(), 2);
        let first = sync.get(board_id(1)).expect("materialized row");
        assert_eq!(first.phase, SyncPhase::Idle);
        assert_eq!(first.last_success, None);
        assert_eq!(first.pending_ops, 2, "task op + label op");
        let second = sync.get(board_id(2)).expect("stored row");
        assert_eq!(second.phase, SyncPhase::Offline);
        assert_eq!(second.last_success, Some(ts(50)));
        assert_eq!(second.pending_ops, 2, "stack op + board op");
        // The aggregate conserves the total depth.
        assert_eq!(sync.aggregate().pending_ops, 4);
    }

    #[test]
    fn derivation_keeps_rows_of_boards_absent_from_the_map() {
        let (boards, stacks, tasks, labels) = fixture();
        let mut sync = SyncStates::new();
        sync.set_stored(board_id(99), SyncPhase::Syncing, None);
        derive_sync_states(&boards, &stacks, &tasks, &labels, &[], &mut sync);
        let orphan = sync.get(board_id(99)).expect("orphan row kept");
        assert_eq!(orphan.phase, SyncPhase::Syncing);
        assert_eq!(orphan.pending_ops, 0);
    }

    proptest! {
        #![proptest_config(crate::test_support::proptest_config(256))]

        #[test]
        fn every_outbox_op_attributes_to_exactly_one_board(
            state in crate::test_support::persisted_state_strategy(),
        ) {
            let mut sync = SyncStates::new();
            derive_sync_states(
                &state.boards,
                &state.stacks,
                &state.tasks,
                &state.labels,
                &state.outbox,
                &mut sync,
            );
            // No-ceiling pin: no op is silently uncounted.
            let total: u32 = sync
                .iter()
                .map(|(_, row)| row.pending_ops)
                .fold(0_u32, u32::saturating_add);
            let depth = u32::try_from(state.outbox.len()).unwrap_or(u32::MAX);
            prop_assert_eq!(total, depth, "per-board counts sum to the outbox depth");
            // Every board carries a row.
            for board in state.boards.keys() {
                prop_assert!(sync.get(*board).is_some());
            }
        }
    }

    #[test]
    fn bound_boards_include_tombstones_and_sort_by_id() {
        let mut state = AppState::default();
        let mut tombstoned = crate::entities::Board {
            id: board_id(5),
            remote: crate::test_support::test_binding(crate::ids::RemoteBoardId(50)),
            title: "old".into(),
            color: Color::new("0000ff"),
            archived: false,
            deleted: true,
            clocks: BoardClocks {
                title: ts(100),
                color: ts(100),
                archived: ts(100),
                deleted: ts(100),
            },
            remote_seen: None,
        };
        let unbound = crate::entities::Board {
            id: board_id(6),
            remote: None,
            ..tombstoned.clone()
        };
        let bound = crate::entities::Board {
            id: board_id(2),
            ..tombstoned.clone()
        };
        tombstoned.deleted = false;
        state.boards.insert(tombstoned.id, tombstoned);
        state.boards.insert(unbound.id, unbound);
        state.boards.insert(bound.id, bound);
        assert_eq!(state.bound_boards(), vec![board_id(2), board_id(5)]);
    }

    #[test]
    fn bound_boards_of_filters_by_connector_tag() {
        let mut state = AppState::default();
        let make_board = |id: u64, connector: Option<crate::ids::ConnectorId>| {
            let remote = connector.map(|connector| crate::ids::RemoteBoardBinding {
                connector,
                board: crate::ids::RemoteBoardId(id * 10),
            });
            let mut board = crate::entities::Board {
                id: board_id(id.into()),
                remote,
                title: "b".into(),
                color: Color::new("0000ff"),
                archived: false,
                deleted: false,
                clocks: BoardClocks {
                    title: ts(100),
                    color: ts(100),
                    archived: ts(100),
                    deleted: ts(100),
                },
                remote_seen: None,
            };
            if id == 6 {
                board.deleted = true; // a tombstone stays in the scoped set
            }
            board
        };
        state.boards.insert(
            board_id(2),
            make_board(2, Some(crate::test_support::test_connector())),
        );
        state
            .boards
            .insert(board_id(5), make_board(5, Some(ConnectorId::from("beta"))));
        state.boards.insert(
            board_id(6),
            make_board(6, Some(crate::test_support::test_connector())),
        );
        state.boards.insert(board_id(7), make_board(7, None));

        let work = crate::test_support::test_connector();
        assert_eq!(state.bound_boards_of(&work), vec![board_id(2), board_id(6)]);
        assert_eq!(
            state.bound_boards_of(&ConnectorId::from("beta")),
            vec![board_id(5)]
        );
        assert_eq!(state.bound_boards_of(&ConnectorId::from("gamma")), []);
    }
}

#[cfg(test)]
mod tests {
    use super::AppState;
    use crate::entities::{BoardClocks, Color, StackClocks};
    use crate::ids::{BoardId, StackId, TaskId};
    use crate::test_support;
    use chrono::TimeZone;
    use proptest::prelude::*;

    fn ts(secs: i64) -> chrono::DateTime<chrono::Utc> {
        chrono::Utc.timestamp_opt(secs, 0).unwrap()
    }

    fn board(raw: u128, title: &str) -> crate::entities::Board {
        crate::entities::Board {
            id: BoardId::from(uuid::Uuid::from_u128(raw)),
            remote: None,
            title: title.into(),
            color: Color::new("0082c9"),
            archived: false,
            deleted: false,
            clocks: BoardClocks {
                title: ts(100),
                color: ts(100),
                archived: ts(100),
                deleted: ts(100),
            },
            remote_seen: None,
        }
    }

    fn stack(raw: u128, on: BoardId, title: &str, order: i64) -> crate::entities::Stack {
        crate::entities::Stack {
            id: StackId::from(uuid::Uuid::from_u128(raw)),
            remote: None,
            board: on,
            title: title.into(),
            order,
            archived: false,
            deleted: false,
            clocks: StackClocks {
                title: ts(100),
                order: ts(100),
                deleted: ts(100),
            },
            remote_seen: None,
        }
    }

    #[test]
    fn inbox_board_adopts_the_canonical_first_exact_titled_live_board() {
        let mut state = AppState::default();
        assert_eq!(state.inbox_board(), None, "an empty state has no inbox");
        let late = board(9, "Inbox");
        let early = board(2, "Inbox");
        state.boards.insert(late.id, late);
        assert_eq!(
            state.inbox_board(),
            Some(BoardId::from(uuid::Uuid::from_u128(9)))
        );
        state.boards.insert(early.id, early);
        assert_eq!(
            state.inbox_board(),
            Some(BoardId::from(uuid::Uuid::from_u128(2))),
            "the smallest board id wins"
        );
        let decoy = board(1, "Inbox items");
        state.boards.insert(decoy.id, decoy);
        assert_eq!(
            state.inbox_board(),
            Some(BoardId::from(uuid::Uuid::from_u128(2))),
            "titles match exactly"
        );
    }

    #[test]
    fn an_archived_or_deleted_inbox_titled_board_is_not_adopted() {
        let mut state = AppState::default();
        let mut archived = board(3, "Inbox");
        archived.archived = true;
        state.boards.insert(archived.id, archived);
        assert_eq!(state.inbox_board(), None, "archived is not live");
        let mut deleted = board(4, "Inbox");
        deleted.deleted = true;
        state.boards.insert(deleted.id, deleted);
        assert_eq!(state.inbox_board(), None);
    }

    #[test]
    fn canonical_first_stack_picks_smallest_order_then_id() {
        let mut state = AppState::default();
        let on = board(5, "b");
        state.boards.insert(on.id, on.clone());
        state.stacks.insert(
            StackId::from(uuid::Uuid::from_u128(302)),
            stack(302, on.id, "todo", 0),
        );
        state.stacks.insert(
            StackId::from(uuid::Uuid::from_u128(101)),
            stack(101, on.id, "doing", 1),
        );
        assert_eq!(
            state.canonical_first_stack(on.id),
            Some(StackId::from(uuid::Uuid::from_u128(302))),
            "order dominates id"
        );
        let mut tie = stack(201, on.id, "done", 0);
        state.stacks.insert(tie.id, tie.clone());
        assert_eq!(
            state.canonical_first_stack(on.id),
            Some(StackId::from(uuid::Uuid::from_u128(201))),
            "a tie breaks on the smaller stack id"
        );
        tie.deleted = true;
        state.stacks.insert(tie.id, tie);
        assert_eq!(
            state.canonical_first_stack(on.id),
            Some(StackId::from(uuid::Uuid::from_u128(302))),
            "tombstoned stacks are skipped"
        );
    }

    #[test]
    fn backlog_stack_matches_exactly_titled_backlogs_only() {
        let mut state = AppState::default();
        let on = board(5, "b");
        state.boards.insert(on.id, on.clone());
        assert_eq!(state.backlog_stack(on.id), None);
        state.stacks.insert(
            StackId::from(uuid::Uuid::from_u128(301)),
            stack(301, on.id, "Backlog items", 0),
        );
        assert_eq!(state.backlog_stack(on.id), None, "exact titles only");
        let want = stack(401, on.id, "Backlog", 2);
        state.stacks.insert(want.id, want.clone());
        assert_eq!(state.backlog_stack(on.id), Some(want.id));
    }

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(256))]

        #[test]
        fn live_tasks_exclude_tombstones_and_sort_canonically(
            state in test_support::app_state_strategy(),
        ) {
            // Exact-set equality, not a filter: an empty result must be
            // distinguishable from the real answer.
            let mut expected: Vec<TaskId> = state
                .tasks
                .values()
                .filter(|t| t.is_live())
                .map(|t| t.id)
                .collect();
            expected.sort_by_key(|id| {
                let t = &state.tasks[id];
                (t.order, *id)
            });
            let live = state.live_tasks();
            prop_assert_eq!(live.iter().map(|t| t.id).collect::<Vec<_>>(), expected);
            for pair in live.windows(2) {
                prop_assert!(pair[0].order < pair[1].order
                    || (pair[0].order == pair[1].order && pair[0].id < pair[1].id));
            }
        }

        #[test]
        fn tasks_in_stack_only_contains_that_stack(
            state in test_support::app_state_strategy(),
        ) {
            for stack_id in state.stacks.keys() {
                // Exact membership, in canonical order.
                let mut expected: Vec<TaskId> = state
                    .tasks
                    .values()
                    .filter(|t| t.is_live() && t.stack == *stack_id)
                    .map(|t| t.id)
                    .collect();
                expected.sort_by_key(|id| {
                    let t = &state.tasks[id];
                    (t.order, *id)
                });
                let got = state.tasks_in_stack(*stack_id);
                prop_assert_eq!(got.iter().map(|t| t.id).collect::<Vec<_>>(), expected);
                for task in &got {
                    prop_assert_eq!(task.stack, *stack_id);
                }
            }
        }

        #[test]
        fn app_state_serialization_is_deterministic(state in test_support::app_state_strategy()) {
            let first = serde_json::to_string(&state).unwrap();
            let second = serde_json::to_string(&state).unwrap();
            prop_assert_eq!(first, second);
        }
    }
}
