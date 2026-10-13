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
    AppState, Clock, CommandOutcome, EngineSignal, IdGenerator, Notification, RemoteBoard,
    RemoteBoardId, RemoteBoardSnapshot, Stack, StackClocks, StackId, StateCommand, SyncCommand,
    SyncErrorKind, SyncPhase, SyncReport, SystemEvent,
};
use taskboard_state::{EngineStartupError, ExecuteError, ReminderEngineConfig, spawn_state_engine};

mod common;

use common::{FixedClock, T0, seed_board_state, test_validators, ts};

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

    /// Re-arms the failure injector (the edge-trigger test walks the
    /// fail -> recover -> fail cycle).
    fn rearm(&self, failures: u32) {
        self.remaining_failures
            .store(failures, std::sync::atomic::Ordering::SeqCst);
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

/// The engine's dependencies, with every seam exposed. Reminders are
/// disabled by default (the auto-advance policy); reminder tests opt in
/// via the `reminders` field.
struct Spawner {
    repo: Arc<dyn TaskRepository>,
    ids: Arc<dyn IdGenerator>,
    clock: Arc<dyn Clock>,
    reminders: ReminderEngineConfig,
}

impl Default for Spawner {
    fn default() -> Self {
        // Every scenario is seeded with one live board: the domain
        // catalogue rejects board-scoped creates (`NoBoard`) otherwise.
        Self {
            repo: Arc::new(InMemoryRepository::with_state(seed_board_state("board"))),
            ids: Arc::new(CountingIds::new()),
            clock: FixedClock::new(T0),
            reminders: ReminderEngineConfig::disabled(),
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
    // The receiver is dropped immediately: no engine.rs test consumes
    // notifications (the reminder suite owns that; the disabled config
    // never sends anyway).
    let (notifier, _notifications) = tokio::sync::mpsc::channel::<Notification>(64);
    let (handle, join) = spawn_state_engine(
        spawner.repo,
        spawner.ids,
        spawner.clock,
        sync_out,
        report_rx,
        system_tx.clone(),
        system_rx,
        notifier,
        spawner.reminders,
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

/// Waits until the engine has entered drain mode. There is no state-level
/// observation of the transition, so this polls with a harmless command
/// (`RequestSync` has no `Err` outcome other than `EngineGone`) under a
/// deadline — the reply *is* the drain-mode signal.
async fn wait_draining(handle: &taskboard_state::EngineHandle) {
    tokio::time::timeout(Duration::from_secs(60), async {
        loop {
            if handle
                .execute(StateCommand::RequestSync)
                .await
                .is_err_and(|err| matches!(err, ExecuteError::EngineGone))
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("engine entered drain mode within the deadline");
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
            snapshot: Box::new(RemoteBoardSnapshot {
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
            }),
            validators: test_validators(),
            pushes: vec![],
            read_at: chrono::DateTime::<chrono::Utc>::MAX_UTC,
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
            pushes: vec![],

            read_at: chrono::DateTime::<chrono::Utc>::MAX_UTC,
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

/// A8: `Shutdown` starts the shutdown drain; the loop exits once the
/// reports channel closes (timeout guard, not a sleep).
#[tokio::test]
async fn a8_shutdown_drains_then_exits_when_reports_close() {
    let h = harness().await;
    h.system_tx.send(SystemEvent::Shutdown).expect("broadcast");
    drop(h.report_tx); // the sync actor exiting is what ends the drain
    tokio::time::timeout(Duration::from_secs(60), h.join)
        .await
        .expect("engine exits after drain")
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

/// A17: `StorageFailed` is edge-triggered — one event per failure epoch
/// (a dying disk fires once, not once per command), reset by any
/// successful persistence.
#[tokio::test]
async fn a17_storage_failed_is_edge_triggered_per_epoch() {
    let repo = Arc::new(FlakyRepository::new(1));
    let h = spawn(Spawner {
        repo: repo.clone() as Arc<dyn TaskRepository>,
        ..Spawner::default()
    })
    .await;
    let mut events = h.system_tx.subscribe();

    let drain = |events: &mut tokio::sync::broadcast::Receiver<SystemEvent>| {
        let mut count = 0;
        while matches!(
            events.try_recv(),
            Ok(SystemEvent::StorageFailed
                | SystemEvent::NetworkLost
                | SystemEvent::NetworkRestored
                | SystemEvent::Shutdown)
        ) {
            count += 1;
        }
        count
    };

    // First failure: the event fires.
    let failed = h
        .handle
        .execute(StateCommand::CreateStack {
            title: "doomed".into(),
            order: 1,
        })
        .await;
    assert!(matches!(
        failed,
        Err(ExecuteError::Storage(RepositoryError::Unavailable))
    ));
    h.handle.flush().await;
    assert_eq!(drain(&mut events), 1, "the first failure fires the event");

    // A second command failing in the same epoch: still one event.
    repo.rearm(1);
    let _ = h
        .handle
        .execute(StateCommand::CreateStack {
            title: "doomed again".into(),
            order: 2,
        })
        .await;
    h.handle.flush().await;
    assert_eq!(drain(&mut events), 0, "the same epoch must not re-fire");

    // Recovery resets the edge; the next failure fires again.
    let retried = h
        .handle
        .execute(StateCommand::CreateStack {
            title: "survivor".into(),
            order: 1,
        })
        .await;
    assert!(retried.is_ok(), "recovery succeeds");
    h.handle.flush().await;
    assert_eq!(drain(&mut events), 0);
    repo.rearm(1);
    let _ = h
        .handle
        .execute(StateCommand::CreateStack {
            title: "doomed once more".into(),
            order: 3,
        })
        .await;
    h.handle.flush().await;
    assert_eq!(drain(&mut events), 1, "a new epoch fires again");
    drop(h);
}

/// A10: executing on a stopped engine yields `EngineGone`.
#[tokio::test]
async fn a10_execute_after_shutdown_is_engine_gone() {
    let h = harness().await;
    h.system_tx.send(SystemEvent::Shutdown).expect("broadcast");
    wait_draining(&h.handle).await;

    // The engine is draining, not gone: it still services its inbox to
    // decline the command with a reply (never a hang).
    let result = h
        .handle
        .execute(StateCommand::CreateStack {
            title: "late".into(),
            order: 1,
        })
        .await;
    assert!(matches!(result, Err(ExecuteError::EngineGone)));

    drop(h.report_tx);
    tokio::time::timeout(Duration::from_secs(60), h.join)
        .await
        .expect("engine exits after drain")
        .expect("clean");
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
    let (system_tx, system_rx) = tokio::sync::broadcast::channel::<SystemEvent>(8);
    let (notifier, _notifications) = tokio::sync::mpsc::channel::<Notification>(64);
    let result = spawn_state_engine(
        Arc::new(DeadRepository),
        Arc::new(CountingIds::new()),
        FixedClock::new(T0),
        sync_out,
        report_rx,
        system_tx,
        system_rx,
        notifier,
        ReminderEngineConfig::disabled(),
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

// ---------------------------------------------------------------------
// SyncStateReader routing (phase 4 §4.1)
// ---------------------------------------------------------------------

/// The `SyncStateReader` impl replies with the engine's full persisted
/// shape — outbox and validators included, which the published `AppState`
/// deliberately omits — reflecting every previously accepted message (FIFO,
/// same durability story as the `Flush` barrier).
#[tokio::test]
async fn read_state_returns_the_post_apply_persisted_shape() {
    use taskboard_domain::SyncStateReader as _;

    let h = spawn(Spawner::default()).await;
    h.handle
        .execute(StateCommand::CreateStack {
            title: "backlog".into(),
            order: 1,
        })
        .await
        .expect("accepted");

    let read = h.handle.read_state().await.expect("engine reachable");
    assert_eq!(read.outbox.len(), 1, "the outbox travels in the reply");
    assert_eq!(read.stacks.len(), 1, "the accepted stack is in the reply");
    let app = h.handle.shared_state().load_full();
    assert_eq!(read.stacks, app.stacks, "memory ≡ the replied shape");
}

/// A stopped engine surfaces as the port's transient `Unavailable`, not a
/// panic and not a hang.
#[tokio::test]
async fn read_state_on_a_stopped_engine_maps_to_unavailable() {
    use taskboard_domain::SyncStateReader as _;

    let h = spawn(Spawner::default()).await;
    h.join.abort();
    // Give the loop a chance to observe the abort; either way the handle's
    // send eventually fails once the task is gone.
    let outcome = h.handle.read_state().await;
    assert!(
        matches!(outcome, Err(RepositoryError::Unavailable)),
        "a stopped engine must map to Unavailable, got {outcome:?}"
    );
}

/// A `Completed` report's validators are persisted under the engine's own
/// board binding (phase 4 decision 4) — and identical validators on the
/// next report append nothing (iff-different rule).
#[tokio::test]
async fn completed_report_persists_validators_under_the_board_binding() {
    let h = spawn(Spawner::default()).await;
    let validators = taskboard_domain::BoardPullValidators {
        boards: taskboard_domain::SyncValidators {
            etag: Some("\"b1\"".into()),
            last_modified: None,
        },
        stacks: taskboard_domain::SyncValidators {
            etag: Some("\"s1\"".into()),
            last_modified: None,
        },
        archived_stacks: taskboard_domain::SyncValidators {
            etag: Some("\"a1\"".into()),
            last_modified: None,
        },
    };
    h.report_tx
        .send(SyncReport::Completed {
            snapshot: Box::new(RemoteBoardSnapshot {
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
            }),
            validators: validators.clone(),
            pushes: vec![],
            read_at: chrono::DateTime::<chrono::Utc>::MAX_UTC,
        })
        .await
        .expect("report channel open");
    eventually(&h.handle, |app| {
        app.boards
            .values()
            .any(|b| b.remote == Some(RemoteBoardId(77)))
    })
    .await;

    // Deterministic settle via a Flush: the batch is durable.
    h.handle.flush().await;
    let read = taskboard_domain::SyncStateReader::read_state(&h.handle)
        .await
        .expect("reachable");
    assert_eq!(
        read.validators.get(&taskboard_domain::ValidatorKey::Boards),
        Some(&validators.boards),
    );
    assert_eq!(
        read.validators
            .get(&taskboard_domain::ValidatorKey::Stacks(RemoteBoardId(77))),
        Some(&validators.stacks),
    );
    assert_eq!(
        read.validators
            .get(&taskboard_domain::ValidatorKey::ArchivedStacks(
                RemoteBoardId(77)
            )),
        Some(&validators.archived_stacks),
    );
}

/// A `Failed` report carrying completed pushes lands them (decision 3):
/// the op completes, the echo binds, the phase returns to `Idle` — and no
/// tombstoning happens for the missing snapshot.
#[allow(clippy::too_many_lines)] // the binding setup is the scenario
#[tokio::test]
async fn failed_report_with_pushes_applies_the_evidence() {
    use taskboard_domain::SyncStateReader as _;

    let h = spawn(Spawner::default()).await;
    // Bind board 77 and its stack 9 first, so the create's echo can bind.
    h.report_tx
        .send(SyncReport::Completed {
            snapshot: Box::new(RemoteBoardSnapshot {
                board: RemoteBoard {
                    id: RemoteBoardId(77),
                    title: "remote board".into(),
                    color: "00ff00".into(),
                    archived: false,
                    deleted_at: None,
                    last_modified: ts(T0),
                },
                stacks: vec![taskboard_domain::RemoteStack {
                    id: taskboard_domain::RemoteStackRef {
                        board: RemoteBoardId(77),
                        stack: taskboard_domain::RemoteStackId(9),
                    },
                    title: "backlog".into(),
                    order: 0,
                    archived: false,
                    deleted_at: None,
                    last_modified: ts(T0),
                }],
                tasks: vec![],
                labels: vec![],
            }),
            validators: test_validators(),
            pushes: vec![],
            read_at: chrono::DateTime::<chrono::Utc>::MAX_UTC,
        })
        .await
        .expect("report channel open");
    eventually(&h.handle, |app| !app.stacks.is_empty()).await;
    h.handle.flush().await;
    let stack_id = *taskboard_domain::SyncStateReader::read_state(&h.handle)
        .await
        .expect("read")
        .stacks
        .keys()
        .next()
        .expect("adopted stack");
    let CommandOutcome::CreatedTask(task_id) = h
        .handle
        .execute(StateCommand::CreateTask {
            title: "offline draft".into(),
            stack: stack_id,
            order: 0,
        })
        .await
        .expect("accepted")
    else {
        panic!("wrong receipt");
    };
    let op_id = h
        .handle
        .read_state()
        .await
        .expect("read")
        .outbox
        .iter()
        .find(|op| matches!(op.op, taskboard_domain::LocalOp::CreateTask(id) if id == task_id))
        .map(|op| op.op_id)
        .expect("the create op is queued");

    let mut echo_card: RemoteBoardSnapshot = RemoteBoardSnapshot {
        board: RemoteBoard {
            id: RemoteBoardId(77),
            title: "b".into(),
            color: "00ff00".into(),
            archived: false,
            deleted_at: None,
            last_modified: ts(T0),
        },
        stacks: vec![],
        tasks: vec![],
        labels: vec![],
    };
    echo_card.tasks.push(taskboard_domain::RemoteTask {
        id: taskboard_domain::RemoteCardRef {
            board: RemoteBoardId(77),
            stack: taskboard_domain::RemoteStackId(9),
            card: taskboard_domain::RemoteCardId(42),
        },
        title: "offline draft".into(),
        description: String::new(),
        duedate: None,
        done: None,
        stack: taskboard_domain::RemoteStackId(9),
        order: 0,
        labels: std::collections::BTreeSet::new(),
        archived: false,
        last_modified: ts(T0 + 60),
    });
    let echo = echo_card.tasks.pop().expect("echo task");
    h.report_tx
        .send(SyncReport::Failed {
            kind: SyncErrorKind::Network,
            pushes: vec![taskboard_domain::PushOutcome {
                op: op_id,
                result: taskboard_domain::PushResult::Applied {
                    echo: Some(taskboard_domain::RemoteEcho::Task(echo)),
                },
            }],

            read_at: chrono::DateTime::<chrono::Utc>::MAX_UTC,
        })
        .await
        .expect("report channel open");

    eventually(&h.handle, |app| {
        app.sync.pending_ops == 0
            && app.tasks.get(&task_id).is_some_and(|t| {
                t.remote
                    == Some(taskboard_domain::RemoteCardRef {
                        board: RemoteBoardId(77),
                        stack: taskboard_domain::RemoteStackId(9),
                        card: taskboard_domain::RemoteCardId(42),
                    })
            })
            && app.sync.phase == SyncPhase::Idle
    })
    .await;
}

// ---------------------------------------------------------------------
// G-series: shutdown linger-drain semantics (ADR 0008)
// ---------------------------------------------------------------------

/// A failed cycle report whose ingestion flips the published phase — the
/// observable evidence that a report *landed*.
fn failed_report() -> SyncReport {
    SyncReport::Failed {
        kind: SyncErrorKind::Network,
        pushes: vec![],
        read_at: ts(T0),
    }
}

/// G1: a report sent *after* `Shutdown` is still ingested — the drain
/// keeps landing cycle evidence instead of dropping it (the
/// duplicate-create window).
#[tokio::test]
async fn g1_shutdown_keeps_ingesting_reports() {
    let h = harness().await;
    h.system_tx.send(SystemEvent::Shutdown).expect("broadcast");
    h.report_tx
        .send(failed_report())
        .await
        .expect("channel open");

    eventually(&h.handle, |app| {
        app.sync.phase
            == SyncPhase::Failed {
                last_error: SyncErrorKind::Network,
            }
    })
    .await;

    drop(h.report_tx);
    tokio::time::timeout(Duration::from_secs(60), h.join)
        .await
        .expect("engine exits after drain")
        .expect("clean");
}

/// G2: in drain mode `Execute` is declined with `EngineGone` and `Flush`
/// still answers promptly (the barrier never hangs).
#[tokio::test]
async fn g2_drain_declines_execute_and_answers_flush() {
    let h = harness().await;
    h.system_tx.send(SystemEvent::Shutdown).expect("broadcast");
    wait_draining(&h.handle).await;

    let declined = h
        .handle
        .execute(StateCommand::CreateStack {
            title: "late".into(),
            order: 1,
        })
        .await;
    assert!(matches!(declined, Err(ExecuteError::EngineGone)));

    tokio::time::timeout(Duration::from_secs(60), h.handle.flush())
        .await
        .expect("drain-mode flush answers promptly");

    drop(h.report_tx);
    tokio::time::timeout(Duration::from_secs(60), h.join)
        .await
        .expect("engine exits after drain")
        .expect("clean");
}

/// G3: closing the reports channel after `Shutdown` ends the drain — the
/// engine task exits (the sync actor owns the sender, so its exit is the
/// drain's end signal).
#[tokio::test]
async fn g3_reports_close_ends_the_drain() {
    let h = harness().await;
    h.system_tx.send(SystemEvent::Shutdown).expect("broadcast");
    drop(h.report_tx);
    tokio::time::timeout(Duration::from_secs(60), h.join)
        .await
        .expect("drain ends when reports close")
        .expect("clean");
}

/// G4: `Shutdown` with the reports channel *already* closed exits
/// immediately — no drain that can never end.
#[tokio::test]
async fn g4_shutdown_with_reports_already_closed_exits_immediately() {
    let h = harness().await;
    drop(h.report_tx);
    // Let the engine observe the closure first (it logs and keeps
    // running as a daemon); the shutdown then drains an empty channel.
    eventually_idle(&h.handle).await;
    h.system_tx.send(SystemEvent::Shutdown).expect("broadcast");
    tokio::time::timeout(Duration::from_secs(60), h.join)
        .await
        .expect("immediate exit")
        .expect("clean");
}

/// G5: a report sent *before* `Shutdown` is ingested before the drain
/// begins (biased order: reports are drained ahead of system events).
#[tokio::test]
async fn g5_pre_shutdown_reports_ingest_before_drain() {
    let h = harness().await;
    h.report_tx
        .send(failed_report())
        .await
        .expect("channel open");
    h.system_tx.send(SystemEvent::Shutdown).expect("broadcast");

    eventually(&h.handle, |app| {
        app.sync.phase
            == SyncPhase::Failed {
                last_error: SyncErrorKind::Network,
            }
    })
    .await;

    drop(h.report_tx);
    tokio::time::timeout(Duration::from_secs(60), h.join)
        .await
        .expect("engine exits after drain")
        .expect("clean");
}

/// G6: a second `Shutdown` during the drain is ignored — idempotent, no
/// state corruption, no premature exit.
#[tokio::test]
async fn g6_second_shutdown_during_drain_is_ignored() {
    let h = harness().await;
    h.system_tx.send(SystemEvent::Shutdown).expect("broadcast");
    h.system_tx.send(SystemEvent::Shutdown).expect("broadcast");
    drop(h.report_tx);
    tokio::time::timeout(Duration::from_secs(60), h.join)
        .await
        .expect("engine exits after drain")
        .expect("clean");
}

/// Polls until the engine has observed the reports-channel closure
/// (observable as... nothing in state — so this waits for the *absence*
/// of an exit across a bounded window instead: the daemon contract).
async fn eventually_idle(handle: &taskboard_state::EngineHandle) {
    // The engine must still be running: a command is accepted (a no-op
    // create against an unbound board would be rejected — use flush as a
    // liveness probe instead).
    tokio::time::timeout(Duration::from_secs(60), handle.flush())
        .await
        .expect("engine still services commands after reports closed");
}

// ---------------------------------------------------------------------
// A-series: stand-alone board commands
// ---------------------------------------------------------------------

/// A13: `CreateBoard` on an empty db replies `CreatedBoard`, publishes the
/// board, and persists exactly the `UpsertBoard` (no outbox op).
#[tokio::test]
async fn a13_create_board_persists_and_publishes() {
    let repo = Arc::new(InMemoryRepository::with_state(PersistedState::default()));
    let h = spawn(Spawner {
        repo: repo.clone(),
        ..Spawner::default()
    })
    .await;
    let mut signals = h.handle.subscribe();
    let outcome = h
        .handle
        .execute(StateCommand::CreateBoard {
            title: "personal".into(),
            color: taskboard_domain::Color::new("3366cc"),
        })
        .await
        .expect("accepted");
    let CommandOutcome::CreatedBoard(board_id) = outcome else {
        panic!("wrong receipt: {outcome:?}");
    };
    let app = h.handle.shared_state().load_full();
    let board = &app.boards[&board_id];
    assert_eq!(board.title, "personal");
    assert_eq!(board.remote, None);
    // Exactly one upsert in the batch: no outbox op was enqueued.
    assert_eq!(repo.snapshot().outbox, [], "no outbox op was enqueued");
    assert_eq!(drain_signals(&mut signals), 1, "published");
    drop(h);
}

/// A14: `CreateBoard` while a live board exists is a typed rejection.
#[tokio::test]
async fn a14_create_board_refuses_a_second_live_board() {
    let h = harness().await;
    let err = h
        .handle
        .execute(StateCommand::CreateBoard {
            title: "second".into(),
            color: taskboard_domain::Color::new("000000"),
        })
        .await
        .expect_err("rejected");
    assert!(matches!(
        err,
        ExecuteError::Rejected(taskboard_domain::CommandError::BoardExists)
    ));
    drop(h);
}

/// A15: `BindBoard` mutates the board row in memory and persistence.
#[tokio::test]
async fn a15_bind_board_sets_the_remote_binding() {
    let h = harness().await;
    let outcome = h
        .handle
        .execute(StateCommand::BindBoard {
            remote: RemoteBoardId(42),
        })
        .await
        .expect("accepted");
    assert_eq!(outcome, CommandOutcome::Applied);
    let app = h.handle.shared_state().load_full();
    assert!(
        app.boards
            .values()
            .any(|b| b.remote == Some(RemoteBoardId(42)))
    );
    drop(h);
}

/// A16: `MoveStack` routes end to end — `Applied`, the order change is
/// published, the sync nudge rides the enqueued op; an unchanged order
/// is an accepted `NoOp` that enqueues nothing.
#[tokio::test]
async fn a16_move_stack_routes_applies_and_nudges_iff_changed() {
    let mut h = harness().await;
    let created = h
        .handle
        .execute(StateCommand::CreateStack {
            title: "todo".into(),
            order: 0,
        })
        .await
        .expect("accepted");
    let CommandOutcome::CreatedStack(stack_id) = created else {
        panic!("wrong receipt");
    };
    // Drain the setup's nudge + signal.
    while h.sync_in.try_recv().is_ok() {}

    let outcome = h
        .handle
        .execute(StateCommand::MoveStack {
            id: stack_id,
            order: 7,
        })
        .await
        .expect("accepted");
    assert_eq!(outcome, CommandOutcome::Applied);
    let app = h.handle.shared_state().load_full();
    assert_eq!(app.stacks[&stack_id].order, 7);
    drop(app);
    assert!(
        h.sync_in.recv().await.is_some(),
        "an enqueued op forwards the sync nudge"
    );

    // Unchanged order: NoOp, no nudge.
    let outcome = h
        .handle
        .execute(StateCommand::MoveStack {
            id: stack_id,
            order: 7,
        })
        .await
        .expect("accepted");
    assert_eq!(outcome, CommandOutcome::NoOp);
    assert!(h.sync_in.try_recv().is_err(), "a no-op enqueues nothing");
    drop(h);
}

// ---------------------------------------------------------------------
// WI-H: the engine survives connector death
// ---------------------------------------------------------------------

/// A detached/died sync actor drops the reports sender mid-life; the
/// engine is a daemon and must keep serving local edits (the
/// "continuing without sync" contract the WI-H detach relies on).
/// `RequestSync` afterwards answers `NoOp` — the nudge fails closed on
/// the dead channel without marking `Syncing`.
#[tokio::test]
async fn h1_engine_continues_without_sync_after_the_reports_channel_closes() {
    let h = harness().await;
    // The actor is gone: its inbox receiver is gone with it.
    drop(h.sync_in);

    // The connector dies: the reports sender drops (the actor's exit).
    drop(h.report_tx);

    // Local editing keeps working: create a stack and a task.
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
            title: "offline edit".into(),
            stack: stack_id,
            order: 1,
        })
        .await
        .expect("accepted");
    assert!(matches!(outcome, CommandOutcome::CreatedTask(_)));

    // The sync nudge fails closed: NoOp, and the phase never lies.
    let outcome = h
        .handle
        .execute(StateCommand::RequestSync)
        .await
        .expect("the engine still answers");
    assert_eq!(outcome, CommandOutcome::NoOp);
    assert_eq!(
        h.handle.shared_state().load_full().sync.phase,
        SyncPhase::Idle,
        "a dead connector must not mark the engine Syncing"
    );
}

// ---------------------------------------------------------------------
// WI-J: the connector registry and attach
// ---------------------------------------------------------------------

/// Poll/receive deadline shared by the j-series (paused-time guarded).
const RECV_DEADLINE: Duration = Duration::from_secs(60);

/// Timeout-guarded receive of one nudged `SyncCommand` — the reply *is*
/// the completion signal, never a sleep.
async fn expect_nudge(rx: &mut tokio::sync::mpsc::Receiver<SyncCommand>) -> SyncCommand {
    tokio::time::timeout(RECV_DEADLINE, rx.recv())
        .await
        .expect("nudge arrives within the deadline")
        .expect("the nudged sender is live")
}

/// j1: attach registers the sender; every nudge issued *after* the ack
/// fans out through it (the ack's FIFO proof).
#[tokio::test]
async fn j1_attach_registers_the_sender_and_later_nudges_fan_out() {
    let h = harness().await;
    let (tx, mut rx) = tokio::sync::mpsc::channel::<SyncCommand>(8);

    let receipt = h.handle.attach_connector(tx).await.expect("attach ack");
    assert_eq!(receipt, taskboard_state::AttachReceipt::Attached);

    let outcome = h
        .handle
        .execute(StateCommand::RequestSync)
        .await
        .expect("accepted");
    assert_eq!(outcome, CommandOutcome::SyncRequested);
    assert_eq!(expect_nudge(&mut rx).await, SyncCommand::SyncNow);
}

/// j2: the standalone dead entry (receiver dropped at boot, reports
/// channel still open — the parked-sender world) prunes on the first
/// fan-out, and the registry keeps accepting entries afterwards.
#[tokio::test]
async fn j2_standalone_dead_entry_prunes_and_the_registry_keeps_working() {
    let h = harness().await;
    drop(h.sync_in);

    // Fail closed: NoOp, and no `Syncing` transient despite the open
    // reports channel (fact 4's trap, now with a parked sender).
    let outcome = h
        .handle
        .execute(StateCommand::RequestSync)
        .await
        .expect("accepted");
    assert_eq!(outcome, CommandOutcome::NoOp);
    assert_eq!(
        h.handle.shared_state().load_full().sync.phase,
        SyncPhase::Idle
    );

    // The prune emptied the registry; it still accepts entries.
    let (tx, mut rx) = tokio::sync::mpsc::channel::<SyncCommand>(8);
    let receipt = h.handle.attach_connector(tx).await.expect("attach ack");
    assert_eq!(receipt, taskboard_state::AttachReceipt::Attached);

    let outcome = h
        .handle
        .execute(StateCommand::RequestSync)
        .await
        .expect("accepted");
    assert_eq!(outcome, CommandOutcome::SyncRequested);
    assert_eq!(expect_nudge(&mut rx).await, SyncCommand::SyncNow);
}

/// j3: fan-out forwards iff any send lands; `Closed` entries prune
/// lazily over both dead paths — a sender dead *after* attach (the
/// detached-actor path) and, per j2, the one dead at boot.
#[tokio::test]
async fn j3_fan_out_forwards_iff_any_send_lands_and_prunes_dead_entries() {
    let h = harness().await;
    // The boot entry is dead by construction here (j2's standalone
    // origin) and prunes on the first fan-out pass.
    drop(h.sync_in);
    let (tx_a, rx_a) = tokio::sync::mpsc::channel::<SyncCommand>(8);
    let (tx_b, mut rx_b) = tokio::sync::mpsc::channel::<SyncCommand>(8);
    h.handle.attach_connector(tx_a).await.expect("attach ack");
    h.handle.attach_connector(tx_b).await.expect("attach ack");

    // A's receiver dies after attach (the detached-actor dead path):
    // the nudge still forwards through B, and A's dead entry is pruned
    // in the same pass.
    drop(rx_a);
    let outcome = h
        .handle
        .execute(StateCommand::RequestSync)
        .await
        .expect("accepted");
    assert_eq!(outcome, CommandOutcome::SyncRequested);
    assert_eq!(expect_nudge(&mut rx_b).await, SyncCommand::SyncNow);

    // All dead: the registry is empty and the nudge fails closed.
    drop(rx_b);
    let outcome = h
        .handle
        .execute(StateCommand::RequestSync)
        .await
        .expect("accepted");
    assert_eq!(outcome, CommandOutcome::NoOp);
}

/// j4: a full connector inbox still counts as forwarded (`Full` is a
/// coalesce, not a failure) — pinned through the new fan-out, with the
/// boot entry dropped so only the full inbox can carry the outcome.
#[tokio::test]
async fn j4_full_connector_inbox_still_counts_as_forwarded() {
    let h = harness().await;
    drop(h.sync_in);
    // Capacity 1, never drained: the first nudge lands (`Ok`), the
    // second coalesces (`Full`).
    let (tx, _rx_kept) = tokio::sync::mpsc::channel::<SyncCommand>(1);
    h.handle.attach_connector(tx).await.expect("attach ack");

    let outcome = h
        .handle
        .execute(StateCommand::RequestSync)
        .await
        .expect("accepted");
    assert_eq!(outcome, CommandOutcome::SyncRequested);
    let outcome = h
        .handle
        .execute(StateCommand::RequestSync)
        .await
        .expect("accepted");
    assert_eq!(outcome, CommandOutcome::SyncRequested);
}

/// j5: attach during drain is declined typed — and the decline leaves
/// the loop's exit conditions untouched (the engine still finishes its
/// drain once the reports channel closes).
#[tokio::test]
async fn j5_attach_during_drain_is_declined_typed() {
    let h = harness().await;
    h.system_tx
        .send(SystemEvent::Shutdown)
        .expect("system live");
    wait_draining(&h.handle).await;

    let (tx, _rx) = tokio::sync::mpsc::channel::<SyncCommand>(8);
    let receipt = h.handle.attach_connector(tx).await.expect("attach ack");
    assert_eq!(receipt, taskboard_state::AttachReceipt::Draining);

    drop(h.report_tx);
    tokio::time::timeout(RECV_DEADLINE, h.join)
        .await
        .expect("the engine drains and stops within the deadline")
        .expect("the loop exits cleanly");
}
