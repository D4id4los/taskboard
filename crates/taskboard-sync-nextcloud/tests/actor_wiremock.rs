// SPDX-License-Identifier: MIT OR Apache-2.0
//! A-series: the sync actor against wiremock under paused tokio time (one
//! behavior per test). The actor reads through `Arc<InMemoryRepository>` —
//! the same fake the engine uses, so memory ≡ disk invariants hold across
//! the pair; reports and system events arrive over the injected channels
//! with timeout guards.
//!
//! Determinism: `#[tokio::test(start_paused)]` freezes the tokio clock;
//! the only place time advances is the `next_report`/`start_server` wait
//! helpers, so actor timers (client retries, backoff deadlines) fire
//! exactly while the test waits for an expected report — never between
//! assertions. The wait helpers' deadline is wall-clock
//! (`std::time::Instant`): the stepping is virtual, but the actor's
//! progress depends on real wiremock I/O. `cfg().poll_interval` sits far beyond every test horizon, so
//! cycles are nudge/`SetBoard`/backoff-driven only and wiremock request
//! counts stay an exact observable of how many cycles ran.

use std::sync::Arc;
use std::time::Duration;

use taskboard_domain::test_support::InMemoryRepository;
use taskboard_domain::{
    Board, LocalOp, OpId, PendingOp, PersistedState, PushResult, RemoteBoardId, RemoteEcho,
    StackClocks, SyncCommand, SyncErrorKind, SyncReport, SystemEvent, Task, TaskClocks,
    TaskRepository as _,
};
use taskboard_sync_nextcloud::{DeckClient, SyncActorConfig, spawn_sync_actor};
use tokio::sync::{broadcast, mpsc};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

mod common;
use common::FixedClock;
use common::hermetic::{
    BOARD, STACK, T, board_json, boards_json, card_json, mount_conditional_pull, stacks_json,
    start_server, ts, uuid, wait_subscribed,
};

fn cfg() -> SyncActorConfig {
    SyncActorConfig {
        // No poll ticks inside a test horizon: cycles fire only on nudges,
        // `SetBoard`, and backoff deadlines (all test-driven), keeping the
        // request-count observables exact.
        poll_interval: Duration::from_secs(3_600),
        backoff_initial: Duration::from_millis(100),
        backoff_max: Duration::from_millis(400),
        // Every LWW-participating stamp the actor mints is `T + 1000` —
        // after every fixture stamp, preserving the real-clock orderings.
        clock: Arc::new(FixedClock(ts(T + 1000))),
    }
}

/// The actor plus every channel half a test needs.
#[allow(dead_code)] // the repo is kept for symmetry with the I-series harness
struct Actor {
    repo: Arc<InMemoryRepository>,
    commands: mpsc::Sender<SyncCommand>,
    reports: mpsc::Receiver<SyncReport>,
    system_tx: broadcast::Sender<SystemEvent>,
    system_rx: broadcast::Receiver<SystemEvent>,
    join: tokio::task::JoinHandle<()>,
}

// ---- deterministic waiting (the only places time moves) ---------------

/// Receives the next report, advancing time only while waiting — actor
/// timers (retries, backoff, poll deadlines) fire inside this loop and
/// nowhere else. Bounded in *wall-clock* time (the stepping stays
/// virtual): a wedged actor fails the test within the deadline instead
/// of hanging it.
const WALL_DEADLINE: Duration = Duration::from_secs(30);

async fn next_report(actor: &mut Actor) -> SyncReport {
    let deadline = std::time::Instant::now() + WALL_DEADLINE;
    loop {
        if let Ok(report) = actor.reports.try_recv() {
            return report;
        }
        let reports_open = !actor.reports.is_closed();
        assert!(
            std::time::Instant::now() < deadline,
            "actor produced no report within the deadline (reports channel still open: {reports_open})"
        );
        tokio::time::advance(Duration::from_millis(10)).await;
    }
}

fn task_clocks() -> TaskClocks {
    TaskClocks {
        title: ts(T),
        description: ts(T),
        duedate: ts(T),
        done: ts(T),
        position: ts(T),
        labels: ts(T),
        archived: ts(T),
        deleted: ts(T),
    }
}

fn stack_clocks() -> StackClocks {
    StackClocks {
        title: ts(T),
        order: ts(T),
        deleted: ts(T),
    }
}

fn bound_board() -> Board {
    Board {
        id: taskboard_domain::BoardId::from(uuid(1)),
        remote: Some(RemoteBoardId(BOARD)),
        title: "board".into(),
        color: taskboard_domain::Color::new("00ff00"),
        archived: false,
        deleted: false,
        clocks: taskboard_domain::BoardClocks {
            title: ts(T),
            color: taskboard_domain::BoardClocks::epoch(),
            archived: taskboard_domain::BoardClocks::epoch(),
            deleted: taskboard_domain::BoardClocks::epoch(),
        },
        remote_seen: Some(ts(T)),
    }
}

fn bound_stack(local: u128, remote: u64) -> taskboard_domain::Stack {
    taskboard_domain::Stack {
        id: taskboard_domain::StackId::from(uuid(local)),
        remote: Some(taskboard_domain::RemoteStackRef {
            board: RemoteBoardId(BOARD),
            stack: taskboard_domain::RemoteStackId(remote),
        }),
        board: taskboard_domain::BoardId::from(uuid(1)),
        title: "col".into(),
        order: 0,
        archived: false,
        deleted: false,
        clocks: stack_clocks(),
        remote_seen: Some(ts(T)),
    }
}

fn draft(task_raw: u128, stack_local: u128, title: &str) -> (Task, PendingOp) {
    let task = Task {
        id: taskboard_domain::TaskId::from(uuid(task_raw)),
        remote: None,
        title: title.into(),
        description: String::new(),
        duedate: None,
        done: None,
        stack: taskboard_domain::StackId::from(uuid(stack_local)),
        order: 0,
        labels: std::collections::BTreeSet::new(),
        archived: false,
        deleted: false,
        clocks: task_clocks(),
        remote_seen: None,
    };
    let op = PendingOp {
        op_id: OpId(uuid(task_raw + 1)),
        op: LocalOp::CreateTask(task.id),
        queued_at: ts(T),
    };
    (task, op)
}

/// The standard seed: a bound board, the given bound stacks, and offline
/// drafts (task id `raw`, create op id `raw + 1`) in the named stacks.
fn repo_with(
    stacks: &[(u128, u64)],
    drafts: &[(u128, u128, u64)], // (task raw, stack local, stack remote)
) -> Arc<InMemoryRepository> {
    let mut state = PersistedState::default();
    let board = bound_board();
    state.boards.insert(board.id, board);
    for (local, remote) in stacks {
        let stack = bound_stack(*local, *remote);
        state.stacks.insert(stack.id, stack);
    }
    for (task_raw, stack_local, _) in drafts {
        let (task, op) = draft(*task_raw, *stack_local, "created offline");
        state.tasks.insert(task.id, task);
        state.outbox.push(op);
    }
    Arc::new(InMemoryRepository::with_state(state))
}

/// Mounts the standard happy pull: boards listing, the board detail
/// (authoritative labels read), active stacks, empty archived stacks —
/// all live, with `ETag`s so conditional follow-ups work.
async fn mount_standard_pull(server: &MockServer, cards: serde_json::Value) {
    for (p, body) in [
        (
            "/index.php/apps/deck/api/v1.0/boards".to_string(),
            boards_json(false, &serde_json::json!([])),
        ),
        (
            format!("/index.php/apps/deck/api/v1.0/boards/{BOARD}"),
            board_json(false, &serde_json::json!([])),
        ),
        (
            format!("/index.php/apps/deck/api/v1.0/boards/{BOARD}/stacks"),
            stacks_json(&cards),
        ),
        (
            format!("/index.php/apps/deck/api/v1.0/boards/{BOARD}/stacks/archived"),
            "[]".to_string(),
        ),
    ] {
        Mock::given(method("GET"))
            .and(path(p))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("ETag", "\"gen-1\"")
                    .set_body_string(body),
            )
            .mount(server)
            .await;
    }
}

fn spawn(server: &MockServer, repo: Arc<InMemoryRepository>) -> Actor {
    spawn_with_cfg(server, repo, cfg())
}

fn spawn_with_cfg(
    server: &MockServer,
    repo: Arc<InMemoryRepository>,
    cfg: SyncActorConfig,
) -> Actor {
    let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
    let (commands, command_rx) = mpsc::channel(16);
    let (report_tx, report_rx) = mpsc::channel(16);
    let (system_tx, system_rx) = broadcast::channel(16);
    let join = spawn_sync_actor(
        client,
        repo.clone(),
        command_rx,
        report_tx,
        system_tx.clone(),
        cfg,
    );
    Actor {
        repo,
        commands,
        reports: report_rx,
        system_tx,
        system_rx,
        join,
    }
}

/// Arms the pull target and awaits the triggered cycle's report.
async fn set_board_and_report(actor: &mut Actor) -> SyncReport {
    actor
        .commands
        .send(SyncCommand::SetBoard(RemoteBoardId(BOARD)))
        .await
        .expect("commands channel open");
    next_report(actor).await
}

// ---------------------------------------------------------------------

/// A1: a nudge triggers a cycle that pushes the offline create and pulls
/// the board back — the report carries the echo-bound push outcome and the
/// snapshot contains the created card (push-before-pull), from a *live*
/// board (the detail read answered, so no tombstone may appear).
#[tokio::test(start_paused = true)]
async fn a1_nudge_pushes_the_create_and_pulls_it_back() {
    let server = start_server().await;
    mount_standard_pull(
        &server,
        serde_json::json!([card_json(55, "created offline")]),
    )
    .await;
    Mock::given(method("POST"))
        .and(path(format!(
            "/index.php/apps/deck/api/v1.0/boards/{BOARD}/stacks/{STACK}/cards"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(card_json(55, "created offline")))
        .mount(&server)
        .await;

    let mut actor = spawn(&server, repo_with(&[(2, STACK)], &[(100, 2, STACK)]));
    actor.commands.send(SyncCommand::SyncNow).await.unwrap();
    let SyncReport::Completed {
        snapshot, pushes, ..
    } = next_report(&mut actor).await
    else {
        panic!("expected a Completed report");
    };
    assert!(
        snapshot.board.deleted_at.is_none(),
        "the board detail answered: the snapshot board is live"
    );
    assert_eq!(pushes.len(), 1, "the create pushed once");
    assert!(matches!(
        &pushes[0].result,
        PushResult::Applied { echo: Some(RemoteEcho::Task(view)) }
            if view.id.card == taskboard_domain::RemoteCardId(55)
    ));
    assert!(
        snapshot.tasks.iter().any(|t| t.title == "created offline"),
        "the pushed card is in the snapshot"
    );
}

/// A2: a second cycle against an unchanged server keeps the report
/// complete — 4 reads per cycle (3 conditional + the board-detail labels
/// read), with the in-memory pull cache ready to fill any `304` (the
/// dedicated 304 behavior is A10's).
#[tokio::test(start_paused = true)]
async fn a2_unchanged_poll_ticks_on_304s_from_the_cache() {
    let server = start_server().await;
    mount_conditional_pull(&server, &serde_json::json!([])).await;

    let mut actor = spawn(&server, repo_with(&[(2, STACK)], &[]));
    let first = set_board_and_report(&mut actor).await;
    let SyncReport::Completed {
        snapshot: first_snap,
        ..
    } = first
    else {
        panic!("expected a Completed report, got {first:?}");
    };
    assert!(first_snap.board.deleted_at.is_none());

    actor.commands.send(SyncCommand::SyncNow).await.unwrap();
    let second = next_report(&mut actor).await;
    let SyncReport::Completed { snapshot, .. } = second else {
        panic!("expected a Completed report");
    };
    assert_eq!(snapshot.board.id, RemoteBoardId(BOARD));
    assert!(
        snapshot.board.deleted_at.is_none(),
        "the board detail answered: the snapshot board is live"
    );
    assert_eq!(
        snapshot.stacks.len(),
        1,
        "the cached listing carries the board's stack"
    );
    assert_eq!(
        server.received_requests().await.unwrap().len(),
        8,
        "4 reads per cycle (3 conditional + the board-detail labels read)"
    );
}

/// A3: a nudge storm arriving while a cycle runs collapses into exactly
/// one rerun (the ADR 0006 absorber), observed via request counts.
#[tokio::test(start_paused = true)]
async fn a3_nudge_storm_collapse_into_one_rerun() {
    let server = start_server().await;
    mount_standard_pull(&server, serde_json::json!([])).await;

    let mut actor = spawn(&server, repo_with(&[(2, STACK)], &[]));
    let _ = set_board_and_report(&mut actor).await; // cycle 1

    for _ in 0..5 {
        actor.commands.send(SyncCommand::SyncNow).await.unwrap();
    }
    let _ = next_report(&mut actor).await; // cycle 2: the first buffered nudge
    let _ = next_report(&mut actor).await; // cycle 3: the storm coalesces into one rerun
    assert_eq!(
        server.received_requests().await.unwrap().len(),
        12,
        "exactly three cycles (4 reads each): initial + first nudge + one coalesced rerun"
    );
    // And then silence: five nudges never mean five cycles. Advancing five
    // simulated seconds (still inside the 3 600 s poll horizon) must not
    // produce a report.
    for _ in 0..500 {
        assert!(
            actor.reports.try_recv().is_err(),
            "no fourth cycle may fire"
        );
        tokio::time::advance(Duration::from_millis(10)).await;
    }
}

/// A4: a transport failure mid-outbox (two creates succeed, the third
/// exhausts its retry budget on 503s) reports `Failed { Network, [2] }`
/// with no snapshot; the remaining op stays queued.
#[tokio::test(start_paused = true)]
async fn a4_transport_failure_mid_push_keeps_evidence_and_queue() {
    let server = start_server().await;
    for (stack, card, title) in [
        (STACK, 55_u64, "created offline"),
        (8, 56, "created offline"),
    ] {
        Mock::given(method("POST"))
            .and(path(format!(
                "/index.php/apps/deck/api/v1.0/boards/{BOARD}/stacks/{stack}/cards"
            )))
            .respond_with(ResponseTemplate::new(200).set_body_json(card_json(card, title)))
            .mount(&server)
            .await;
    }
    Mock::given(method("POST"))
        .and(path(format!(
            "/index.php/apps/deck/api/v1.0/boards/{BOARD}/stacks/9/cards"
        )))
        .respond_with(ResponseTemplate::new(503))
        .expect(3)
        .mount(&server)
        .await;
    mount_standard_pull(&server, serde_json::json!([])).await;

    let mut actor = spawn(
        &server,
        repo_with(
            &[(2, STACK), (3, 8), (4, 9)],
            &[(100, 2, STACK), (110, 3, 8), (120, 4, 9)],
        ),
    );
    let report = set_board_and_report(&mut actor).await;
    let SyncReport::Failed { kind, pushes, .. } = report else {
        panic!("expected a Failed report");
    };
    assert_eq!(kind, SyncErrorKind::Network);
    assert_eq!(
        pushes.len(),
        2,
        "the two completed creates are the evidence"
    );
    assert!(
        pushes
            .iter()
            .all(|p| matches!(p.result, PushResult::Applied { .. }))
    );
    assert!(
        !pushes.iter().any(|p| p.op == OpId(uuid(121)))
            && !pushes.iter().any(|p| p.op == OpId(uuid(120))),
        "the aborted op and its neighbours emit no outcomes (decision 8)"
    );
    let () = server.verify().await;
}

/// A7: a transport-class failure broadcasts `NetworkLost` exactly once,
/// failures back off with doubling (the second cycle fires no earlier than
/// the initial backoff — a *simulated*-time fact under the paused clock),
/// and the first success reports `Completed` *before* broadcasting
/// `NetworkRestored`.
#[tokio::test(start_paused = true)]
async fn a7_offline_episode_broadcasts_lost_once_then_restored() {
    let server = start_server().await;
    let down = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let responder_down = down.clone();
    let responder = move |req: &wiremock::Request| {
        if responder_down.load(std::sync::atomic::Ordering::SeqCst) {
            return ResponseTemplate::new(503);
        }
        let p = req.url.path().to_string();
        let body = if p.ends_with("/stacks") {
            stacks_json(&serde_json::json!([]))
        } else if p.ends_with("/stacks/archived") {
            "[]".to_string()
        } else if p.ends_with(&format!("/boards/{BOARD}")) {
            board_json(false, &serde_json::json!([]))
        } else {
            boards_json(false, &serde_json::json!([]))
        };
        ResponseTemplate::new(200)
            .insert_header("ETag", "\"gen-1\"")
            .set_body_string(body)
    };
    Mock::given(method("GET"))
        .respond_with(responder)
        .mount(&server)
        .await;

    let mut actor = spawn(&server, repo_with(&[(2, STACK)], &[]));
    let first = set_board_and_report(&mut actor).await;
    assert!(matches!(first, SyncReport::Completed { .. }));

    // Go down: the nudged cycle fails transport-class and broadcasts once.
    down.store(true, std::sync::atomic::Ordering::SeqCst);
    let lost_at = tokio::time::Instant::now();
    actor.commands.send(SyncCommand::SyncNow).await.unwrap();
    let failed = next_report(&mut actor).await;
    assert!(matches!(
        failed,
        SyncReport::Failed {
            kind: SyncErrorKind::Network,
            ..
        }
    ));
    assert_eq!(
        actor.system_rx.recv().await.expect("broadcast open"),
        SystemEvent::NetworkLost
    );

    // Still down: the backoff cycle fails again WITHOUT re-broadcasting,
    // no earlier than the initial backoff (simulated time).
    let failed2 = next_report(&mut actor).await;
    assert!(matches!(
        failed2,
        SyncReport::Failed {
            kind: SyncErrorKind::Network,
            ..
        }
    ));
    assert!(
        lost_at.elapsed() >= cfg().backoff_initial,
        "the second cycle respected the initial backoff"
    );
    assert!(
        actor.system_rx.try_recv().is_err(),
        "exactly one NetworkLost per offline episode"
    );

    // Recover: the next backoff cycle succeeds, reports Completed first,
    // then broadcasts `NetworkRestored`.
    down.store(false, std::sync::atomic::Ordering::SeqCst);
    let ok = next_report(&mut actor).await;
    assert!(matches!(ok, SyncReport::Completed { .. }));
    assert_eq!(
        actor.system_rx.recv().await.expect("broadcast open"),
        SystemEvent::NetworkRestored
    );
}

/// A8 (multi-board chunk 3): with nothing bound, cycles report
/// `Failed { NoBoard }` — and a commanded target no persisted binding
/// holds does NOT resurrect a cycle: the target-liveness guard evicts
/// it (the ceremonies bind before they nudge; a binding-less `SetBoard`
/// is the operator-error shape and reports honestly).
#[tokio::test(start_paused = true)]
async fn a8_no_board_reported_without_a_persisted_binding() {
    let server = start_server().await;
    let mut actor = spawn(&server, Arc::new(InMemoryRepository::new()));

    actor.commands.send(SyncCommand::SyncNow).await.unwrap();
    let report = next_report(&mut actor).await;
    assert!(matches!(
        report,
        SyncReport::Failed {
            kind: SyncErrorKind::NoBoard,
            ..
        }
    ));

    mount_standard_pull(&server, serde_json::json!([])).await;
    let report = set_board_and_report(&mut actor).await;
    assert!(matches!(
        report,
        SyncReport::Failed {
            kind: SyncErrorKind::NoBoard,
            ..
        }
    ));
    assert!(actor.system_rx.try_recv().is_err(), "no network events");
}

/// A9: archived listings merge into the snapshot with `archived = true` —
/// the listing decides the flag, so the engine never tombstones an
/// archived card as absent.
#[tokio::test(start_paused = true)]
async fn a9_archived_listing_merges_without_tombstoning() {
    let server = start_server().await;
    let mut archived_card = card_json(77, "archived remotely");
    archived_card["archived"] = serde_json::json!(false); // wire lag: the listing decides
    archived_card["stackId"] = serde_json::json!(99);
    Mock::given(method("GET"))
        .and(path("/index.php/apps/deck/api/v1.0/boards"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("ETag", "\"gen-1\"")
                .set_body_string(boards_json(false, &serde_json::json!([]))),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!(
            "/index.php/apps/deck/api/v1.0/boards/{BOARD}"
        )))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("ETag", "\"gen-1\"")
                .set_body_string(board_json(false, &serde_json::json!([]))),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!(
            "/index.php/apps/deck/api/v1.0/boards/{BOARD}/stacks"
        )))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("ETag", "\"gen-1\"")
                .set_body_string(stacks_json(&serde_json::json!([]))),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!(
            "/index.php/apps/deck/api/v1.0/boards/{BOARD}/stacks/archived"
        )))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("ETag", "\"gen-1\"")
                .set_body_string(
                    serde_json::json!([{
                        "id": 99, "title": "old", "boardId": BOARD, "order": 99,
                        "lastModified": T, "deletedAt": 0, "archived": true,
                        "cards": [archived_card]
                    }])
                    .to_string(),
                ),
        )
        .mount(&server)
        .await;

    let mut actor = spawn(&server, repo_with(&[(2, STACK)], &[]));
    let report = set_board_and_report(&mut actor).await;
    let SyncReport::Completed { snapshot, .. } = report else {
        panic!("expected a Completed report");
    };
    let archived = snapshot
        .tasks
        .iter()
        .find(|t| t.title == "archived remotely")
        .expect("the archived card is in the snapshot");
    assert!(archived.archived, "the archived listing decides the flag");
    assert_eq!(archived.id.stack, taskboard_domain::RemoteStackId(99));
}

/// A10: warm validators with a cold cache force one unconditional refetch
/// per 304-ing endpoint — never a partial snapshot. The seeded validator
/// matches the server, so the first cycle's conditional reads all 304.
#[tokio::test(start_paused = true)]
async fn a10_304_with_a_cold_cache_refetches_unconditionally() {
    let server = start_server().await;
    mount_conditional_pull(&server, &serde_json::json!([])).await;
    let repo = repo_with(&[(2, STACK)], &[]);
    for key in [
        taskboard_domain::ValidatorKey::Boards,
        taskboard_domain::ValidatorKey::Stacks(RemoteBoardId(BOARD)),
        taskboard_domain::ValidatorKey::ArchivedStacks(RemoteBoardId(BOARD)),
    ] {
        InMemoryRepository::apply(
            repo.as_ref(),
            vec![taskboard_domain::PersistenceAction::UpsertValidators(
                key,
                taskboard_domain::SyncValidators {
                    etag: Some("\"gen-1\"".into()),
                    last_modified: None,
                },
            )],
        )
        .await
        .expect("apply");
    }

    let mut actor = spawn(&server, repo);
    let report = set_board_and_report(&mut actor).await;
    let SyncReport::Completed {
        snapshot,
        validators,
        ..
    } = report
    else {
        panic!("expected a Completed report");
    };
    assert_eq!(snapshot.board.id, RemoteBoardId(BOARD));
    // 3 conditional 304s + 3 unconditional refetches + 1 board detail.
    assert_eq!(server.received_requests().await.unwrap().len(), 7);
    assert_eq!(
        validators.boards.etag.as_deref(),
        Some("\"gen-1\""),
        "the refetch's validators travel in the report"
    );
}

/// A11: board absence from the listing is confirmed with a GET before a
/// tombstone is synthesized — a 404 yields the delete-wins snapshot.
#[tokio::test(start_paused = true)]
async fn a11_absent_board_is_confirmed_then_tombstoned() {
    let server = start_server().await;
    Mock::given(method("GET"))
        .and(path("/index.php/apps/deck/api/v1.0/boards"))
        .respond_with(ResponseTemplate::new(200).set_body_string(serde_json::json!([]).to_string()))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!(
            "/index.php/apps/deck/api/v1.0/boards/{BOARD}"
        )))
        .respond_with(ResponseTemplate::new(404))
        .expect(1)
        .mount(&server)
        .await;

    let mut actor = spawn(&server, repo_with(&[(2, STACK)], &[]));
    let report = set_board_and_report(&mut actor).await;
    let SyncReport::Completed { snapshot, .. } = report else {
        panic!("expected a Completed report");
    };
    server.verify().await;
    assert_eq!(snapshot.board.id, RemoteBoardId(BOARD));
    assert!(
        snapshot.board.deleted_at.is_some(),
        "the synthesized board carries the delete-wins tombstone"
    );
    assert!(snapshot.stacks.is_empty() && snapshot.tasks.is_empty());
}

/// A12: `Shutdown` exits the loop cleanly (the `JoinHandle` completes).
/// The test waits for the actor's broadcast *subscription* (a structural
/// fact, not a timed one) before sending.
#[tokio::test(start_paused = true)]
async fn a12_shutdown_exits_the_loop() {
    let server = start_server().await;
    let actor = spawn(&server, repo_with(&[(2, STACK)], &[]));
    wait_subscribed(&actor.system_tx).await;
    actor.system_tx.send(SystemEvent::Shutdown).unwrap();
    for _ in 0..10_000 {
        if actor.join.is_finished() {
            break;
        }
        tokio::task::yield_now().await;
    }
    assert!(
        actor.join.is_finished(),
        "actor exits on Shutdown without any timer"
    );
    actor.join.await.expect("actor exits without panicking");
}

/// A13: a stack create and a card create in that stack push in dependency
/// order — the stack POST precedes the card POST, which uses the echo's
/// stack id.
#[tokio::test(start_paused = true)]
async fn a13_stack_create_precedes_card_create() {
    let server = start_server().await;
    let mut state = PersistedState::default();
    let board = bound_board();
    let stack = taskboard_domain::Stack {
        id: taskboard_domain::StackId::from(uuid(5)),
        remote: None,
        board: board.id,
        title: "new col".into(),
        order: 1,
        archived: false,
        deleted: false,
        clocks: stack_clocks(),
        remote_seen: None,
    };
    let (task, task_op) = draft(100, 5, "draft");
    state.boards.insert(board.id, board);
    state.stacks.insert(stack.id, stack);
    state.tasks.insert(task.id, task);
    state.outbox.push(PendingOp {
        op_id: OpId(uuid(300)),
        op: LocalOp::CreateStack(taskboard_domain::StackId::from(uuid(5))),
        queued_at: ts(T),
    });
    state.outbox.push(task_op);
    let repo = Arc::new(InMemoryRepository::with_state(state));

    Mock::given(method("POST"))
        .and(path(format!(
            "/index.php/apps/deck/api/v1.0/boards/{BOARD}/stacks"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": 21, "title": "new col", "boardId": BOARD, "order": 1,
            "lastModified": T, "deletedAt": 0, "archived": false, "cards": []
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(format!(
            "/index.php/apps/deck/api/v1.0/boards/{BOARD}/stacks/21/cards"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": 66, "title": "draft", "stackId": 21, "type": "plain",
            "order": 0, "lastModified": T, "labels": [], "archived": false,
            "duedate": null, "done": null
        })))
        .mount(&server)
        .await;
    mount_standard_pull(&server, serde_json::json!([])).await;

    let mut actor = spawn(&server, repo);
    let report = set_board_and_report(&mut actor).await;
    let SyncReport::Completed { pushes, .. } = report else {
        panic!("expected a Completed report");
    };
    assert_eq!(pushes.len(), 2, "both creates pushed");

    let requests = server.received_requests().await.unwrap();
    let stack_post = requests
        .iter()
        .position(|r| {
            r.method.as_str() == "POST"
                && r.url.path().ends_with(&format!("/boards/{BOARD}/stacks"))
        })
        .expect("stack POST present");
    let card_post = requests
        .iter()
        .position(|r| r.url.path().ends_with("/stacks/21/cards"))
        .expect("card POST present");
    assert!(
        stack_post < card_post,
        "stack POST must precede the card POST"
    );
}

/// A5+A6 (actor half): the typed rejection outcomes travel in the report —
/// a 400 PUT surfaces as `Rejected { BadRequest }` (whose dead-lettering
/// the engine applies, and which the I-series then asserts as the visible
/// `Failed` phase). The `NotFound` → `RemoteMissing` classification for a
/// pre-flight GET is pinned at the executor layer
/// (`preflight_404_on_an_update_maps_to_remote_missing`).
#[tokio::test(start_paused = true)]
async fn a5_a6_rejection_outcomes_travel_in_the_report() {
    let server = start_server().await;
    let repo = repo_with(&[(2, STACK)], &[]);
    // A task bound to card 55 with a pending update op (and nothing else).
    let mut state = repo.snapshot();
    let stack_remote = state
        .stacks
        .values()
        .next()
        .and_then(|s| s.remote)
        .expect("bound stack");
    let (task, _) = draft(100, 2, "created offline");
    let task_id = task.id;
    let mut task = task;
    task.remote = Some(taskboard_domain::RemoteCardRef {
        board: RemoteBoardId(BOARD),
        stack: stack_remote.stack,
        card: taskboard_domain::RemoteCardId(55),
    });
    state.tasks.insert(task.id, task);
    state.outbox.push(PendingOp {
        op_id: OpId(uuid(400)),
        op: LocalOp::UpdateTask(task_id),
        queued_at: ts(T),
    });
    let repo = Arc::new(InMemoryRepository::with_state(state));

    // The pre-flight GET succeeds; the PUT answers 400.
    Mock::given(method("GET"))
        .and(path(format!(
            "/index.php/apps/deck/api/v1.0/boards/{BOARD}/stacks/{STACK}/cards/55"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(card_json(55, "created offline")))
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path(format!(
            "/index.php/apps/deck/api/v1.0/boards/{BOARD}/stacks/{STACK}/cards/55"
        )))
        .respond_with(ResponseTemplate::new(400))
        .mount(&server)
        .await;
    mount_standard_pull(&server, serde_json::json!([])).await;

    let mut actor = spawn(&server, repo);
    let report = set_board_and_report(&mut actor).await;
    let SyncReport::Completed { pushes, .. } = report else {
        panic!("expected a Completed report");
    };
    assert!(
        pushes.iter().any(|p| matches!(
            p.result,
            PushResult::Rejected {
                kind: SyncErrorKind::BadRequest
            }
        )),
        "the 400 PUT is a typed BadRequest rejection: {pushes:?}"
    );
}

/// A14: a `Retry-After` advertisement on the exhausting 429 becomes a
/// floor on the *autonomous* backoff deadline. The advertised 2 s sits
/// strictly between the 100 ms initial backoff and the 30 s cap — the
/// discriminating zone. The observable is the server-side request count
/// over paused time: within the window before the advertised deadline
/// the only limited requests are the first cycle's exhausted retries
/// (3); a floor-ignoring implementation would fire the next cycle after
/// the 100 ms curve and add more. Flipping healthy lets the cycle
/// succeed; the floor is gone with the streak (success schedules
/// `poll_interval`).
#[tokio::test(start_paused = true)]
async fn a14_rate_limited_poll_waits_the_advertised_retry_after() {
    let cfg_rate = SyncActorConfig {
        poll_interval: Duration::from_secs(3_600),
        backoff_initial: Duration::from_millis(100),
        backoff_max: Duration::from_secs(30),
        clock: Arc::new(FixedClock(ts(T + 1000))),
    };
    let server = start_server().await;
    let limited = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let limited_hits = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let responder_limited = limited.clone();
    let responder_hits = limited_hits.clone();
    let responder = move |req: &wiremock::Request| {
        if responder_limited.load(std::sync::atomic::Ordering::SeqCst) {
            responder_hits.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            return ResponseTemplate::new(429).insert_header("Retry-After", "2");
        }
        let p = req.url.path().to_string();
        let body = if p.ends_with("/stacks") {
            stacks_json(&serde_json::json!([]))
        } else if p.ends_with("/stacks/archived") {
            "[]".to_string()
        } else if p.ends_with(&format!("/boards/{BOARD}")) {
            board_json(false, &serde_json::json!([]))
        } else {
            boards_json(false, &serde_json::json!([]))
        };
        ResponseTemplate::new(200)
            .insert_header("ETag", "\"gen-1\"")
            .set_body_string(body)
    };
    Mock::given(method("GET"))
        .respond_with(responder)
        .mount(&server)
        .await;

    let mut actor = spawn_with_cfg(&server, repo_with(&[(2, STACK)], &[]), cfg_rate);
    let first = set_board_and_report(&mut actor).await;
    assert!(matches!(first, SyncReport::Completed { .. }));

    limited.store(true, std::sync::atomic::Ordering::SeqCst);
    actor.commands.send(SyncCommand::SyncNow).await.unwrap();
    let failed = next_report(&mut actor).await;
    assert!(matches!(
        failed,
        SyncReport::Failed {
            kind: SyncErrorKind::Network,
            ..
        }
    ));
    assert_eq!(
        actor.system_rx.recv().await.expect("broadcast open"),
        SystemEvent::NetworkLost
    );
    let first_failure_at = tokio::time::Instant::now();
    let retries_exhausted = limited_hits.load(std::sync::atomic::Ordering::SeqCst);

    // Walk the window before the advertised deadline in explicit 10 ms
    // steps: each step keeps the runtime's earliest timer bounded, so the
    // cycle's 2 s backoff deadline (past the window) can never fire
    // mid-check. A floor-ignoring implementation fires the next cycle at
    // the 100 ms curve and adds limited requests inside this window.
    let window_end = first_failure_at + Duration::from_millis(1_900);
    while tokio::time::Instant::now() < window_end {
        tokio::time::advance(Duration::from_millis(10)).await;
    }
    assert_eq!(
        limited_hits.load(std::sync::atomic::Ordering::SeqCst),
        retries_exhausted,
        "no still-limited cycle before the advertised Retry-After elapsed"
    );

    // Still limited: the backoff cycle fires at/after the advertised
    // deadline and exhausts the retry budget again.
    let retried = next_report(&mut actor).await;
    assert!(matches!(
        retried,
        SyncReport::Failed {
            kind: SyncErrorKind::Network,
            ..
        }
    ));
    assert!(
        limited_hits.load(std::sync::atomic::Ordering::SeqCst) > retries_exhausted,
        "the still-limited cycle did hit the server"
    );

    // Healthy again: the next backoff cycle succeeds.
    limited.store(false, std::sync::atomic::Ordering::SeqCst);
    let ok = next_report(&mut actor).await;
    assert!(matches!(ok, SyncReport::Completed { .. }));
}

/// A15: a listed board whose *detail* answers a non-403/404 error (a hard
/// 500) fails the whole cycle — `Failed { Server }`, no snapshot, hence no
/// tombstone. A dead-server condition must never declare the network lost.
#[tokio::test(start_paused = true)]
async fn a15_detail_500_fails_the_cycle_without_a_tombstone() {
    let server = start_server().await;
    Mock::given(method("GET"))
        .and(path("/index.php/apps/deck/api/v1.0/boards"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(boards_json(false, &serde_json::json!([]))),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!(
            "/index.php/apps/deck/api/v1.0/boards/{BOARD}"
        )))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;

    let mut actor = spawn(&server, repo_with(&[(2, STACK)], &[]));
    let report = set_board_and_report(&mut actor).await;
    assert!(
        matches!(
            report,
            SyncReport::Failed {
                kind: SyncErrorKind::Server,
                ..
            }
        ),
        "a hard detail 500 fails the cycle as Server"
    );
    assert_eq!(
        server.received_requests().await.unwrap().len(),
        2,
        "the cycle stopped at the detail failure: listing + detail, no stacks reads"
    );
    assert!(
        actor.system_rx.try_recv().is_err(),
        "a dead server is not a lost network"
    );
}

/// A16: a board still present in the (lagging) listing whose detail
/// answers 403 is dead — the tombstone is synthesized from the listing
/// entry and the cycle *completes*. The stacks endpoints answer healthy
/// 200s (empty listings), so the 403 detail is the only death signal in
/// the scenario.
#[tokio::test(start_paused = true)]
async fn a16_forbidden_detail_while_listed_synthesizes_the_tombstone() {
    let server = start_server().await;
    Mock::given(method("GET"))
        .and(path("/index.php/apps/deck/api/v1.0/boards"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(boards_json(false, &serde_json::json!([]))),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!(
            "/index.php/apps/deck/api/v1.0/boards/{BOARD}"
        )))
        .respond_with(ResponseTemplate::new(403))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!(
            "/index.php/apps/deck/api/v1.0/boards/{BOARD}/stacks"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_string("[]"))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!(
            "/index.php/apps/deck/api/v1.0/boards/{BOARD}/stacks/archived"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_string("[]"))
        .mount(&server)
        .await;

    let mut actor = spawn(&server, repo_with(&[(2, STACK)], &[]));
    let report = set_board_and_report(&mut actor).await;
    let SyncReport::Completed { snapshot, .. } = report else {
        panic!("expected a Completed report: the tombstone snapshot is valid");
    };
    assert_eq!(snapshot.board.id, RemoteBoardId(BOARD));
    assert!(
        snapshot.board.deleted_at.is_some(),
        "the synthesized board carries the delete-wins tombstone"
    );
    assert!(snapshot.stacks.is_empty() && snapshot.tasks.is_empty());
    assert_eq!(
        server.received_requests().await.unwrap().len(),
        4,
        "listing, detail, and both stacks reads: the pull ran through the dead-board path"
    );
}

/// A17: a 404 stacks listing on a *live* board (the detail answered) must
/// propagate — the dead-board carve-out's `if dead_board` conjunct forbids
/// carving it to an empty board, which would merge-tombstone a live
/// board's whole task set.
#[tokio::test(start_paused = true)]
async fn a17_stacks_404_on_a_live_board_fails_the_cycle() {
    let server = start_server().await;
    Mock::given(method("GET"))
        .and(path("/index.php/apps/deck/api/v1.0/boards"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(boards_json(false, &serde_json::json!([]))),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!(
            "/index.php/apps/deck/api/v1.0/boards/{BOARD}"
        )))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(board_json(false, &serde_json::json!([]))),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!(
            "/index.php/apps/deck/api/v1.0/boards/{BOARD}/stacks"
        )))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    let mut actor = spawn(&server, repo_with(&[(2, STACK)], &[]));
    let report = set_board_and_report(&mut actor).await;
    assert!(
        matches!(
            report,
            SyncReport::Failed {
                kind: SyncErrorKind::Server,
                ..
            }
        ),
        "a read-side 404 on a live board fails the cycle, never completes empty"
    );
    assert_eq!(
        server.received_requests().await.unwrap().len(),
        3,
        "listing, detail, active stacks — the archived read is never reached"
    );
    assert!(
        actor.system_rx.try_recv().is_err(),
        "a dead server is not a lost network"
    );
}

/// A18: the issue #258 regression — a pre-write fetch 404 on a card
/// another client *moved* (the binding's stack component is stale) must
/// never classify `RemoteMissing`: the executor re-resolves the card from
/// the fresh listing and the queued edit lands at the fresh location.
/// Today's code tombstones the task, cancels the edit, and the same
/// cycle's pull re-adopts the card as a twin — the exact silent loss.
#[allow(clippy::too_many_lines)] // fixture seeding is the scenario
#[tokio::test(start_paused = true)]
async fn a18_remote_moved_card_survives_its_queued_edit() {
    // Remote truth: card 55 lives under stack 8; our binding says stack 7.
    let moved_listing = serde_json::json!([{
        "id": 8, "title": "moved-here", "boardId": BOARD, "order": 1,
        "lastModified": T, "deletedAt": 0, "archived": false,
        "cards": [{ "id": 55, "title": "remote title", "stackId": 8,
                    "type": "plain", "order": 3, "labels": [],
                    "archived": false, "duedate": null, "done": null }]
    }]);
    let fresh_detail = serde_json::json!({ "id": 55, "title": "remote title",
        "stackId": 8, "type": "plain", "order": 3, "labels": [],
        "archived": false, "duedate": null, "done": null });

    let server = start_server().await;
    for (p, body) in [
        (
            "/index.php/apps/deck/api/v1.0/boards".to_string(),
            boards_json(false, &serde_json::json!([])),
        ),
        (
            format!("/index.php/apps/deck/api/v1.0/boards/{BOARD}"),
            board_json(false, &serde_json::json!([])),
        ),
        (
            format!("/index.php/apps/deck/api/v1.0/boards/{BOARD}/stacks"),
            moved_listing.to_string(),
        ),
        (
            format!("/index.php/apps/deck/api/v1.0/boards/{BOARD}/stacks/archived"),
            "[]".to_string(),
        ),
    ] {
        Mock::given(method("GET"))
            .and(path(p))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("ETag", "\"gen-1\"")
                    .set_body_string(body),
            )
            .mount(&server)
            .await;
    }
    // The stale pre-write fetch; the re-resolve's fresh detail; the PUT.
    Mock::given(method("GET"))
        .and(path(format!(
            "/index.php/apps/deck/api/v1.0/boards/{BOARD}/stacks/{STACK}/cards/55"
        )))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!(
            "/index.php/apps/deck/api/v1.0/boards/{BOARD}/stacks/8/cards/55"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(fresh_detail.clone()))
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path(format!(
            "/index.php/apps/deck/api/v1.0/boards/{BOARD}/stacks/8/cards/55"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": 55, "title": "edited locally", "stackId": 8,
            "type": "plain", "order": 5, "labels": [], "archived": false,
            "duedate": null, "done": null
        })))
        .mount(&server)
        .await;

    // Local state: card 55 bound at (BOARD, 7, 55), an UpdateTask queued.
    let mut state = PersistedState::default();
    state.boards.insert(bound_board().id, bound_board());
    let stack_a = bound_stack(2, STACK);
    state.stacks.insert(stack_a.id, stack_a);
    let stack_b = bound_stack(3, 8);
    state.stacks.insert(stack_b.id, stack_b);
    let mut task = Task {
        id: taskboard_domain::TaskId::from(uuid(10)),
        remote: Some(taskboard_domain::RemoteCardRef {
            board: RemoteBoardId(BOARD),
            stack: taskboard_domain::RemoteStackId(STACK),
            card: taskboard_domain::RemoteCardId(55),
        }),
        title: "edited locally".into(),
        description: String::new(),
        duedate: None,
        done: None,
        stack: taskboard_domain::StackId::from(uuid(2)),
        order: 5,
        labels: std::collections::BTreeSet::new(),
        archived: false,
        deleted: false,
        clocks: task_clocks(),
        remote_seen: Some(ts(T)),
    };
    task.clocks.title = ts(T + 1);
    state.tasks.insert(task.id, task);
    state.outbox.push(PendingOp {
        op_id: OpId(uuid(11)),
        op: LocalOp::UpdateTask(taskboard_domain::TaskId::from(uuid(10))),
        queued_at: ts(T),
    });

    let mut actor = spawn(&server, Arc::new(InMemoryRepository::with_state(state)));
    actor.commands.send(SyncCommand::SyncNow).await.unwrap();
    let SyncReport::Completed {
        snapshot, pushes, ..
    } = next_report(&mut actor).await
    else {
        panic!("expected a Completed report");
    };

    assert!(
        !pushes
            .iter()
            .any(|outcome| matches!(outcome.result, PushResult::RemoteMissing)),
        "a stale-binding 404 must never classify RemoteMissing: {pushes:?}"
    );
    assert!(
        pushes.iter().any(|outcome| {
            matches!(
                &outcome.result,
                PushResult::Applied { echo: Some(RemoteEcho::Task(view)) }
                    if view.id.stack == taskboard_domain::RemoteStackId(8)
                        && view.id.card == taskboard_domain::RemoteCardId(55)
            )
        }),
        "the edit landed at the card's fresh location: {pushes:?}"
    );
    let view = snapshot
        .tasks
        .iter()
        .find(|t| t.id.card == taskboard_domain::RemoteCardId(55))
        .expect("the moved card is in the post-push snapshot");
    assert_eq!(
        view.id.stack,
        taskboard_domain::RemoteStackId(8),
        "the snapshot reports the card under its fresh stack"
    );
}

// ---- the injected clock seam (#259) ---------------------------------

/// The instant every actor-minted stamp carries under `cfg()`: strictly
/// after the fixtures' base stamp, so today's real-clock orderings hold.
fn stamped() -> chrono::DateTime<chrono::Utc> {
    ts(T + 1000)
}

/// A19: the dead-board tombstone's stamps come from the injected clock —
/// the observation instant, not an untestable wall read (the tombstone
/// must win every LWW against the fixture stamps).
#[tokio::test(start_paused = true)]
async fn a19_dead_board_tombstone_stamps_come_from_the_injected_clock() {
    let server = start_server().await;
    Mock::given(method("GET"))
        .and(path("/index.php/apps/deck/api/v1.0/boards"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(boards_json(false, &serde_json::json!([]))),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!(
            "/index.php/apps/deck/api/v1.0/boards/{BOARD}"
        )))
        .respond_with(ResponseTemplate::new(403))
        .mount(&server)
        .await;
    for p in [
        format!("/index.php/apps/deck/api/v1.0/boards/{BOARD}/stacks"),
        format!("/index.php/apps/deck/api/v1.0/boards/{BOARD}/stacks/archived"),
    ] {
        Mock::given(method("GET"))
            .and(path(p))
            .respond_with(ResponseTemplate::new(200).set_body_string("[]"))
            .mount(&server)
            .await;
    }

    let mut actor = spawn(&server, repo_with(&[(2, STACK)], &[]));
    let report = set_board_and_report(&mut actor).await;
    let SyncReport::Completed { snapshot, .. } = report else {
        panic!("expected a Completed report: the tombstone snapshot is valid");
    };
    assert_eq!(snapshot.board.deleted_at, Some(stamped()));
    assert!(
        snapshot.board.last_modified >= stamped(),
        "the tombstone is floored at the observation instant"
    );
}

/// A20: the report envelope's `read_at` comes from the injected clock —
/// both the Completed path (a18 exercises it incidentally) and the
/// `NoBoard` failure path, asserted here in isolation.
#[tokio::test(start_paused = true)]
async fn a20_report_read_at_comes_from_the_injected_clock() {
    let server = start_server().await;
    let mut actor = spawn(&server, Arc::new(InMemoryRepository::new()));
    actor.commands.send(SyncCommand::SyncNow).await.unwrap();
    let report = next_report(&mut actor).await;
    let SyncReport::Failed { read_at, .. } = report else {
        panic!("expected the NoBoard failure report");
    };
    assert_eq!(read_at, stamped());
}

/// A21: an archive-flag mismatch is stamped with the observation instant
/// from the injected clock (the server offers no version stamp for the
/// flag — "observed now" is the only truthful recency).
#[tokio::test(start_paused = true)]
async fn a21_archive_mismatch_stamp_uses_the_injected_clock() {
    let server = start_server().await;
    let mut archived_card = card_json(55, "archived remotely");
    archived_card["archived"] = serde_json::json!(true);
    mount_standard_pull(&server, serde_json::json!([archived_card.clone()])).await;
    Mock::given(method("GET"))
        .and(path(format!(
            "/index.php/apps/deck/api/v1.0/boards/{BOARD}/stacks/{STACK}/cards/55"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(archived_card))
        .mount(&server)
        .await;

    // One bound task, locally not archived — the detail refresh's mismatch.
    let mut state = PersistedState::default();
    state.boards.insert(bound_board().id, bound_board());
    let stack = bound_stack(2, STACK);
    state.stacks.insert(stack.id, stack);
    let task = Task {
        id: taskboard_domain::TaskId::from(uuid(10)),
        remote: Some(taskboard_domain::RemoteCardRef {
            board: RemoteBoardId(BOARD),
            stack: taskboard_domain::RemoteStackId(STACK),
            card: taskboard_domain::RemoteCardId(55),
        }),
        title: "bound".into(),
        description: String::new(),
        duedate: None,
        done: None,
        stack: taskboard_domain::StackId::from(uuid(2)),
        order: 0,
        labels: std::collections::BTreeSet::new(),
        archived: false,
        deleted: false,
        clocks: task_clocks(),
        remote_seen: Some(ts(T)),
    };
    state.tasks.insert(task.id, task);

    let mut actor = spawn(&server, Arc::new(InMemoryRepository::with_state(state)));
    let report = set_board_and_report(&mut actor).await;
    let SyncReport::Completed { snapshot, .. } = report else {
        panic!("expected a Completed report");
    };
    let view = snapshot
        .tasks
        .iter()
        .find(|t| t.id.card == taskboard_domain::RemoteCardId(55))
        .expect("the bound card is in the snapshot");
    assert!(view.archived, "the authoritative detail settles the flag");
    assert_eq!(
        view.last_modified,
        stamped(),
        "the mismatch correction is stamped with the injected observation instant"
    );
}

/// A22: the issue #319 regression — a delete pushed at a stale binding
/// (another client moved the card) 404s, and the pre-fix code classified
/// `RemoteMissing`: the tombstone cleared the binding and the same
/// cycle's pull re-adopted the still-live card as a twin, silently
/// losing the delete intent. Now the delete re-resolves once from the
/// fresh listing and lands at the fresh stack; the pull adopts nothing.
#[allow(clippy::too_many_lines)] // fixture seeding is the scenario
#[tokio::test(start_paused = true)]
async fn a22_remote_moved_card_delete_lands_without_a_twin() {
    let server = start_server().await;
    for (p, body) in [
        (
            "/index.php/apps/deck/api/v1.0/boards".to_string(),
            boards_json(false, &serde_json::json!([])),
        ),
        (
            format!("/index.php/apps/deck/api/v1.0/boards/{BOARD}"),
            board_json(false, &serde_json::json!([])),
        ),
    ] {
        Mock::given(method("GET"))
            .and(path(p))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("ETag", "\"gen-1\"")
                    .set_body_string(body),
            )
            .mount(&server)
            .await;
    }
    // The stacks listing is call-counted: the first serving (the push's
    // re-resolve) still carries card 55 under stack 8; later servings
    // (the pull) omit it — the delete landed. Distinct ETags per serving
    // so the pull's conditional reads always miss the cache.
    let servings = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let listing = servings.clone();
    Mock::given(method("GET"))
        .and(path(format!(
            "/index.php/apps/deck/api/v1.0/boards/{BOARD}/stacks"
        )))
        .respond_with(move |_req: &wiremock::Request| {
            let n = listing.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let body = if n == 0 {
                serde_json::json!([
                    { "id": 7, "title": "col", "boardId": BOARD, "order": 0,
                      "lastModified": T, "deletedAt": 0, "archived": false,
                      "cards": [] },
                    { "id": 8, "title": "moved-here", "boardId": BOARD, "order": 1,
                      "lastModified": T, "deletedAt": 0, "archived": false,
                      "cards": [{ "id": 55, "title": "remote title", "stackId": 8,
                                  "type": "plain", "order": 3, "labels": [],
                                  "archived": false, "duedate": null, "done": null }] }]
                )
            } else {
                serde_json::json!([
                    { "id": 7, "title": "col", "boardId": BOARD, "order": 0,
                      "lastModified": T, "deletedAt": 0, "archived": false,
                      "cards": [] },
                    { "id": 8, "title": "moved-here", "boardId": BOARD, "order": 1,
                      "lastModified": T, "deletedAt": 0, "archived": false,
                      "cards": [] }]
                )
            };
            ResponseTemplate::new(200)
                .insert_header("ETag", format!("\"gen-{n}\""))
                .set_body_json(body)
        })
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!(
            "/index.php/apps/deck/api/v1.0/boards/{BOARD}/stacks/archived"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_string("[]"))
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path(format!(
            "/index.php/apps/deck/api/v1.0/boards/{BOARD}/stacks/{STACK}/cards/55"
        )))
        .respond_with(ResponseTemplate::new(404))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path(format!(
            "/index.php/apps/deck/api/v1.0/boards/{BOARD}/stacks/8/cards/55"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": 55, "title": "remote title", "stackId": 8,
            "type": "plain", "order": 3, "labels": [], "archived": false,
            "duedate": null, "done": null
        })))
        .expect(1)
        .mount(&server)
        .await;

    // Local state: card 55 bound at (BOARD, 7, 55), a DeleteTask queued.
    let mut state = PersistedState::default();
    state.boards.insert(bound_board().id, bound_board());
    let stack_a = bound_stack(2, STACK);
    state.stacks.insert(stack_a.id, stack_a);
    let stack_b = bound_stack(3, 8);
    state.stacks.insert(stack_b.id, stack_b);
    let task = Task {
        id: taskboard_domain::TaskId::from(uuid(10)),
        remote: Some(taskboard_domain::RemoteCardRef {
            board: RemoteBoardId(BOARD),
            stack: taskboard_domain::RemoteStackId(STACK),
            card: taskboard_domain::RemoteCardId(55),
        }),
        title: "deleted locally".into(),
        description: String::new(),
        duedate: None,
        done: None,
        stack: taskboard_domain::StackId::from(uuid(2)),
        order: 3,
        labels: std::collections::BTreeSet::new(),
        archived: false,
        deleted: false,
        clocks: task_clocks(),
        remote_seen: Some(ts(T)),
    };
    state.tasks.insert(task.id, task);
    state.outbox.push(PendingOp {
        op_id: OpId(uuid(11)),
        op: LocalOp::DeleteTask(taskboard_domain::TaskId::from(uuid(10))),
        queued_at: ts(T),
    });

    let mut actor = spawn(&server, Arc::new(InMemoryRepository::with_state(state)));
    actor.commands.send(SyncCommand::SyncNow).await.unwrap();
    let SyncReport::Completed {
        snapshot, pushes, ..
    } = next_report(&mut actor).await
    else {
        panic!("expected a Completed report");
    };

    assert!(
        !pushes
            .iter()
            .any(|outcome| matches!(outcome.result, PushResult::RemoteMissing)),
        "a moved card must not read as missing on the delete path: {pushes:?}"
    );
    assert!(
        pushes.iter().any(|outcome| {
            matches!(
                &outcome.result,
                PushResult::Applied { echo: Some(RemoteEcho::Task(view)) }
                    if view.id.stack == taskboard_domain::RemoteStackId(8)
                        && view.id.card == taskboard_domain::RemoteCardId(55)
            )
        }),
        "the delete landed at the card's fresh location: {pushes:?}"
    );
    assert!(
        !snapshot
            .tasks
            .iter()
            .any(|t| t.id.card == taskboard_domain::RemoteCardId(55)),
        "the tombstone finalized and nothing re-adopted the deleted card"
    );
}

// ---- the #242 gap set (F6-04/05/06/23/24) ------------------------------

/// A23 (F6-04): the board *detail*'s labels reach the snapshot's label set —
/// the tier-2 fact the listings carry `labels: []` makes the detail the
/// only authoritative label source, so dropping `inline_labels` (the
/// audit's survivor mutant) must fail here.
#[tokio::test(start_paused = true)]
async fn a23_pulled_board_labels_reach_the_snapshot() {
    let server = start_server().await;
    let labels = serde_json::json!([{ "id": 1, "title": "todo", "color": "ff0000" }]);
    mount_conditional_pull(&server, &labels).await;

    let mut actor = spawn(&server, repo_with(&[(2, STACK)], &[]));
    let report = set_board_and_report(&mut actor).await;
    let SyncReport::Completed { snapshot, .. } = report else {
        panic!("expected a Completed report");
    };
    assert_eq!(snapshot.labels.len(), 1, "the pulled label set is complete");
    let label = &snapshot.labels[0];
    assert_eq!(label.id.board, RemoteBoardId(BOARD));
    assert_eq!(label.id.label, taskboard_domain::RemoteLabelId(1));
    assert_eq!(label.title, "todo");
    assert_eq!(label.color, "ff0000");
}

/// The two-board conditional pull (boards 42 "alpha board" and 43
/// "beta board", stacks pairwise distinct): 304 iff the request re-emits
/// the `ETag`, else fresh data. Shared by A24 and A25; the shared
/// hermetic helpers are single-board by design, so this stays in-file.
async fn mount_two_board_pull(server: &MockServer) {
    let responder = move |req: &wiremock::Request| {
        let warm = req
            .headers
            .get("If-None-Match")
            .is_some_and(|v| v.to_str().unwrap_or("").contains("gen-1"));
        if warm {
            return ResponseTemplate::new(304).insert_header("ETag", "\"gen-1\"");
        }
        let p = req.url.path().to_string();
        let beta = p.contains("/boards/43");
        let body = if p.ends_with("/stacks/archived") {
            "[]".to_string()
        } else if p.ends_with("/stacks") {
            serde_json::json!([{
                "id": if beta { 70 } else { 7 },
                "title": if beta { "beta col" } else { "alpha col" },
                "boardId": if beta { 43 } else { BOARD },
                "order": 0, "lastModified": T, "deletedAt": 0,
                "archived": false, "cards": []
            }])
            .to_string()
        } else if p.ends_with("/boards") {
            serde_json::json!([
                { "id": BOARD, "title": "alpha board", "color": "00ff00",
                  "lastModified": T, "deletedAt": 0, "archived": false,
                  "labels": [] },
                { "id": 43, "title": "beta board", "color": "0000ff",
                  "lastModified": T, "deletedAt": 0, "archived": false,
                  "labels": [] }
            ])
            .to_string()
        } else {
            // Board detail (42 or 43).
            serde_json::json!({
                "id": if beta { 43 } else { BOARD },
                "title": if beta { "beta board" } else { "alpha board" },
                "color": if beta { "0000ff" } else { "00ff00" },
                "lastModified": T, "deletedAt": 0,
                "archived": false, "labels": []
            })
            .to_string()
        };
        ResponseTemplate::new(200)
            .insert_header("ETag", "\"gen-1\"")
            .set_body_string(body)
    };
    for p in [
        "/index.php/apps/deck/api/v1.0/boards".to_string(),
        format!("/index.php/apps/deck/api/v1.0/boards/{BOARD}"),
        "/index.php/apps/deck/api/v1.0/boards/43".to_string(),
        format!("/index.php/apps/deck/api/v1.0/boards/{BOARD}/stacks"),
        "/index.php/apps/deck/api/v1.0/boards/43/stacks".to_string(),
        format!("/index.php/apps/deck/api/v1.0/boards/{BOARD}/stacks/archived"),
        "/index.php/apps/deck/api/v1.0/boards/43/stacks/archived".to_string(),
    ] {
        Mock::given(method("GET"))
            .and(path(p))
            .respond_with(responder)
            .mount(server)
            .await;
    }
}

/// A24 (F6-05): switching A→B→A serves A again — the `SetBoard` cache
/// clear must not leave board B's cached listings in the slots for board
/// A's 304s. Validators are pinned warm exactly like A10, so every cycle
/// 304s; with the clear, board A's re-bind forces unconditional
/// refetches (7 requests in the final cycle) and the snapshot carries
/// alpha's stack, never beta's.
#[tokio::test(start_paused = true)]
async fn a24_board_switch_serves_the_new_board_after_the_cache_clear() {
    let server = start_server().await;
    mount_two_board_pull(&server).await;
    let repo = repo_with(&[(2, STACK)], &[]);
    // The guard (chunk 3) validates targets against persisted bindings:
    // board 43 needs its own board row to be switchable.
    let mut second = bound_board();
    second.id = taskboard_domain::BoardId::from(uuid(9));
    second.remote = Some(RemoteBoardId(43));
    InMemoryRepository::apply(
        repo.as_ref(),
        vec![taskboard_domain::PersistenceAction::UpsertBoard(second)],
    )
    .await
    .expect("seed board 43");
    for key in [
        taskboard_domain::ValidatorKey::Boards,
        taskboard_domain::ValidatorKey::Stacks(RemoteBoardId(BOARD)),
        taskboard_domain::ValidatorKey::ArchivedStacks(RemoteBoardId(BOARD)),
    ] {
        InMemoryRepository::apply(
            repo.as_ref(),
            vec![taskboard_domain::PersistenceAction::UpsertValidators(
                key,
                taskboard_domain::SyncValidators {
                    etag: Some("\"gen-1\"".into()),
                    last_modified: None,
                },
            )],
        )
        .await
        .expect("apply");
    }

    let mut actor = spawn(&server, repo);
    actor
        .commands
        .send(SyncCommand::SetBoard(RemoteBoardId(BOARD)))
        .await
        .unwrap();
    let first = next_report(&mut actor).await;
    let SyncReport::Completed {
        snapshot: first, ..
    } = first
    else {
        panic!("expected a Completed report");
    };
    assert_eq!(first.board.id, RemoteBoardId(BOARD));

    actor
        .commands
        .send(SyncCommand::SetBoard(RemoteBoardId(43)))
        .await
        .unwrap();
    let second = next_report(&mut actor).await;
    let SyncReport::Completed {
        snapshot: second, ..
    } = second
    else {
        panic!("expected a Completed report");
    };
    assert_eq!(second.board.id, RemoteBoardId(43));
    assert!(
        second.stacks.iter().all(|s| s.title == "beta col"),
        "board 43 serves beta: {:?}",
        second.stacks
    );

    let before_switch = server.received_requests().await.unwrap().len();
    actor
        .commands
        .send(SyncCommand::SetBoard(RemoteBoardId(BOARD)))
        .await
        .unwrap();
    let third = next_report(&mut actor).await;
    let SyncReport::Completed { snapshot, .. } = third else {
        panic!("expected a Completed report");
    };
    assert_eq!(snapshot.board.id, RemoteBoardId(BOARD));
    assert_eq!(
        snapshot
            .stacks
            .iter()
            .map(|s| s.title.as_str())
            .collect::<Vec<_>>(),
        vec!["alpha col"],
        "board 42's snapshot after the switch back is alpha's — no beta bleed"
    );
    assert_eq!(
        server.received_requests().await.unwrap().len() - before_switch,
        7,
        "the clear emptied the cache: 3 conditional 304s + 3 refetches + the detail"
    );
}

/// A25 (F6-06a): a `SetBoard` queued while a cycle runs is absorbed by the
/// ADR 0006 drain loop into exactly one switch — one rerun targeting the
/// new board, then silence. Mutating the `st.target != Some(id)` guard or
/// dropping the rerun flag strands the switch and report 3 never arrives.
#[tokio::test(start_paused = true)]
async fn a25_set_board_during_a_cycle_is_absorbed_into_one_switch() {
    let server = start_server().await;
    mount_two_board_pull(&server).await;

    let repo = repo_with(&[(2, STACK)], &[]);
    // The guard (chunk 3) validates targets against persisted bindings:
    // board 43 needs its own board row to be switchable.
    let mut second = bound_board();
    second.id = taskboard_domain::BoardId::from(uuid(9));
    second.remote = Some(RemoteBoardId(43));
    InMemoryRepository::apply(
        repo.as_ref(),
        vec![taskboard_domain::PersistenceAction::UpsertBoard(second)],
    )
    .await
    .expect("seed board 43");
    let mut actor = spawn(&server, repo);
    actor
        .commands
        .send(SyncCommand::SetBoard(RemoteBoardId(BOARD)))
        .await
        .unwrap();
    let first = next_report(&mut actor).await;
    let SyncReport::Completed {
        snapshot: first, ..
    } = first
    else {
        panic!("expected a Completed report");
    };
    assert_eq!(first.board.id, RemoteBoardId(BOARD));

    // Both commands land in the inbox before the actor is polled again
    // (capacity-backed sends complete without yielding), so the `SetBoard`
    // arrives exactly as the `SyncNow`-driven cycle runs.
    actor.commands.send(SyncCommand::SyncNow).await.unwrap();
    actor
        .commands
        .send(SyncCommand::SetBoard(RemoteBoardId(43)))
        .await
        .unwrap();
    let second = next_report(&mut actor).await;
    let SyncReport::Completed {
        snapshot: second, ..
    } = second
    else {
        panic!("expected a Completed report");
    };
    assert_eq!(
        second.board.id,
        RemoteBoardId(BOARD),
        "the nudged cycle still targets the old board"
    );
    let third = next_report(&mut actor).await;
    let SyncReport::Completed {
        snapshot: third, ..
    } = third
    else {
        panic!("expected the absorbed switch's rerun report");
    };
    assert_eq!(
        third.board.id,
        RemoteBoardId(43),
        "exactly one rerun serves the switched-to board"
    );

    // And then silence: the switch produced no fourth cycle.
    for _ in 0..500 {
        assert!(
            actor.reports.try_recv().is_err(),
            "the switch absorbed into exactly one rerun"
        );
        tokio::time::advance(Duration::from_millis(10)).await;
    }
}

/// A26 (F6-06b): an unreadable engine state backs off without a report,
/// a broadcast, or any HTTP — and the first cycle after the store
/// recovers completes. The fail-reads seam is the only path to the
/// read-failure arm; deleting the streak increment schedules the
/// success-cycle `poll_interval` (3 600 s) instead of the backoff and
/// the recovering report never arrives inside the horizon.
#[tokio::test(start_paused = true)]
async fn a26_unreadable_state_backs_off_then_recovers() {
    let server = start_server().await;
    mount_conditional_pull(&server, &serde_json::json!([])).await;
    let repo = repo_with(&[(2, STACK)], &[]);
    let mut actor = spawn(&server, repo.clone());

    repo.set_read_failure(true);
    actor.commands.send(SyncCommand::SyncNow).await.unwrap();
    let failure_window_start = tokio::time::Instant::now();
    for _ in 0..30 {
        assert!(
            actor.reports.try_recv().is_err(),
            "a read failure produces no report"
        );
        tokio::time::advance(Duration::from_millis(10)).await;
    }
    assert!(
        failure_window_start.elapsed() >= cfg().backoff_initial,
        "the failing cycle consumed the backoff window"
    );
    assert!(
        server.received_requests().await.unwrap().is_empty(),
        "the read precedes every wire call: a storage failure is not network traffic"
    );
    assert!(
        actor.system_rx.try_recv().is_err(),
        "a storage failure is not a lost network"
    );

    repo.set_read_failure(false);
    let recovered = next_report(&mut actor).await;
    assert!(
        matches!(recovered, SyncReport::Completed { .. }),
        "the backoff-scheduled cycle completes once the store reads again"
    );
}

/// A27 (F6-06c): the unlisted+404 tombstone keeps the *local* bound
/// board's title and color — the `bound_board` lookup feeding the
/// synthesized identity, not an empty fallback.
#[tokio::test(start_paused = true)]
async fn a27_unlisted_board_tombstone_keeps_the_local_title() {
    let server = start_server().await;
    Mock::given(method("GET"))
        .and(path("/index.php/apps/deck/api/v1.0/boards"))
        .respond_with(ResponseTemplate::new(200).set_body_string(serde_json::json!([]).to_string()))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!(
            "/index.php/apps/deck/api/v1.0/boards/{BOARD}"
        )))
        .respond_with(ResponseTemplate::new(404))
        .expect(1)
        .mount(&server)
        .await;

    // Distinctively titled local board: the stock helper's "board" is
    // indistinguishable from a fixture fallback (the A18/A22 inline seed
    // precedent).
    let mut state = PersistedState::default();
    let mut board = bound_board();
    board.title = "the local name".into();
    state.boards.insert(board.id, board);
    let stack = bound_stack(2, STACK);
    state.stacks.insert(stack.id, stack);

    let mut actor = spawn(&server, Arc::new(InMemoryRepository::with_state(state)));
    let report = set_board_and_report(&mut actor).await;
    let SyncReport::Completed { snapshot, .. } = report else {
        panic!("expected a Completed report");
    };
    assert_eq!(snapshot.board.id, RemoteBoardId(BOARD));
    assert!(
        snapshot.board.deleted_at.is_some(),
        "the synthesized board carries the delete-wins tombstone"
    );
    assert_eq!(snapshot.board.title, "the local name");
    assert_eq!(snapshot.board.color, "00ff00");
}

/// A28 (F6-23): the archived listing's stack twin dedupes against the
/// active listing and its cards dedupe against `seen_cards`, while the
/// archived-listing flag is adopted with the observation stamp when it
/// disagrees with the local one. Dropping a dedupe guard duplicates or
/// swallows entries; leaving the mismatch unstamped keeps the fixture
/// stamp and loses every LWW against fresher locals.
#[tokio::test(start_paused = true)]
#[allow(clippy::too_many_lines)] // fixture mounting is the scenario
async fn a28_archived_twin_dedupes_and_the_flag_is_adopted_with_the_observed_stamp() {
    let server = start_server().await;
    let mut live_card = card_json(55, "twin");
    live_card["archived"] = serde_json::json!(false); // the lagging live listing
    let mut archived_twin = card_json(55, "twin");
    archived_twin["archived"] = serde_json::json!(true);
    let mut archived_other = card_json(77, "archived elsewhere");
    archived_other["stackId"] = serde_json::json!(99);
    archived_other["archived"] = serde_json::json!(true);
    for (p, body) in [
        (
            "/index.php/apps/deck/api/v1.0/boards".to_string(),
            boards_json(false, &serde_json::json!([])),
        ),
        (
            format!("/index.php/apps/deck/api/v1.0/boards/{BOARD}"),
            board_json(false, &serde_json::json!([])),
        ),
        (
            format!("/index.php/apps/deck/api/v1.0/boards/{BOARD}/stacks"),
            serde_json::json!([{
                "id": STACK, "title": "col", "boardId": BOARD, "order": 0,
                "lastModified": T, "deletedAt": 0, "archived": false,
                "cards": [live_card]
            }])
            .to_string(),
        ),
        (
            format!("/index.php/apps/deck/api/v1.0/boards/{BOARD}/stacks/archived"),
            serde_json::json!([
                { "id": STACK, "title": "col", "boardId": BOARD, "order": 0,
                  "lastModified": T, "deletedAt": 0, "archived": true,
                  "cards": [archived_twin] },
                { "id": 99, "title": "old", "boardId": BOARD, "order": 99,
                  "lastModified": T, "deletedAt": 0, "archived": true,
                  "cards": [archived_other] }
            ])
            .to_string(),
        ),
    ] {
        Mock::given(method("GET"))
            .and(path(p))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("ETag", "\"gen-1\"")
                    .set_body_string(body),
            )
            .mount(&server)
            .await;
    }
    // The authoritative detail for the one *bound* card; unbound cards
    // keep the listing view, so exactly one detail request may happen.
    let mut detail = card_json(55, "twin");
    detail["archived"] = serde_json::json!(true);
    Mock::given(method("GET"))
        .and(path(format!(
            "/index.php/apps/deck/api/v1.0/boards/{BOARD}/stacks/{STACK}/cards/55"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(detail))
        .expect(1)
        .mount(&server)
        .await;

    // One bound task at card 55, locally not archived — the flag
    // disagreement that drives the detail refresh.
    let mut state = PersistedState::default();
    state.boards.insert(bound_board().id, bound_board());
    let stack = bound_stack(2, STACK);
    state.stacks.insert(stack.id, stack);
    let task = Task {
        id: taskboard_domain::TaskId::from(uuid(10)),
        remote: Some(taskboard_domain::RemoteCardRef {
            board: RemoteBoardId(BOARD),
            stack: taskboard_domain::RemoteStackId(STACK),
            card: taskboard_domain::RemoteCardId(55),
        }),
        title: "twin".into(),
        description: String::new(),
        duedate: None,
        done: None,
        stack: taskboard_domain::StackId::from(uuid(2)),
        order: 0,
        labels: std::collections::BTreeSet::new(),
        archived: false,
        deleted: false,
        clocks: task_clocks(),
        remote_seen: Some(ts(T)),
    };
    state.tasks.insert(task.id, task);

    let mut actor = spawn(&server, Arc::new(InMemoryRepository::with_state(state)));
    let report = set_board_and_report(&mut actor).await;
    let SyncReport::Completed { snapshot, .. } = report else {
        panic!("expected a Completed report");
    };
    server.verify().await;

    let twins: Vec<_> = snapshot
        .tasks
        .iter()
        .filter(|t| t.id.card == taskboard_domain::RemoteCardId(55))
        .collect();
    assert_eq!(twins.len(), 1, "the archived twin must not duplicate 55");
    assert!(
        twins[0].archived,
        "the authoritative detail settles the flag"
    );
    assert_eq!(
        twins[0].last_modified,
        stamped(),
        "the mismatch correction is stamped with the observation instant"
    );
    let others: Vec<_> = snapshot
        .tasks
        .iter()
        .filter(|t| t.id.card == taskboard_domain::RemoteCardId(77))
        .collect();
    assert_eq!(others.len(), 1, "the unbound archived card appears once");
    assert!(others[0].archived, "the archived listing decides the flag");
    let mut stack_ids: Vec<_> = snapshot.stacks.iter().map(|s| s.id.stack).collect();
    stack_ids.sort();
    assert_eq!(
        stack_ids,
        vec![
            taskboard_domain::RemoteStackId(7),
            taskboard_domain::RemoteStackId(99)
        ],
        "the stack twin dedupes: exactly stacks 7 and 99"
    );
}
