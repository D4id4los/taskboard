// SPDX-License-Identifier: MIT OR Apache-2.0
//! I-series: hermetic end-to-end scenarios — the State Engine, the sync
//! actor, and the in-memory repository in one process against wiremock.
//!
//! The full channel graph is wired exactly as the Phase 5 bootstrap will:
//! engine nudges → actor commands, actor reports → engine ingestion, one
//! shared system broadcast (engine subscribes, actor publishes). The fake
//! repository is the *same* object for both sides, so memory ≡ disk holds
//! across the pair and every assertion reads the engine's published
//! `AppState` — the surface a real UI would see.
//!
//! Determinism: `#[tokio::test(start_paused)]` freezes the tokio clock; the
//! `start_server` boot helper and the `eventually` pollers are the only
//! places time advances, so the actor's poll ticks fire exactly while a
//! test waits on a predicate — never between assertions.

use std::sync::Arc;
use std::time::Duration;

use taskboard_domain::test_support::{CountingIds, InMemoryRepository};
use taskboard_domain::{
    AppState, Clock, CommandOutcome, SyncCommand, SyncPhase, SyncReport, SystemEvent,
};
use taskboard_state::spawn_state_engine;
use taskboard_sync_nextcloud::{DeckClient, SyncActorConfig, spawn_sync_actor};
use tokio::sync::{broadcast, mpsc};
use wiremock::matchers::{method, path, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

mod common;
use common::hermetic::{
    BOARD, STACK, T, board_json, boards_json, card_json, mount_conditional_pull, stacks_json,
    start_server, ts, uuid, wait_subscribed,
};

/// This suite's boards carry the seeded "urgent" label in listings and
/// detail payloads alike.
fn board_labels() -> serde_json::Value {
    serde_json::json!([{ "id": 3, "title": "urgent", "color": "ff0000", "boardId": BOARD }])
}

const CFG: SyncActorConfig = SyncActorConfig {
    poll_interval: Duration::from_millis(200),
    backoff_initial: Duration::from_millis(100),
    backoff_max: Duration::from_millis(400),
};

/// The engine half of the pair, plus every channel half a test needs to
/// spawn (and re-spawn) actors against it.
#[allow(dead_code)] // per-test subsets
struct Stack {
    server: MockServer,
    engine: taskboard_state::EngineHandle,
    engine_join: tokio::task::JoinHandle<()>,
    report_tx: mpsc::Sender<SyncReport>,
    system_tx: broadcast::Sender<SystemEvent>,
    repo: Arc<InMemoryRepository>,
}

async fn boot(server: MockServer, repo_state: taskboard_domain::PersistedState) -> Stack {
    let repo = Arc::new(InMemoryRepository::with_state(repo_state));
    let ids: Arc<dyn taskboard_domain::IdGenerator> = Arc::new(CountingIds::new());
    let clock: Arc<dyn Clock> = Arc::new(taskboard_domain::SystemClock);
    let (nudges_tx, _nudges_rx) = mpsc::channel(16);
    let (report_tx, report_rx) = mpsc::channel(16);
    let (system_tx, system_engine_rx) = broadcast::channel(16);

    let (engine, engine_join) = spawn_state_engine(
        repo.clone(),
        ids,
        clock,
        nudges_tx,
        report_rx,
        system_engine_rx,
    )
    .await
    .expect("engine boots");

    Stack {
        server,
        engine,
        engine_join,
        report_tx,
        system_tx,
        repo,
    }
}

/// Spawns an actor against the stack on a fresh command channel; returns
/// the command half (for `SetBoard`/manual nudges) and the join handle.
fn spawn_actor(stack: &Stack) -> (mpsc::Sender<SyncCommand>, tokio::task::JoinHandle<()>) {
    spawn_actor_with_cfg(stack, CFG)
}

fn spawn_actor_with_cfg(
    stack: &Stack,
    cfg: SyncActorConfig,
) -> (mpsc::Sender<SyncCommand>, tokio::task::JoinHandle<()>) {
    let client = DeckClient::new(&stack.server.uri(), "u", "t").unwrap();
    let state_reader: Arc<dyn taskboard_domain::SyncStateReader> = Arc::new(stack.engine.clone());
    let (commands, command_rx) = mpsc::channel(16);
    let join = spawn_sync_actor(
        client,
        state_reader,
        command_rx,
        stack.report_tx.clone(),
        stack.system_tx.clone(),
        cfg,
    );
    (commands, join)
}

/// Arms the pull target through the actor's command channel.
async fn bind_board(commands: &mpsc::Sender<SyncCommand>) {
    commands
        .send(SyncCommand::SetBoard(RemoteBoardId0(BOARD)))
        .await
        .expect("commands channel open");
}

// `RemoteBoardId` alias to keep the helper signatures short.
use taskboard_domain::RemoteBoardId as RemoteBoardId0;

async fn eventually(engine: &taskboard_state::EngineHandle, pred: impl Fn(&AppState) -> bool) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
    loop {
        if pred(&engine.shared_state().load_full()) {
            return;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "predicate satisfied within the deadline"
        );
        // Time advances only here: the actor's poll ticks fire while the
        // test waits, never between assertions.
        tokio::time::advance(Duration::from_millis(5)).await;
    }
}

/// The seeded state: a bound board + bound stack, nothing else.
fn seeded_state() -> taskboard_domain::PersistedState {
    let mut state = taskboard_domain::PersistedState::default();
    let board = taskboard_domain::Board {
        id: taskboard_domain::BoardId::from(uuid(1)),
        remote: Some(RemoteBoardId0(BOARD)),
        title: "board".into(),
        color: taskboard_domain::Color::new("00ff00"),
        archived: false,
        deleted: false,
        remote_seen: Some(ts(T)),
    };
    let stack = taskboard_domain::Stack {
        id: taskboard_domain::StackId::from(uuid(2)),
        remote: Some(taskboard_domain::RemoteStackRef {
            board: RemoteBoardId0(BOARD),
            stack: taskboard_domain::RemoteStackId(STACK),
        }),
        board: board.id,
        title: "col".into(),
        order: 0,
        archived: false,
        deleted: false,
        clocks: taskboard_domain::StackClocks {
            title: ts(T),
            order: ts(T),
            deleted: ts(T),
        },
        remote_seen: Some(ts(T)),
    };
    state.boards.insert(board.id, board);
    state.stacks.insert(stack.id, stack);
    state
}

/// Mounts the happy pull with the given cards (fresh `ETag` generation per
/// response set, so later cycles can go conditional).
async fn mount_pull(server: &MockServer, cards: serde_json::Value) {
    for (p, body) in [
        (
            "/index.php/apps/deck/api/v1.0/boards".to_string(),
            boards_json(false, &board_labels()),
        ),
        (
            format!("/index.php/apps/deck/api/v1.0/boards/{BOARD}"),
            board_json(false, &board_labels()),
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

// ---------------------------------------------------------------------

/// A miniature stateful Deck: listings always reflect the mutations, so
/// the snapshot completeness contract holds like it does on a real server.
/// Cards carry no `deletedAt` (the wire truth); mutation stamps advance a
/// logical clock.
#[derive(Clone, Default)]
struct DeckSim {
    stacks: Arc<std::sync::Mutex<Vec<serde_json::Value>>>,
    cards: Arc<std::sync::Mutex<Vec<serde_json::Value>>>,
    ops: Arc<std::sync::Mutex<Vec<String>>>,
    next_id: Arc<std::sync::atomic::AtomicU64>,
    now: Arc<std::sync::atomic::AtomicI64>,
}

impl DeckSim {
    fn new() -> Self {
        Self {
            now: Arc::new(std::sync::atomic::AtomicI64::new(T + 1)),
            ..Self::default()
        }
    }

    fn stamp(&self) -> i64 {
        self.now.fetch_add(60, std::sync::atomic::Ordering::SeqCst) + 60
    }

    fn id(&self) -> u64 {
        self.next_id
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            + 100
    }

    /// The stacks listing with cards nested into their stacks.
    fn stacks_body(&self) -> Vec<serde_json::Value> {
        let cards = self.cards.lock().unwrap().clone();
        self.stacks
            .lock()
            .unwrap()
            .iter()
            .map(|stack| {
                let id = stack["id"].as_u64().unwrap_or(0);
                let mut stack = stack.clone();
                stack["cards"] = serde_json::Value::Array(
                    cards
                        .iter()
                        .filter(|c| c["stackId"].as_u64() == Some(id))
                        .cloned()
                        .collect(),
                );
                stack
            })
            .collect()
    }

    /// Mounts the whole simulated board on the server.
    #[allow(clippy::too_many_lines)] // one wiremock mount per simulated route
    async fn mount(&self, server: &MockServer) {
        let base = format!("/index.php/apps/deck/api/v1.0/boards/{BOARD}");
        let sim = self.clone();
        Mock::given(method("GET"))
            .and(path("/index.php/apps/deck/api/v1.0/boards"))
            .respond_with(move |_req: &wiremock::Request| {
                ResponseTemplate::new(200)
                    .insert_header("ETag", "\"gen-1\"")
                    .set_body_string(boards_json(false, &board_labels()))
            })
            .mount(server)
            .await;
        Mock::given(method("GET"))
            .and(path(base.clone()))
            .respond_with(move |_req: &wiremock::Request| {
                ResponseTemplate::new(200).set_body_string(board_json(false, &board_labels()))
            })
            .mount(server)
            .await;
        let sim2 = sim.clone();
        Mock::given(method("GET"))
            .and(path(format!("{base}/stacks")))
            .respond_with(move |_req: &wiremock::Request| {
                ResponseTemplate::new(200)
                    .insert_header("ETag", "\"gen-1\"")
                    .set_body_string(serde_json::Value::Array(sim2.stacks_body()).to_string())
            })
            .mount(server)
            .await;
        Mock::given(method("GET"))
            .and(path(format!("{base}/stacks/archived")))
            .respond_with(move |_req: &wiremock::Request| {
                ResponseTemplate::new(200)
                    .insert_header("ETag", "\"gen-1\"")
                    .set_body_string("[]")
            })
            .mount(server)
            .await;
        // POST stacks (create).
        let sim4 = sim.clone();
        Mock::given(method("POST"))
            .and(path(format!("{base}/stacks")))
            .respond_with(move |req: &wiremock::Request| {
                let body: serde_json::Value = serde_json::from_slice(&req.body).unwrap();
                let stack = serde_json::json!({
                    "id": sim4.id(), "title": body["title"], "boardId": BOARD,
                    "order": body["order"], "lastModified": sim4.stamp(),
                    "deletedAt": 0, "archived": false, "cards": []
                });
                sim4.stacks.lock().unwrap().push(stack.clone());
                ResponseTemplate::new(200).set_body_json(stack)
            })
            .mount(server)
            .await;
        // POST cards (create) into any stack of the board.
        let sim5 = sim.clone();
        Mock::given(method("POST"))
            .and(path_regex(format!("{base}/stacks/[0-9]+/cards")))
            .respond_with(move |req: &wiremock::Request| {
                let body: serde_json::Value = serde_json::from_slice(&req.body).unwrap();
                let path = req.url.path().to_string();
                let stack_num: u64 = path
                    .rsplit('/')
                    .nth(1)
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(STACK);
                let card = serde_json::json!({
                    "id": sim5.id(), "title": body["title"], "stackId": stack_num,
                    "type": "plain", "order": body["order"].as_i64().unwrap_or(0),
                    "description": body["description"].as_str().unwrap_or(""),
                    "lastModified": sim5.stamp(), "labels": [], "archived": false,
                    "duedate": null, "done": null
                });
                sim5.cards.lock().unwrap().push(card.clone());
                ResponseTemplate::new(200).set_body_json(card)
            })
            .mount(server)
            .await;
        // GET one card (the executor's fetch-before-write pre-flight).
        let sim9 = sim.clone();
        Mock::given(method("GET"))
            .and(path_regex(format!("{base}/stacks/[0-9]+/cards/[0-9]+")))
            .respond_with(move |req: &wiremock::Request| {
                let path = req.url.path().to_string();
                let card_num: u64 = path
                    .rsplit('/')
                    .next()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
                match sim9
                    .cards
                    .lock()
                    .unwrap()
                    .iter()
                    .find(|c| c["id"].as_u64() == Some(card_num))
                    .cloned()
                {
                    Some(card) => ResponseTemplate::new(200).set_body_json(card),
                    None => ResponseTemplate::new(404),
                }
            })
            .mount(server)
            .await;
        // PUT card (full round-trip update — label-set changes ride it).
        let sim10 = sim.clone();
        Mock::given(method("PUT"))
            .and(path_regex(format!("{base}/stacks/[0-9]+/cards/[0-9]+$")))
            .respond_with(move |req: &wiremock::Request| {
                let body: serde_json::Value = serde_json::from_slice(&req.body).unwrap();
                let path = req.url.path().to_string();
                let card_num: u64 = path
                    .rsplit('/')
                    .next()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
                let mut cards = sim10.cards.lock().unwrap();
                if let Some(card) = cards
                    .iter_mut()
                    .find(|c| c["id"].as_u64() == Some(card_num))
                {
                    *card = body.clone();
                }
                sim10.ops.lock().unwrap().push("put".into());
                ResponseTemplate::new(200).set_body_json(body)
            })
            .mount(server)
            .await;
        // PUT reorder/move (mounted before the generic PUT card route, and
        // anchored so the two cannot shadow each other).
        let sim6 = sim.clone();
        Mock::given(method("PUT"))
            .and(path_regex(format!(
                "{base}/stacks/[0-9]+/cards/[0-9]+/reorder$"
            )))
            .respond_with(move |req: &wiremock::Request| {
                let body: serde_json::Value = serde_json::from_slice(&req.body).unwrap();
                let path = req.url.path().to_string();
                let card_num: u64 = path
                    .rsplit('/')
                    .nth(1)
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
                let mut cards = sim6.cards.lock().unwrap();
                if let Some(card) = cards
                    .iter_mut()
                    .find(|c| c["id"].as_u64() == Some(card_num))
                {
                    card["stackId"] = body["stackId"].clone();
                    card["order"] = body["order"].clone();
                    card["lastModified"] = serde_json::json!(sim6.stamp());
                }
                sim6.ops.lock().unwrap().push("reorder".into());
                ResponseTemplate::new(200).set_body_string("[]")
            })
            .mount(server)
            .await;
        // PUT assignLabel.
        let sim7 = sim.clone();
        Mock::given(method("PUT"))
            .and(path_regex(format!(
                "{base}/stacks/[0-9]+/cards/[0-9]+/assignLabel"
            )))
            .respond_with(move |req: &wiremock::Request| {
                let body: serde_json::Value = serde_json::from_slice(&req.body).unwrap();
                let path = req.url.path().to_string();
                let card_num: u64 = path
                    .rsplit('/')
                    .nth(1)
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
                let mut cards = sim7.cards.lock().unwrap();
                if let Some(card) = cards
                    .iter_mut()
                    .find(|c| c["id"].as_u64() == Some(card_num))
                {
                    card["labels"]
                        .as_array_mut()
                        .expect("labels array")
                        .push(body["labelId"].clone());
                }
                sim7.ops.lock().unwrap().push("assign".into());
                ResponseTemplate::new(200).set_body_json(
                    cards
                        .iter()
                        .find(|c| c["id"].as_u64() == Some(card_num))
                        .cloned()
                        .unwrap_or(serde_json::Value::Null),
                )
            })
            .mount(server)
            .await;
        // DELETE card.
        let sim8 = sim.clone();
        Mock::given(method("DELETE"))
            .and(path_regex(format!("{base}/stacks/[0-9]+/cards/[0-9]+")))
            .respond_with(move |req: &wiremock::Request| {
                let path = req.url.path().to_string();
                let card_num: u64 = path
                    .rsplit('/')
                    .next()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
                let removed = sim8
                    .cards
                    .lock()
                    .unwrap()
                    .iter()
                    .find(|c| c["id"].as_u64() == Some(card_num))
                    .cloned();
                sim8.cards
                    .lock()
                    .unwrap()
                    .retain(|c| c["id"].as_u64() != Some(card_num));
                sim8.ops.lock().unwrap().push("delete".into());
                ResponseTemplate::new(200).set_body_json(removed.unwrap_or(serde_json::Value::Null))
            })
            .mount(server)
            .await;
    }
}

/// I1 — the roadmap's offline journey: tasks are created offline through
/// the engine, then the bound sync target produces a cycle that pushes the
/// drafts and pulls the board; the remote binding becomes visible in the
/// published `AppState`.
#[tokio::test(start_paused = true)]
async fn i1_offline_creates_round_trip_to_the_server() {
    let server = start_server().await;
    let sim = DeckSim::new();
    sim.mount(&server).await;
    // The board's live column exists server-side from the start.
    sim.stacks.lock().unwrap().push(serde_json::json!({
        "id": STACK, "title": "col", "boardId": BOARD, "order": 0,
        "lastModified": T, "deletedAt": 0, "archived": false, "cards": []
    }));

    let stack = boot(server, seeded_state()).await;
    let (commands, _actor) = spawn_actor(&stack);
    // Deterministic: wait for the broadcast subscription (a structural
    // fact) instead of sleeping through the actor's first tick.
    wait_subscribed(&stack.system_tx).await;
    bind_board(&commands).await;

    let CommandOutcome::CreatedStack(stack_id) = stack
        .engine
        .execute(taskboard_domain::StateCommand::CreateStack {
            title: "col2".into(),
            order: 1,
        })
        .await
        .expect("accepted")
    else {
        panic!("wrong receipt");
    };
    let CommandOutcome::CreatedTask(task_id) = stack
        .engine
        .execute(taskboard_domain::StateCommand::CreateTask {
            title: "offline draft".into(),
            stack: stack_id,
            order: 0,
        })
        .await
        .expect("accepted")
    else {
        panic!("wrong receipt");
    };

    // The engine's nudges drive cycles; the round trip ends with the
    // card bound in the published state (the offline draft survives).
    eventually(&stack.engine, |app| {
        app.tasks
            .get(&task_id)
            .is_some_and(|t| t.remote.is_some() && t.title == "offline draft")
    })
    .await;
    assert!(
        stack.engine.shared_state().load_full().sync.phase == SyncPhase::Idle,
        "the cycle completed cleanly"
    );
    assert!(
        stack.repo.snapshot().outbox.is_empty(),
        "the engine persisted the completed push"
    );
}

/// I2 — remote edit wins per LWW: a server-side newer edit overwrites the
/// locally stale one on the next pull.
#[tokio::test(start_paused = true)]
async fn i2_remote_newer_edit_adopts() {
    let server = start_server().await;
    // The card exists remotely with a NEWER lastModified than the local
    // stale edit (local clocks ~T; remote edit T + 600).
    let mut card = card_json(55, "server wins");
    card["lastModified"] = serde_json::json!(T + 600);
    mount_pull(&server, serde_json::json!([card])).await;

    let mut state = seeded_state();
    let stack_id = taskboard_domain::StackId::from(uuid(2));
    let task = taskboard_domain::Task {
        id: taskboard_domain::TaskId::from(uuid(100)),
        remote: Some(taskboard_domain::RemoteCardRef {
            board: RemoteBoardId0(BOARD),
            stack: taskboard_domain::RemoteStackId(STACK),
            card: taskboard_domain::RemoteCardId(55),
        }),
        title: "stale local edit".into(),
        description: String::new(),
        duedate: None,
        done: None,
        stack: stack_id,
        order: 0,
        labels: std::collections::BTreeSet::new(),
        archived: false,
        deleted: false,
        clocks: taskboard_domain::TaskClocks {
            title: ts(T + 10),
            description: ts(T),
            duedate: ts(T),
            done: ts(T),
            position: ts(T),
            labels: ts(T),
            archived: ts(T),
            deleted: ts(T),
        },
        remote_seen: Some(ts(T + 10)),
    };
    state.tasks.insert(task.id, task);
    let stack = boot(server, state).await;
    let (commands, _actor) = spawn_actor(&stack);
    wait_subscribed(&stack.system_tx).await;
    bind_board(&commands).await;

    eventually(&stack.engine, |app| {
        app.tasks
            .get(&taskboard_domain::TaskId::from(uuid(100)))
            .is_some_and(|t| t.title == "server wins")
    })
    .await;
}

/// I3 — concurrent edits resolve per-field: the locally newer title
/// survives while the remotely newer description adopts.
#[tokio::test(start_paused = true)]
async fn i3_concurrent_edits_resolve_per_field() {
    let server = start_server().await;
    let mut card = card_json(55, "older remote title");
    card["description"] = serde_json::json!("newer remote description");
    card["lastModified"] = serde_json::json!(T + 100); // newer than remote_seen
    mount_pull(&server, serde_json::json!([card])).await;

    let mut state = seeded_state();
    let stack_id = taskboard_domain::StackId::from(uuid(2));
    let task = taskboard_domain::Task {
        id: taskboard_domain::TaskId::from(uuid(100)),
        remote: Some(taskboard_domain::RemoteCardRef {
            board: RemoteBoardId0(BOARD),
            stack: taskboard_domain::RemoteStackId(STACK),
            card: taskboard_domain::RemoteCardId(55),
        }),
        // Locally newer than anything remote: LWW keeps it.
        title: "local newer title".into(),
        description: "stale local description".into(),
        duedate: None,
        done: None,
        stack: stack_id,
        order: 0,
        labels: std::collections::BTreeSet::new(),
        archived: false,
        deleted: false,
        clocks: taskboard_domain::TaskClocks {
            title: ts(T + 500),
            description: ts(T + 10),
            duedate: ts(T),
            done: ts(T),
            position: ts(T),
            labels: ts(T),
            archived: ts(T),
            deleted: ts(T),
        },
        remote_seen: Some(ts(T + 10)),
    };
    state.tasks.insert(task.id, task);
    let stack = boot(server, state).await;
    let (commands, _actor) = spawn_actor(&stack);
    wait_subscribed(&stack.system_tx).await;
    bind_board(&commands).await;

    eventually(&stack.engine, |app| {
        matches!(
            app.tasks.get(&taskboard_domain::TaskId::from(uuid(100))),
            Some(t) if t.title == "local newer title"
                && t.description == "newer remote description"
        )
    })
    .await;
}

/// I4 — full two-way: move + label assign push to the server (reorder and
/// assignLabel requests observed), then a local delete deletes remotely.
#[tokio::test(start_paused = true)]
#[allow(clippy::too_many_lines)] // fixture seeding is the scenario
async fn i4_move_label_and_delete_push_to_the_server() {
    let server = start_server().await;
    let sim = DeckSim::new();
    sim.mount(&server).await;
    // The two server-side columns and the card, matching the seeded state.
    sim.stacks.lock().unwrap().extend([
        serde_json::json!({
            "id": STACK, "title": "col", "boardId": BOARD, "order": 0,
            "lastModified": T, "deletedAt": 0, "archived": false, "cards": []
        }),
        serde_json::json!({
            "id": 8, "title": "col2", "boardId": BOARD, "order": 1,
            "lastModified": T, "deletedAt": 0, "archived": false, "cards": []
        }),
    ]);
    sim.cards.lock().unwrap().push(serde_json::json!({
        "id": 55, "title": "draft", "stackId": STACK, "type": "plain",
        "order": 0, "lastModified": T, "labels": [], "archived": false,
        "duedate": null, "done": null
    }));

    let mut state = seeded_state();
    // A second bound stack as the move target, a bound label, and the bound
    // task — each with a pending op.
    let stack2 = taskboard_domain::Stack {
        id: taskboard_domain::StackId::from(uuid(5)),
        remote: Some(taskboard_domain::RemoteStackRef {
            board: RemoteBoardId0(BOARD),
            stack: taskboard_domain::RemoteStackId(8),
        }),
        board: taskboard_domain::BoardId::from(uuid(1)),
        title: "col2".into(),
        order: 1,
        archived: false,
        deleted: false,
        clocks: taskboard_domain::StackClocks {
            title: ts(T),
            order: ts(T),
            deleted: ts(T),
        },
        remote_seen: Some(ts(T)),
    };
    let label = taskboard_domain::Label {
        id: taskboard_domain::LabelId::from(uuid(6)),
        remote: Some(taskboard_domain::RemoteLabelRef {
            board: RemoteBoardId0(BOARD),
            label: taskboard_domain::RemoteLabelId(3),
        }),
        board: taskboard_domain::BoardId::from(uuid(1)),
        title: "urgent".into(),
        color: taskboard_domain::Color::new("ff0000"),
        deleted: false,
        clocks: taskboard_domain::LabelClocks {
            title: ts(T),
            color: ts(T),
            deleted: ts(T),
        },
        remote_seen: Some(ts(T)),
    };
    let task_id = taskboard_domain::TaskId::from(uuid(100));
    let task = taskboard_domain::Task {
        id: task_id,
        remote: Some(taskboard_domain::RemoteCardRef {
            board: RemoteBoardId0(BOARD),
            stack: taskboard_domain::RemoteStackId(STACK),
            card: taskboard_domain::RemoteCardId(55),
        }),
        title: "draft".into(),
        description: String::new(),
        duedate: None,
        done: None,
        stack: taskboard_domain::StackId::from(uuid(5)),
        order: 1,
        labels: std::collections::BTreeSet::from([taskboard_domain::LabelId::from(uuid(6))]),
        archived: false,
        deleted: false,
        clocks: taskboard_domain::TaskClocks {
            title: ts(T),
            description: ts(T),
            duedate: ts(T),
            done: ts(T),
            position: ts(T + 10),
            labels: ts(T + 10),
            archived: ts(T),
            deleted: ts(T),
        },
        remote_seen: Some(ts(T)),
    };
    state.stacks.insert(stack2.id, stack2);
    state.labels.insert(label.id, label);
    state.tasks.insert(task.id, task);
    for (raw, op) in [
        (400, taskboard_domain::LocalOp::MoveTask(task_id)),
        (
            401,
            taskboard_domain::LocalOp::AssignLabel(
                task_id,
                taskboard_domain::LabelId::from(uuid(6)),
            ),
        ),
    ] {
        state.outbox.push(taskboard_domain::PendingOp {
            op_id: taskboard_domain::OpId(uuid(raw)),
            op,
            queued_at: ts(T),
        });
    }
    let stack = boot(server, state).await;
    let (commands, _actor) = spawn_actor(&stack);
    wait_subscribed(&stack.system_tx).await;
    bind_board(&commands).await;

    eventually(&stack.engine, |app| app.sync.pending_ops == 0).await;
    // The move and the label assignment both reached the server.
    eventually_no_state(|| {
        let ops = sim.ops.lock().unwrap();
        ops.contains(&"reorder".to_string()) && ops.contains(&"put".to_string())
    })
    .await;
    {
        // The label assignment rode the full-send PUT's label array.
        let cards = sim.cards.lock().unwrap();
        let card = cards
            .iter()
            .find(|c| c["id"].as_u64() == Some(55))
            .expect("the card is still listed");
        assert_eq!(card["stackId"], serde_json::json!(8), "the move landed");
        let labels = card["labels"].as_array().expect("labels array");
        assert!(
            labels.iter().any(|l| l.as_u64() == Some(3)),
            "the label rides the PUT's whole label array: {card}"
        );
    }

    // Now the delete: tombstone through the engine, pushed on the next
    // cycle, and the op disappears from the persisted outbox.
    stack
        .engine
        .execute(taskboard_domain::StateCommand::DeleteTask { id: task_id })
        .await
        .expect("accepted");
    eventually_no_state(|| sim.ops.lock().unwrap().contains(&"delete".to_string())).await;
    eventually(&stack.engine, |app| {
        app.sync.pending_ops == 0
            && app
                .tasks
                .get(&task_id)
                .is_some_and(|t| t.deleted && t.remote.is_none())
    })
    .await;
}

async fn eventually_no_state(pred: impl Fn() -> bool) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
    loop {
        if pred() {
            return;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "predicate satisfied within the deadline"
        );
        // Time advances only here: the actor's poll ticks fire while the
        // test waits, never between assertions.
        tokio::time::advance(Duration::from_millis(5)).await;
    }
}

/// I5 — restart continuity: after dropping the actor and respawning with a
/// fresh client over the SAME engine and repository, the persisted
/// validators resume conditional polling: the unchanged server answers
/// 304s, the rebuilt cache fills, and cycles keep succeeding. (With the
/// old actor gone, the engine's dead nudge channel costs nothing.)
#[tokio::test(start_paused = true)]
async fn i5_restart_resumes_conditional_polling() {
    let server = start_server().await;
    mount_conditional_pull(&server, &board_labels()).await;

    // First lifetime: one full cycle (cold cache → fresh data + validators
    // persisted by the engine).
    let stack = boot(server, seeded_state()).await;
    let (commands, first_actor) = spawn_actor(&stack);
    wait_subscribed(&stack.system_tx).await;
    bind_board(&commands).await;
    eventually(&stack.engine, |app| {
        app.sync.phase == SyncPhase::Idle && app.sync.last_success.is_some()
    })
    .await;
    stack.engine.flush().await;
    let validators = stack.repo.snapshot().validators;
    assert!(
        validators.contains_key(&taskboard_domain::ValidatorKey::Boards),
        "validators persisted after the first cycle"
    );

    // Restart: drop the first actor, respawn with a fresh client over the
    // same engine and repository. Await (not sleep for) the abort: the
    // handle reaching `finished` is a structural fact.
    first_actor.abort();
    for _ in 0..10_000 {
        if first_actor.is_finished() {
            break;
        }
        tokio::task::yield_now().await;
    }
    assert!(first_actor.is_finished(), "the aborted actor stops");
    let before = stack.server.received_requests().await.unwrap().len();
    let (_commands2, _second_actor) = spawn_actor(&stack);
    let first_success = stack.engine.shared_state().load_full().sync.last_success;
    eventually(&stack.engine, |app| {
        app.sync.last_success.is_some()
            && app.sync.phase == SyncPhase::Idle
            && app.sync.last_success != first_success
    })
    .await;
    stack.engine.flush().await;
    assert_eq!(
        stack.repo.snapshot().validators,
        validators,
        "the conditional cycle re-persisted identical validators"
    );
    let after = stack.server.received_requests().await.unwrap().len();
    assert!(
        after > before,
        "the restarted actor polled (its nudges come from poll ticks)"
    );
}
