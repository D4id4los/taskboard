// SPDX-License-Identifier: MIT OR Apache-2.0
//! Async black-box routing suite (A-series): one behavior per test, at
//! the engine-thread layer only — command *semantics* are pinned in the
//! domain's pure suite and are not re-asserted here.
//!
//! Determinism: fixed/advancing logical clock, counting ids, oneshot
//! replies and the Flush barrier as completion signals, paused-time
//! polling with timeout guards (no wall-clock sleeps).
//!
//! The suite is Miri-clean and runs under Miri too (it only exercises
//! basic tokio scheduling, no timers); the transition logic it routes to
//! is Miri-covered in the domain and in `core.rs`.

use std::sync::Arc;
use std::time::Duration;

use taskboard_domain::persistence::{
    BoxFuture, PersistedState, PersistenceAction, RepositoryError, TaskRepository,
};
use taskboard_domain::test_support::{CountingIds, InMemoryRepository};
use taskboard_domain::{
    AppState, Clock, CommandOutcome, EngineSignal, IdGenerator, RemoteBoard, RemoteBoardId,
    RemoteBoardSnapshot, Stack, StackClocks, StackId, StateCommand, SyncCommand, SyncErrorKind,
    SyncPhase, SyncReport, SystemEvent,
};
use taskboard_state::{EngineStartupError, ExecuteError, spawn_state_engine};

mod common;

use common::{FixedClock, T0, seed_board_state, ts};

/// Fails the first `failures` applies with `Unavailable`, then delegates —
/// non-mutating on failure, matching transactional reality.
#[derive(Debug)]
struct FlakyRepository {
    inner: InMemoryRepository,
    remaining_failures: std::sync::atomic::AtomicU32,
}

impl FlakyRepository {
    fn new(failures: u32) -> Self {
        Self {
            inner: InMemoryRepository::with_state(seed_board_state("board")),
            remaining_failures: std::sync::atomic::AtomicU32::new(failures),
        }
    }
}

impl TaskRepository for FlakyRepository {
    fn load(&self) -> BoxFuture<'_, Result<PersistedState, RepositoryError>> {
        self.inner.load()
    }

    fn apply(&self, actions: Vec<PersistenceAction>) -> BoxFuture<'_, Result<(), RepositoryError>> {
        Box::pin(async move {
            let should_fail = self
                .remaining_failures
                .try_update(
                    std::sync::atomic::Ordering::SeqCst,
                    std::sync::atomic::Ordering::SeqCst,
                    |n| (n > 0).then(|| n - 1),
                )
                .is_ok();
            if should_fail {
                return Err(RepositoryError::Unavailable);
            }
            self.inner.apply(actions).await
        })
    }
}

/// Always fails loads: the boot-hydration failure class.
#[derive(Debug)]
struct DeadRepository;

impl TaskRepository for DeadRepository {
    fn load(&self) -> BoxFuture<'_, Result<PersistedState, RepositoryError>> {
        Box::pin(async { Err(RepositoryError::Unavailable) })
    }

    fn apply(
        &self,
        _actions: Vec<PersistenceAction>,
    ) -> BoxFuture<'_, Result<(), RepositoryError>> {
        Box::pin(async { Err(RepositoryError::Unavailable) })
    }
}

/// The engine's dependencies, with every seam exposed.
struct Spawner {
    repo: Arc<dyn TaskRepository>,
    ids: Arc<dyn IdGenerator>,
    clock: Arc<dyn Clock>,
}

impl Default for Spawner {
    fn default() -> Self {
        // Every scenario is seeded with one live board: the domain
        // catalogue rejects board-scoped creates (`NoBoard`) otherwise.
        Self {
            repo: Arc::new(InMemoryRepository::with_state(seed_board_state("board"))),
            ids: Arc::new(CountingIds::new()),
            clock: FixedClock::new(T0),
        }
    }
}

/// All engine channel halves a test may need.
struct Harness {
    handle: taskboard_state::EngineHandle,
    sync_in: tokio::sync::mpsc::Receiver<SyncCommand>,
    report_tx: tokio::sync::mpsc::Sender<SyncReport>,
    system_tx: tokio::sync::broadcast::Sender<SystemEvent>,
    join: tokio::task::JoinHandle<()>,
}

async fn spawn(spawner: Spawner) -> Harness {
    let (sync_out, sync_in) = tokio::sync::mpsc::channel::<SyncCommand>(8);
    let (report_tx, report_rx) = tokio::sync::mpsc::channel::<SyncReport>(8);
    let (system_tx, system_rx) = tokio::sync::broadcast::channel::<SystemEvent>(8);
    let (handle, join) = spawn_state_engine(
        spawner.repo,
        spawner.ids,
        spawner.clock,
        sync_out,
        report_rx,
        system_rx,
    )
    .await
    .expect("engine boots");
    Harness {
        handle,
        sync_in,
        report_tx,
        system_tx,
        join,
    }
}

async fn harness() -> Harness {
    spawn(Spawner::default()).await
}

/// Polls `predicate` against the shared state with a (paused-time)
/// deadline — never a fixed sleep.
async fn eventually(handle: &taskboard_state::EngineHandle, pred: impl Fn(&AppState) -> bool) {
    let deadline = Duration::from_secs(60);
    tokio::time::timeout(deadline, async {
        loop {
            if pred(&handle.shared_state().load_full()) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("predicate satisfied within the deadline");
}

/// Drains pending signals and returns how many repaint triggers arrived.
fn drain_signals(rx: &mut tokio::sync::broadcast::Receiver<EngineSignal>) -> usize {
    let mut count = 0;
    while rx.try_recv().is_ok() {
        count += 1;
    }
    count
}

// ---------------------------------------------------------------------
// A-series
// ---------------------------------------------------------------------

/// A1: the execute reply carries the receipt and the entity is queryable
/// through the shared state.
#[tokio::test]
async fn a1_execute_reply_carries_receipt_and_state_is_queryable() {
    let h = harness().await;
    let stack = h
        .handle
        .execute(StateCommand::CreateStack {
            title: "backlog".into(),
            order: 1,
        })
        .await
        .expect("accepted");
    let CommandOutcome::CreatedStack(stack_id) = stack else {
        panic!("wrong receipt: {stack:?}");
    };
    let outcome = h
        .handle
        .execute(StateCommand::CreateTask {
            title: "write tests".into(),
            stack: stack_id,
            order: 1,
        })
        .await
        .expect("accepted");
    let CommandOutcome::CreatedTask(task_id) = outcome else {
        panic!("wrong receipt: {outcome:?}");
    };
    let app = h.handle.shared_state().load_full();
    assert_eq!(app.tasks[&task_id].title, "write tests");
    assert_eq!(app.tasks[&task_id].stack, stack_id);
    drop(h);
}

/// A2: a reply-less dispatch still persists and signals (observed after
/// the flush barrier).
#[tokio::test]
async fn a2_dispatch_persists_and_signals() {
    let h = harness().await;
    let mut signals = h.handle.subscribe();
    let stack = h
        .handle
        .execute(StateCommand::CreateStack {
            title: "backlog".into(),
            order: 1,
        })
        .await
        .expect("accepted");
    let CommandOutcome::CreatedStack(stack_id) = stack else {
        panic!("wrong receipt");
    };
    // The setup command already published (entity + syncing transient);
    // drain so the dispatch's own signal is isolated.
    let _ = drain_signals(&mut signals);

    h.handle.dispatch(StateCommand::CreateTask {
        title: "silent".into(),
        stack: stack_id,
        order: 1,
    });
    h.handle.flush().await;

    let app = h.handle.shared_state().load_full();
    assert!(app.tasks.values().any(|t| t.title == "silent"));
    assert_eq!(
        drain_signals(&mut signals),
        1,
        "exactly one repaint trigger"
    );
}

/// A3: the flush barrier settles every previously dispatched command.
#[tokio::test]
async fn a3_flush_barrier_settles_dispatches() {
    let h = harness().await;
    let stack = h
        .handle
        .execute(StateCommand::CreateStack {
            title: "backlog".into(),
            order: 1,
        })
        .await
        .expect("accepted");
    let CommandOutcome::CreatedStack(stack_id) = stack else {
        panic!("wrong receipt");
    };
    for n in 0..5 {
        h.handle.dispatch(StateCommand::CreateTask {
            title: format!("task {n}"),
            stack: stack_id,
            order: n,
        });
    }
    h.handle.flush().await;
    let app = h.handle.shared_state().load_full();
    assert_eq!(
        app.tasks.len(),
        5,
        "every dispatch landed before flush returned"
    );
}

/// A4: the nudge is forwarded iff the accepted batch enqueued >= 1 op.
#[tokio::test]
async fn a4_nudge_forwarded_iff_ops_enqueued() {
    let mut h = harness().await;
    let stack = h
        .handle
        .execute(StateCommand::CreateStack {
            title: "backlog".into(),
            order: 1,
        })
        .await
        .expect("accepted");
    let CommandOutcome::CreatedStack(stack_id) = stack else {
        panic!("wrong receipt");
    };

    // A mutating command forwards a nudge…
    h.handle
        .execute(StateCommand::CreateTask {
            title: "task".into(),
            stack: stack_id,
            order: 1,
        })
        .await
        .expect("accepted");
    let CommandOutcome::CreatedTask(task_id) = h
        .handle
        .execute(StateCommand::CreateTask {
            title: "second".into(),
            stack: stack_id,
            order: 2,
        })
        .await
        .expect("accepted")
    else {
        panic!("wrong receipt");
    };
    assert!(matches!(h.sync_in.try_recv(), Ok(SyncCommand::SyncNow)));
    // Drain the earlier commands' nudges before the no-op check.
    while h.sync_in.try_recv().is_ok() {}

    // …a no-op command does not.
    h.handle
        .execute(StateCommand::SetTaskDone {
            id: task_id,
            done: false, // already open: accepted no-op
        })
        .await
        .expect("accepted");
    assert!(
        h.sync_in.try_recv().is_err(),
        "a no-op must not nudge the sync actor"
    );
}

/// A5: a `Completed` report is ingested — the adopted entity and the
/// last-success stamp become visible in the published state.
#[tokio::test]
async fn a5_completed_report_ingestion_publishes() {
    let h = harness().await;
    h.report_tx
        .send(SyncReport::Completed {
            snapshot: RemoteBoardSnapshot {
                board: RemoteBoard {
                    id: RemoteBoardId(77),
                    title: "remote board".into(),
                    color: "00ff00".into(),
                    archived: false,
                    deleted_at: None,
                    last_modified: ts(T0),
                },
                stacks: vec![],
                tasks: vec![],
                labels: vec![],
            },
            pushes: vec![],
        })
        .await
        .expect("report channel open");

    eventually(&h.handle, |app| {
        !app.boards.is_empty()
            && app
                .boards
                .values()
                .any(|b| b.remote == Some(RemoteBoardId(77)))
            && app.sync.last_success == Some(ts(T0))
            && app.sync.phase == SyncPhase::Idle
    })
    .await;
}

/// A6: a `Failed` report sets the typed phase.
#[tokio::test]
async fn a6_failed_report_sets_typed_phase() {
    let h = harness().await;
    h.report_tx
        .send(SyncReport::Failed {
            kind: SyncErrorKind::Auth,
        })
        .await
        .expect("report channel open");

    eventually(&h.handle, |app| {
        app.sync.phase
            == SyncPhase::Failed {
                last_error: SyncErrorKind::Auth,
            }
    })
    .await;
}

/// A7: `NetworkLost` persists the Offline phase; `NetworkRestored` only
/// forwards a nudge (no self-declared phase change).
#[tokio::test]
async fn a7_network_lost_persists_offline_and_restored_only_nudges() {
    let mut h = harness().await;
    h.system_tx
        .send(SystemEvent::NetworkLost)
        .expect("broadcast");
    eventually(&h.handle, |app| app.sync.phase == SyncPhase::Offline).await;

    h.system_tx
        .send(SystemEvent::NetworkRestored)
        .expect("broadcast");
    // No state changes on restore: the nudge itself is the completion
    // signal, awaited with a paused-time deadline (never a fixed sleep).
    let nudge = tokio::time::timeout(Duration::from_secs(60), h.sync_in.recv()).await;
    assert!(
        matches!(nudge, Ok(Some(SyncCommand::SyncNow))),
        "NetworkRestored must forward a sync nudge, got {nudge:?}"
    );
    assert_eq!(
        h.handle.shared_state().load_full().sync.phase,
        SyncPhase::Offline,
        "restoration alone is no evidence; the phase stays until a report"
    );
}

/// A8: `Shutdown` stops the loop; the `JoinHandle` resolves (timeout
/// guard, not a sleep).
#[tokio::test]
async fn a8_shutdown_stops_the_loop() {
    let h = harness().await;
    h.system_tx.send(SystemEvent::Shutdown).expect("broadcast");
    tokio::time::timeout(Duration::from_secs(60), h.join)
        .await
        .expect("engine exits on shutdown")
        .expect("clean exit");
}

/// A9: a storage failure replies `Storage`, leaves the published state
/// unchanged and signal-free, and the next command succeeds against the
/// unchanged state.
#[tokio::test]
async fn a9_storage_failure_reverts_nothing_and_recovers() {
    let h = spawn(Spawner {
        repo: Arc::new(FlakyRepository::new(1)),
        ..Spawner::default()
    })
    .await;
    let mut signals = h.handle.subscribe();

    let failed = h
        .handle
        .execute(StateCommand::CreateStack {
            title: "doomed".into(),
            order: 1,
        })
        .await;
    assert!(
        matches!(
            failed,
            Err(ExecuteError::Storage(RepositoryError::Unavailable))
        ),
        "the first apply must surface as a storage failure, got {failed:?}"
    );
    assert!(
        h.handle.shared_state().load_full().stacks.is_empty(),
        "memory never advanced"
    );
    h.handle.flush().await;
    assert_eq!(
        drain_signals(&mut signals),
        0,
        "a failed command must not signal"
    );

    // Retry succeeds against the unchanged state.
    let retried = h
        .handle
        .execute(StateCommand::CreateStack {
            title: "doomed".into(),
            order: 1,
        })
        .await;
    assert!(matches!(retried, Ok(CommandOutcome::CreatedStack(_))));
}

/// A10: executing on a stopped engine yields `EngineGone`.
#[tokio::test]
async fn a10_execute_after_shutdown_is_engine_gone() {
    let h = harness().await;
    h.system_tx.send(SystemEvent::Shutdown).expect("broadcast");
    tokio::time::timeout(Duration::from_secs(60), h.join)
        .await
        .expect("engine exits")
        .expect("clean");

    let result = h
        .handle
        .execute(StateCommand::CreateStack {
            title: "late".into(),
            order: 1,
        })
        .await;
    assert!(matches!(result, Err(ExecuteError::EngineGone)));
}

/// A11: boot hydration publishes the persisted state before any command.
#[tokio::test]
async fn a11_boot_hydration_publishes_pre_loop() {
    let (state, stack_id) = seeded_stack_state();
    let h = spawn(Spawner {
        repo: Arc::new(InMemoryRepository::with_state(state)),
        ..Spawner::default()
    })
    .await;

    let app = h.handle.shared_state().load_full();
    assert!(
        app.stacks.contains_key(&stack_id),
        "the hydrated stack is visible before any command"
    );
    assert_eq!(app.sync.pending_ops, 0);
}

/// A12: a failing boot repo surfaces `EngineStartupError::Load` and no
/// engine is started.
#[tokio::test]
async fn a12_boot_failure_surfaces_load_error() {
    let (sync_out, _sync_in) = tokio::sync::mpsc::channel::<SyncCommand>(8);
    let (_report_tx, report_rx) = tokio::sync::mpsc::channel::<SyncReport>(8);
    let (_system_tx, system_rx) = tokio::sync::broadcast::channel::<SystemEvent>(8);
    let result = spawn_state_engine(
        Arc::new(DeadRepository),
        Arc::new(CountingIds::new()),
        FixedClock::new(T0),
        sync_out,
        report_rx,
        system_rx,
    )
    .await;
    assert!(matches!(
        result,
        Err(EngineStartupError::Load(RepositoryError::Unavailable))
    ));
}

// ---------------------------------------------------------------------
// shared fixtures
// ---------------------------------------------------------------------

fn seeded_stack_state() -> (PersistedState, StackId) {
    let ids = CountingIds::new();
    let board_id = ids.new_board_id();
    let stack = Stack {
        id: ids.new_stack_id(),
        remote: None,
        board: board_id,
        title: "backlog".into(),
        order: 1,
        archived: false,
        deleted: false,
        clocks: StackClocks {
            title: ts(T0),
            order: ts(T0),
            deleted: ts(T0),
        },
        remote_seen: None,
    };
    let stack_id = stack.id;
    let mut state = PersistedState::default();
    state.stacks.insert(stack.id, stack);
    (state, stack_id)
}
