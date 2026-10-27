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
    AppState, Clock, CommandOutcome, EngineSignal, IdGenerator, NotifierMessage, RemoteBoard,
    RemoteBoardId, RemoteBoardSnapshot, Stack, StackClocks, StackId, StateCommand, SyncCommand,
    SyncErrorKind, SyncPhase, SyncReport, SystemEvent,
};
use taskboard_state::{EngineStartupError, ExecuteError, ReminderEngineConfig, spawn_state_engine};

mod common;

use common::{FixedClock, T0, seed_board_state, seed_bound_board_state, test_validators, ts};

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

/// Delegates to an in-memory repository and records every applied batch —
/// the observation seam for "append nothing" assertions (the domain fake
/// keeps state only).
#[derive(Debug)]
struct RecordingRepository {
    inner: InMemoryRepository,
    log: std::sync::Mutex<Vec<Vec<PersistenceAction>>>,
}

impl RecordingRepository {
    fn seeded() -> Self {
        Self {
            inner: InMemoryRepository::with_state(seed_board_state("board")),
            log: std::sync::Mutex::new(Vec::new()),
        }
    }

    fn batches(&self) -> Vec<Vec<PersistenceAction>> {
        self.log.lock().expect("poisoned").clone()
    }
}

impl TaskRepository for RecordingRepository {
    fn load(&self) -> BoxFuture<'_, Result<PersistedState, RepositoryError>> {
        self.inner.load()
    }

    fn apply(&self, actions: Vec<PersistenceAction>) -> BoxFuture<'_, Result<(), RepositoryError>> {
        Box::pin(async move {
            self.log.lock().expect("poisoned").push(actions.clone());
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
        // Every scenario is seeded with one live board: board-scoped
        // commands name their target, and `Harness::board_id` expects
        // exactly one.
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
    /// The engine's notifier out half: the withdraw direction's witness
    /// (the reminder suite owns fire-path coverage).
    notifications: tokio::sync::mpsc::Receiver<NotifierMessage>,
    join: tokio::task::JoinHandle<()>,
}

async fn spawn(spawner: Spawner) -> Harness {
    let (sync_out, sync_in) = tokio::sync::mpsc::channel::<SyncCommand>(8);
    let (report_tx, report_rx) = tokio::sync::mpsc::channel::<SyncReport>(8);
    let (system_tx, system_rx) = tokio::sync::broadcast::channel::<SystemEvent>(8);
    let (notifier, notifications) = tokio::sync::mpsc::channel::<NotifierMessage>(64);
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
        notifications,
        join,
    }
}

async fn harness() -> Harness {
    spawn(Spawner::default()).await
}

impl Harness {
    /// The harness's single seeded board id (`seed_board_state` mints
    /// exactly one).
    fn board_id(&self) -> taskboard_domain::BoardId {
        *self
            .handle
            .shared_state()
            .load()
            .boards
            .keys()
            .next()
            .expect("the fixture has exactly one board")
    }
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
            board: h.board_id(),
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
            board: h.board_id(),
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
            board: h.board_id(),
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
            board: h.board_id(),
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
            app_data: taskboard_domain::AppDataEvidence::Absent,
            duration: std::time::Duration::ZERO,
            connector: taskboard_domain::test_support::test_connector(),
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
            features: taskboard_domain::DeckFeatures::CONSERVATIVE,
        })
        .await
        .expect("report channel open");

    eventually(&h.handle, |app| {
        !app.boards.is_empty()
            && app.boards.values().any(|b| {
                b.remote == taskboard_domain::test_support::test_binding(RemoteBoardId(77))
            })
            && app.sync.aggregate().last_success == Some(ts(T0))
            && app.sync.aggregate().phase == SyncPhase::Idle
    })
    .await;
}

/// A6: a `Failed` report sets the typed phase.
#[tokio::test]
async fn a6_failed_report_sets_typed_phase() {
    // The per-board status write addresses bound boards.
    let h = spawn(Spawner {
        repo: Arc::new(InMemoryRepository::with_state(
            common::seed_bound_board_state("board"),
        )),
        ..Spawner::default()
    })
    .await;
    h.report_tx
        .send(SyncReport::Failed {
            duration: std::time::Duration::ZERO,
            board: None,
            connector: taskboard_domain::test_support::test_connector(),
            kind: SyncErrorKind::Auth,
            pushes: vec![],

            read_at: chrono::DateTime::<chrono::Utc>::MAX_UTC,
            features: taskboard_domain::DeckFeatures::CONSERVATIVE,
        })
        .await
        .expect("report channel open");

    eventually(&h.handle, |app| {
        app.sync.aggregate().phase
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
    let mut h = spawn(Spawner {
        repo: Arc::new(InMemoryRepository::with_state(
            common::seed_bound_board_state("board"),
        )),
        ..Spawner::default()
    })
    .await;
    h.system_tx
        .send(SystemEvent::NetworkLost)
        .expect("broadcast");
    eventually(&h.handle, |app| {
        app.sync.aggregate().phase == SyncPhase::Offline
    })
    .await;

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
        h.handle.shared_state().load_full().sync.aggregate().phase,
        SyncPhase::Offline,
        "restoration alone is no evidence; the phase stays until a report"
    );
}

/// A7's per-connector sibling: `ConnectorLost(c)` flips only c's bound
/// boards to `Offline`; the board bound to another connector keeps its
/// phase (the scoped write derives its target set from the binding tags).
#[tokio::test]
async fn a7c_connector_lost_flips_only_that_connectors_boards() {
    let mut state = common::seed_bound_board_state("alpha-board");
    let alpha_board = *state.boards.keys().next().expect("seeded board");
    // The second board is a re-tagged clone of the seeded shape with a
    // distinct id (`common::board` mints the same deterministic id).
    let beta_board = taskboard_domain::BoardId::from(uuid::Uuid::from_u128(2));
    let mut beta = state.boards[&alpha_board].clone();
    beta.id = beta_board;
    beta.title = "beta-board".into();
    beta.remote = Some(taskboard_domain::RemoteBoardBinding {
        connector: taskboard_domain::ConnectorId::from("beta"),
        board: taskboard_domain::RemoteBoardId(43),
    });
    state.boards.insert(beta_board, beta);
    let h = spawn(Spawner {
        repo: Arc::new(InMemoryRepository::with_state(state)),
        ..Spawner::default()
    })
    .await;

    h.system_tx
        .send(SystemEvent::ConnectorLost(
            taskboard_domain::ConnectorId::from("nextcloud"),
        ))
        .expect("broadcast");
    eventually(&h.handle, |app| {
        app.sync
            .get(alpha_board)
            .is_some_and(|row| row.phase == SyncPhase::Offline)
    })
    .await;
    assert_eq!(
        h.handle
            .shared_state()
            .load_full()
            .sync
            .get(beta_board)
            .map(|row| row.phase),
        Some(SyncPhase::Idle),
        "the other connector's board keeps its phase"
    );
}

/// A7's empty-target sibling: a `ConnectorLost` for a connector with no
/// bound boards writes nothing anywhere.
#[tokio::test]
async fn a7d_connector_lost_without_bound_boards_writes_nothing() {
    let h = spawn(Spawner {
        repo: Arc::new(InMemoryRepository::with_state(
            common::seed_bound_board_state("board"),
        )),
        ..Spawner::default()
    })
    .await;
    let mut signals = h.handle.subscribe();

    h.system_tx
        .send(SystemEvent::ConnectorLost(
            taskboard_domain::ConnectorId::from("beta"),
        ))
        .expect("broadcast");
    h.handle.flush().await;
    assert_eq!(drain_signals(&mut signals), 0, "no write, no repaint");
    assert_eq!(
        h.handle.shared_state().load_full().sync.aggregate().phase,
        SyncPhase::Idle,
        "no board flipped"
    );
}

/// A7's per-connector restore sibling: `ConnectorRestored(c)` forwards
/// the sync nudge and changes no phase.
#[tokio::test]
async fn a7e_connector_restored_forwards_the_nudge_only() {
    let mut h = spawn(Spawner {
        repo: Arc::new(InMemoryRepository::with_state(
            common::seed_bound_board_state("board"),
        )),
        ..Spawner::default()
    })
    .await;

    h.system_tx
        .send(SystemEvent::ConnectorRestored(
            taskboard_domain::ConnectorId::from("nextcloud"),
        ))
        .expect("broadcast");
    let nudge = tokio::time::timeout(Duration::from_secs(60), h.sync_in.recv()).await;
    assert!(
        matches!(nudge, Ok(Some(SyncCommand::SyncNow))),
        "ConnectorRestored must forward a sync nudge, got {nudge:?}"
    );
    assert_eq!(
        h.handle.shared_state().load_full().sync.aggregate().phase,
        SyncPhase::Idle,
        "restoration alone is no evidence; the phase stays until a report"
    );
}

/// A7's draining sibling: per-connector network events arriving during
/// the shutdown drain are ignored — no offline write, no nudge.
#[tokio::test]
async fn a7f_draining_engine_ignores_connector_events() {
    let mut h = spawn(Spawner {
        repo: Arc::new(InMemoryRepository::with_state(
            common::seed_bound_board_state("board"),
        )),
        ..Spawner::default()
    })
    .await;
    h.system_tx.send(SystemEvent::Shutdown).expect("broadcast");

    h.system_tx
        .send(SystemEvent::ConnectorLost(
            taskboard_domain::ConnectorId::from("nextcloud"),
        ))
        .expect("broadcast");
    h.system_tx
        .send(SystemEvent::ConnectorRestored(
            taskboard_domain::ConnectorId::from("nextcloud"),
        ))
        .expect("broadcast");
    h.handle.flush().await;
    while h.sync_in.try_recv().is_ok() {}
    assert_eq!(
        h.handle.shared_state().load_full().sync.aggregate().phase,
        SyncPhase::Idle,
        "draining: no offline write"
    );
    assert!(
        h.sync_in.try_recv().is_err(),
        "draining: no nudge forwarded"
    );

    drop(h.report_tx);
    tokio::time::timeout(Duration::from_secs(60), h.join)
        .await
        .expect("engine exits after drain")
        .expect("clean exit");
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
            board: h.board_id(),
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
            board: h.board_id(),
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
            board: h.board_id(),
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
            board: h.board_id(),
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
            board: h.board_id(),
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
            board: h.board_id(),
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
            board: h.board_id(),
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
    assert_eq!(app.sync.aggregate().pending_ops, 0);
}

/// A12: a failing boot repo surfaces `EngineStartupError::Load` and no
/// engine is started.
#[tokio::test]
async fn a12_boot_failure_surfaces_load_error() {
    let (sync_out, _sync_in) = tokio::sync::mpsc::channel::<SyncCommand>(8);
    let (_report_tx, report_rx) = tokio::sync::mpsc::channel::<SyncReport>(8);
    let (system_tx, system_rx) = tokio::sync::broadcast::channel::<SystemEvent>(8);
    let (notifier, _notifications) = tokio::sync::mpsc::channel::<NotifierMessage>(64);
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
            board: h.board_id(),
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
            app_data: taskboard_domain::AppDataEvidence::Absent,
            duration: std::time::Duration::ZERO,
            connector: taskboard_domain::test_support::test_connector(),
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
            features: taskboard_domain::DeckFeatures::CONSERVATIVE,
        })
        .await
        .expect("report channel open");
    eventually(&h.handle, |app| {
        app.boards
            .values()
            .any(|b| b.remote == taskboard_domain::test_support::test_binding(RemoteBoardId(77)))
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

/// The report's own board owns the validator keys at ≥2 bound boards:
/// a `Completed` report for the *second* board files its validators
/// under the second board's remote id — never under the first-bound
/// board's (the pre-chunk-4 pick, which would misfile them at ≥2).
#[tokio::test]
async fn validators_file_under_the_reported_board_not_the_first_bound_one() {
    let (state, _second) = two_bound_boards();
    let h = spawn(Spawner {
        repo: Arc::new(InMemoryRepository::with_state(state)),
        ..Spawner::default()
    })
    .await;

    let mut validators = etag_validators();
    validators.stacks.etag = Some("\"s9\"".into());
    validators.archived_stacks.etag = Some("\"a9\"".into());
    h.report_tx
        .send(completed_report_for(RemoteBoardId(9), validators.clone()))
        .await
        .expect("report channel open");
    h.handle.flush().await;

    let read = taskboard_domain::SyncStateReader::read_state(&h.handle)
        .await
        .expect("reachable");
    assert_eq!(
        read.validators
            .get(&taskboard_domain::ValidatorKey::Stacks(RemoteBoardId(9))),
        Some(&validators.stacks),
        "the reported board owns the stacks key",
    );
    assert_eq!(
        read.validators
            .get(&taskboard_domain::ValidatorKey::ArchivedStacks(
                RemoteBoardId(9)
            )),
        Some(&validators.archived_stacks),
    );
    assert_eq!(
        read.validators
            .get(&taskboard_domain::ValidatorKey::Stacks(RemoteBoardId(7))),
        None,
        "nothing misfiles under the first-bound board's remote id",
    );
}

/// One `Completed` report per board settles *each* bound board's own
/// status row — the per-board map, not a single shared phase.
#[tokio::test]
async fn per_board_reports_settle_each_bound_boards_row() {
    let (state, _second) = two_bound_boards();
    let h = spawn(Spawner {
        repo: Arc::new(InMemoryRepository::with_state(state)),
        ..Spawner::default()
    })
    .await;

    for remote in [RemoteBoardId(9), RemoteBoardId(7)] {
        h.report_tx
            .send(completed_report_for(remote, test_validators()))
            .await
            .expect("report channel open");
    }
    eventually(&h.handle, |app| {
        app.boards.len() == 2
            && app.boards.values().all(|b| {
                app.sync.get(b.id).is_some_and(|row| {
                    row.phase == SyncPhase::Idle && row.last_success == Some(ts(T0))
                })
            })
    })
    .await;
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
            app_data: taskboard_domain::AppDataEvidence::Absent,
            duration: std::time::Duration::ZERO,
            connector: taskboard_domain::test_support::test_connector(),
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
            features: taskboard_domain::DeckFeatures::CONSERVATIVE,
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
        scheduled: None,
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
            duration: std::time::Duration::ZERO,
            board: None,
            connector: taskboard_domain::test_support::test_connector(),
            kind: SyncErrorKind::Network,
            pushes: vec![taskboard_domain::PushOutcome {
                op: op_id,
                result: taskboard_domain::PushResult::Applied {
                    echo: Some(taskboard_domain::RemoteEcho::Task(echo)),
                },
            }],

            read_at: chrono::DateTime::<chrono::Utc>::MAX_UTC,
            features: taskboard_domain::DeckFeatures::CONSERVATIVE,
        })
        .await
        .expect("report channel open");

    eventually(&h.handle, |app| {
        app.sync.aggregate().pending_ops == 0
            && app.tasks.get(&task_id).is_some_and(|t| {
                t.remote
                    == Some(taskboard_domain::RemoteCardRef {
                        board: RemoteBoardId(77),
                        stack: taskboard_domain::RemoteStackId(9),
                        card: taskboard_domain::RemoteCardId(42),
                    })
            })
            && app.sync.aggregate().phase == SyncPhase::Idle
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
        duration: std::time::Duration::ZERO,
        board: None,
        connector: taskboard_domain::test_support::test_connector(),
        kind: SyncErrorKind::Network,
        pushes: vec![],
        read_at: ts(T0),
        features: taskboard_domain::DeckFeatures::CONSERVATIVE,
    }
}

/// G1: a report sent *after* `Shutdown` is still ingested — the drain
/// keeps landing cycle evidence instead of dropping it (the
/// duplicate-create window).
#[tokio::test]
async fn g1_shutdown_keeps_ingesting_reports() {
    // The per-board status write addresses bound boards.
    let h = spawn(Spawner {
        repo: Arc::new(InMemoryRepository::with_state(
            common::seed_bound_board_state("board"),
        )),
        ..Spawner::default()
    })
    .await;
    h.system_tx.send(SystemEvent::Shutdown).expect("broadcast");
    h.report_tx
        .send(failed_report())
        .await
        .expect("channel open");

    eventually(&h.handle, |app| {
        app.sync.aggregate().phase
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
            board: h.board_id(),
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
    // The per-board status write addresses bound boards.
    let h = spawn(Spawner {
        repo: Arc::new(InMemoryRepository::with_state(
            common::seed_bound_board_state("board"),
        )),
        ..Spawner::default()
    })
    .await;
    h.report_tx
        .send(failed_report())
        .await
        .expect("channel open");
    h.system_tx.send(SystemEvent::Shutdown).expect("broadcast");

    eventually(&h.handle, |app| {
        app.sync.aggregate().phase
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

/// A14: `CreateBoard` always mints — a second create lands a distinct
/// board (the refusal lift; titles are not uniqueness keys).
#[tokio::test]
async fn a14_create_board_always_mints_a_distinct_board() {
    let h = harness().await;
    let outcome = h
        .handle
        .execute(StateCommand::CreateBoard {
            title: "second".into(),
            color: taskboard_domain::Color::new("000000"),
        })
        .await
        .expect("accepted");
    let CommandOutcome::CreatedBoard(second) = outcome else {
        panic!("wrong receipt: {outcome:?}");
    };
    assert!(
        h.handle.shared_state().load().boards.contains_key(&second),
        "the second board is live in the published state"
    );
    drop(h);
}

/// A15: `BindBoard` mutates the board row in memory and persistence.
#[tokio::test]
async fn a15_bind_board_sets_the_remote_binding() {
    // The harness board ships bound (the configured-install fixture);
    // the binding act itself needs an unbound one.
    let mut unbound = common::seed_bound_board_state("board");
    for board in unbound.boards.values_mut() {
        board.remote = None;
    }
    let h = spawn(Spawner {
        repo: Arc::new(InMemoryRepository::with_state(unbound)),
        ..Spawner::default()
    })
    .await;
    let outcome = h
        .handle
        .execute(StateCommand::BindBoard {
            board: h.board_id(),
            remote: RemoteBoardId(42),
            connector: taskboard_domain::test_support::test_connector(),
        })
        .await
        .expect("accepted");
    assert_eq!(outcome, CommandOutcome::Bound { wiped_refs: 0 });
    let app = h.handle.shared_state().load_full();
    assert!(
        app.boards
            .values()
            .any(|b| b.remote == taskboard_domain::test_support::test_binding(RemoteBoardId(42)))
    );
    drop(h);
}

/// Routing only (the semantics live in the domain corpus): `UnbindBoard`
/// executes end to end — the published state loses the binding and the
/// board's stored status row drops to the derived default — and a
/// repeat is an accepted `NoOp`.
#[tokio::test]
async fn unbind_arm_executes_publishes_and_repeats_as_noop() {
    let mut seeded = common::seed_bound_board_state("board");
    let board_id = seeded.boards.values().next().expect("one board").id;
    // A stale stored status row: the unbind must drop it (a stale
    // `Failed`/`Synced` row for an unbound board would be a lie).
    seeded.sync.set_stored(
        board_id,
        SyncPhase::Failed {
            last_error: SyncErrorKind::Network,
        },
        Some(chrono::Utc::now()),
    );
    let h = spawn(Spawner {
        repo: Arc::new(InMemoryRepository::with_state(seeded)),
        ..Spawner::default()
    })
    .await;

    let outcome = h
        .handle
        .execute(StateCommand::UnbindBoard { board: board_id })
        .await
        .expect("accepted");
    assert_eq!(outcome, CommandOutcome::Unbound { stranded_ops: 0 });

    let app = h.handle.shared_state().load_full();
    assert_eq!(app.boards[&board_id].remote, None, "binding cleared");
    let row = app.sync.get(board_id).expect("derived row");
    assert_eq!(row.phase, SyncPhase::Idle, "stored status dropped");
    assert_eq!(row.last_success, None, "stored last success dropped");

    let outcome = h
        .handle
        .execute(StateCommand::UnbindBoard { board: board_id })
        .await
        .expect("accepted");
    assert_eq!(outcome, CommandOutcome::NoOp, "idempotent repeat");
    drop(h);
}

/// Routing only: `ReconcileByTitle` over a settled single-board state
/// is an accepted `NoOp` (no candidates) — the engine rides the
/// domain's plan-apply-interpret path untouched.
#[tokio::test]
async fn reconcile_arm_over_settled_state_is_noop() {
    let h = harness().await;
    let outcome = h
        .handle
        .execute(StateCommand::ReconcileByTitle {
            board: h.board_id(),
        })
        .await
        .expect("accepted");
    assert_eq!(outcome, CommandOutcome::NoOp);
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
            board: h.board_id(),
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
    let board = h.board_id();
    // The actor is gone: its inbox receiver is gone with it.
    drop(h.sync_in);

    // The connector dies: the reports sender drops (the actor's exit).
    drop(h.report_tx);

    // Local editing keeps working: create a stack and a task.
    let stack = h
        .handle
        .execute(StateCommand::CreateStack {
            board,
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
        h.handle.shared_state().load_full().sync.aggregate().phase,
        SyncPhase::Idle,
        "a dead connector must not mark the engine Syncing"
    );
}

// ---------------------------------------------------------------------
// F4: loop lifecycle + ingestion-equality negatives (#240)
// ---------------------------------------------------------------------

/// F4-01: with the command inbox and the reports channel both closed, the
/// engine stays a daemon while a system-event producer still lives — the
/// `NetworkLost` event must still be ingested into the persisted Offline
/// phase. (The mirror contract h1 pins for connector death.)
#[tokio::test]
async fn engine_stays_a_daemon_when_commands_and_reports_close() {
    // The per-board offline write addresses bound boards.
    let h = spawn(Spawner {
        repo: Arc::new(InMemoryRepository::with_state(
            common::seed_bound_board_state("board"),
        )),
        ..Spawner::default()
    })
    .await;
    let Harness {
        handle,
        report_tx,
        system_tx,
        ..
    } = h;
    let state = handle.shared_state();
    drop(handle); // closes the command inbox
    drop(report_tx);

    system_tx
        .send(SystemEvent::NetworkLost)
        .expect("the engine is still a system-channel receiver");

    let deadline = Duration::from_secs(60);
    tokio::time::timeout(deadline, async {
        loop {
            if state.load_full().sync.aggregate().phase == SyncPhase::Offline {
                return;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("the live engine ingests the network event after its other sources closed");
}

/// #316 / #240: with every source closed — commands, reports, and (the
/// #316 fix) the system channel's external producers — the engine task
/// joins cleanly. The mid-test execute pins that the engine serves
/// while any source lives: the exit is all-sources-closed, not
/// any-source-closed.
#[tokio::test(start_paused = true)]
async fn engine_exits_when_all_sources_close() {
    let Harness {
        handle,
        report_tx,
        system_tx,
        join,
        ..
    } = harness().await;

    drop(report_tx);
    drop(system_tx); // the last external strong sender: the channel closes

    // Commands are still open: the engine must still be serving.
    let outcome = handle
        .execute(StateCommand::CreateStack {
            board: *handle
                .shared_state()
                .load()
                .boards
                .keys()
                .next()
                .expect("one board"),
            title: "alive".into(),
            order: 1,
        })
        .await
        .expect("the engine serves while its command inbox is open");
    assert!(matches!(outcome, CommandOutcome::CreatedStack(_)));

    drop(handle); // all sources closed

    tokio::time::timeout(Duration::from_secs(60), join)
        .await
        .expect("the engine exits once every source closed")
        .expect("clean exit");
}

/// #316 (upgrade-on-send semantics): once every external producer of the
/// system channel has died, a storage failure emits nothing — the channel
/// is closed, no receiver can appear — and the loop is unaffected: the
/// engine answers the failure with `Storage`, then recovers.
#[tokio::test]
async fn storage_failed_is_silent_after_the_last_external_producer_dies() {
    let repo = Arc::new(FlakyRepository::new(1));
    let h = spawn(Spawner {
        repo: repo.clone() as Arc<dyn TaskRepository>,
        ..Spawner::default()
    })
    .await;
    let mut events = h.system_tx.subscribe();
    let board = h.board_id();
    drop(h.system_tx); // the last external strong sender

    let failed = h
        .handle
        .execute(StateCommand::CreateStack {
            board,
            title: "doomed".into(),
            order: 1,
        })
        .await;
    assert!(matches!(
        failed,
        Err(ExecuteError::Storage(RepositoryError::Unavailable))
    ));
    h.handle.flush().await;
    // Typed: nothing was ever delivered, and the channel is observably
    // closed (try_recv surfaces Closed only on a closed, drained
    // buffer — an arriving StorageFailed would surface Ok(..)).
    assert!(matches!(
        events.try_recv(),
        Err(tokio::sync::broadcast::error::TryRecvError::Closed)
    ));

    // The loop survived the upgrade failure: recovery still serves.
    let recovered = h
        .handle
        .execute(StateCommand::CreateStack {
            board,
            title: "after".into(),
            order: 1,
        })
        .await;
    assert!(matches!(recovered, Ok(CommandOutcome::CreatedStack(_))));
}

/// A `Completed` report's validator triple with etag-bearing keys —
/// non-default, so the first report has something to store.
fn etag_validators() -> taskboard_domain::BoardPullValidators {
    let mk = |etag: &str| taskboard_domain::SyncValidators {
        etag: Some(etag.into()),
        last_modified: None,
    };
    taskboard_domain::BoardPullValidators {
        boards: mk("\"b1\""),
        stacks: mk("\"s1\""),
        archived_stacks: mk("\"a1\""),
    }
}

/// The fixture `Completed` report: board 77, no stacks/tasks/labels,
/// etag validators, fixed `read_at` — byte-identical on every call.
/// Chunk 4's engine-side fold wiring: the sidecar evidence and the
/// probed flags pass through ingestion and land on the report's board
/// row — the published `AppState` carries the typed warning (F3's
/// engine half) without the app layer re-deriving anything.
#[tokio::test]
async fn completed_report_derives_sidecar_health_and_features_on_the_row() {
    let h = spawn(Spawner {
        repo: Arc::new(InMemoryRepository::with_state(
            common::seed_bound_board_state("board"),
        )),
        ..Spawner::default()
    })
    .await;
    let evidence = taskboard_domain::AppDataEvidence::Unreadable(
        taskboard_domain::AppDataUnreadable::FormatTooNew { format: 99 },
    );
    let mut report = completed_report();
    let taskboard_domain::SyncReport::Completed {
        ref mut app_data, ..
    } = report
    else {
        unreachable!("fixture");
    };
    *app_data = evidence;
    h.report_tx.send(report).await.expect("report channel open");
    eventually(&h.handle, |app| {
        app.sync.iter().any(|(_, row)| {
            row.sidecar
                == taskboard_domain::SidecarHealth::Unreadable(
                    taskboard_domain::AppDataUnreadable::FormatTooNew { format: 99 },
                )
        })
    })
    .await;
}

fn completed_report() -> SyncReport {
    completed_report_for(RemoteBoardId(77), etag_validators())
}

/// [`completed_report`] parameterized by the reported board and its
/// validators — the ≥2-boards scenarios name a non-default board.
fn completed_report_for(
    board: RemoteBoardId,
    validators: taskboard_domain::BoardPullValidators,
) -> SyncReport {
    SyncReport::Completed {
        app_data: taskboard_domain::AppDataEvidence::Absent,
        duration: std::time::Duration::ZERO,
        connector: taskboard_domain::test_support::test_connector(),
        snapshot: Box::new(RemoteBoardSnapshot {
            board: RemoteBoard {
                id: board,
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
        validators,
        pushes: vec![],
        read_at: chrono::DateTime::<chrono::Utc>::MAX_UTC,
        features: taskboard_domain::DeckFeatures::CONSERVATIVE,
    }
}

/// Two live boards bound to *distinct* remote boards, both tagged with
/// the engine's default connector — the ≥2-boards-per-connector shape
/// the per-board report semantics exist for. Board-id order puts
/// `first` (remote 7) ahead of `second` (remote 9): a first-bound pick
/// and the report's own board disagree exactly here.
fn two_bound_boards() -> (PersistedState, taskboard_domain::BoardId) {
    let mut state = PersistedState::default();
    for (raw, remote) in [(1_u128, 7_u64), (2, 9)] {
        let mut board = common::board("bound");
        board.id = taskboard_domain::BoardId::from(uuid::Uuid::from_u128(raw));
        board.remote = Some(taskboard_domain::RemoteBoardBinding {
            connector: taskboard_domain::ConnectorId::from("nextcloud"),
            board: RemoteBoardId(remote),
        });
        state.boards.insert(board.id, board);
    }
    let second = taskboard_domain::BoardId::from(uuid::Uuid::from_u128(2));
    (state, second)
}

/// F4-02a: sending the *identical* `Completed` report a second time
/// appends nothing — the iff-differs batching means no second batch
/// reaches the repository at all (the empty-batch fast path).
#[tokio::test]
async fn identical_completed_report_appends_nothing() {
    let repo = Arc::new(RecordingRepository::seeded());
    let h = spawn(Spawner {
        repo: repo.clone() as Arc<dyn TaskRepository>,
        ..Spawner::default()
    })
    .await;

    h.report_tx
        .send(completed_report())
        .await
        .expect("report channel open");
    eventually(&h.handle, |app| {
        app.boards
            .values()
            .any(|b| b.remote == taskboard_domain::test_support::test_binding(RemoteBoardId(77)))
            && app.sync.aggregate().last_success == Some(ts(T0))
            && app.sync.aggregate().phase == SyncPhase::Idle
    })
    .await;
    h.handle.flush().await;

    // The identical report, then a marker report whose ingestion is the
    // settle signal (the identical report publishes nothing to wait for;
    // mpsc FIFO orders it first).
    h.report_tx
        .send(completed_report())
        .await
        .expect("report channel open");
    h.report_tx
        .send(SyncReport::Failed {
            duration: std::time::Duration::ZERO,
            board: None,
            connector: taskboard_domain::test_support::test_connector(),
            kind: SyncErrorKind::Auth,
            pushes: vec![],
            read_at: chrono::DateTime::<chrono::Utc>::MAX_UTC,
            features: taskboard_domain::DeckFeatures::CONSERVATIVE,
        })
        .await
        .expect("report channel open");
    eventually(&h.handle, |app| {
        app.sync.aggregate().phase
            == SyncPhase::Failed {
                last_error: SyncErrorKind::Auth,
            }
    })
    .await;

    let batches = repo.batches();
    assert_eq!(
        batches.len(),
        2,
        "the identical report must contribute zero batches"
    );
    let first = &batches[0];
    assert_eq!(
        first
            .iter()
            .filter(|a| matches!(a, PersistenceAction::UpsertValidators(..)))
            .count(),
        3,
        "the first report stores its three validators"
    );
    assert_eq!(
        first
            .iter()
            .filter(|a| matches!(a, PersistenceAction::UpsertBoard(..)))
            .count(),
        1,
        "the first report adopts the board"
    );
    for batch in batches.iter().skip(1) {
        assert!(
            !batch
                .iter()
                .any(|a| matches!(a, PersistenceAction::UpsertValidators(..))),
            "no batch after the first may re-store validators"
        );
    }
    assert_eq!(
        batches[1].len(),
        1,
        "the marker batch is the status write alone"
    );
    assert!(matches!(
        batches[1][0],
        PersistenceAction::UpsertSyncStatus(..)
    ));
}

/// F4-02b: a subsequent `Failed` report carries the last-success stamp
/// forward instead of clearing it — the status write reads the engine's
/// carried status, not a default.
#[tokio::test]
async fn failed_report_keeps_the_last_success_stamp() {
    let h = harness().await;
    h.report_tx
        .send(completed_report())
        .await
        .expect("report channel open");
    eventually(&h.handle, |app| {
        app.sync.aggregate().last_success == Some(ts(T0))
            && app.sync.aggregate().phase == SyncPhase::Idle
    })
    .await;
    let stamp = h
        .handle
        .shared_state()
        .load_full()
        .sync
        .aggregate()
        .last_success;

    h.report_tx
        .send(SyncReport::Failed {
            duration: std::time::Duration::ZERO,
            board: None,
            connector: taskboard_domain::test_support::test_connector(),
            kind: SyncErrorKind::Network,
            pushes: vec![],
            read_at: chrono::DateTime::<chrono::Utc>::MAX_UTC,
            features: taskboard_domain::DeckFeatures::CONSERVATIVE,
        })
        .await
        .expect("report channel open");
    eventually(&h.handle, |app| {
        app.sync.aggregate().phase
            == SyncPhase::Failed {
                last_error: SyncErrorKind::Network,
            }
    })
    .await;

    assert_eq!(
        h.handle
            .shared_state()
            .load_full()
            .sync
            .aggregate()
            .last_success,
        stamp,
        "the failure verdict must not erase the last success"
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
        h.handle.shared_state().load_full().sync.aggregate().phase,
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

// ---------------------------------------------------------------------
// K-series: sync counters on the published rows + engine metrics (#156/#145)
// ---------------------------------------------------------------------

/// A `Completed` report with a non-empty snapshot (two stacks, three
/// tasks, one label) and one landed push outcome.
fn counter_report(board: RemoteBoardId, duration: std::time::Duration) -> SyncReport {
    SyncReport::Completed {
        app_data: taskboard_domain::AppDataEvidence::Absent,
        duration,
        connector: taskboard_domain::test_support::test_connector(),
        snapshot: Box::new(RemoteBoardSnapshot {
            board: RemoteBoard {
                id: board,
                title: "remote board".into(),
                color: "00ff00".into(),
                archived: false,
                deleted_at: None,
                last_modified: ts(T0),
            },
            stacks: [0_i64, 1]
                .into_iter()
                .map(|i| taskboard_domain::RemoteStack {
                    id: taskboard_domain::RemoteStackRef {
                        board,
                        stack: taskboard_domain::RemoteStackId(i.unsigned_abs()),
                    },
                    title: format!("col{i}"),
                    order: i,
                    archived: false,
                    deleted_at: None,
                    last_modified: ts(T0),
                })
                .collect(),
            tasks: vec![],
            labels: vec![taskboard_domain::RemoteLabel {
                id: taskboard_domain::RemoteLabelRef {
                    board,
                    label: taskboard_domain::RemoteLabelId(5),
                },
                title: "bug".into(),
                color: "ff0000".into(),
                deleted_at: None,
                last_modified: ts(T0),
            }],
        }),
        validators: test_validators(),
        pushes: vec![taskboard_domain::PushOutcome {
            op: taskboard_domain::OpId(uuid::Uuid::from_u128(500)),
            result: taskboard_domain::PushResult::Applied { echo: None },
        }],
        read_at: chrono::DateTime::<chrono::Utc>::MAX_UTC,
        features: taskboard_domain::DeckFeatures::CONSERVATIVE,
    }
}

fn bound_harness() -> Spawner {
    Spawner {
        repo: Arc::new(InMemoryRepository::with_state(
            common::seed_bound_board_state("board"),
        )),
        ..Spawner::default()
    }
}

/// A completed report counts its cycle, its landed push, every pulled
/// entity (the board itself included: 1 + 2 stacks + 3 tasks + 1 label —
/// here tasks are empty, so 1 + 2 + 0 + 1), and the cycle's wall duration
/// onto its board's row.
#[tokio::test]
async fn k1_a_completed_report_counts_cycle_pushes_pulls_and_duration_on_its_board() {
    let h = spawn(bound_harness()).await;
    h.report_tx
        .send(counter_report(
            RemoteBoardId(42),
            std::time::Duration::from_millis(250),
        ))
        .await
        .expect("report channel open");

    eventually(&h.handle, |app| app.sync.aggregate().counters.cycles == 1).await;
    let counters = h
        .handle
        .shared_state()
        .load_full()
        .sync
        .aggregate()
        .counters;
    assert_eq!(counters.cycles, 1);
    assert_eq!(counters.pushes, 1);
    assert_eq!(counters.pulls, 4);
    assert_eq!(counters.failures, 0);
    assert_eq!(
        counters.last_cycle,
        Some(std::time::Duration::from_millis(250))
    );
}

/// The trap pin: a no-change cycle (the identical report replays to an
/// empty action batch) still counts its cycle AND still publishes — the
/// counters ride the existing ingestion wake; no new wake source.
#[tokio::test]
async fn k2_a_no_change_cycle_still_counts_and_publishes() {
    let h = spawn(bound_harness()).await;
    let mut signals = h.handle.subscribe();
    let report = counter_report(RemoteBoardId(42), std::time::Duration::from_millis(5));
    h.report_tx.send(report.clone()).await.expect("open");
    eventually(&h.handle, |app| app.sync.aggregate().counters.cycles == 1).await;
    let _ = drain_signals(&mut signals);

    h.report_tx.send(report).await.expect("open");
    eventually(&h.handle, |app| app.sync.aggregate().counters.cycles == 2).await;
    let signal = tokio::time::timeout(std::time::Duration::from_secs(60), signals.recv()).await;
    assert!(
        matches!(signal, Ok(Ok(EngineSignal::StateUpdated))),
        "the counted no-change cycle must publish: got {signal:?}"
    );
}

/// A connector-wide failure (no board identity) counts the cycle, the
/// failure, and the duration on *every* board bound to the reporting
/// connector — pushes untouched.
#[tokio::test]
async fn k3_a_connector_wide_failure_counts_every_connector_board() {
    let (state, _) = two_bound_boards();
    let h = spawn(Spawner {
        repo: Arc::new(InMemoryRepository::with_state(state)),
        ..Spawner::default()
    })
    .await;
    h.report_tx
        .send(SyncReport::Failed {
            connector: taskboard_domain::test_support::test_connector(),
            kind: SyncErrorKind::Network,
            pushes: vec![],
            read_at: ts(T0),
            board: None,
            duration: std::time::Duration::from_millis(100),
            features: taskboard_domain::DeckFeatures::CONSERVATIVE,
        })
        .await
        .expect("report channel open");

    eventually(&h.handle, |app| {
        app.boards.len() == 2
            && app
                .boards
                .keys()
                .all(|b| app.sync.get(*b).is_some_and(|r| r.counters.cycles == 1))
    })
    .await;
    for board in h.handle.shared_state().load_full().boards.keys() {
        let counters = h
            .handle
            .shared_state()
            .load_full()
            .sync
            .get(*board)
            .unwrap()
            .counters
            .clone();
        assert_eq!(counters.cycles, 1);
        assert_eq!(counters.failures, 1);
        assert_eq!(counters.pushes, 0);
        assert_eq!(
            counters.last_cycle,
            Some(std::time::Duration::from_millis(100))
        );
    }
}

/// A board-scoped failure counts only the resolved failing board.
#[tokio::test]
async fn k4_a_board_scoped_failure_counts_only_the_failing_board() {
    let (state, second) = two_bound_boards();
    let h = spawn(Spawner {
        repo: Arc::new(InMemoryRepository::with_state(state)),
        ..Spawner::default()
    })
    .await;
    // The evidence arm (board-scoped) requires push evidence: an empty
    // `pushes` is the connector-wide verdict by the ingestion's guard.
    h.report_tx
        .send(SyncReport::Failed {
            connector: taskboard_domain::test_support::test_connector(),
            kind: SyncErrorKind::Auth,
            pushes: vec![taskboard_domain::PushOutcome {
                op: taskboard_domain::OpId(uuid::Uuid::from_u128(501)),
                result: taskboard_domain::PushResult::Applied { echo: None },
            }],
            read_at: ts(T0),
            board: Some(RemoteBoardId(9)),
            duration: std::time::Duration::from_millis(40),
            features: taskboard_domain::DeckFeatures::CONSERVATIVE,
        })
        .await
        .expect("report channel open");

    eventually(&h.handle, |app| {
        app.sync.get(second).is_some_and(|r| r.counters.cycles == 1)
    })
    .await;
    let app = h.handle.shared_state().load_full();
    let failing = app.sync.get(second).unwrap().counters.clone();
    assert_eq!(failing.cycles, 1);
    assert_eq!(failing.failures, 1);
    assert_eq!(
        failing.last_cycle,
        Some(std::time::Duration::from_millis(40))
    );
    for (board, row) in app.sync.iter() {
        if *board != second {
            assert_eq!(
                row.counters.cycles, 0,
                "the other board's counters are untouched"
            );
        }
    }
}

/// The offline episode edge: two `NetworkLost` events inside one episode
/// count once; a restore (a cycle's evidence lands, the phase leaves
/// `Offline`) re-arms the edge for the next loss.
#[tokio::test]
async fn k5_network_lost_counts_one_episode_per_board_until_restored() {
    let h = spawn(bound_harness()).await;
    h.system_tx.send(SystemEvent::NetworkLost).expect("send");
    h.system_tx.send(SystemEvent::NetworkLost).expect("send");
    eventually(&h.handle, |app| {
        app.sync.aggregate().counters.offline_episodes == 1
    })
    .await;

    // Restore: the next report is the evidence; it moves the phase out of
    // `Offline`, re-arming the episode edge.
    h.system_tx
        .send(SystemEvent::NetworkRestored)
        .expect("send");
    h.report_tx
        .send(counter_report(RemoteBoardId(42), std::time::Duration::ZERO))
        .await
        .expect("report channel open");
    eventually(&h.handle, |app| {
        app.sync.aggregate().phase == SyncPhase::Idle
    })
    .await;
    h.system_tx.send(SystemEvent::NetworkLost).expect("send");
    eventually(&h.handle, |app| {
        app.sync.aggregate().counters.offline_episodes == 2
    })
    .await;
}

/// Counters are cumulative per process run: a fresh engine over the same
/// repository starts from zero by construction (the counters never enter
/// the persisted shape).
#[tokio::test]
async fn k6_counters_restart_from_zero_on_a_fresh_engine_over_the_same_repo() {
    let repo: Arc<dyn TaskRepository> = Arc::new(InMemoryRepository::with_state(
        common::seed_bound_board_state("board"),
    ));
    let first = spawn(Spawner {
        repo: repo.clone(),
        ..Spawner::default()
    })
    .await;
    first
        .report_tx
        .send(counter_report(
            RemoteBoardId(42),
            std::time::Duration::from_millis(1),
        ))
        .await
        .expect("report channel open");
    eventually(&first.handle, |app| {
        app.sync.aggregate().counters.cycles == 1
    })
    .await;
    first.join.abort();
    drop(first);

    let second = spawn(Spawner {
        repo,
        ..Spawner::default()
    })
    .await;
    eventually(&second.handle, |app| app.boards.len() == 1).await;
    let counters = second
        .handle
        .shared_state()
        .load_full()
        .sync
        .aggregate()
        .counters;
    assert_eq!(counters.cycles, 0, "a new engine restarts from zero");
    assert_eq!(counters.last_cycle, None);
}

/// Nudges and the `Syncing` transient count no cycles — only reports do.
#[tokio::test]
async fn k7_nudges_and_mark_syncing_count_no_cycles() {
    let h = spawn(bound_harness()).await;
    let outcome = h
        .handle
        .execute(StateCommand::RequestSync)
        .await
        .expect("accepted");
    assert_eq!(outcome, CommandOutcome::SyncRequested);
    let counters = h
        .handle
        .shared_state()
        .load_full()
        .sync
        .aggregate()
        .counters;
    assert_eq!(counters.cycles, 0);
    assert_eq!(counters.last_cycle, None);
}

/// The handle's engine metrics: an accepted mutating command counts one
/// command and one publish signal; a semantically rejected one counts the
/// rejection. Exact snapshots — the counters are the status surface's
/// facts, never in `AppState`.
#[tokio::test]
async fn k8_the_handle_counts_commands_rejections_and_signals() {
    use taskboard_state::EngineMetricsSnapshot as Snap;

    let h = harness().await;
    h.handle
        .execute(StateCommand::CreateStack {
            board: h.board_id(),
            title: "backlog".into(),
            order: 1,
        })
        .await
        .expect("accepted");
    assert_eq!(
        h.handle.metrics(),
        Snap {
            commands: 1,
            rejections: 0,
            signals: 1,
        }
    );
    let rejected = h
        .handle
        .execute(StateCommand::CreateTask {
            title: "orphan".into(),
            stack: taskboard_domain::StackId::from(uuid::Uuid::from_u128(424_242)),
            order: 1,
        })
        .await;
    assert!(
        matches!(rejected, Err(ExecuteError::Rejected(_))),
        "an unknown stack must be a typed rejection"
    );
    assert_eq!(
        h.handle.metrics(),
        Snap {
            commands: 2,
            rejections: 1,
            signals: 1,
        }
    );
}

// ---- The withdraw direction, folded-synced arm (R3, design D-10) --------

/// A synced dismissal folded from a `Completed` report's document lands
/// the best-effort `Withdraw` on the notifier channel — the same
/// derivation the local `DismissReminder` path uses. A relayed section
/// re-folds nothing (the fold emits newly-inserted rows only), so the
/// withdrawal is exactly one.
#[tokio::test(start_paused = true)]
#[allow(clippy::too_many_lines)] // the fixture is the length; the assertions are two
async fn folded_synced_dismissal_withdraws_the_toast() {
    use taskboard_domain::{
        AppDataDocument, AppDataEvidence, AppDataSections, DeckFeatures, DismissalEntry,
        DismissalsEntry, ReminderRuleId, RemoteCardId, RemoteCardRef, RemoteStackId,
        RemoteStackRef, dismissal_wire_key, test_support::test_connector,
    };

    const CARD: u64 = 7;
    const STACK: u64 = 5;

    // Local truth: a bound board, one bound stack, one bound task.
    let board_id = taskboard_domain::BoardId::from_uuid(uuid::Uuid::from_u128(1));
    let stack_id = taskboard_domain::StackId::from_uuid(uuid::Uuid::from_u128(2));
    let task_id = taskboard_domain::TaskId::from_uuid(uuid::Uuid::from_u128(3));
    let mut state = seed_bound_board_state("board");
    let board = state.boards.values_mut().next().expect("one board");
    board.id = board_id;
    state.stacks.insert(
        stack_id,
        taskboard_domain::Stack {
            id: stack_id,
            remote: Some(taskboard_domain::RemoteStackRef {
                board: RemoteBoardId(42),
                stack: RemoteStackId(STACK),
            }),
            board: board_id,
            title: "backlog".into(),
            order: 1,
            archived: false,
            deleted: false,
            clocks: taskboard_domain::StackClocks {
                title: ts(T0),
                order: ts(T0),
                deleted: taskboard_domain::BoardClocks::epoch(),
            },
            remote_seen: None,
        },
    );
    state.tasks.insert(
        task_id,
        taskboard_domain::Task {
            id: task_id,
            remote: Some(RemoteCardRef {
                board: RemoteBoardId(42),
                stack: RemoteStackId(STACK),
                card: RemoteCardId(CARD),
            }),
            title: "task".into(),
            description: String::new(),
            duedate: Some(ts(T0)),
            scheduled: None,
            done: None,
            stack: stack_id,
            order: 1,
            labels: std::collections::BTreeSet::new(),
            effort: None,
            importance: None,
            urgency: None,
            recur: None,

            archived: false,
            deleted: false,
            clocks: taskboard_domain::TaskClocks {
                title: ts(T0),
                description: ts(T0),
                duedate: ts(T0),
                scheduled: ts(T0),
                done: ts(T0),
                position: ts(T0),
                labels: ts(T0),
                effort: ts(T0),
                importance: ts(T0),
                urgency: ts(T0),
                recur: ts(T0),

                archived: ts(T0),
                deleted: ts(T0),
            },
            remote_seen: None,
        },
    );

    let spawner = Spawner {
        repo: Arc::new(taskboard_domain::test_support::InMemoryRepository::with_state(state)),
        ..Spawner::default()
    };
    let mut h = spawn(spawner).await;

    // The synced fact: rule 0xa's occurrence at the task's due instant
    // was dismissed on another device.
    let dismissed_rule = ReminderRuleId::from(uuid::Uuid::from_u128(0xa));
    let occurrence = ts(T0);
    let evidence = AppDataEvidence::Document(Box::new(AppDataDocument {
        format: taskboard_domain::appdata::APPDATA_FORMAT,
        board: RemoteBoardId(42),
        sections: AppDataSections {
            tasks: None,
            reminder_rules: None,
            dismissals: Some(
                [(
                    CARD.to_string(),
                    DismissalsEntry {
                        dismissals: [(
                            dismissal_wire_key(dismissed_rule, occurrence),
                            DismissalEntry::Dismissed {
                                dismissed_at: ts(T0 - 30),
                            },
                        )]
                        .into(),
                        unknown: std::collections::BTreeMap::new(),
                    },
                )]
                .into(),
            ),
            unknown: std::collections::BTreeMap::new(),
        },
        unknown: std::collections::BTreeMap::new(),
    }));

    h.report_tx
        .send(SyncReport::Completed {
            app_data: evidence,
            duration: Duration::ZERO,
            connector: test_connector(),
            snapshot: Box::new(RemoteBoardSnapshot {
                board: RemoteBoard {
                    id: RemoteBoardId(42),
                    title: "remote board".into(),
                    color: "00ff00".into(),
                    archived: false,
                    deleted_at: None,
                    last_modified: ts(T0),
                },
                stacks: vec![taskboard_domain::RemoteStack {
                    id: RemoteStackRef {
                        board: RemoteBoardId(42),
                        stack: RemoteStackId(STACK),
                    },
                    title: "backlog".into(),
                    order: 1,
                    archived: false,
                    deleted_at: None,
                    last_modified: ts(T0),
                }],
                tasks: vec![taskboard_domain::RemoteTask {
                    id: RemoteCardRef {
                        board: RemoteBoardId(42),
                        stack: RemoteStackId(STACK),
                        card: RemoteCardId(CARD),
                    },
                    title: "task".into(),
                    description: String::new(),
                    duedate: Some(ts(T0)),
                    scheduled: None,
                    done: None,
                    stack: RemoteStackId(STACK),
                    order: 1,
                    labels: std::collections::BTreeSet::new(),
                    archived: false,
                    last_modified: ts(T0),
                }],
                labels: vec![],
            }),
            validators: test_validators(),
            pushes: vec![],
            read_at: ts(T0),
            features: DeckFeatures::CONSERVATIVE,
        })
        .await
        .expect("report channel open");

    eventually(&h.handle, |app| {
        app.reminders
            .dismissals
            .contains(&taskboard_domain::ReminderDismissal {
                reminder: taskboard_domain::ReminderRef::Rule(dismissed_rule),
                occurrence,
                dismissed_at: ts(T0 - 30),
            })
    })
    .await;

    // The fold landed exactly one new row, so exactly one withdrawal.
    // Bounded wait: the withdrawal follows the publish the predicate
    // above observed.
    let message = tokio::time::timeout(Duration::from_secs(60), async {
        loop {
            match h.notifications.try_recv() {
                Ok(message) => return message,
                Err(_) => tokio::time::advance(Duration::from_millis(1)).await,
            }
        }
    })
    .await
    .expect("the withdraw lands within the deadline");
    assert_eq!(
        message,
        NotifierMessage::Withdraw {
            reminder: taskboard_domain::ReminderRef::Rule(dismissed_rule),
            occurrence,
        }
    );
    assert!(
        h.notifications.try_recv().is_err(),
        "a re-folded row is not re-withdrawn"
    );
}

/// The generation signal is wall-step-immune: two `Completed` reports
/// advance `counters.cycles` monotonically even when the injected clock
/// steps *backward* between them — `last_success` honestly moves back
/// with the clock (a display fact), while the cycle count is what
/// freshness consumers may rely on.
#[tokio::test]
async fn counters_advance_across_a_backward_clock_step() {
    let clock = FixedClock::new(T0);
    let h = spawn(Spawner {
        clock: clock.clone(),
        ..Spawner::default()
    })
    .await;

    h.report_tx
        .send(completed_report())
        .await
        .expect("report channel open");
    eventually(&h.handle, |app| {
        app.sync.aggregate().counters.cycles == 1
            && app.sync.aggregate().last_success == Some(ts(T0))
            && app.sync.aggregate().phase == SyncPhase::Idle
    })
    .await;

    // The backward step: a wall-stamp freshness comparison would read
    // the next report's stamp as older and misreport the cycle.
    clock.advance(-3_600);

    h.report_tx
        .send(completed_report())
        .await
        .expect("report channel open");
    eventually(&h.handle, |app| {
        app.sync.aggregate().counters.cycles == 2
            && app.sync.aggregate().last_success == Some(ts(T0 - 3_600))
            && app.sync.aggregate().phase == SyncPhase::Idle
    })
    .await;
}
