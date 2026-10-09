// SPDX-License-Identifier: MIT OR Apache-2.0
//! Property suite (P-series): the architectural equivalences that gate
//! the engine's design, plus sequence coherence and op freshness.
//!
//! - P2: `EngineCore::interpret` ≡ repository wiring — memory advanced by
//!   the shared `apply_actions` equals the fake repo's post-apply load.
//! - P3: `interpret` ≡ pipeline — interpreting the pipeline's returned
//!   actions (+ the engine-appended status action when applicable)
//!   reproduces the pipeline's returned `AppState`.
//! - P4: sequence coherence — memory ≡ disk after every command,
//!   rejected/no-op commands change nothing, and the number of
//!   `StateUpdated` signals equals the number of state-changing messages.
//! - P5: op freshness — enqueued op ids are unique and address entities
//!   that exist in the post-state.

use std::collections::BTreeSet;
use std::sync::Arc;

use chrono::TimeZone;
use proptest::prelude::*;
use taskboard_domain::StateCommand;
use taskboard_domain::idgen::IdGenerator as _;
use taskboard_domain::persistence::TaskRepository;
use taskboard_domain::test_support::{
    CountingIds, InMemoryRepository, persisted_state_strategy, snapshot_strategy,
};
use taskboard_domain::{
    AppState, Board, BoardId, Clock, Color, CommandOutcome, Label, LabelClocks, LocalOp, OpId,
    PendingOp, PersistedState, PersistenceAction, Stack, StackClocks, StackId, SyncPhase,
    SyncStatus, Task, TaskClocks, TaskId, apply_sync_report,
};
use taskboard_state::EngineCore;
use taskboard_state::spawn_state_engine;

const NOW_SECS: i64 = 36_000;

fn now() -> chrono::DateTime<chrono::Utc> {
    chrono::Utc.timestamp_opt(NOW_SECS, 0).unwrap()
}

fn ts(secs: i64) -> chrono::DateTime<chrono::Utc> {
    chrono::Utc.timestamp_opt(secs, 0).unwrap()
}

fn uuid(raw: u128) -> uuid::Uuid {
    uuid::Uuid::from_u128(raw)
}

// ---------------------------------------------------------------------
// P2: interpret ≡ repository wiring
// ---------------------------------------------------------------------

/// An arbitrary [`PersistenceAction`]. No referential consistency needed:
/// P2 compares two interpreters of the *same* batch.
fn arbitrary_action() -> impl Strategy<Value = PersistenceAction> {
    let board = any::<u128>().prop_map(|raw| {
        PersistenceAction::UpsertBoard(Board {
            id: BoardId::from(uuid(raw)),
            remote: None,
            title: "b".into(),
            color: Color::new("0000ff"),
            archived: false,
            deleted: false,
            remote_seen: None,
        })
    });
    let stack = (any::<u128>(), -100_i64..=100).prop_map(|(raw, order)| {
        PersistenceAction::UpsertStack(Stack {
            id: StackId::from(uuid(raw)),
            remote: None,
            board: BoardId::from(uuid(0)),
            title: "s".into(),
            order,
            archived: false,
            deleted: false,
            clocks: StackClocks {
                title: ts(1),
                order: ts(1),
                deleted: ts(1),
            },
            remote_seen: None,
        })
    });
    let task = any::<u128>().prop_map(|raw| {
        PersistenceAction::UpsertTask(Task {
            id: TaskId::from(uuid(raw)),
            remote: None,
            title: "t".into(),
            description: String::new(),
            duedate: None,
            done: None,
            stack: StackId::from(uuid(0)),
            order: 0,
            labels: BTreeSet::new(),
            archived: false,
            deleted: false,
            clocks: TaskClocks {
                title: ts(1),
                description: ts(1),
                duedate: ts(1),
                done: ts(1),
                position: ts(1),
                labels: ts(1),
                archived: ts(1),
                deleted: ts(1),
            },
            remote_seen: None,
        })
    });
    let label = any::<u128>().prop_map(|raw| {
        PersistenceAction::UpsertLabel(Label {
            id: taskboard_domain::LabelId::from(uuid(raw)),
            remote: None,
            board: BoardId::from(uuid(0)),
            title: "l".into(),
            color: Color::new("00ff00"),
            deleted: false,
            clocks: LabelClocks {
                title: ts(1),
                color: ts(1),
                deleted: ts(1),
            },
            remote_seen: None,
        })
    });
    let enqueue = (any::<u128>(), any::<u128>()).prop_map(|(op_raw, task_raw)| {
        PersistenceAction::EnqueueOp(PendingOp {
            op_id: OpId(uuid(op_raw)),
            op: LocalOp::UpdateTask(TaskId::from(uuid(task_raw))),
            queued_at: ts(1),
        })
    });
    let complete = any::<u128>().prop_map(|raw| PersistenceAction::CompleteOp(OpId(uuid(raw))));
    let fail = any::<u128>().prop_map(|raw| PersistenceAction::FailOp(OpId(uuid(raw))));
    prop_oneof![
        board,
        stack,
        task,
        label,
        enqueue,
        complete,
        fail,
        Just(PersistenceAction::UpsertSyncStatus(SyncStatus {
            phase: SyncPhase::Offline,
            last_success: None,
            pending_ops: 0
        })),
    ]
}

fn arbitrary_batch() -> impl Strategy<Value = Vec<PersistenceAction>> {
    proptest::collection::vec(arbitrary_action(), 0..10)
}

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(256))]

    #[test]
    fn p2_interpret_matches_repository_load(
        persisted in persisted_state_strategy(),
        batch in arbitrary_batch(),
    ) {
        let mut core = EngineCore::from_persisted(persisted.clone());
        let repo = InMemoryRepository::with_state(persisted);
        <InMemoryRepository as ReadyExt>::apply_now(&repo, batch.clone());
        core.interpret(&batch, now());
        prop_assert_eq!(core.persisted_view(), <InMemoryRepository as ReadyExt>::load_now(&repo), "memory must equal disk");
    }

    #[test]
    fn p3_interpret_reproduces_the_pipeline_state(
        persisted in persisted_state_strategy(),
        snapshot in snapshot_strategy(),
        push_plan in proptest::collection::vec(any::<bool>(), 0..4),
    ) {
        // Push outcomes over op ids actually present in the generated
        // outbox: `true` → Applied (no echo, R9d-style), `false` →
        // BadRequest (dead-letters). The pipeline resolves unknown/marked
        // ops by contract.
        let pushes: Vec<taskboard_domain::PushOutcome> = persisted
            .outbox
            .iter()
            .zip(push_plan.iter().chain(std::iter::repeat(&false)))
            .map(|(pending, dead_letter)| taskboard_domain::PushOutcome {
                op: pending.op_id,
                result: if *dead_letter {
                    taskboard_domain::PushResult::Rejected {
                        kind: taskboard_domain::SyncErrorKind::BadRequest,
                    }
                } else {
                    taskboard_domain::PushResult::Applied { echo: None }
                },
            })
            .collect();

        let app = AppState {
            boards: persisted.boards.clone(),
            stacks: persisted.stacks.clone(),
            tasks: persisted.tasks.clone(),
            labels: persisted.labels.clone(),
            sync: persisted.sync.clone(),
            last_updated: None,
        };
        let ids = CountingIds::new();
        let (merged, mut actions) = apply_sync_report(
            &app,
            &persisted.outbox,
            &snapshot,
            &pushes,
            &ids,
            now(),
        );
        // The engine appends the status action iff phase or last_success
        // changed — the same rule the actor applies.
        if merged.sync.phase != persisted.sync.phase
            || merged.sync.last_success != persisted.sync.last_success
        {
            actions.push(PersistenceAction::UpsertSyncStatus(merged.sync.clone()));
        }

        let mut core = EngineCore::from_persisted(persisted);
        core.interpret(&actions, now());
        prop_assert_eq!(core.app(), merged, "interpret ≡ pipeline (ADR 0006 decision 2)");
    }
}

// The fake's futures are immediately ready; a minimal inline poll keeps
// the pure proptests runtime-free while still exercising the port shape.
trait ReadyExt {
    fn apply_now(&self, actions: Vec<PersistenceAction>);
    fn load_now(&self) -> PersistedState;
}

impl ReadyExt for InMemoryRepository {
    fn apply_now(&self, actions: Vec<PersistenceAction>) {
        use std::task::{Context, Poll, Waker};
        let mut fut = Box::pin(self.apply(actions));
        let waker = Waker::noop();
        let mut cx = Context::from_waker(waker);
        match fut.as_mut().poll(&mut cx) {
            Poll::Ready(Ok(())) => {}
            _ => panic!("in-memory apply must be immediately ready"),
        }
    }

    fn load_now(&self) -> PersistedState {
        use std::task::{Context, Poll, Waker};
        let mut fut = Box::pin(self.load());
        let waker = Waker::noop();
        let mut cx = Context::from_waker(waker);
        match fut.as_mut().poll(&mut cx) {
            Poll::Ready(Ok(state)) => state,
            _ => panic!("in-memory load must be immediately ready"),
        }
    }
}

// ---------------------------------------------------------------------
// P4 + P5: engine sequences
// ---------------------------------------------------------------------

/// One generated engine step over index-based pools (task/stack/label);
/// indices wrap modulo the pool length at execution time.
#[cfg_attr(miri, allow(dead_code))]
#[derive(Debug, Clone)]
enum Step {
    CreateTask(i64),
    UpdateTitle {
        idx: usize,
        variant: u8,
    },
    ToggleDone {
        idx: usize,
        done: bool,
    },
    Move {
        idx: usize,
        stack_idx: usize,
        order: i64,
    },
    DeleteTask {
        idx: usize,
    },
    CreateStack(i64),
    RenameStack {
        idx: usize,
        variant: u8,
    },
    DeleteStack {
        idx: usize,
    },
    CreateLabel(u8),
    Assign {
        task_idx: usize,
        label_idx: usize,
    },
    Unassign {
        task_idx: usize,
        label_idx: usize,
    },
    /// A command against an id that does not exist — the rejection path.
    Stale {
        done: bool,
    },
}

fn step_strategy() -> impl Strategy<Value = Step> {
    prop_oneof![
        (-100_i64..=100).prop_map(Step::CreateTask),
        (any::<usize>(), any::<u8>()).prop_map(|(idx, variant)| Step::UpdateTitle { idx, variant }),
        (any::<usize>(), any::<bool>()).prop_map(|(idx, done)| Step::ToggleDone { idx, done }),
        (any::<usize>(), any::<usize>(), -100_i64..=100).prop_map(|(idx, stack_idx, order)| {
            Step::Move {
                idx,
                stack_idx,
                order,
            }
        }),
        any::<usize>().prop_map(|idx| Step::DeleteTask { idx }),
        (-100_i64..=100).prop_map(Step::CreateStack),
        (any::<usize>(), any::<u8>()).prop_map(|(idx, variant)| Step::RenameStack { idx, variant }),
        any::<usize>().prop_map(|idx| Step::DeleteStack { idx }),
        any::<u8>().prop_map(Step::CreateLabel),
        (any::<usize>(), any::<usize>()).prop_map(|(task_idx, label_idx)| Step::Assign {
            task_idx,
            label_idx
        }),
        (any::<usize>(), any::<usize>()).prop_map(|(task_idx, label_idx)| Step::Unassign {
            task_idx,
            label_idx
        }),
        any::<bool>().prop_map(|done| Step::Stale { done }),
    ]
}

/// A persisted seed with exactly one live board, stack, task, and label
/// (ids from the counting fake in construction order).
fn coherent_seed() -> (PersistedState, StackId, TaskId, taskboard_domain::LabelId) {
    let ids = CountingIds::new();
    let board_id = ids.new_board_id();
    let stack = Stack {
        id: ids.new_stack_id(),
        remote: None,
        board: board_id,
        title: "seed stack".into(),
        order: 1,
        archived: false,
        deleted: false,
        clocks: StackClocks {
            title: ts(1),
            order: ts(1),
            deleted: ts(1),
        },
        remote_seen: None,
    };
    let label = Label {
        id: ids.new_label_id(),
        remote: None,
        board: board_id,
        title: "seed label".into(),
        color: Color::new("ff0000"),
        deleted: false,
        clocks: LabelClocks {
            title: ts(1),
            color: ts(1),
            deleted: ts(1),
        },
        remote_seen: None,
    };
    let task = Task {
        id: ids.new_task_id(),
        remote: None,
        title: "seed task".into(),
        description: String::new(),
        duedate: None,
        done: None,
        stack: stack.id,
        order: 1,
        labels: BTreeSet::from([label.id]),
        archived: false,
        deleted: false,
        clocks: TaskClocks {
            title: ts(1),
            description: ts(1),
            duedate: ts(1),
            done: ts(1),
            position: ts(1),
            labels: ts(1),
            archived: ts(1),
            deleted: ts(1),
        },
        remote_seen: None,
    };
    let mut state = PersistedState::default();
    state.boards.insert(
        board_id,
        Board {
            id: board_id,
            remote: None,
            title: "board".into(),
            color: Color::new("0000ff"),
            archived: false,
            deleted: false,
            remote_seen: None,
        },
    );
    let (stack_id, label_id, task_id) = (stack.id, label.id, task.id);
    state.stacks.insert(stack.id, stack);
    state.labels.insert(label.id, label);
    state.tasks.insert(task.id, task);
    (state, stack_id, task_id, label_id)
}

fn stale_task_id() -> TaskId {
    let ids = CountingIds::new();
    for _ in 0..1_000 {
        ids.new_op_id();
    }
    ids.new_task_id()
}

/// Drives one generated sequence through a live engine and checks the
/// coherence properties after every command (P4), then op freshness (P5).
///
/// Environment gate, not a weakened assertion: the sequence property runs
/// the live tokio engine, which `testing_strategy` §10 scopes out of Miri;
/// the semantic equivalences (P2/P3) stay Miri-covered below.
#[cfg_attr(miri, allow(dead_code))]
#[allow(clippy::too_many_lines)] // the step mapping IS the catalogue transcription
fn drive_sequence(steps: &[Step]) -> Result<(), TestCaseError> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .start_paused(true)
        .build()
        .expect("runtime");

    rt.block_on(async {
        let (state, seed_stack, seed_task, seed_label) = coherent_seed();
        let repo = Arc::new(InMemoryRepository::with_state(state));
        // The sync receiver is dropped immediately: nudges fail, so the
        // memory-only `Syncing` transient never appears and memory ≡ disk
        // stays directly comparable.
        let (sync_out, sync_in) = tokio::sync::mpsc::channel::<taskboard_domain::SyncCommand>(8);
        drop(sync_in);
        let (_report_tx, report_rx) = tokio::sync::mpsc::channel::<taskboard_domain::SyncReport>(8);
        let (_system_tx, system_rx) =
            tokio::sync::broadcast::channel::<taskboard_domain::SystemEvent>(8);
        let (handle, _join) = spawn_state_engine(
            repo.clone(),
            Arc::new(CountingIds::new()),
            Arc::new(FixedTestClock),
            sync_out,
            report_rx,
            system_rx,
        )
        .await
        .expect("engine boots");
        let mut signals = handle.subscribe();

        let mut tasks = vec![seed_task];
        let mut stacks = vec![seed_stack];
        let mut labels = vec![seed_label];

        for (n, step) in steps.iter().enumerate() {
            let command = match step {
                Step::CreateTask(order) => StateCommand::CreateTask {
                    title: format!("t{n}"),
                    stack: stacks[0],
                    order: *order,
                },
                Step::UpdateTitle { idx, variant } => StateCommand::UpdateTask {
                    id: tasks[idx % tasks.len()],
                    changes: taskboard_domain::TaskChanges {
                        title: Some(format!("t{n}-{variant}")),
                        ..Default::default()
                    },
                },
                Step::ToggleDone { idx, done } => StateCommand::SetTaskDone {
                    id: tasks[idx % tasks.len()],
                    done: *done,
                },
                Step::Move {
                    idx,
                    stack_idx,
                    order,
                } => StateCommand::MoveTask {
                    id: tasks[idx % tasks.len()],
                    stack: stacks[stack_idx % stacks.len()],
                    order: *order,
                },
                Step::DeleteTask { idx } => StateCommand::DeleteTask {
                    id: tasks[idx % tasks.len()],
                },
                Step::CreateStack(order) => StateCommand::CreateStack {
                    title: format!("s{n}"),
                    order: *order,
                },
                Step::RenameStack { idx, variant } => StateCommand::RenameStack {
                    id: stacks[idx % stacks.len()],
                    new_title: format!("s{n}-{variant}"),
                },
                Step::DeleteStack { idx } => StateCommand::DeleteStack {
                    id: stacks[idx % stacks.len()],
                },
                Step::CreateLabel(variant) => StateCommand::CreateLabel {
                    title: format!("l{n}-{variant}"),
                    color: Color::new("00ff00"),
                },
                Step::Assign {
                    task_idx,
                    label_idx,
                } => StateCommand::AssignLabel {
                    task: tasks[task_idx % tasks.len()],
                    label: labels[label_idx % labels.len()],
                },
                Step::Unassign {
                    task_idx,
                    label_idx,
                } => StateCommand::UnassignLabel {
                    task: tasks[task_idx % tasks.len()],
                    label: labels[label_idx % labels.len()],
                },
                Step::Stale { done } => StateCommand::SetTaskDone {
                    id: stale_task_id(),
                    done: *done,
                },
            };

            let mem_before = handle.shared_state().load_full();
            let disk_before = repo.snapshot();
            let outcome = handle.execute(command).await;
            handle.flush().await;

            let state_changed = matches!(
                outcome,
                Ok(CommandOutcome::Applied
                    | CommandOutcome::CreatedTask(_)
                    | CommandOutcome::CreatedStack(_)
                    | CommandOutcome::CreatedLabel(_))
            );
            match &outcome {
                Ok(CommandOutcome::CreatedTask(id)) => tasks.push(*id),
                Ok(CommandOutcome::CreatedStack(id)) => stacks.push(*id),
                Ok(CommandOutcome::CreatedLabel(id)) => labels.push(*id),
                _ => {}
            }

            let mem_after = handle.shared_state().load_full();
            let disk_after = repo.snapshot();

            if state_changed {
                prop_assert_ne!(
                    &disk_after,
                    &disk_before,
                    "an applied command must change the persisted state"
                );
            } else {
                prop_assert_eq!(
                    &mem_after,
                    &mem_before,
                    "rejected/no-op commands must not advance memory (step {}, outcome {:?})",
                    n,
                    outcome
                );
                prop_assert_eq!(
                    &disk_after,
                    &disk_before,
                    "rejected/no-op commands must not touch disk (step {}, outcome {:?})",
                    n,
                    outcome
                );
            }

            // Memory ≡ disk after every command (the ADR 0006 invariant).
            let disk_projection = AppState {
                boards: disk_after.boards,
                stacks: disk_after.stacks,
                tasks: disk_after.tasks,
                labels: disk_after.labels,
                sync: disk_after.sync,
                last_updated: mem_after.last_updated,
            };
            prop_assert_eq!(
                &disk_projection,
                &*mem_after,
                "memory must equal disk after step {} (outcome {:?})",
                n,
                outcome
            );

            // Signal accounting: exactly the state-changing messages signal.
            let mut signalled = 0;
            while signals.try_recv().is_ok() {
                signalled += 1;
            }
            prop_assert_eq!(
                signalled,
                usize::from(state_changed),
                "signal count must equal state-changing messages (step {})",
                n
            );
        }

        // P5: op freshness across the whole sequence.
        let disk = repo.snapshot();
        let mut seen: BTreeSet<uuid::Uuid> = BTreeSet::new();
        for pending in &disk.outbox {
            prop_assert!(
                seen.insert(pending.op_id.0),
                "duplicate op id {:?}",
                pending.op_id
            );
            let exists = match pending.op {
                LocalOp::CreateTask(id)
                | LocalOp::UpdateTask(id)
                | LocalOp::MoveTask(id)
                | LocalOp::DeleteTask(id) => disk.tasks.contains_key(&id),
                LocalOp::CreateStack(id) | LocalOp::RenameStack(id) | LocalOp::DeleteStack(id) => {
                    disk.stacks.contains_key(&id)
                }
                LocalOp::CreateLabel(id) | LocalOp::UpdateLabel(id) | LocalOp::DeleteLabel(id) => {
                    disk.labels.contains_key(&id)
                }
                LocalOp::AssignLabel(task, label) | LocalOp::UnassignLabel(task, label) => {
                    disk.tasks.contains_key(&task) && disk.labels.contains_key(&label)
                }
            };
            prop_assert!(exists, "op {:?} addresses a missing entity", pending.op_id);
        }
        Ok(())
    })
}

#[derive(Debug)]
struct FixedTestClock;

impl Clock for FixedTestClock {
    fn now(&self) -> chrono::DateTime<chrono::Utc> {
        now()
    }
}

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(64))]

    // Environment gate (testing_strategy §10): runs the live tokio
    // engine — out of Miri's scope; native runs cover it fully.
    #[cfg_attr(miri, ignore)]
    #[test]
    fn p4_p5_sequences_are_coherent_and_fresh(steps in proptest::collection::vec(step_strategy(), 0..10)) {
        drive_sequence(&steps)?;
    }
}
