// SPDX-License-Identifier: MIT OR Apache-2.0
//! A-series: the sync actor against wiremock under paused tokio time (one
//! behavior per test). The actor reads through `Arc<InMemoryRepository>` —
//! the same fake the engine uses, so memory ≡ disk invariants hold across
//! the pair; reports and system events arrive over the injected channels
//! with timeout guards.
//!
//! Determinism: `#[tokio::test]` makes every timer (client
//! retries, poll/backoff deadlines) advance deterministically; wiremock
//! request counts are the observable of how many cycles ran.

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
use uuid::Uuid;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const BOARD: u64 = 42;
const STACK: u64 = 7;
const T: i64 = 1_700_000_000;

const CFG: SyncActorConfig = SyncActorConfig {
    poll_interval: Duration::from_millis(200),
    backoff_initial: Duration::from_millis(100),
    backoff_max: Duration::from_millis(400),
};

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

fn uuid(raw: u128) -> Uuid {
    Uuid::from_u128(raw)
}

fn ts(secs: i64) -> chrono::DateTime<chrono::Utc> {
    chrono::Utc.timestamp_opt(secs, 0).unwrap()
}

use chrono::TimeZone as _;

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

fn boards_json(dead: bool) -> String {
    serde_json::json!([{
        "id": BOARD, "title": "board", "color": "00ff00",
        "lastModified": T, "deletedAt": if dead { T + 5 } else { 0 },
        "archived": false, "labels": []
    }])
    .to_string()
}

fn stacks_json(cards: &serde_json::Value) -> String {
    serde_json::json!([{
        "id": STACK, "title": "col", "boardId": BOARD, "order": 0,
        "lastModified": T, "deletedAt": 0, "archived": false,
        "cards": cards
    }])
    .to_string()
}

fn card_json(id: u64, title: &str) -> serde_json::Value {
    serde_json::json!({
        "id": id, "title": title, "stackId": STACK, "type": "plain",
        "order": 0, "lastModified": T, "labels": [], "archived": false,
        "duedate": null, "done": null
    })
}

/// Mounts the standard happy pull: boards (with the board), active stacks,
/// empty archived stacks — all with `ETag`s so conditional follow-ups work.
async fn mount_standard_pull(server: &MockServer, cards: serde_json::Value) {
    for (p, body) in [
        (
            "/index.php/apps/deck/api/v1.0/boards".to_string(),
            boards_json(false),
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
    spawn_with_cfg(server, repo, CFG)
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

/// Guarded report receive (a stuck actor fails the test, never wedges it).
async fn next_report(actor: &mut Actor) -> SyncReport {
    tokio::time::timeout(Duration::from_secs(60), actor.reports.recv())
        .await
        .expect("actor produced a report in time")
        .expect("report channel open")
}

// ---------------------------------------------------------------------

/// A1: a nudge triggers a cycle that pushes the offline create and pulls
/// the board back — the report carries the echo-bound push outcome and the
/// snapshot contains the created card (push-before-pull).
#[tokio::test]
async fn a1_nudge_pushes_the_create_and_pulls_it_back() {
    let server = MockServer::start().await;
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
    let report = next_report(&mut actor).await;
    let SyncReport::Completed {
        snapshot, pushes, ..
    } = set_board_and_report(&mut actor).await
    else {
        panic!("expected a Completed report, got {report:?}");
    };
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

/// Pull endpoints that answer 304 whenever the caller re-emits the `ETag`
/// they issued, else fresh data.
async fn mount_conditional_pull(server: &MockServer) {
    let conditional = move |req: &wiremock::Request| {
        let warm = req
            .headers
            .get("If-None-Match")
            .is_some_and(|v| v.to_str().unwrap_or("").contains("gen-1"));
        if warm {
            return ResponseTemplate::new(304).insert_header("ETag", "\"gen-1\"");
        }
        let p = req.url.path().to_string();
        let body = if p.ends_with("/stacks") {
            stacks_json(&serde_json::json!([]))
        } else if p.ends_with("/stacks/archived") {
            "[]".to_string()
        } else {
            boards_json(false)
        };
        ResponseTemplate::new(200)
            .insert_header("ETag", "\"gen-1\"")
            .set_body_string(body)
    };
    for p in [
        "/index.php/apps/deck/api/v1.0/boards".to_string(),
        format!("/index.php/apps/deck/api/v1.0/boards/{BOARD}/stacks"),
        format!("/index.php/apps/deck/api/v1.0/boards/{BOARD}/stacks/archived"),
    ] {
        Mock::given(method("GET"))
            .and(path(p))
            .respond_with(conditional)
            .mount(server)
            .await;
    }
}

/// A2: a second cycle against an unchanged server runs on 304s — the cache
/// fills the listings and the report stays complete, with no unconditional
/// refetches.
#[tokio::test]
async fn a2_unchanged_poll_ticks_on_304s_from_the_cache() {
    let server = MockServer::start().await;
    mount_conditional_pull(&server).await;

    let mut actor = spawn(&server, repo_with(&[(2, STACK)], &[]));
    let first = set_board_and_report(&mut actor).await;
    assert!(matches!(first, SyncReport::Completed { .. }), "{first:?}");

    actor.commands.send(SyncCommand::SyncNow).await.unwrap();
    let second = next_report(&mut actor).await;
    let SyncReport::Completed { snapshot, .. } = second else {
        panic!("expected a Completed report");
    };
    assert_eq!(snapshot.board.id, RemoteBoardId(BOARD));
    assert_eq!(
        snapshot.stacks.len(),
        1,
        "the cached listing carries the board's stack"
    );
    assert_eq!(
        server.received_requests().await.unwrap().len(),
        6,
        "3 reads per cycle; the 304s filled from the cache"
    );
}

/// A3: a nudge storm arriving while a cycle runs collapses into exactly
/// one rerun (the ADR 0006 absorber), observed via request counts.
#[tokio::test]
async fn a3_nudge_storm_collapse_into_one_rerun() {
    let server = MockServer::start().await;
    mount_standard_pull(&server, serde_json::json!([])).await;

    // A poll interval far beyond the test horizon: only nudges drive cycles.
    let cfg = SyncActorConfig {
        poll_interval: Duration::from_secs(3_600),
        ..CFG
    };
    let mut actor = spawn_with_cfg(&server, repo_with(&[(2, STACK)], &[]), cfg);
    let _ = set_board_and_report(&mut actor).await; // cycle 1

    for _ in 0..5 {
        actor.commands.send(SyncCommand::SyncNow).await.unwrap();
    }
    let _ = next_report(&mut actor).await; // cycle 2: the first buffered nudge
    let _ = next_report(&mut actor).await; // cycle 3: the storm coalesces into one rerun
    assert_eq!(
        server.received_requests().await.unwrap().len(),
        9,
        "exactly three cycles: initial + first nudge + one coalesced rerun"
    );
    // And then silence: five nudges never mean five cycles.
    assert!(
        tokio::time::timeout(Duration::from_secs(2), actor.reports.recv())
            .await
            .is_err(),
        "no fourth cycle may fire"
    );
}

/// A4: a transport failure mid-outbox (two creates succeed, the third
/// exhausts its retry budget on 503s) reports `Failed { Network, [2] }`
/// with no snapshot; the remaining op stays queued.
#[tokio::test]
async fn a4_transport_failure_mid_push_keeps_evidence_and_queue() {
    let server = MockServer::start().await;
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
    let SyncReport::Failed { kind, pushes } = report else {
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
}

/// A7: a transport-class failure broadcasts `NetworkLost` exactly once,
/// failures back off with doubling, and the first success reports
/// `Completed` *before* broadcasting `NetworkRestored`.
#[tokio::test]
async fn a7_offline_episode_broadcasts_lost_once_then_restored() {
    let server = MockServer::start().await;
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
        } else {
            boards_json(false)
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
    let lost_at = std::time::Instant::now();
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
    // no earlier than the initial backoff.
    let failed2 = next_report(&mut actor).await;
    assert!(matches!(
        failed2,
        SyncReport::Failed {
            kind: SyncErrorKind::Network,
            ..
        }
    ));
    assert!(
        lost_at.elapsed() >= CFG.backoff_initial,
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

/// A8: a `SetBoard` before any binding triggers a cycle that reports
/// `Failed { NoBoard }`; after the target is set a normal cycle runs.
#[tokio::test]
async fn a8_no_board_reported_once_a_target_is_commanded() {
    let server = MockServer::start().await;
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
    assert!(matches!(report, SyncReport::Completed { .. }));
    assert!(actor.system_rx.try_recv().is_err(), "no network events");
}

/// A9: archived listings merge into the snapshot with `archived = true` —
/// the listing decides the flag, so the engine never tombstones an
/// archived card as absent.
#[tokio::test]
async fn a9_archived_listing_merges_without_tombstoning() {
    let server = MockServer::start().await;
    let mut archived_card = card_json(77, "archived remotely");
    archived_card["archived"] = serde_json::json!(false); // wire lag: the listing decides
    archived_card["stackId"] = serde_json::json!(99);
    Mock::given(method("GET"))
        .and(path("/index.php/apps/deck/api/v1.0/boards"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("ETag", "\"gen-1\"")
                .set_body_string(boards_json(false)),
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
#[tokio::test]
async fn a10_304_with_a_cold_cache_refetches_unconditionally() {
    let server = MockServer::start().await;
    mount_conditional_pull(&server).await;
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
    // 3 conditional 304s + 3 unconditional refetches.
    assert_eq!(server.received_requests().await.unwrap().len(), 6);
    assert_eq!(
        validators.boards.etag.as_deref(),
        Some("\"gen-1\""),
        "the refetch's validators travel in the report"
    );
}

/// A11: board absence from the listing is confirmed with a GET before a
/// tombstone is synthesized — a 404 yields the delete-wins snapshot.
#[tokio::test]
async fn a11_absent_board_is_confirmed_then_tombstoned() {
    let server = MockServer::start().await;
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
#[tokio::test]
async fn a12_shutdown_exits_the_loop() {
    let server = MockServer::start().await;
    let mut actor = spawn(&server, repo_with(&[(2, STACK)], &[]));
    // The actor subscribes on its first poll; give it that beat before
    // broadcasting (a broadcast sent before subscribing is never delivered).
    tokio::time::sleep(Duration::from_millis(100)).await;
    actor.system_tx.send(SystemEvent::Shutdown).unwrap();
    tokio::time::timeout(Duration::from_secs(10), &mut actor.join)
        .await
        .expect("actor exits on Shutdown")
        .expect("no panic");
}

/// A13: a stack create and a card create in that stack push in dependency
/// order — the stack POST precedes the card POST, which uses the echo's
/// stack id.
#[tokio::test]
async fn a13_stack_create_precedes_card_create() {
    let server = MockServer::start().await;
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
/// `NotFound` on an update as `RemoteMissing`, a 400 PUT as
/// `Rejected { BadRequest }` (whose dead-lettering the engine applies, and
/// which the I-series then asserts as the visible `Failed` phase).
#[tokio::test]
async fn a5_a6_rejection_outcomes_travel_in_the_report() {
    let server = MockServer::start().await;
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
