// SPDX-License-Identifier: MIT OR Apache-2.0
// Environment gate, not a weakened assertion: this suite drives the live
// tokio engine (timers, channel scheduling), which testing_strategy §10
// scopes out of Miri ("pure logic, serialization, and state-transition
// tests"). The transition logic it routes to is Miri-covered in the
// domain and in `core.rs`.
#![cfg_attr(miri, allow(dead_code))]
// insta itself is not Miri-clean (it opens a socketpair for its UI
// plumbing) — the S-series is environment-gated here and fully exercised
// in every native/CI gate. The engine logic beneath it IS Miri-covered
// (A-series + the domain pure suites).
//! Insta snapshot suite (S-series): the scenarios that define the
//! engine's behavioral contract. Each scenario captures the persisted
//! `PersistedState` as the whole-state disk contract, and checks the
//! published `AppState` against it structurally (memory ≡ disk) plus a
//! small memory-only tail snapshot (`{sync, last_updated}`). Insta
//! diffs read as contract changes (roadmap Phase 3 exit criterion).

use std::sync::Arc;

use taskboard_domain::test_support::{CountingIds, InMemoryRepository};
use taskboard_domain::{
    Color, CommandOutcome, LocalOp, NotifierMessage, PushOutcome, PushResult, RemoteBoard,
    RemoteBoardSnapshot, StateCommand, SyncCommand, SyncErrorKind, SyncPhase, SyncReport,
    SystemEvent,
};
use taskboard_state::{ReminderEngineConfig, spawn_state_engine};

use insta::assert_yaml_snapshot;

mod common;

use common::{FixedClock, T0, seed_board_state, stranger_task_id, test_validators, ts};

/// A board-bearing fake repo (no command creates boards) plus the
/// running engine over it. The clock advances one minute per message.
struct Scenario {
    handle: taskboard_state::EngineHandle,
    repo: Arc<InMemoryRepository>,
    report_tx: tokio::sync::mpsc::Sender<SyncReport>,
    system_tx: tokio::sync::broadcast::Sender<SystemEvent>,
    clock: Arc<FixedClock>,
}

impl Scenario {
    async fn start() -> Self {
        Self::start_with(seed_board_state("kiosk")).await
    }

    /// Stand-alone scenario: an empty db — `CreateBoard` is legal here.
    async fn start_empty() -> Self {
        Self::start_with(taskboard_domain::PersistedState::default()).await
    }

    async fn start_with(state: taskboard_domain::PersistedState) -> Self {
        let repo = Arc::new(InMemoryRepository::with_state(state));
        let clock = FixedClock::new(T0);
        let (sync_out, _sync_in) = tokio::sync::mpsc::channel::<SyncCommand>(8);
        let (report_tx, report_rx) = tokio::sync::mpsc::channel::<SyncReport>(8);
        let (system_tx, system_rx) = tokio::sync::broadcast::channel::<SystemEvent>(8);
        let (notifier, _notifications) = tokio::sync::mpsc::channel::<NotifierMessage>(64);
        let (handle, _join) = spawn_state_engine(
            repo.clone(),
            Arc::new(CountingIds::new()),
            clock.clone(),
            sync_out,
            report_rx,
            system_tx.clone(),
            system_rx,
            notifier,
            ReminderEngineConfig::disabled(),
        )
        .await
        .expect("engine boots");
        Self {
            handle,
            repo,
            report_tx,
            system_tx,
            clock,
        }
    }

    /// The scenario's single board id (the seed or the minted one).
    fn board_id(&self) -> taskboard_domain::BoardId {
        *self
            .handle
            .shared_state()
            .load()
            .boards
            .keys()
            .next()
            .expect("the scenario has exactly one board")
    }

    async fn run(&mut self, command: StateCommand) -> CommandOutcome {
        self.clock.advance(60);
        self.handle.execute(command).await.expect("accepted")
    }

    /// Like [`run`](Self::run), but keeps rejection results for tests
    /// that exercise them.
    async fn run_raw(
        &mut self,
        command: StateCommand,
    ) -> Result<CommandOutcome, taskboard_state::ExecuteError> {
        self.clock.advance(60);
        self.handle.execute(command).await
    }

    /// The persisted whole-state contract dump stays the disk-shape
    /// defense (unexpected fields, renames, serde drift all surface
    /// here); the published half narrows to a *checked* memory ≡ disk
    /// equality plus the memory-only tail. New `AppState` fields must
    /// join the equality or the [`PublishedTail`] explicitly — nothing
    /// else covers a published-only field anymore.
    fn snap(&self, scenario: &str) {
        let app = self.handle.shared_state().load_full();
        let persisted = self.repo.snapshot();
        assert_eq!(app.boards, persisted.boards, "memory ≡ disk: boards");
        assert_eq!(app.stacks, persisted.stacks, "memory ≡ disk: stacks");
        assert_eq!(app.tasks, persisted.tasks, "memory ≡ disk: tasks");
        assert_eq!(app.labels, persisted.labels, "memory ≡ disk: labels");
        assert_eq!(
            app.reminders, persisted.reminders,
            "memory ≡ disk: reminders"
        );
        assert_eq!(app.tracking, persisted.tracking, "memory ≡ disk: tracking");
        assert_sync_stored(&app.sync, &persisted.sync);
        assert_yaml_snapshot!(format!("{scenario}_persisted_state"), &persisted);
        let tail = PublishedTail {
            sync: &app.sync,
            last_updated: app.last_updated,
        };
        assert_yaml_snapshot!(format!("{scenario}_published_tail"), &tail);
    }
}

/// The published half's snapshot body: the memory-only overlay
/// (`SyncStates`, with its per-row `counters`/`sidecar`/`features`) and
/// the live engine stamp. Field order matches the old published dump's
/// suffix so migrated files read as a verbatim tail.
#[derive(serde::Serialize)]
struct PublishedTail<'a> {
    sync: &'a taskboard_domain::SyncStates,
    last_updated: Option<chrono::DateTime<chrono::Utc>>,
}

/// The stored projection of `app.sync` ≡ `persisted.sync`: equal key
/// sets and, per board, equal `phase`, `last_success`, `pending_ops`.
/// `SyncStatus.counters`/`sidecar`/`features` are deliberately excluded:
/// they are documented memory-only overlays (`taskboard-domain/src/
/// state.rs` — "Never persisted", `set_stored` never touches them), so
/// the stored side always carries their serde defaults. Excluding them
/// here is the documented projection, not a weakening — a divergence in
/// any stored field still fires.
fn assert_sync_stored(
    app: &taskboard_domain::SyncStates,
    persisted: &taskboard_domain::SyncStates,
) {
    assert_eq!(
        app.iter().count(),
        persisted.iter().count(),
        "memory ≡ disk: sync key sets differ in size"
    );
    for (board, published_row) in app.iter() {
        let stored_row = persisted
            .get(*board)
            .unwrap_or_else(|| panic!("memory ≡ disk: sync row missing on disk for {board:?}"));
        assert_eq!(
            published_row.phase, stored_row.phase,
            "memory ≡ disk: sync phase differs for {board:?}"
        );
        assert_eq!(
            published_row.last_success, stored_row.last_success,
            "memory ≡ disk: sync last_success differs for {board:?}"
        );
        assert_eq!(
            published_row.pending_ops, stored_row.pending_ops,
            "memory ≡ disk: sync pending_ops differs for {board:?}"
        );
    }
}

/// Load-bearing pin: a divergence in any stored sync field must fire
/// [`assert_sync_stored`] (one case per stored field).
#[test]
#[should_panic(expected = "memory ≡ disk: sync")]
fn assert_sync_stored_pins_a_stored_phase_divergence() {
    let board = common::board("kiosk").id;
    let mut disk = taskboard_domain::SyncStates::new();
    disk.set_stored(board, taskboard_domain::SyncPhase::Idle, None);
    let mut memory = disk.clone();
    memory.set_stored(board, taskboard_domain::SyncPhase::Offline, None);
    assert_sync_stored(&memory, &disk);
}

#[test]
#[should_panic(expected = "memory ≡ disk: sync")]
fn assert_sync_stored_pins_a_stored_last_success_divergence() {
    let board = common::board("kiosk").id;
    let mut disk = taskboard_domain::SyncStates::new();
    disk.set_stored(board, taskboard_domain::SyncPhase::Idle, None);
    let mut memory = disk.clone();
    memory.set_stored(board, taskboard_domain::SyncPhase::Idle, Some(ts(T0)));
    assert_sync_stored(&memory, &disk);
}

#[test]
#[should_panic(expected = "memory ≡ disk: sync")]
fn assert_sync_stored_pins_a_stored_pending_ops_divergence() {
    let board = common::board("kiosk").id;
    let mut disk = taskboard_domain::SyncStates::new();
    disk.set_stored(board, taskboard_domain::SyncPhase::Idle, None);
    let mut memory = disk.clone();
    // `pending_ops` is not settable via `set_stored`; the memory-side
    // overlay path (`get_mut`) is how the engine's derived depth lands.
    if let Some(row) = memory.get_mut(board) {
        row.pending_ops = 3;
    }
    assert_sync_stored(&memory, &disk);
}

/// A divergence confined to the memory-only overlays (`counters`,
/// `sidecar`, `features`) must NOT fire — that is the documented
/// projection, not a weakening.
#[test]
fn assert_sync_stored_ignores_memory_only_overlays() {
    use taskboard_domain::{DeckFeatures, SidecarHealth, SyncCounters};

    let board = common::board("kiosk").id;
    let mut disk = taskboard_domain::SyncStates::new();
    disk.set_stored(board, taskboard_domain::SyncPhase::Idle, None);
    let mut memory = disk.clone();
    if let Some(row) = memory.get_mut(board) {
        row.counters = SyncCounters {
            cycles: 7,
            ..SyncCounters::default()
        };
        row.sidecar = SidecarHealth::Healthy;
        row.features = DeckFeatures::CONSERVATIVE;
    }
    assert_sync_stored(&memory, &disk);
}

/// S1 — fresh-boot authoring: stack → task → edit → done → label →
/// assign → move → delete.
#[cfg_attr(miri, ignore)] // insta needs socketpair (environment gate)
#[tokio::test]
async fn s1_fresh_boot_authoring() {
    let mut s = Scenario::start().await;
    let CommandOutcome::CreatedStack(stack) = s
        .run(StateCommand::CreateStack {
            board: s.board_id(),
            title: "todo".into(),
            order: 1,
        })
        .await
    else {
        panic!("wrong receipt");
    };
    let CommandOutcome::CreatedTask(task) = s
        .run(StateCommand::CreateTask {
            title: "write report".into(),
            stack,
            order: 1,
        })
        .await
    else {
        panic!("wrong receipt");
    };
    s.run(StateCommand::UpdateTask {
        id: task,
        changes: taskboard_domain::TaskChanges {
            title: None,
            description: Some("draft the Q4 report".into()),
            duedate: Some(Some(ts(T0 + 86_400))),
            scheduled: None,
            effort: None,
            importance: None,
            urgency: None,
            archived: None,
        },
    })
    .await;
    s.run(StateCommand::SetTaskDone {
        id: task,
        done: true,
    })
    .await;
    let CommandOutcome::CreatedLabel(label) = s
        .run(StateCommand::CreateLabel {
            board: s.board_id(),
            title: "urgent".into(),
            color: Color::new("ff0000"),
        })
        .await
    else {
        panic!("wrong receipt");
    };
    s.run(StateCommand::AssignLabel { task, label }).await;
    let CommandOutcome::CreatedStack(later) = s
        .run(StateCommand::CreateStack {
            board: s.board_id(),
            title: "doing".into(),
            order: 2,
        })
        .await
    else {
        panic!("wrong receipt");
    };
    s.run(StateCommand::MoveTask {
        id: task,
        stack: later,
        order: 9,
    })
    .await;
    s.run(StateCommand::DeleteTask { id: task }).await;
    s.snap("s1");
    drop(s);
}

/// S2 — offline queue accumulation: many edits, no sync; the outbox
/// (and its derived `pending_ops`) grows with every accepted intent.
#[cfg_attr(miri, ignore)] // insta needs socketpair (environment gate)
#[tokio::test]
async fn s2_offline_queue_accumulation() {
    let mut s = Scenario::start().await;
    let CommandOutcome::CreatedStack(stack) = s
        .run(StateCommand::CreateStack {
            board: s.board_id(),
            title: "inbox".into(),
            order: 1,
        })
        .await
    else {
        panic!("wrong receipt");
    };
    let mut tasks = Vec::new();
    for n in 0..3 {
        let CommandOutcome::CreatedTask(task) = s
            .run(StateCommand::CreateTask {
                title: format!("task {n}"),
                stack,
                order: n,
            })
            .await
        else {
            panic!("wrong receipt");
        };
        tasks.push(task);
    }
    s.run(StateCommand::UpdateTask {
        id: tasks[0],
        changes: taskboard_domain::TaskChanges {
            title: Some("task 0 (renamed offline)".into()),
            ..Default::default()
        },
    })
    .await;
    s.run(StateCommand::SetTaskDone {
        id: tasks[1],
        done: true,
    })
    .await;
    s.snap("s2");
}

/// S3 — sync ingestion: a `Completed` report binds the board and pulls
/// remote entities in.
#[cfg_attr(miri, ignore)] // insta needs socketpair (environment gate)
#[tokio::test]
async fn s3_sync_ingestion_binds_and_pulls() {
    let s = Scenario::start().await;
    s.clock.advance(60);
    s.report_tx
        .send(SyncReport::Completed {
            app_data: taskboard_domain::AppDataEvidence::Absent,
            duration: std::time::Duration::from_millis(250),
            connector: taskboard_domain::test_support::test_connector(),
            snapshot: Box::new(RemoteBoardSnapshot {
                board: RemoteBoard {
                    id: taskboard_domain::RemoteBoardId(77),
                    title: "kiosk".into(),
                    color: "0000ff".into(),
                    archived: false,
                    deleted_at: None,
                    last_modified: ts(T0),
                },
                stacks: vec![taskboard_domain::RemoteStack {
                    id: taskboard_domain::RemoteStackRef {
                        board: taskboard_domain::RemoteBoardId(77),
                        stack: taskboard_domain::RemoteStackId(1),
                    },
                    title: "remote column".into(),
                    order: 1,
                    archived: false,
                    deleted_at: None,
                    last_modified: ts(T0),
                }],
                tasks: vec![taskboard_domain::RemoteTask {
                    id: taskboard_domain::RemoteCardRef {
                        board: taskboard_domain::RemoteBoardId(77),
                        stack: taskboard_domain::RemoteStackId(1),
                        card: taskboard_domain::RemoteCardId(10),
                    },
                    title: "remote card".into(),
                    description: String::new(),
                    duedate: None,
                    scheduled: None,
                    done: None,
                    stack: taskboard_domain::RemoteStackId(1),
                    order: 1,
                    labels: std::collections::BTreeSet::default(),
                    archived: false,
                    last_modified: ts(T0),
                }],
                labels: vec![],
            }),
            validators: test_validators(),
            pushes: vec![],
            read_at: chrono::DateTime::<chrono::Utc>::MAX_UTC,
            features: taskboard_domain::DeckFeatures::CONSERVATIVE,
        })
        .await
        .expect("report channel open");
    // Deterministic settle: the report is processed when the published
    // state shows the adopted board binding.
    tokio::time::timeout(std::time::Duration::from_secs(60), async {
        loop {
            let bound = s
                .handle
                .shared_state()
                .load_full()
                .boards
                .values()
                .any(|b| {
                    b.remote
                        == taskboard_domain::test_support::test_binding(
                            taskboard_domain::RemoteBoardId(77),
                        )
                });
            if bound {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("report settles");
    s.snap("s3");
}

/// S4 — sync resolution: push outcomes complete ops (and dead-letter the
/// rejected one), draining `pending_ops`.
#[cfg_attr(miri, ignore)] // insta needs socketpair (environment gate)
#[tokio::test]
async fn s4_sync_resolution_drains_outbox() {
    let mut s = Scenario::start().await;
    let CommandOutcome::CreatedStack(stack) = s
        .run(StateCommand::CreateStack {
            board: s.board_id(),
            title: "todo".into(),
            order: 1,
        })
        .await
    else {
        panic!("wrong receipt");
    };
    let _task = s
        .run(StateCommand::CreateTask {
            title: "pushed".into(),
            stack,
            order: 1,
        })
        .await;
    let _label = s
        .run(StateCommand::CreateLabel {
            board: s.board_id(),
            title: "dead-lettered".into(),
            color: Color::new("000000"),
        })
        .await;

    // Read the op ids the engine enqueued (deterministic via CountingIds).
    let outbox = s.repo.snapshot().outbox;
    let push = |op: taskboard_domain::OpId, result: PushResult| PushOutcome { op, result };
    let mut pushes = Vec::new();
    for pending in &outbox {
        let result = match pending.op {
            LocalOp::CreateLabel(_) => PushResult::Rejected {
                kind: SyncErrorKind::BadRequest,
            },
            _ => PushResult::Applied { echo: None },
        };
        pushes.push(push(pending.op_id, result));
    }

    s.clock.advance(60);
    s.report_tx
        .send(SyncReport::Completed {
            app_data: taskboard_domain::AppDataEvidence::Absent,
            duration: std::time::Duration::from_millis(250),
            connector: taskboard_domain::test_support::test_connector(),
            snapshot: Box::new(RemoteBoardSnapshot {
                board: RemoteBoard {
                    id: taskboard_domain::RemoteBoardId(77),
                    title: "kiosk".into(),
                    color: "0000ff".into(),
                    archived: false,
                    deleted_at: None,
                    last_modified: ts(T0 + 120),
                },
                stacks: vec![],
                tasks: vec![],
                labels: vec![],
            }),
            pushes,
            validators: test_validators(),
            read_at: chrono::DateTime::<chrono::Utc>::MAX_UTC,
            features: taskboard_domain::DeckFeatures::CONSERVATIVE,
        })
        .await
        .expect("report channel open");
    tokio::time::timeout(std::time::Duration::from_secs(60), async {
        loop {
            if s.handle
                .shared_state()
                .load_full()
                .sync
                .aggregate()
                .last_success
                .is_some()
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("report settles");
    s.snap("s4");
}

/// S5 — rejection sequence: stale-id and no-op commands leave state and
/// outbox untouched.
#[cfg_attr(miri, ignore)] // insta needs socketpair (environment gate)
#[tokio::test]
async fn s5_rejections_leave_state_untouched() {
    let mut s = Scenario::start().await;
    let CommandOutcome::CreatedStack(stack) = s
        .run(StateCommand::CreateStack {
            board: s.board_id(),
            title: "todo".into(),
            order: 1,
        })
        .await
    else {
        panic!("wrong receipt");
    };
    let CommandOutcome::CreatedTask(task) = s
        .run(StateCommand::CreateTask {
            title: "real".into(),
            stack,
            order: 1,
        })
        .await
    else {
        panic!("wrong receipt");
    };
    let before = s.repo.snapshot();

    // Stale id: unknown task.
    let stranger = stranger_task_id();
    assert!(matches!(
        s.run_raw(StateCommand::SetTaskDone {
            id: stranger,
            done: true,
        })
        .await,
        Err(taskboard_state::ExecuteError::Rejected(
            taskboard_domain::CommandError::UnknownTask(_)
        ))
    ));
    // No-ops and rejections interleave; nothing may touch the state.
    let _ = s
        .run_raw(StateCommand::SetTaskDone {
            id: task,
            done: false,
        })
        .await
        .expect("accepted");
    assert!(matches!(
        s.run_raw(StateCommand::DeleteTask { id: stranger }).await,
        Err(taskboard_state::ExecuteError::Rejected(
            taskboard_domain::CommandError::UnknownTask(_)
        ))
    ));

    s.snap("s5");
    assert_eq!(
        s.repo.snapshot(),
        before,
        "rejections and no-ops must not touch the persisted state"
    );
}

/// S6 — network-lost/restored cycle: the phase machine's transitions.
#[cfg_attr(miri, ignore)] // insta needs socketpair (environment gate)
#[tokio::test]
async fn s6_network_lost_restored_cycle() {
    // The offline transition addresses bound boards (C8): the scenario's
    // subject is a configured install, so it seeds the bound shape.
    let mut s = Scenario::start_with(common::seed_bound_board_state("kiosk")).await;
    let CommandOutcome::CreatedStack(_stack) = s
        .run(StateCommand::CreateStack {
            board: s.board_id(),
            title: "todo".into(),
            order: 1,
        })
        .await
    else {
        panic!("wrong receipt");
    };

    s.clock.advance(60);
    s.system_tx
        .send(SystemEvent::NetworkLost)
        .expect("broadcast");
    // Deterministic settle: poll for the persisted Offline phase.
    tokio::time::timeout(std::time::Duration::from_secs(60), async {
        loop {
            if s.repo.snapshot().sync.aggregate().phase == SyncPhase::Offline {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("offline transition settles");

    s.clock.advance(60);
    s.system_tx
        .send(SystemEvent::NetworkRestored)
        .expect("broadcast");
    // No phase change on restore; the next report fails with a typed kind.
    s.clock.advance(60);
    s.report_tx
        .send(SyncReport::Failed {
            duration: std::time::Duration::from_millis(100),
            board: None,
            connector: taskboard_domain::test_support::test_connector(),
            kind: SyncErrorKind::Network,
            pushes: vec![],

            read_at: chrono::DateTime::<chrono::Utc>::MAX_UTC,
            features: taskboard_domain::DeckFeatures::CONSERVATIVE,
        })
        .await
        .expect("report channel open");
    tokio::time::timeout(std::time::Duration::from_secs(60), async {
        loop {
            if s.repo.snapshot().sync.aggregate().phase
                == (SyncPhase::Failed {
                    last_error: SyncErrorKind::Network,
                })
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("failed phase settles");
    s.snap("s6");
}

/// S7 — stand-alone first run: board → stack → task authored from an
/// empty db, then `BindBoard` attaches the local board to a remote one
/// (the outbox stays queued for the connector's first cycle).
#[cfg_attr(miri, ignore)] // insta needs socketpair (environment gate)
#[tokio::test]
async fn s7_standalone_first_run_and_bind() {
    let mut s = Scenario::start_empty().await;
    let CommandOutcome::CreatedBoard(board) = s
        .run(StateCommand::CreateBoard {
            title: "personal".into(),
            color: Color::new("3366cc"),
        })
        .await
    else {
        panic!("wrong receipt");
    };
    // The refusal lift: a second create mints a distinct board.
    let CommandOutcome::CreatedBoard(second_board) = s
        .run_raw(StateCommand::CreateBoard {
            title: "second".into(),
            color: Color::new("000000"),
        })
        .await
        .expect("the lift mints")
    else {
        panic!("wrong receipt");
    };
    assert_ne!(second_board, board);
    let CommandOutcome::CreatedStack(stack) = s
        .run(StateCommand::CreateStack {
            board: s.board_id(),
            title: "todo".into(),
            order: 1,
        })
        .await
    else {
        panic!("wrong receipt");
    };
    s.run(StateCommand::CreateTask {
        title: "first task".into(),
        stack,
        order: 1,
    })
    .await;
    s.run(StateCommand::BindBoard {
        board: s.board_id(),
        remote: taskboard_domain::RemoteBoardId(77),
        connector: taskboard_domain::test_support::test_connector(),
    })
    .await;
    // Rebinding to the same remote is the accepted no-op.
    let _ = s
        .run_raw(StateCommand::BindBoard {
            board: s.board_id(),
            remote: taskboard_domain::RemoteBoardId(77),
            connector: taskboard_domain::test_support::test_connector(),
        })
        .await
        .expect("accepted");
    let app = s.handle.shared_state().load_full();
    assert_eq!(
        app.boards[&board].remote,
        taskboard_domain::test_support::test_binding(taskboard_domain::RemoteBoardId(77))
    );
    s.snap("s7");
    drop(s);
}

/// Warm validator bundles (non-empty: the engine ignores empty bundles
/// by design — nothing reusable to store).
fn warm_validators() -> taskboard_domain::BoardPullValidators {
    let warm = taskboard_domain::SyncValidators {
        etag: Some("\"w1\"".into()),
        last_modified: Some("warm".into()),
    };
    taskboard_domain::BoardPullValidators {
        boards: warm.clone(),
        stacks: warm.clone(),
        archived_stacks: warm,
    }
}

/// S8 — board color/archive edits: each verb stamps its own field clock
/// only and converges on the single full-send `update_board` op.
#[cfg_attr(miri, ignore)] // insta needs socketpair (environment gate)
#[tokio::test]
async fn s8_boards_color_archive() {
    let mut s = Scenario::start_empty().await;
    let CommandOutcome::CreatedBoard(_board) = s
        .run(StateCommand::CreateBoard {
            title: "personal".into(),
            color: Color::new("3366cc"),
        })
        .await
    else {
        panic!("wrong receipt");
    };
    s.run(StateCommand::SetBoardColor {
        board: s.board_id(),
        color: Color::new("00c2e0"),
    })
    .await;
    // A same-color repeat is the accepted no-op (nothing persisted).
    s.run(StateCommand::SetBoardColor {
        board: s.board_id(),
        color: Color::new("00c2e0"),
    })
    .await;
    s.run(StateCommand::SetBoardArchived {
        board: s.board_id(),
        archived: true,
    })
    .await;
    s.snap("s8");
    drop(s);
}

/// S9 — board delete ceremony: the tombstone cascades to the live
/// stacks/tasks/label, cancels the doomed entities' queued ops, and
/// queues exactly one `delete_board` op.
#[cfg_attr(miri, ignore)] // insta needs socketpair (environment gate)
#[tokio::test]
async fn s9_boards_delete_cascade() {
    let mut s = Scenario::start_empty().await;
    let CommandOutcome::CreatedBoard(board) = s
        .run(StateCommand::CreateBoard {
            title: "doomed".into(),
            color: Color::new("3366cc"),
        })
        .await
    else {
        panic!("wrong receipt");
    };
    let CommandOutcome::CreatedStack(first) = s
        .run(StateCommand::CreateStack {
            board: s.board_id(),
            title: "todo".into(),
            order: 1,
        })
        .await
    else {
        panic!("wrong receipt");
    };
    let CommandOutcome::CreatedStack(second) = s
        .run(StateCommand::CreateStack {
            board: s.board_id(),
            title: "doing".into(),
            order: 2,
        })
        .await
    else {
        panic!("wrong receipt");
    };
    for n in 0..3 {
        s.run(StateCommand::CreateTask {
            title: format!("task {n}"),
            stack: if n == 2 { second } else { first },
            order: n,
        })
        .await;
    }
    s.run(StateCommand::CreateLabel {
        board,
        title: "urgent".into(),
        color: Color::new("ff0000"),
    })
    .await;
    // A queued op of a doomed task: the delete must cancel it.
    let outbox = s.repo.snapshot().outbox;
    let task_op = outbox
        .iter()
        .find(|o| matches!(o.op, LocalOp::CreateTask(_)))
        .expect("a queued create op");

    s.run(StateCommand::DeleteBoard { board }).await;
    // Repeat: the accepted terminal no-op.
    s.run(StateCommand::DeleteBoard { board }).await;
    let after = s.repo.snapshot();
    assert!(
        !after.outbox.iter().any(|o| o.op_id == task_op.op_id),
        "the doomed task's queued op is cancelled"
    );
    s.snap("s9");
    drop(s);
}

/// S10 — board reset: content, queued ops, validators, and reminder
/// facts clear in one bulk action; the board row and the sync status
/// survive (a bound board re-materializes from the next pull).
#[cfg_attr(miri, ignore)] // insta needs socketpair (environment gate)
#[tokio::test]
async fn s10_boards_reset() {
    let mut s = Scenario::start_empty().await;
    let CommandOutcome::CreatedBoard(board) = s
        .run(StateCommand::CreateBoard {
            title: "repair".into(),
            color: Color::new("3366cc"),
        })
        .await
    else {
        panic!("wrong receipt");
    };
    let CommandOutcome::CreatedStack(stack) = s
        .run(StateCommand::CreateStack {
            board: s.board_id(),
            title: "todo".into(),
            order: 1,
        })
        .await
    else {
        panic!("wrong receipt");
    };
    let CommandOutcome::CreatedTask(task) = s
        .run(StateCommand::CreateTask {
            title: "doomed".into(),
            stack,
            order: 1,
        })
        .await
    else {
        panic!("wrong receipt");
    };
    s.run(StateCommand::AddReminderRule {
        task,
        offset: taskboard_domain::ReminderOffset::at_due(),
    })
    .await;
    s.run(StateCommand::BindBoard {
        board: s.board_id(),
        remote: taskboard_domain::RemoteBoardId(77),
        connector: taskboard_domain::test_support::test_connector(),
    })
    .await;
    // Warm validators: the reset must clear them (the 304 trap).
    s.clock.advance(60);
    s.report_tx
        .send(SyncReport::Completed {
            app_data: taskboard_domain::AppDataEvidence::Absent,
            duration: std::time::Duration::ZERO,
            connector: taskboard_domain::test_support::test_connector(),
            snapshot: Box::new(RemoteBoardSnapshot {
                board: RemoteBoard {
                    id: taskboard_domain::RemoteBoardId(77),
                    title: "repair".into(),
                    color: "3366cc".into(),
                    archived: false,
                    deleted_at: None,
                    last_modified: ts(T0),
                },
                stacks: vec![],
                tasks: vec![],
                labels: vec![],
            }),
            validators: warm_validators(),
            pushes: vec![],
            read_at: chrono::DateTime::<chrono::Utc>::MAX_UTC,
            features: taskboard_domain::DeckFeatures::CONSERVATIVE,
        })
        .await
        .expect("report channel open");
    tokio::time::timeout(std::time::Duration::from_secs(60), async {
        loop {
            if !s.repo.snapshot().validators.is_empty() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("validators settle");

    s.run(StateCommand::ResetBoard { board }).await;
    // Repeat: the accepted already-empty no-op.
    s.run(StateCommand::ResetBoard { board }).await;
    let after = s.repo.snapshot();
    assert!(
        after.stacks.is_empty()
            && after.tasks.is_empty()
            && after.labels.is_empty()
            && after.outbox.is_empty()
            && after.validators.is_empty()
            && after.reminders.rules.is_empty(),
        "the reset clears content, ops, validators, and reminder facts"
    );
    assert_eq!(after.boards.len(), 1, "the board row survives");
    s.snap("s10");
    drop(s);
}

/// S30 — first capture (#347, D-V5 v2): the unbound Inbox board, its
/// Inbox stack, and the task are minted in one batch; the board row
/// carries `remote: None` and no board op is queued — the stack + task
/// ops strand honestly until the user binds. The second capture appends
/// to the same stack (adoption, canonical-first landing).
#[cfg_attr(miri, ignore)]
#[tokio::test]
async fn s30_capture_creates_the_unbound_inbox_then_appends() {
    let mut s = Scenario::start_empty().await;
    let CommandOutcome::CreatedTask(_) = s
        .run(StateCommand::CaptureTask {
            title: "first thought".into(),
            board: None,
        })
        .await
    else {
        panic!("wrong receipt");
    };
    s.snap("s30a");
    s.run(StateCommand::CaptureTask {
        title: "second thought".into(),
        board: None,
    })
    .await;
    s.snap("s30b");
}

/// S31 — board-directed capture: `Some(board)` on the seeded board
/// (no Backlog) creates the Backlog appended; a second capture lands in
/// it. The seeded board stays untouched otherwise.
#[cfg_attr(miri, ignore)]
#[tokio::test]
async fn s31_capture_board_directed_creates_the_backlog() {
    let mut s = Scenario::start().await;
    let board = s.board_id();
    let CommandOutcome::CreatedTask(_) = s
        .run(StateCommand::CaptureTask {
            title: "triaged thought".into(),
            board: Some(board),
        })
        .await
    else {
        panic!("wrong receipt");
    };
    s.run(StateCommand::CaptureTask {
        title: "second thought".into(),
        board: Some(board),
    })
    .await;
    s.snap("s31");
}

/// S32 — adoption: a live board titled exactly `Inbox` is adopted (not
/// duplicated); an *archived* one is not — a fresh unbound Inbox is
/// minted instead. The unknown board id is the one typed rejection.
#[cfg_attr(miri, ignore)]
#[tokio::test]
async fn s32_capture_adopts_a_live_inbox_not_an_archived_one() {
    let mut s = Scenario::start_empty().await;
    let CommandOutcome::CreatedBoard(live_inbox) = s
        .run(StateCommand::CreateBoard {
            title: "Inbox".into(),
            color: Color::new("3366cc"),
        })
        .await
    else {
        panic!("wrong receipt");
    };
    s.run(StateCommand::CaptureTask {
        title: "adopted".into(),
        board: None,
    })
    .await;
    s.snap("s32a");

    // Archive the live inbox; the next capture must mint a fresh board.
    s.run(StateCommand::SetBoardArchived {
        board: live_inbox,
        archived: true,
    })
    .await;
    s.run(StateCommand::CaptureTask {
        title: "not adopted".into(),
        board: None,
    })
    .await;
    s.snap("s32b");

    // The happy path never errors — only a stranger board id does.
    let error = s
        .run_raw(StateCommand::CaptureTask {
            title: "x".into(),
            board: Some(stranger_board_id()),
        })
        .await
        .expect_err("an unknown board is the typed rejection");
    assert!(matches!(
        error,
        taskboard_state::ExecuteError::Rejected(taskboard_domain::CommandError::UnknownBoard(_))
    ));
}

/// A fired-notification seed: board + stack + a live open task with a due
/// date, one explicit rule (2 h before due, so the occurrence is in the
/// past at T0), and the delivery receipt for that occurrence — exactly
/// one active notification, no prior dismissals.
fn seed_fired_rule_state() -> taskboard_domain::PersistedState {
    use taskboard_domain::idgen::IdGenerator as _;
    use taskboard_domain::{
        PersistedState, ReminderDelivery, ReminderOffset, ReminderRef, ReminderRule, Stack,
        StackClocks, Task, TaskClocks,
    };
    let ids = CountingIds::new();
    let board = common::board("kiosk");
    let stack = Stack {
        id: ids.new_stack_id(),
        remote: None,
        board: board.id,
        title: "todo".into(),
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
    let task = Task {
        id: ids.new_task_id(),
        remote: None,
        title: "fired task".into(),
        description: String::new(),
        duedate: Some(ts(T0 + 3_600)),
        scheduled: None,
        done: None,
        stack: stack.id,
        order: 1,
        labels: std::collections::BTreeSet::default(),
        effort: None,
        importance: None,
        urgency: None,
        archived: false,
        deleted: false,
        clocks: TaskClocks {
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
            archived: ts(T0),
            deleted: ts(T0),
        },
        remote_seen: None,
    };
    let rule = ReminderRule {
        id: ids.new_reminder_rule_id(),
        task: task.id,
        offset: ReminderOffset::before(chrono::TimeDelta::try_hours(-2).unwrap())
            .expect("negative offset"),
        created_at: ts(T0 - 60),
        updated_at: ts(T0 - 60),
    };
    let mut state = taskboard_domain::PersistedState {
        boards: [(board.id, board)].into_iter().collect(),
        stacks: [(stack.id, stack)].into_iter().collect(),
        tasks: [(task.id, task)].into_iter().collect(),
        ..PersistedState::default()
    };
    let rule_id = rule.id;
    state.reminders.rules.insert(rule.id, rule);
    state.reminders.deliveries.insert(ReminderDelivery {
        reminder: ReminderRef::Rule(rule_id),
        occurrence: ts(T0 - 3_600),
        delivered_at: ts(T0 - 3_600),
    });
    state
}

/// S34 — user dismissal (R3): the in-app Dismiss button's command
/// persists the dismissal stamped by the injected clock and queues the
/// watched task's `UpdateAppData` op (rule refs ride the synced
/// `dismissals` section via the document lane); the repeat is the
/// accepted no-op.
#[cfg_attr(miri, ignore)]
#[tokio::test]
async fn s34_dismiss_reminder_persists_and_repeats_as_noop() {
    let seed = seed_fired_rule_state();
    let rule_id = *seed.reminders.rules.keys().next().expect("one rule");
    let task_id = *seed.tasks.keys().next().expect("one task");
    let mut s = Scenario::start_with(seed).await;
    s.run(StateCommand::DismissReminder {
        reminder: taskboard_domain::ReminderRef::Rule(rule_id),
        occurrence: ts(T0 - 3_600),
    })
    .await;
    // Repeat: a dismissal is permanent for the occurrence.
    s.run(StateCommand::DismissReminder {
        reminder: taskboard_domain::ReminderRef::Rule(rule_id),
        occurrence: ts(T0 - 3_600),
    })
    .await;
    let persisted = s.repo.snapshot();
    assert_eq!(persisted.reminders.dismissals.len(), 1, "first write wins");
    assert!(persisted.outbox.iter().any(|op| matches!(
        op.op,
        taskboard_domain::LocalOp::UpdateAppData(t) if t == task_id
    )));
    s.snap("s34");
    drop(s);
}

/// S35 — the reopen hazard (R3.5.5): Mark Done dismisses the task's
/// currently-active occurrences as explicit rows in the same batch;
/// reopening emits nothing, and the stale occurrence stays dismissed.
#[cfg_attr(miri, ignore)]
#[tokio::test]
async fn s35_mark_done_dismisses_active_occurrences_reopen_does_not_resurrect() {
    let seed = seed_fired_rule_state();
    let task_id = *seed.tasks.keys().next().expect("one task");
    let mut s = Scenario::start_with(seed).await;
    s.run(StateCommand::SetTaskDone {
        id: task_id,
        done: true,
    })
    .await;
    s.snap("s35a");
    s.run(StateCommand::SetTaskDone {
        id: task_id,
        done: false,
    })
    .await;
    let persisted = s.repo.snapshot();
    assert_eq!(
        persisted.reminders.dismissals.len(),
        1,
        "the reopen emits nothing and dismisses nothing away"
    );
    s.snap("s35b");
    drop(s);
}

/// An id guaranteed absent from the state (the rejection test's target).
fn stranger_board_id() -> taskboard_domain::BoardId {
    common::stranger_board_id()
}

/// S33 — canonical-first among duplicates: two `Backlog`-titled stacks
/// on one board; board-directed capture lands in the smaller
/// `(order, id)` one.
#[cfg_attr(miri, ignore)]
#[tokio::test]
async fn s33_capture_lands_canonical_first_among_backlogs() {
    let mut s = Scenario::start().await;
    let board = s.board_id();
    s.run(StateCommand::CreateStack {
        board,
        title: "Backlog".into(),
        order: 5,
    })
    .await;
    s.run(StateCommand::CaptureTask {
        title: "earliest backlog".into(),
        board: Some(board),
    })
    .await;
    s.snap("s33");
}

/// S36 — the tracking surface enters the disk contract (DR-5/1): a
/// start mints the open row (s36a); a stop stamps it and a `LogTime`
/// adds a closed retroactive row (s36b). The serialized entry shape is
/// the contract chunk 2's codec must round-trip.
#[cfg_attr(miri, ignore)]
#[tokio::test]
async fn s36_tracking_rows_enter_the_disk_contract() {
    let mut s = Scenario::start().await;
    let CommandOutcome::CreatedStack(stack) = s
        .run(StateCommand::CreateStack {
            board: s.board_id(),
            title: "todo".into(),
            order: 1,
        })
        .await
    else {
        panic!("wrong receipt");
    };
    let CommandOutcome::CreatedTask(task) = s
        .run(StateCommand::CreateTask {
            title: "tracked work".into(),
            stack,
            order: 1,
        })
        .await
    else {
        panic!("wrong receipt");
    };
    let CommandOutcome::TrackingStarted { .. } = s.run(StateCommand::StartTracking { task }).await
    else {
        panic!("wrong receipt");
    };
    s.snap("s36a");

    s.run(StateCommand::StopTracking).await;
    s.run(StateCommand::LogTime {
        task,
        start: ts(T0 - 86_400),
        end: ts(T0 - 79_200),
    })
    .await;
    s.snap("s36b");
}
