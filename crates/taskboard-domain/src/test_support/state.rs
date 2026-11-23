// SPDX-License-Identifier: MIT OR Apache-2.0
//! Strategies for state-level shapes: the full `AppState` (referentially
//! consistent), the persisted dataset, and sync phases.

use std::collections::BTreeMap;

use chrono::{DateTime, TimeZone, Utc};
use proptest::prelude::*;

use super::entities::{label_clocks_strategy, stack_clocks_strategy, task_clocks_strategy};
use super::{
    board_id_strategy, label_id_strategy, op_id_strategy, remote_number, stack_id_strategy,
    string_strategy, task_id_strategy, timestamp_strategy,
};
use crate::entities::{Board, Color, Label, Stack, Task};
use crate::ids::{
    LabelId, RemoteBoardId, RemoteCardId, RemoteCardRef, RemoteLabelId, RemoteLabelRef,
    RemoteStackId, RemoteStackRef, StackId, TaskId,
};
use crate::outbox::{LocalOp, PendingOp};
use crate::persistence::{PersistedState, SyncValidators, ValidatorKey};
use crate::state::{AppState, SyncErrorKind, SyncPhase, SyncStates};

/// A remotely-created entity bound with `board_remote` (or a fixed
/// fallback when the board itself is unbound, keeping refs well-formed).
fn board_remote_or(board_remote: Option<RemoteBoardId>) -> RemoteBoardId {
    board_remote.unwrap_or(RemoteBoardId::deck(1))
}

/// One row of [`app_state_strategy`]'s board.
#[derive(Debug, Clone)]
pub struct BoardRow {
    /// Local board id.
    pub id: crate::ids::BoardId,
    /// Remote binding.
    pub remote: Option<RemoteBoardId>,
    /// Board title.
    pub title: String,
    /// Raw color string.
    pub color: String,
    /// Archived flag.
    pub archived: bool,
    /// Tombstone flag.
    pub deleted: bool,
    /// Remote baseline.
    pub remote_seen: Option<DateTime<Utc>>,
}

/// One row of [`app_state_strategy`]'s stacks.
#[derive(Debug, Clone)]
pub struct StackRow {
    /// Local stack id.
    pub id: StackId,
    /// Remote stack number (`None` = unbound).
    pub remote: Option<u64>,
    /// Stack title.
    pub title: String,
    /// Sort order.
    pub order: i64,
    /// Archived flag.
    pub archived: bool,
    /// Tombstone flag.
    pub deleted: bool,
    /// Per-field write clocks.
    pub clocks: crate::entities::StackClocks,
    /// Remote baseline.
    pub remote_seen: Option<DateTime<Utc>>,
}

/// One row of [`app_state_strategy`]'s labels.
#[derive(Debug, Clone)]
pub struct LabelRow {
    /// Local label id.
    pub id: LabelId,
    /// Remote label number (`None` = unbound).
    pub remote: Option<u64>,
    /// Label title.
    pub title: String,
    /// Raw color string.
    pub color: String,
    /// Tombstone flag.
    pub deleted: bool,
    /// Per-field write clocks.
    pub clocks: crate::entities::LabelClocks,
    /// Remote baseline.
    pub remote_seen: Option<DateTime<Utc>>,
}

/// One row of [`app_state_strategy`]'s tasks. `stack_idx` indexes the
/// generated stack list and `label_idxs` the generated label list; both
/// are wrapped modulo the actual lengths during assembly, which keeps the
/// state referentially consistent.
#[derive(Debug, Clone)]
pub struct TaskRow {
    /// Local task id.
    pub id: TaskId,
    /// Remote card number (`None` = unbound/local-only).
    pub remote: Option<u64>,
    /// Task title.
    pub title: String,
    /// Task description.
    pub description: String,
    /// Due date.
    pub duedate: Option<DateTime<Utc>>,
    /// Scheduled start.
    pub scheduled: Option<DateTime<Utc>>,
    /// Completion timestamp.
    pub done: Option<DateTime<Utc>>,
    /// Index into the generated stack list.
    pub stack_idx: usize,
    /// Sort order.
    pub order: i64,
    /// Indices into the generated label list.
    pub label_idxs: Vec<usize>,
    /// Archived flag.
    pub archived: bool,
    /// Tombstone flag.
    pub deleted: bool,
    /// Per-field write clocks.
    pub clocks: crate::entities::TaskClocks,
    /// Remote baseline.
    pub remote_seen: Option<DateTime<Utc>>,
    /// The observed remote card snapshot, if any.
    pub remote_baseline: Option<crate::entities::RemoteTaskBaseline>,
}

fn board_row() -> impl Strategy<Value = BoardRow> {
    (
        board_id_strategy(),
        proptest::option::of(remote_number().prop_map(RemoteBoardId::deck)),
        string_strategy(),
        string_strategy(),
        any::<bool>(),
        any::<bool>(),
        proptest::option::of(timestamp_strategy()),
    )
        .prop_map(
            |(id, remote, title, color, archived, deleted, remote_seen)| BoardRow {
                id,
                remote,
                title,
                color,
                archived,
                deleted,
                remote_seen,
            },
        )
}

fn stack_rows() -> impl Strategy<Value = Vec<StackRow>> {
    prop::collection::vec(
        (
            stack_id_strategy(),
            proptest::option::of(remote_number()),
            string_strategy(),
            -100_i64..=100,
            any::<bool>(),
            any::<bool>(),
            stack_clocks_strategy(),
            proptest::option::of(timestamp_strategy()),
        ),
        1..5,
    )
    .prop_map(|rows| {
        rows.into_iter()
            .map(
                |(id, remote, title, order, archived, deleted, clocks, remote_seen)| StackRow {
                    id,
                    remote,
                    title,
                    order,
                    archived,
                    deleted,
                    clocks,
                    remote_seen,
                },
            )
            .collect()
    })
}

fn label_rows() -> impl Strategy<Value = Vec<LabelRow>> {
    prop::collection::vec(
        (
            label_id_strategy(),
            proptest::option::of(remote_number()),
            string_strategy(),
            string_strategy(),
            any::<bool>(),
            label_clocks_strategy(),
            proptest::option::of(timestamp_strategy()),
        ),
        0..5,
    )
    .prop_map(|rows| {
        rows.into_iter()
            .map(
                |(id, remote, title, color, deleted, clocks, remote_seen)| LabelRow {
                    id,
                    remote,
                    title,
                    color,
                    deleted,
                    clocks,
                    remote_seen,
                },
            )
            .collect()
    })
}

fn task_rows() -> impl Strategy<Value = Vec<TaskRow>> {
    prop::collection::vec(
        (
            (
                task_id_strategy(),
                proptest::option::of(remote_number()),
                string_strategy(),
                string_strategy(),
                proptest::option::of(timestamp_strategy()),
                proptest::option::of(timestamp_strategy()),
                proptest::option::of(timestamp_strategy()),
                0_usize..4,
            ),
            (
                -100_i64..=100,
                prop::collection::vec(0_usize..5, 0..3),
                // (archived, deleted)
                (any::<bool>(), any::<bool>()),
                task_clocks_strategy(),
                proptest::option::of(timestamp_strategy()),
                proptest::option::of(
                    crate::test_support::entities::remote_task_baseline_strategy(),
                ),
            ),
        ),
        0..12,
    )
    .prop_map(|rows| {
        rows.into_iter()
            .map(
                |(
                    (id, remote, title, description, duedate, scheduled, done, stack_idx),
                    (order, label_idxs, (archived, deleted), clocks, remote_seen, remote_baseline),
                )| TaskRow {
                    id,
                    remote,
                    title,
                    description,
                    duedate,
                    scheduled,
                    done,
                    stack_idx,
                    order,
                    label_idxs,
                    archived,
                    deleted,
                    clocks,
                    remote_seen,
                    remote_baseline,
                },
            )
            .collect()
    })
}

/// A full `AppState` with referentially consistent rows: one board, stacks
/// and labels bound to it, tasks bound to the stacks, task label sets
/// drawn from the board's labels.
/// The reminder-free base of [`app_state_strategy`] (kept separate so
/// the reminder fold-in reads as one composition step).
#[allow(clippy::too_many_lines)] // row structs keep it readable; length is inherent
fn raw_app_state_strategy() -> impl Strategy<Value = AppState> {
    (board_row(), stack_rows(), label_rows(), task_rows()).prop_map(
        |(board, stack_rows, label_rows, task_rows)| {
            let mut boards = BTreeMap::new();
            boards.insert(
                board.id,
                Board {
                    id: board.id,
                    remote: board.remote.map(|board| crate::ids::RemoteBoardBinding {
                        connector: crate::test_support::test_connector(),
                        board,
                    }),
                    title: board.title,
                    color: Color::new(board.color),
                    archived: board.archived,
                    deleted: board.deleted,
                    clocks: crate::entities::BoardClocks {
                        color: crate::entities::BoardClocks::epoch(),
                        archived: crate::entities::BoardClocks::epoch(),
                        deleted: crate::entities::BoardClocks::epoch(),
                        title: crate::entities::BoardClocks::epoch(),
                    },
                    authors: crate::entities::BoardAuthors::default(),
                    remote_seen: board.remote_seen,
                },
            );

            let mut stacks = BTreeMap::new();
            let stack_bindings: Vec<(StackId, Option<RemoteStackId>)> = stack_rows
                .iter()
                .map(|row| (row.id, row.remote.map(RemoteStackId::deck)))
                .collect();
            for row in stack_rows {
                stacks.insert(
                    row.id,
                    Stack {
                        id: row.id,
                        remote: row.remote.map(|num| RemoteStackRef {
                            board: board_remote_or(board.remote),
                            stack: RemoteStackId::deck(num),
                        }),
                        board: board.id,
                        title: row.title,
                        order: row.order,
                        archived: row.archived,
                        deleted: row.deleted,
                        clocks: row.clocks,
                        authors: crate::entities::StackAuthors::default(),
                        remote_seen: row.remote_seen,
                    },
                );
            }

            let mut labels = BTreeMap::new();
            let label_ids: Vec<LabelId> = label_rows.iter().map(|row| row.id).collect();
            for row in label_rows {
                labels.insert(
                    row.id,
                    Label {
                        id: row.id,
                        remote: row.remote.map(|num| RemoteLabelRef {
                            board: board_remote_or(board.remote),
                            label: RemoteLabelId::deck(num),
                        }),
                        board: board.id,
                        title: row.title,
                        color: Color::new(row.color),
                        deleted: row.deleted,
                        clocks: row.clocks,
                        authors: crate::entities::LabelAuthors::default(),
                        remote_seen: row.remote_seen,
                    },
                );
            }

            let mut tasks = BTreeMap::new();
            for row in task_rows {
                let (stack_id, stack_remote) = stack_bindings[row.stack_idx % stack_bindings.len()];
                let label_set: BTreeMap<LabelId, crate::entities::MembershipState> = row
                    .label_idxs
                    .into_iter()
                    .filter(|_| !label_ids.is_empty())
                    .map(|i| label_ids[i % label_ids.len()])
                    .map(|id| {
                        (
                            id,
                            crate::entities::MembershipState::Added {
                                at: row.clocks.position,
                            },
                        )
                    })
                    .collect();
                tasks.insert(
                    row.id,
                    Task {
                        id: row.id,
                        remote: row.remote.map(|num| RemoteCardRef {
                            board: board_remote_or(board.remote),
                            stack: stack_remote.unwrap_or(RemoteStackId::deck(1)),
                            card: RemoteCardId::deck(num),
                        }),
                        title: row.title,
                        description: row.description,
                        duedate: row.duedate,
                        scheduled: row.scheduled,
                        done: row.done,
                        stack: stack_id,
                        order: row.order,
                        label_memberships: label_set,
                        effort: None,
                        importance: None,
                        urgency: None,
                        recur: None,
                        kind: crate::entities::TaskKind::Standard,

                        archived: row.archived,
                        deleted: row.deleted,
                        clocks: row.clocks,
                        authors: crate::entities::TaskAuthors::default(),
                        remote_seen: row.remote_seen,
                        remote_baseline: row.remote_baseline,
                    },
                );
            }

            AppState {
                undo: crate::journal::UndoView::default(),
                roster: crate::roster::RestoreRoster::default(),
                settings: crate::settings::SettingsSnapshot::default(),
                boards,
                stacks,
                tasks,
                labels,
                reminders: crate::reminders::ReminderState::default(),
                tracking: crate::tracking::TrackingState::default(),
                sync: SyncStates::new(),
                last_updated: None,
                habits: crate::habit::HabitState::default(),
                reflection: crate::reflection::ReflectionState::default(),
                roaming: crate::settings::RoamingSettings::default(),
            }
        },
    )
}

/// A full `AppState` with a referentially-consistent, compaction-
/// normalized reminder section over its tasks (D14: one shared strategy
/// family, no parallel reminder universe).
pub fn app_state_strategy() -> impl Strategy<Value = AppState> {
    (
        raw_app_state_strategy(),
        super::reminders::reminder_rows_strategy(),
        super::tracking_state_strategy(),
    )
        .prop_map(|(state, rows, mut tracking)| {
            // Tracking entries join the referential guarantee: chunk 1
            // mints entries only for live tasks and soft-deleted tasks
            // keep their map rows, so a reachable entry always names a
            // generated task (an absent-task entry becomes reachable
            // only with the roaming chunk's fold, which skips
            // unresolvable rows). Entries drawn against an empty pool
            // are unreachable and dropped.
            let pool: Vec<crate::ids::TaskId> = state.tasks.keys().copied().collect();
            tracking.entries = tracking
                .entries
                .into_iter()
                .filter_map(|(id, mut entry)| {
                    let first = *pool.first()?;
                    let pick = pool
                        .get(
                            usize::try_from(entry.id.as_uuid().as_u128() % pool.len() as u128)
                                .unwrap_or(0),
                        )
                        .copied()
                        .unwrap_or(first);
                    entry.task = pick;
                    Some((id, entry))
                })
                .collect();
            AppState {
                reminders: super::reminders::build_reminders(&state.tasks, rows),
                tracking,
                ..state
            }
        })
}

/// One row of [`persisted_state_strategy`]'s outbox.
#[derive(Debug, Clone)]
pub struct OutboxRow {
    /// Operation id.
    pub op_id: crate::outbox::OpId,
    /// Operation kind (index into all 15 `LocalOp` variants; resolved
    /// against the generated entity lists, rows without a matching entity
    /// are dropped).
    pub kind: usize,
    /// Queue timestamp.
    pub queued_at: DateTime<Utc>,
}

fn outbox_rows() -> impl Strategy<Value = Vec<OutboxRow>> {
    prop::collection::vec((op_id_strategy(), 0_usize..15, timestamp_strategy()), 0..6).prop_map(
        |rows| {
            rows.into_iter()
                .map(|(op_id, kind, queued_at)| OutboxRow {
                    op_id,
                    kind,
                    queued_at,
                })
                .collect()
        },
    )
}

/// A fully populated persisted state (same referential guarantees as
/// [`app_state_strategy`], plus outbox and validators).
pub fn persisted_state_strategy() -> impl Strategy<Value = PersistedState> {
    (
        app_state_strategy(),
        outbox_rows(),
        prop::option::of(remote_number().prop_map(RemoteBoardId::deck)),
        prop::option::of(string_strategy()),
        prop::option::of(string_strategy()),
        prop::option::of(timestamp_strategy()),
        any::<u32>(),
        prop::option::of(sync_phase_strategy()),
    )
        .prop_map(
            |(
                state,
                op_rows,
                validator_board,
                etag,
                last_modified,
                last_success,
                pending_ops,
                phase,
            )| {
                let outbox = build_outbox(&state, op_rows);
                let validators = build_validators(validator_board, etag, last_modified);
                // One stored row per generated board; `pending_ops` is
                // derived, so the random counter is dropped here.
                let mut sync = SyncStates::new();
                for board in state.boards.keys() {
                    sync.set_stored(*board, phase.unwrap_or(SyncPhase::Idle), last_success);
                }
                let _ = pending_ops;
                PersistedState {
                    journal: crate::journal::JournalState::default(),
                    boards: state.boards,
                    stacks: state.stacks,
                    tasks: state.tasks,
                    labels: state.labels,
                    outbox,
                    validators,
                    sync,
                    reminders: state.reminders,
                    tracking: state.tracking,
                    habits: state.habits,
                    reflection: state.reflection,
                    roaming: crate::settings::RoamingSettings::default(),
                }
            },
        )
}

/// The fixed remote baseline and queue timestamp the repair below
/// stamps on entities it touches. Drawn-clock diversity is preserved
/// because the repair is the identity on every already-conforming draw
/// (the common case); a repaired draw was previously rejected outright,
/// so any repaired timestamp is strictly new coverage.
fn repair_seen() -> DateTime<Utc> {
    Utc.timestamp_opt(1_000_000_000, 0).unwrap()
}

/// A [`PersistedState`] whose shape matches the mirror-snapshot
/// precondition of the push-phase equivalence property
/// (`pipeline.rs::apply_push_report_equals_the_pipeline_push_phase`):
/// a bound, live, seen board; every bound stack, label, and task seen;
/// a non-empty outbox. Built as a minimal-repair post-transform of
/// [`persisted_state_strategy`] — a draw is changed only where a
/// clause fails, and every already-conforming state passes through
/// untouched, so the value space is a superset of what the old
/// five-clause `prop_filter` admitted. Reusable by adoption wherever
/// that same shape is needed.
pub fn push_ready_state_strategy() -> impl Strategy<Value = PersistedState> {
    persisted_state_strategy().prop_map(repair_for_push_phase)
}

/// Total, minimal-repair post-transform enforcing the push-phase shape:
/// repairs each violated clause and leaves every already-conforming
/// state untouched. Carries the five invariants as `debug_assert!`s —
/// executable documentation here (compiled out in release); pinned in
/// test builds by the pipeline invariant property.
fn repair_for_push_phase(mut state: PersistedState) -> PersistedState {
    // Clause 1–3: a bound, seen, live board. The strategy generates
    // exactly one board, so the repair works on that board *in place*
    // (bind it, stamp it, undelete it). Synthesizing a second board
    // instead would create cross-board topology the equivalence
    // property's validated domain never contained (its old filter
    // admitted only single-board states) — repaired multi-board shapes
    // provably diverge between the two pipelines (issue #1080), so the
    // repair stays inside the validated domain.
    let seen = repair_seen();
    repair_board_clause(&mut state, seen);

    // Clause 4: every bound entity seen. Existing `Some` values are never
    // overwritten (drawn `last_modified` diversity stays); unbound
    // entities are untouched.
    repair_seen_clauses(&mut state, seen);

    // Clause 5: a non-empty outbox. After the board repair a board always
    // exists, so an `UpdateBoard` op resolves; one append is sufficient
    // (the property needs `len >= 1` and nothing more). The op targets
    // the repaired (bound) board — the only board a draw has.
    let bound_board_id = state
        .boards
        .values()
        .find(|b| b.remote.is_some())
        .map(|b| b.id)
        .expect("board repaired above");
    if state.outbox.is_empty() {
        state.outbox.push(PendingOp {
            op_id: crate::outbox::OpId(uuid::Uuid::new_v4()),
            op: LocalOp::UpdateBoard(bound_board_id),
            queued_at: seen,
        });
    }

    let board_ok = state
        .boards
        .values()
        .find(|b| b.remote.is_some())
        .is_some_and(|b| b.remote_seen.is_some() && !b.deleted);
    debug_assert!(board_ok, "clauses 1–3: the bound board is seen and live");
    debug_assert!(
        state
            .stacks
            .values()
            .filter(|s| s.remote.is_some())
            .all(|s| s.remote_seen.is_some()),
        "clause 4: bound stacks seen"
    );
    debug_assert!(
        state
            .labels
            .values()
            .filter(|l| l.remote.is_some())
            .all(|l| l.remote_seen.is_some()),
        "clause 4: bound labels seen"
    );
    debug_assert!(
        state
            .tasks
            .values()
            .filter(|t| t.remote.is_some())
            .all(|t| t.remote_seen.is_some()),
        "clause 4: bound tasks seen"
    );
    debug_assert!(
        !state.outbox.is_empty(),
        "clause 5: the outbox is non-empty"
    );
    state
}

/// The cross-board extension of [`push_ready_state_strategy`] for the
/// push-phase equivalence property
/// (`pipeline.rs::apply_push_report_equals_the_pipeline_push_phase`):
/// the same push-phase shape **plus** a second, unbound, live board
/// targeted by one outbox op — the divergence class of issue #1080
/// ("a board op targets an *unbound* board while a *bound live* board
/// exists"). Built as [`repair_for_push_phase`] (reused verbatim)
/// followed by a total, minimal-repair cross-board step; the
/// single-board value space is unchanged, so the union of the two
/// strategies widens the property's domain without narrowing anything.
pub fn push_ready_cross_board_state_strategy() -> impl Strategy<Value = PersistedState> {
    persisted_state_strategy().prop_map(cross_board_repair)
}

/// The total, minimal-repair cross-board step layered on
/// [`repair_for_push_phase`]: synthesize an unbound live second board
/// (fresh [`crate::ids::BoardId`], `remote: None`, epoch clocks) when
/// the repaired state has none, and append one `UpdateBoard` op
/// targeting it (fresh `OpId`, queued at [`repair_seen`]) — always,
/// since the freshly minted board id cannot collide with a drawn op's
/// target. Synthesized ids via `uuid::new_v4` inside
/// `prop_map` follow the R3 precedent (constant, not fresh RNG, for
/// timestamps) — shrinking stays structural.
fn cross_board_repair(state: PersistedState) -> PersistedState {
    let mut state = repair_for_push_phase(state);
    let bound = state
        .boards
        .values()
        .find(|b| b.remote.is_some() && !b.deleted)
        .expect("repair_for_push_phase bound a live board");
    debug_assert_eq!(
        state
            .boards
            .values()
            .filter(|b| b.remote.is_some() && !b.deleted)
            .count(),
        1,
        "exactly one bound live board exists"
    );
    let second_id = crate::ids::BoardId::from_uuid(uuid::Uuid::new_v4());
    let board = Board {
        id: second_id,
        // Deliberately unbound: the divergence class requires an op
        // targeting a board with no server relationship while a bound
        // live board exists.
        remote: None,
        // Drawn title/color: cloned off the bound board so the
        // synthesized board reuses the draw's randomness instead of a
        // fresh RNG stream.
        title: bound.title.clone(),
        color: bound.color.clone(),
        archived: false,
        deleted: false,
        clocks: crate::entities::BoardClocks {
            color: crate::entities::BoardClocks::epoch(),
            archived: crate::entities::BoardClocks::epoch(),
            deleted: crate::entities::BoardClocks::epoch(),
            title: crate::entities::BoardClocks::epoch(),
        },
        authors: crate::entities::BoardAuthors::default(),
        remote_seen: None,
    };
    state.boards.insert(second_id, board);
    // The divergence trigger, always appended: `second_id` is freshly
    // minted, so no drawn op can already target it.
    state.outbox.push(PendingOp {
        op_id: crate::outbox::OpId(uuid::Uuid::new_v4()),
        op: LocalOp::UpdateBoard(second_id),
        queued_at: repair_seen(),
    });
    debug_assert!(
        state
            .boards
            .get(&second_id)
            .is_some_and(|b| b.remote.is_none() && !b.deleted),
        "the second board is unbound and live"
    );
    state
}

/// Repairs clause 4: every bound stack, label, and task gets a remote
/// baseline; entities already carrying one keep their drawn value.
fn repair_seen_clauses(state: &mut PersistedState, seen: DateTime<Utc>) {
    for stack in state.stacks.values_mut() {
        if stack.remote.is_some() && stack.remote_seen.is_none() {
            stack.remote_seen = Some(seen);
        }
    }
    for label in state.labels.values_mut() {
        if label.remote.is_some() && label.remote_seen.is_none() {
            label.remote_seen = Some(seen);
        }
    }
    for task in state.tasks.values_mut() {
        if task.remote.is_some() && task.remote_seen.is_none() {
            task.remote_seen = Some(seen);
        }
    }
}

/// Repairs clauses 1–3 in place on the draw's existing board(s): bind
/// the first board with a fresh deck number, stamp its baseline, and
/// clear its tombstone. A no-op when the draw already carries a bound,
/// seen, live board — so drawn bindings and baselines survive on
/// conforming draws. A synthesized row is inserted only in the
/// (strategy-unreachable, defensive) no-boards case, keeping the
/// repair total — the function never falls back to rejection.
fn repair_board_clause(state: &mut PersistedState, seen: DateTime<Utc>) {
    if state
        .boards
        .values()
        .any(|b| b.remote.is_some() && b.remote_seen.is_some() && !b.deleted)
    {
        return;
    }
    // The next unused deck number avoids colliding with other boards'
    // bindings.
    let next_remote = state
        .boards
        .values()
        .filter_map(|b| Some(b.remote.as_ref()?.board.as_deck().unwrap_or(0)))
        .max()
        .unwrap_or(0)
        + 1;
    let pick = state.boards.values().next().map(|b| b.id);
    if let Some(id) = pick {
        let board = state.boards.get_mut(&id).expect("pick came from the map");
        board.remote = Some(crate::ids::RemoteBoardBinding {
            connector: crate::test_support::test_connector(),
            board: crate::ids::RemoteBoardId::deck(next_remote),
        });
        board.remote_seen = Some(seen);
        board.deleted = false;
    } else {
        let id = crate::ids::BoardId::from_uuid(uuid::Uuid::new_v4());
        let board = Board {
            id,
            remote: Some(crate::ids::RemoteBoardBinding {
                connector: crate::test_support::test_connector(),
                board: crate::ids::RemoteBoardId::deck(1),
            }),
            title: String::from("push-phase repair board"),
            color: Color::new(String::from("0000ff")),
            archived: false,
            deleted: false,
            clocks: crate::entities::BoardClocks {
                color: crate::entities::BoardClocks::epoch(),
                archived: crate::entities::BoardClocks::epoch(),
                deleted: crate::entities::BoardClocks::epoch(),
                title: crate::entities::BoardClocks::epoch(),
            },
            authors: crate::entities::BoardAuthors::default(),
            remote_seen: Some(seen),
        };
        state.boards.insert(id, board);
    }
}

/// Maps op-kind rows onto the generated entities; rows whose targets don't
/// exist are skipped (the outbox never references missing entities).
fn build_outbox(state: &AppState, op_rows: Vec<OutboxRow>) -> Vec<PendingOp> {
    let task_id = state.tasks.keys().next().copied();
    let stack_id = state.stacks.keys().next().copied();
    let label_id = state.labels.keys().next().copied();
    let board_id = state.boards.keys().next().copied();
    let mut outbox = Vec::new();
    for row in op_rows {
        let t = task_id;
        let s = stack_id;
        let l = label_id;
        let b = board_id;
        let op = match row.kind {
            0 => t.map(LocalOp::CreateTask),
            1 => t.map(LocalOp::UpdateTask),
            2 => t.map(LocalOp::MoveTask),
            3 => t.map(LocalOp::DeleteTask),
            4 => s.map(LocalOp::CreateStack),
            5 => s.map(LocalOp::RenameStack),
            6 => s.map(LocalOp::DeleteStack),
            7 => l.map(LocalOp::CreateLabel),
            8 => l.map(LocalOp::UpdateLabel),
            9 => l.map(LocalOp::DeleteLabel),
            10 => t.zip(l).map(|(t, l)| LocalOp::AssignLabel(t, l)),
            11 => t.zip(l).map(|(t, l)| LocalOp::UnassignLabel(t, l)),
            12 => s.map(LocalOp::MoveStack),
            13 => b.map(LocalOp::UpdateBoard),
            14 => b.map(LocalOp::DeleteBoard),
            // All 15 variants are named above; unreachable, defensive only.
            _ => None,
        };
        if let Some(op) = op {
            outbox.push(PendingOp {
                op_id: row.op_id,
                op,
                queued_at: row.queued_at,
            });
        }
    }
    outbox
}

fn build_validators(
    board: Option<RemoteBoardId>,
    etag: Option<String>,
    last_modified: Option<String>,
) -> BTreeMap<ValidatorKey, SyncValidators> {
    let mut validators = BTreeMap::new();
    if let Some(board) = board {
        validators.insert(
            ValidatorKey::Boards,
            SyncValidators {
                etag: etag.clone(),
                last_modified: last_modified.clone(),
            },
        );
        validators.insert(
            ValidatorKey::Stacks(board),
            SyncValidators {
                etag: etag.clone(),
                last_modified: last_modified.clone(),
            },
        );
        validators.insert(
            ValidatorKey::ArchivedStacks(board),
            SyncValidators {
                etag,
                last_modified,
            },
        );
    }
    validators
}

fn sync_phase_strategy() -> impl Strategy<Value = SyncPhase> {
    prop_oneof![
        Just(SyncPhase::Idle),
        Just(SyncPhase::Syncing),
        Just(SyncPhase::Offline),
        (0_usize..7).prop_map(|kind| SyncPhase::Failed {
            last_error: match kind {
                0 => SyncErrorKind::Network,
                1 => SyncErrorKind::Auth,
                2 => SyncErrorKind::Forbidden,
                3 => SyncErrorKind::Server,
                4 => SyncErrorKind::BadRequest,
                5 => SyncErrorKind::LocalData,
                _ => SyncErrorKind::NoBoard,
            },
        }),
    ]
}
