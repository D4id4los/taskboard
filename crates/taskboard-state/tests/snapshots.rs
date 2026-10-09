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
//! engine's behavioral contract. Each snapshot captures BOTH the
//! published `AppState` and the fake repository's `PersistedState` —
//! memory and disk must always tell the same story. Insta diffs read as
//! contract changes (roadmap Phase 3 exit criterion).

use std::sync::Arc;

use taskboard_domain::test_support::{CountingIds, InMemoryRepository};
use taskboard_domain::{
    Color, CommandOutcome, LocalOp, PushOutcome, PushResult, RemoteBoard, RemoteBoardSnapshot,
    StateCommand, SyncCommand, SyncErrorKind, SyncPhase, SyncReport, SystemEvent,
};
use taskboard_state::spawn_state_engine;

use insta::assert_yaml_snapshot;

mod common;

use common::{FixedClock, T0, seed_board_state, stranger_task_id, ts};

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
        let repo = Arc::new(InMemoryRepository::with_state(seed_board_state("kiosk")));
        let clock = FixedClock::new(T0);
        let (sync_out, _sync_in) = tokio::sync::mpsc::channel::<SyncCommand>(8);
        let (report_tx, report_rx) = tokio::sync::mpsc::channel::<SyncReport>(8);
        let (system_tx, system_rx) = tokio::sync::broadcast::channel::<SystemEvent>(8);
        let (handle, _join) = spawn_state_engine(
            repo.clone(),
            Arc::new(CountingIds::new()),
            clock.clone(),
            sync_out,
            report_rx,
            system_rx,
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

    /// Captures the two contract views under scenario-unique names.
    fn snap(&self, scenario: &str) {
        let app = self.handle.shared_state().load_full();
        assert_yaml_snapshot!(format!("{scenario}_published_app_state"), &*app);
        let persisted = self.repo.snapshot();
        assert_yaml_snapshot!(format!("{scenario}_persisted_state"), &persisted);
    }
}

/// S1 — fresh-boot authoring: stack → task → edit → done → label →
/// assign → move → delete.
#[cfg_attr(miri, ignore)] // insta needs socketpair (environment gate)
#[tokio::test]
async fn s1_fresh_boot_authoring() {
    let mut s = Scenario::start().await;
    let CommandOutcome::CreatedStack(stack) = s
        .run(StateCommand::CreateStack {
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
            snapshot: RemoteBoardSnapshot {
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
                    done: None,
                    stack: taskboard_domain::RemoteStackId(1),
                    order: 1,
                    labels: std::collections::BTreeSet::default(),
                    archived: false,
                    last_modified: ts(T0),
                }],
                labels: vec![],
            },
            pushes: vec![],
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
                .any(|b| b.remote == Some(taskboard_domain::RemoteBoardId(77)));
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
            snapshot: RemoteBoardSnapshot {
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
            },
            pushes,
        })
        .await
        .expect("report channel open");
    tokio::time::timeout(std::time::Duration::from_secs(60), async {
        loop {
            if s.handle
                .shared_state()
                .load_full()
                .sync
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
    let mut s = Scenario::start().await;
    let CommandOutcome::CreatedStack(_stack) = s
        .run(StateCommand::CreateStack {
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
            if s.repo.snapshot().sync.phase == SyncPhase::Offline {
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
            kind: SyncErrorKind::Network,
        })
        .await
        .expect("report channel open");
    tokio::time::timeout(std::time::Duration::from_secs(60), async {
        loop {
            if s.repo.snapshot().sync.phase
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
