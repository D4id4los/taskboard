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
        let mut aggregated = SyncStatus::default();
        for status in self.0.values() {
            // A strictly higher rank replaces; an equal rank keeps the
            // first (a `Failed` tie therefore keeps the lowest board id,
            // since iteration is sorted).
            if phase_rank(status.phase) > phase_rank(aggregated.phase) {
                aggregated.phase = status.phase;
            }
            aggregated.last_success = std::cmp::max(aggregated.last_success, status.last_success);
            aggregated.pending_ops = aggregated.pending_ops.saturating_add(status.pending_ops);
        }
        aggregated
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
        | LocalOp::UnassignLabel(task, _) => tasks
            .get(task)
            .and_then(|t| stacks.get(&t.stack))
            .map(|s| s.board),
        LocalOp::CreateLabel(label) | LocalOp::UpdateLabel(label) | LocalOp::DeleteLabel(label) => {
            labels.get(label).map(|l| l.board)
        }
    }
}

impl AppState {
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
            done: None,
            stack: StackId::from(Uuid::from_u128(s)),
            order: 0,
            labels: BTreeSet::default(),
            archived: false,
            deleted: false,
            clocks: TaskClocks {
                title: ts(100),
                description: ts(100),
                duedate: ts(100),
                done: ts(100),
                position: ts(100),
                labels: ts(100),
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
    use crate::ids::TaskId;
    use crate::test_support;
    use proptest::prelude::*;

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
