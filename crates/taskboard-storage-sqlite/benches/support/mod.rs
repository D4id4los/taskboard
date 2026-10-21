// SPDX-License-Identifier: MIT OR Apache-2.0
//! Deterministic fixtures for the persistence hot-path benches: one
//! builder parameterized by task count, every value a pure function of
//! N and the entity index — no rng, no proptest strategies, no clock
//! reads. Shared between the bench target and the fixture-contract test
//! via `#[path]` (plan D1).

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, TimeDelta, TimeZone, Utc};
use taskboard_domain::entities::{
    Board, Color, Label, LabelClocks, Stack, StackClocks, Task, TaskClocks,
};
use taskboard_domain::ids::{
    BoardId, LabelId, RemoteBoardId, RemoteCardId, RemoteCardRef, RemoteLabelId, RemoteLabelRef,
    RemoteStackId, RemoteStackRef, StackId, TaskId,
};
use taskboard_domain::outbox::{LocalOp, OpId, PendingOp};
use taskboard_domain::persistence::{
    PersistedState, PersistenceAction, SyncValidators, ValidatorKey,
};
use taskboard_domain::reminders::{ReminderOffset, ReminderRule, ReminderState};
use taskboard_domain::state::{SyncPhase, SyncStates};
use taskboard_domain::{BoardClocks, ReminderRuleId};

/// The N values the benches measure (dispatch P3 mandate).
pub const N_SIZES: [usize; 3] = [100, 1_000, 10_000];

/// Stack count (fixed across N; tasks fan out over them round-robin).
pub const STACK_COUNT: usize = 4;
/// Label count (fixed across N; tasks get 1–2 each round-robin).
pub const LABEL_COUNT: usize = 6;

/// Outbox ops for N tasks (one `UpdateTask` per 10th task).
#[must_use]
pub fn outbox_count(n: usize) -> usize {
    n / 10
}

/// Reminder rules for N tasks (one `-15 min` rule per 20th task).
#[must_use]
pub fn rule_count(n: usize) -> usize {
    n / 20
}

/// Disjoint uuid bases, one per entity kind: distinct ids across kinds
/// by construction, `Uuid::from_u128(BASE + i)` within a kind.
const BOARD_BASE: u128 = 0x10_0000_0000;
const STACK_BASE: u128 = 0x20_0000_0000;
const LABEL_BASE: u128 = 0x30_0000_0000;
const TASK_BASE: u128 = 0x40_0000_0000;
const OP_BASE: u128 = 0x50_0000_0000;
const RULE_BASE: u128 = 0x60_0000_0000;

/// Fixture time origin: 2024-01-01T00:00:00Z. Stamps are
/// `EPOCH_SECS + <family offset> + i` — distinct and strictly
/// increasing in build order per clock family.
const EPOCH_SECS: i64 = 1_704_067_200;

#[must_use]
fn ts(secs: i64) -> DateTime<Utc> {
    Utc.timestamp_opt(EPOCH_SECS + secs, 0)
        .single()
        .expect("fixture stamp in range")
}

#[must_use]
fn id(base: u128, i: usize) -> uuid::Uuid {
    uuid::Uuid::from_u128(base + u128::try_from(i).expect("fixture index fits u128"))
}

#[must_use]
fn ix(i: usize) -> i64 {
    i64::try_from(i).expect("fixture index fits i64")
}

#[must_use]
fn ux(i: usize) -> u64 {
    u64::try_from(i).expect("fixture index fits u64")
}

fn fixture_board() -> Board {
    Board {
        id: BoardId::from(id(BOARD_BASE, 0)),
        remote: Some(taskboard_domain::ids::RemoteBoardBinding {
            connector: taskboard_domain::ConnectorId::from("nextcloud"),
            board: RemoteBoardId(77),
        }),
        title: "Bench board".into(),
        color: Color::new("3366ff"),
        archived: false,
        deleted: false,
        clocks: BoardClocks {
            title: ts(0),
            color: ts(0),
            archived: ts(0),
            deleted: ts(0),
        },
        remote_seen: Some(ts(0)),
    }
}

fn fixture_stack(s: usize, board: &Board) -> Stack {
    let stamp = ts(ix(s) + 1);
    Stack {
        id: StackId::from(id(STACK_BASE, s)),
        remote: Some(RemoteStackRef {
            board: RemoteBoardId(77),
            stack: RemoteStackId(7 + ux(s)),
        }),
        board: board.id,
        title: format!("Stack {s}"),
        order: ix(s),
        archived: false,
        deleted: false,
        clocks: StackClocks {
            title: stamp,
            order: stamp,
            deleted: stamp,
        },
        remote_seen: Some(stamp),
    }
}

fn fixture_label(l: usize, board: &Board) -> Label {
    let stamp = ts(ix(l) + 2);
    Label {
        id: LabelId::from(id(LABEL_BASE, l)),
        remote: Some(RemoteLabelRef {
            board: RemoteBoardId(77),
            label: RemoteLabelId(9 + ux(l)),
        }),
        board: board.id,
        title: format!("Label {l}"),
        color: Color::new("00cc66"),
        deleted: false,
        clocks: LabelClocks {
            title: stamp,
            color: stamp,
            deleted: stamp,
        },
        remote_seen: Some(stamp),
    }
}

fn fixture_task(i: usize, stacks: &[Stack], labels: &[Label]) -> Task {
    let mut label_set = BTreeSet::new();
    label_set.insert(labels[i % LABEL_COUNT].id);
    if !i.is_multiple_of(2) {
        label_set.insert(labels[(i + 3) % LABEL_COUNT].id);
    }
    Task {
        id: TaskId::from(id(TASK_BASE, i)),
        remote: Some(RemoteCardRef {
            board: RemoteBoardId(77),
            stack: RemoteStackId(7),
            card: RemoteCardId(1_000 + ux(i)),
        }),
        title: format!("Task {i:05} bench fixture"),
        description: format!("Deterministic bench fixture task number {i}."),
        duedate: i.is_multiple_of(3).then(|| ts(ix(i) + 1_000_000)),
        // Interim (chunk-1 storage step): the codec does not persist the
        // family columns yet, so the fixture carries only what survives
        // the round-trip; 0007 wiring enriches this.
        scheduled: None,
        done: i.is_multiple_of(10).then(|| ts(ix(i) + 2_000_000)),
        stack: stacks[i % STACK_COUNT].id,
        order: ix(i),
        labels: label_set,
        effort: None,
        importance: None,
        urgency: None,
        archived: false,
        deleted: false,
        clocks: task_clocks(i),
        remote_seen: Some(ts(ix(i) + 500)),
    }
}

fn task_clocks(i: usize) -> TaskClocks {
    let t = ts(ix(i));
    let epoch = taskboard_domain::entities::BoardClocks::epoch();
    TaskClocks {
        title: t,
        description: t,
        duedate: t,
        scheduled: epoch,
        done: t,
        position: t,
        labels: t,
        effort: epoch,
        importance: epoch,
        urgency: epoch,
        archived: t,
        deleted: t,
    }
}

fn fixture_op(j: usize, tasks: &[Task]) -> PendingOp {
    PendingOp {
        op_id: OpId(id(OP_BASE, j)),
        op: LocalOp::UpdateTask(tasks[10 * j].id),
        queued_at: ts(ix(j) + 3_000_000),
    }
}

fn fixture_rule(k: usize, tasks: &[Task]) -> ReminderRule {
    ReminderRule {
        id: ReminderRuleId::from(id(RULE_BASE, k)),
        task: tasks[20 * k].id,
        offset: ReminderOffset::before(TimeDelta::minutes(-15))
            .expect("negative delta is before due"),
        created_at: ts(ix(k) + 4_000_000),
    }
}

fn fixture_validators() -> BTreeMap<ValidatorKey, SyncValidators> {
    BTreeMap::from([
        (
            ValidatorKey::Boards,
            SyncValidators {
                etag: Some("\"bench-boards-etag\"".into()),
                last_modified: Some("Mon, 01 Jan 2024 00:00:00 GMT".into()),
            },
        ),
        (
            ValidatorKey::Stacks(RemoteBoardId(77)),
            SyncValidators {
                etag: Some("\"bench-stacks-etag\"".into()),
                last_modified: Some("Mon, 01 Jan 2024 00:00:01 GMT".into()),
            },
        ),
    ])
}

/// The whole-state `apply` batch for `state` — `tests/common/mod.rs`'s
/// `actions_from_state` vocabulary (boards → stacks → labels → tasks →
/// outbox → validators → sync status) plus the reminder-rule partition;
/// that helper lives under `tests/` and cannot be imported by a bench
/// target (plan §1.3).
#[must_use]
fn whole_state_actions(state: &PersistedState) -> Vec<PersistenceAction> {
    let mut actions: Vec<PersistenceAction> = state
        .boards
        .values()
        .cloned()
        .map(PersistenceAction::UpsertBoard)
        .collect();
    actions.extend(
        state
            .stacks
            .values()
            .cloned()
            .map(PersistenceAction::UpsertStack),
    );
    actions.extend(
        state
            .labels
            .values()
            .cloned()
            .map(PersistenceAction::UpsertLabel),
    );
    actions.extend(
        state
            .tasks
            .values()
            .cloned()
            .map(PersistenceAction::UpsertTask),
    );
    actions.extend(
        state
            .outbox
            .iter()
            .cloned()
            .map(PersistenceAction::EnqueueOp),
    );
    actions.extend(
        state
            .reminders
            .rules
            .values()
            .cloned()
            .map(PersistenceAction::UpsertReminderRule),
    );
    actions.extend(
        state
            .validators
            .iter()
            .map(|(key, bundle)| PersistenceAction::UpsertValidators(*key, bundle.clone())),
    );
    for (board, status) in state.sync.iter() {
        actions.push(PersistenceAction::UpsertSyncStatus(*board, status.clone()));
    }
    actions
}

/// The deterministic fixture at task count `n`: the full
/// [`PersistedState`] plus the whole-state `apply` batch that persists
/// it (entity upserts, outbox enqueues, validator upserts, sync status,
/// reminder-rule upserts).
#[must_use]
pub fn build_fixture(n: usize) -> Fixture {
    let board = fixture_board();
    let stacks: Vec<Stack> = (0..STACK_COUNT).map(|s| fixture_stack(s, &board)).collect();
    let labels: Vec<Label> = (0..LABEL_COUNT).map(|l| fixture_label(l, &board)).collect();
    let tasks: Vec<Task> = (0..n).map(|i| fixture_task(i, &stacks, &labels)).collect();
    let ops: Vec<PendingOp> = (0..outbox_count(n))
        .map(|j| fixture_op(j, &tasks))
        .collect();
    let rules: Vec<ReminderRule> = (0..rule_count(n))
        .map(|k| fixture_rule(k, &tasks))
        .collect();

    let mut sync = SyncStates::new();
    sync.set_stored(board.id, SyncPhase::Idle, Some(ts(4_000_000)));
    let state = PersistedState {
        boards: BTreeMap::from([(board.id, board.clone())]),
        stacks: stacks.iter().map(|s| (s.id, s.clone())).collect(),
        tasks: tasks.iter().map(|t| (t.id, t.clone())).collect(),
        labels: labels.iter().map(|l| (l.id, l.clone())).collect(),
        outbox: ops.clone(),
        validators: fixture_validators(),
        sync,
        reminders: ReminderState {
            rules: rules.iter().map(|r| (r.id, r.clone())).collect(),
            ..ReminderState::default()
        },
    };

    Fixture {
        actions: whole_state_actions(&state),
        state,
    }
}

/// A built fixture: the state it represents and the batch that
/// persists it.
pub struct Fixture {
    /// The in-memory state the fixture encodes.
    pub state: PersistedState,
    /// The whole-state `apply` batch for [`Fixture::state`].
    pub actions: Vec<PersistenceAction>,
}
