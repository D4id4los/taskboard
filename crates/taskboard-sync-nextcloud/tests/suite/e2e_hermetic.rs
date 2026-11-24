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
//! test waits on a predicate — never between assertions. The wait
//! helpers' deadline is wall-clock (`std::time::Instant`): the stepping
//! is virtual, but the actor's progress depends on real wiremock I/O.

use std::sync::Arc;
use std::time::Duration;

use taskboard_domain::test_support::{CountingIds, InMemoryRepository};
use taskboard_domain::{
    AppState, Clock, CommandOutcome, SyncCommand, SyncPhase, SyncReport, SystemEvent,
    TaskRepository,
};
use taskboard_state::spawn_state_engine;
use taskboard_sync_nextcloud::{DeckClient, SyncActorConfig, spawn_sync_actor};
use tokio::sync::{broadcast, mpsc};
use wiremock::matchers::{method, path, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::common::SteppingClock;
use crate::common::hermetic::{
    BOARD, STACK, T, board_json, boards_json, card_json, mount_conditional_pull, stacks_json,
    start_server, ts, uuid, wait_subscribed,
};

/// This suite's boards carry the seeded "urgent" label in listings and
/// detail payloads alike.
fn board_labels() -> serde_json::Value {
    serde_json::json!([{ "id": 3, "title": "urgent", "color": "ff0000", "boardId": BOARD }])
}

/// The cadences the engine is seeded with (the actor reads them live
/// through the engine facet): whole seconds, the snapshot's unit.
fn settings() -> taskboard_domain::SettingsSnapshot {
    taskboard_domain::SettingsSnapshot {
        poll_interval_secs: taskboard_domain::TickSecs::new(1).expect("nonzero"),
        backoff_initial_secs: taskboard_domain::TickSecs::new(1).expect("nonzero"),
        backoff_max_secs: taskboard_domain::TickSecs::new(4).expect("nonzero"),
        ..taskboard_domain::SettingsSnapshot::default()
    }
}

fn cfg(clock: Arc<dyn Clock>) -> SyncActorConfig {
    SyncActorConfig {
        clock,
        day: taskboard_domain::DayCalendar {
            day_start_time: chrono::NaiveTime::MIN,
            offset: taskboard_state::DayRollConfig::disabled().offset,
        },
        author: None,
    }
}

/// The engine half of the pair, plus every channel half a test needs to
/// spawn (and re-spawn) actors against it.
#[allow(dead_code)] // per-test subsets
struct Stack {
    server: Arc<MockServer>,
    engine: taskboard_state::EngineHandle,
    engine_join: tokio::task::JoinHandle<()>,
    report_tx: mpsc::Sender<SyncReport>,
    system_tx: broadcast::Sender<SystemEvent>,
    repo: Arc<InMemoryRepository>,
    /// The one clock engine and actor share (the pair must never run on
    /// split time: the self-clobber boundary compares their stamps).
    clock: Arc<dyn Clock>,
}

async fn boot(
    server: Arc<MockServer>,
    repo_state: taskboard_domain::PersistedState,
    clock: Arc<dyn Clock>,
) -> Stack {
    boot_with_ids(server, repo_state, clock, Arc::new(CountingIds::new())).await
}

/// Boot with explicit ids: a restart over restored state must not mint
/// ids the persisted rows already carry (the fake generator's small
/// integers would collide with the pre-crash op ids and the crash
/// journey's re-enqueued delete would alias a completed op).
async fn boot_with_ids(
    server: Arc<MockServer>,
    repo_state: taskboard_domain::PersistedState,
    clock: Arc<dyn Clock>,
    ids: Arc<dyn taskboard_domain::IdGenerator>,
) -> Stack {
    let repo = Arc::new(InMemoryRepository::with_state(repo_state));
    let (nudges_tx, _nudges_rx) = mpsc::channel(16);
    let (report_tx, report_rx) = mpsc::channel(16);
    let (system_tx, system_engine_rx) = broadcast::channel(16);

    let (notifier, _notifications) = mpsc::channel::<taskboard_domain::NotifierMessage>(64);
    let (engine, engine_join) = spawn_state_engine(
        repo.clone(),
        ids,
        clock.clone(),
        nudges_tx,
        report_rx,
        system_tx.clone(),
        system_engine_rx,
        notifier,
        settings(),
        taskboard_state::DayRollConfig::disabled(),
        None,
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
        clock,
    }
}

/// Spawns an actor against the stack on a fresh command channel; returns
/// the command half (for manual nudges) and the join handle.
fn spawn_actor(stack: &Stack) -> (mpsc::Sender<SyncCommand>, tokio::task::JoinHandle<()>) {
    spawn_actor_with_cfg(stack, cfg(stack.clock.clone()))
}

fn spawn_actor_with_cfg(
    stack: &Stack,
    cfg: SyncActorConfig,
) -> (mpsc::Sender<SyncCommand>, tokio::task::JoinHandle<()>) {
    let client = DeckClient::new(&stack.server.uri(), "u", "t").unwrap();
    let state_reader: Arc<dyn taskboard_domain::SyncStateReader> = Arc::new(stack.engine.clone());
    let (commands, command_rx) = mpsc::channel(16);
    let join = spawn_sync_actor(
        taskboard_domain::ConnectorId::from("nextcloud"),
        client,
        state_reader,
        command_rx,
        stack.report_tx.clone(),
        stack.system_tx.clone(),
        cfg,
    );
    (commands, join)
}

/// Nudges a cycle: the board set derives from the persisted bindings
/// seeded into the stack's repo.
async fn nudge_cycle(commands: &mpsc::Sender<SyncCommand>) {
    commands
        .send(SyncCommand::SyncNow)
        .await
        .expect("commands channel open");
}

// `RemoteBoardId` alias to keep the helper signatures short.
use taskboard_domain::RemoteBoardId as RemoteBoardId0;

/// Wall-clock budget per wait: the paused-clock stepping below is virtual,
/// but the sync actor's progress depends on real loopback I/O, so the
/// deadline must be measured in time that resource actually consumes.
const WALL_DEADLINE: Duration = Duration::from_secs(30);

async fn eventually(engine: &taskboard_state::EngineHandle, pred: impl Fn(&AppState) -> bool) {
    let deadline = std::time::Instant::now() + WALL_DEADLINE;
    loop {
        let state = engine.shared_state().load_full();
        if pred(&state) {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "predicate not satisfied within the deadline (sync.phase: {:?})",
            state.sync.aggregate().phase
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
        remote: Some(taskboard_domain::RemoteBoardBinding {
            connector: taskboard_domain::ConnectorId::from("nextcloud"),
            board: RemoteBoardId0::deck(BOARD),
        }),
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
        authors: taskboard_domain::entities::BoardAuthors::default(),
        remote_seen: Some(ts(T)),
    };
    let stack = taskboard_domain::Stack {
        id: taskboard_domain::StackId::from(uuid(2)),
        remote: Some(taskboard_domain::RemoteStackRef {
            board: RemoteBoardId0::deck(BOARD),
            stack: taskboard_domain::RemoteStackId::deck(STACK),
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
        authors: taskboard_domain::entities::StackAuthors::default(),
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
    /// The pair's shared clock: sim-minted stamps must land inside the
    /// crash-window detection's `[queued_at − 60s, now + 60s]`, so they
    /// read the same timeline the engine's stamps do (the fixture's
    /// `createdAt` on minted cards — an absent field would decode to
    /// `MIN_UTC` and fail the window safe, silently defeating detection).
    clock: Option<Arc<dyn Clock>>,
}

impl DeckSim {
    fn new(clock: Arc<dyn Clock>) -> Self {
        Self {
            now: Arc::new(std::sync::atomic::AtomicI64::new(T + 1)),
            clock: Some(clock),
            ..Self::default()
        }
    }

    fn stamp(&self) -> i64 {
        let now_secs = match &self.clock {
            Some(clock) => clock.now().timestamp(),
            None => i64::MAX,
        };
        let prev = self
            .now
            .fetch_max(now_secs, std::sync::atomic::Ordering::SeqCst);
        prev.max(now_secs) + 1
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
                let stamp = sim5.stamp();
                let card = serde_json::json!({
                    "id": sim5.id(), "title": body["title"], "stackId": stack_num,
                    "type": "plain", "order": body["order"].as_i64().unwrap_or(0),
                    "description": body["description"].as_str().unwrap_or(""),
                    "createdAt": stamp, "lastModified": stamp, "labels": [],
                    "archived": false, "duedate": null, "done": null
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
    let server = std::sync::Arc::new(start_server().await);
    let clock: Arc<dyn Clock> = Arc::new(SteppingClock::new(ts(T + 1000), 1));
    let sim = DeckSim::new(Arc::clone(&clock));
    sim.mount(&server).await;
    // The board's live column exists server-side from the start.
    sim.stacks.lock().unwrap().push(serde_json::json!({
        "id": STACK, "title": "col", "boardId": BOARD, "order": 0,
        "lastModified": T, "deletedAt": 0, "archived": false, "cards": []
    }));

    let clock: Arc<dyn Clock> = Arc::new(SteppingClock::new(ts(T + 1000), 1));
    let stack = boot(server, seeded_state(), Arc::clone(&clock)).await;
    let (commands, _actor) = spawn_actor(&stack);
    // Deterministic: wait for the broadcast subscription (a structural
    // fact) instead of sleeping through the actor's first tick.
    wait_subscribed(&stack.system_tx).await;
    nudge_cycle(&commands).await;
    let board_id = *stack
        .engine
        .shared_state()
        .load()
        .boards
        .keys()
        .next()
        .expect("the seeded board");

    let CommandOutcome::CreatedStack(stack_id) = stack
        .engine
        .execute(taskboard_domain::StateCommand::CreateStack {
            board: board_id,
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
        stack
            .engine
            .shared_state()
            .load_full()
            .sync
            .aggregate()
            .phase
            == SyncPhase::Idle,
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
    let server = std::sync::Arc::new(start_server().await);
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
            board: RemoteBoardId0::deck(BOARD),
            stack: taskboard_domain::RemoteStackId::deck(STACK),
            card: taskboard_domain::RemoteCardId::deck(55),
        }),
        title: "stale local edit".into(),
        description: String::new(),
        duedate: None,
        scheduled: None,
        done: None,
        stack: stack_id,
        order: 0,
        label_memberships: std::collections::BTreeMap::new(),
        effort: None,
        importance: None,
        urgency: None,
        recur: None,
        kind: taskboard_domain::entities::TaskKind::Standard,

        archived: false,
        deleted: false,
        clocks: taskboard_domain::TaskClocks {
            title: ts(T + 10),
            description: ts(T),
            duedate: ts(T),
            scheduled: ts(T),
            done: ts(T),
            position: ts(T),
            effort: ts(T),
            importance: ts(T),
            urgency: ts(T),
            recur: ts(T),
            kind: ts(T),

            archived: ts(T),
            deleted: ts(T),
        },
        authors: taskboard_domain::entities::TaskAuthors::default(),
        remote_seen: Some(ts(T + 10)),
        remote_baseline: None,
    };
    state.tasks.insert(task.id, task);
    let clock: Arc<dyn Clock> = Arc::new(SteppingClock::new(ts(T + 1000), 1));
    let stack = boot(server, state, clock).await;
    let (commands, _actor) = spawn_actor(&stack);
    wait_subscribed(&stack.system_tx).await;
    nudge_cycle(&commands).await;

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
    let server = std::sync::Arc::new(start_server().await);
    let mut card = card_json(55, "older remote title");
    card["description"] = serde_json::json!("newer remote description");
    card["lastModified"] = serde_json::json!(T + 100); // newer than remote_seen
    mount_pull(&server, serde_json::json!([card])).await;

    let mut state = seeded_state();
    let stack_id = taskboard_domain::StackId::from(uuid(2));
    let task = taskboard_domain::Task {
        id: taskboard_domain::TaskId::from(uuid(100)),
        remote: Some(taskboard_domain::RemoteCardRef {
            board: RemoteBoardId0::deck(BOARD),
            stack: taskboard_domain::RemoteStackId::deck(STACK),
            card: taskboard_domain::RemoteCardId::deck(55),
        }),
        // Locally newer than anything remote: LWW keeps it.
        title: "local newer title".into(),
        description: "stale local description".into(),
        duedate: None,
        scheduled: None,
        done: None,
        stack: stack_id,
        order: 0,
        label_memberships: std::collections::BTreeMap::new(),
        effort: None,
        importance: None,
        urgency: None,
        recur: None,
        kind: taskboard_domain::entities::TaskKind::Standard,

        archived: false,
        deleted: false,
        clocks: taskboard_domain::TaskClocks {
            title: ts(T + 500),
            description: ts(T + 10),
            duedate: ts(T),
            scheduled: ts(T),
            done: ts(T),
            position: ts(T),
            effort: ts(T),
            importance: ts(T),
            urgency: ts(T),
            recur: ts(T),
            kind: ts(T),

            archived: ts(T),
            deleted: ts(T),
        },
        authors: taskboard_domain::entities::TaskAuthors::default(),
        remote_seen: Some(ts(T + 10)),
        remote_baseline: None,
    };
    state.tasks.insert(task.id, task);
    let clock: Arc<dyn Clock> = Arc::new(SteppingClock::new(ts(T + 1000), 1));
    let stack = boot(server, state, clock).await;
    let (commands, _actor) = spawn_actor(&stack);
    wait_subscribed(&stack.system_tx).await;
    nudge_cycle(&commands).await;

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
    let server = std::sync::Arc::new(start_server().await);
    let clock: Arc<dyn Clock> = Arc::new(SteppingClock::new(ts(T + 1000), 1));
    let sim = DeckSim::new(Arc::clone(&clock));
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
        "order": 0, "createdAt": T, "lastModified": T, "labels": [],
        "archived": false, "duedate": null, "done": null
    }));

    let mut state = seeded_state();
    // A second bound stack as the move target, a bound label, and the bound
    // task — each with a pending op.
    let stack2 = taskboard_domain::Stack {
        id: taskboard_domain::StackId::from(uuid(5)),
        remote: Some(taskboard_domain::RemoteStackRef {
            board: RemoteBoardId0::deck(BOARD),
            stack: taskboard_domain::RemoteStackId::deck(8),
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
        authors: taskboard_domain::entities::StackAuthors::default(),
        remote_seen: Some(ts(T)),
    };
    let label = taskboard_domain::Label {
        id: taskboard_domain::LabelId::from(uuid(6)),
        remote: Some(taskboard_domain::RemoteLabelRef {
            board: RemoteBoardId0::deck(BOARD),
            label: taskboard_domain::RemoteLabelId::deck(3),
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
        authors: taskboard_domain::entities::LabelAuthors::default(),
        remote_seen: Some(ts(T)),
    };
    let task_id = taskboard_domain::TaskId::from(uuid(100));
    let task = taskboard_domain::Task {
        id: task_id,
        remote: Some(taskboard_domain::RemoteCardRef {
            board: RemoteBoardId0::deck(BOARD),
            stack: taskboard_domain::RemoteStackId::deck(STACK),
            card: taskboard_domain::RemoteCardId::deck(55),
        }),
        title: "draft".into(),
        description: String::new(),
        duedate: None,
        scheduled: None,
        done: None,
        stack: taskboard_domain::StackId::from(uuid(5)),
        order: 1,
        label_memberships: std::collections::BTreeMap::from([(
            taskboard_domain::LabelId::from(uuid(6)),
            taskboard_domain::entities::MembershipState::Added { at: ts(T) },
        )]),
        effort: None,
        importance: None,
        urgency: None,
        recur: None,
        kind: taskboard_domain::entities::TaskKind::Standard,

        archived: false,
        deleted: false,
        clocks: taskboard_domain::TaskClocks {
            title: ts(T),
            description: ts(T),
            duedate: ts(T),
            scheduled: ts(T),
            done: ts(T),
            position: ts(T + 10),
            effort: ts(T + 10),
            importance: ts(T + 10),
            urgency: ts(T + 10),
            recur: ts(T + 10),
            kind: ts(T + 10),

            archived: ts(T),
            deleted: ts(T),
        },
        authors: taskboard_domain::entities::TaskAuthors::default(),
        remote_seen: Some(ts(T)),
        remote_baseline: None,
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
    let stack = boot(server, state, clock).await;
    let (commands, _actor) = spawn_actor(&stack);
    wait_subscribed(&stack.system_tx).await;
    nudge_cycle(&commands).await;

    eventually(&stack.engine, |app| app.sync.aggregate().pending_ops == 0).await;
    // The move and the label assignment both reached the server.
    eventually_no_state(|| {
        let ops = sim.ops.lock().unwrap();
        ops.contains(&"reorder".to_string())
            && ops.contains(&"put".to_string())
            && ops.contains(&"assign".to_string())
    })
    .await;
    {
        // Tier-2 verified (Deck 1.19): the card-update PUT rejects a
        // non-empty `labels` field, so the assignment rides the
        // assignLabel sub-endpoint after the field PUT.
        let cards = sim.cards.lock().unwrap();
        let card = cards
            .iter()
            .find(|c| c["id"].as_u64() == Some(55))
            .expect("the card is still listed");
        assert_eq!(card["stackId"], serde_json::json!(8), "the move landed");
        let labels = card["labels"].as_array().expect("labels array");
        assert!(
            labels.iter().any(|l| l.as_u64() == Some(3)),
            "the label was assigned via the sub-endpoint: {card}"
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
        app.sync.aggregate().pending_ops == 0
            && app
                .tasks
                .get(&task_id)
                .is_some_and(|t| t.deleted && t.remote.is_none())
    })
    .await;
}

async fn eventually_no_state(pred: impl Fn() -> bool) {
    let deadline = std::time::Instant::now() + WALL_DEADLINE;
    loop {
        if pred() {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "predicate not satisfied within the deadline"
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
    let server = std::sync::Arc::new(start_server().await);
    mount_conditional_pull(&server, &board_labels()).await;

    // First lifetime: one full cycle (cold cache → fresh data + validators
    // persisted by the engine).
    let clock: Arc<dyn Clock> = Arc::new(SteppingClock::new(ts(T + 1000), 1));
    let stack = boot(server, seeded_state(), clock).await;
    let (commands, first_actor) = spawn_actor(&stack);
    wait_subscribed(&stack.system_tx).await;
    nudge_cycle(&commands).await;
    eventually(&stack.engine, |app| {
        app.sync.aggregate().phase == SyncPhase::Idle && app.sync.aggregate().last_success.is_some()
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
    let first_success = stack
        .engine
        .shared_state()
        .load_full()
        .sync
        .aggregate()
        .last_success;
    eventually(&stack.engine, |app| {
        app.sync.aggregate().last_success.is_some()
            && app.sync.aggregate().phase == SyncPhase::Idle
            && app.sync.aggregate().last_success != first_success
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

/// I6 (DR-4/R, #150) — the crash-window duplicate journey: a create POST
/// completes server-side, the landing batch is lost to a crash, the
/// restart replays the create (a second card), and the pull reconciles
/// to one card — the first POST's — with the replayed twin deleted
/// through the 404-tolerant ref-delete and zero duplicate local rows.
/// Deterministic: shared stepping clock, `eventually` polling, no sleeps.
#[tokio::test(start_paused = true)]
#[allow(clippy::too_many_lines)] // the journey IS the scenario
async fn i6_crash_window_duplicate_reconciles_to_one_card() {
    let server = std::sync::Arc::new(start_server().await);
    // One clock for both lifetimes: the sim's stamps, the op's
    // `queued_at`, and the reports' `now` must share a timeline for the
    // detection window to hold.
    let clock: Arc<dyn Clock> = Arc::new(SteppingClock::new(ts(T + 1000), 1));
    let sim = DeckSim::new(Arc::clone(&clock));
    sim.mount(&server).await;
    sim.stacks.lock().unwrap().push(serde_json::json!({
        "id": STACK, "title": "col", "boardId": BOARD, "order": 0,
        "lastModified": T, "deletedAt": 0, "archived": false, "cards": []
    }));

    // Lifetime 1: create the task offline, then nudge one cycle — the
    // create POST lands server-side (card A).
    let stack = boot(Arc::clone(&server), seeded_state(), Arc::clone(&clock)).await;
    let (commands, actor1) = spawn_actor(&stack);
    wait_subscribed(&stack.system_tx).await;
    let stack_id = taskboard_domain::StackId::from(uuid(2));
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
    stack.engine.flush().await;
    // The crash point: the repo state BEFORE the cycle — the create op
    // queued, the task unbound.
    let pre_crash = stack.repo.snapshot();
    nudge_cycle(&commands).await;
    eventually_no_state(|| sim.cards.lock().unwrap().len() == 1).await;
    let card_a = sim.cards.lock().unwrap()[0]["id"].as_u64().unwrap();

    // The crash: drop actor and engine; the storage reboots on the
    // pre-cycle state — exactly "POST completed, landing batch lost".
    actor1.abort();
    stack.engine_join.abort();

    // Lifetime 2: restart on the restored repo; the sim still holds A.
    // The restarted engine's id counter starts past the restored rows'.
    let ids: Arc<dyn taskboard_domain::IdGenerator> = Arc::new(CountingIds::new());
    for _ in 0..100 {
        ids.new_op_id();
    }
    let stack = boot_with_ids(Arc::clone(&server), pre_crash, Arc::clone(&clock), ids).await;
    // The honesty rule's structural receiver: nothing may broadcast.
    let mut system_rx = stack.system_tx.subscribe();
    let (commands, _actor2) = spawn_actor(&stack);
    wait_subscribed(&stack.system_tx).await;
    nudge_cycle(&commands).await;

    // The replay POSTs a second card (B), the pull lists A + B, the
    // collapse rebinds T to A and ref-deletes B; the next cycle drains
    // the delete. One live task, bound to A's ref, outbox empty.
    eventually(&stack.engine, |app| {
        app.tasks.get(&task_id).is_some_and(|t| {
            t.is_live()
                && t.remote.is_some_and(|r| {
                    r.card.as_deck() == Some(card_a) && app.sync.aggregate().pending_ops == 0
                })
        })
    })
    .await;
    eventually_no_state(|| {
        let cards = sim.cards.lock().unwrap();
        cards.len() == 1 && cards[0]["id"].as_u64() == Some(card_a)
    })
    .await;
    stack.engine.flush().await;
    assert!(
        stack.repo.snapshot().outbox.is_empty(),
        "the create completed and the ref-delete drained"
    );
    assert!(
        sim.ops.lock().unwrap().contains(&"delete".to_string()),
        "the doomed replay twin was deleted server-side"
    );
    let app = stack.engine.shared_state().load_full();
    assert_eq!(
        app.tasks.values().filter(|t| t.is_live()).count(),
        1,
        "zero duplicate local rows"
    );
    assert_eq!(app.sync.aggregate().phase, SyncPhase::Idle);
    assert!(
        system_rx.try_recv().is_err(),
        "the honesty rule: no SystemEvent on any cycle of the journey"
    );
}

/// I7 (DR-4/R, #150) — the actor-tier boundary pin: with no create in
/// scope, a same-titled same-stack content-equal card is a stranger's
/// twin, not ours — the pull R1-adopts it unchanged and the unbound
/// local task stays untouched (the translator carries the stamps; the
/// policy lives in the engine).
#[tokio::test(start_paused = true)]
async fn i7_a_strangers_twin_card_adopts_without_a_create_in_scope() {
    let server = start_server().await;
    let mut card = card_json(55, "offline draft");
    card["createdAt"] = serde_json::json!(T + 5);
    card["lastModified"] = serde_json::json!(T + 5);
    mount_pull(&server, serde_json::json!([card])).await;

    let mut state = seeded_state();
    let task = taskboard_domain::Task {
        id: taskboard_domain::TaskId::from(uuid(100)),
        remote: None,
        title: "offline draft".into(),
        description: String::new(),
        duedate: None,
        scheduled: None,
        done: None,
        stack: taskboard_domain::StackId::from(uuid(2)),
        order: 0,
        label_memberships: std::collections::BTreeMap::new(),
        effort: None,
        importance: None,
        urgency: None,
        recur: None,
        kind: taskboard_domain::entities::TaskKind::Standard,
        archived: false,
        deleted: false,
        clocks: taskboard_domain::TaskClocks {
            title: ts(T),
            description: ts(T),
            duedate: ts(T),
            scheduled: ts(T),
            done: ts(T),
            position: ts(T),
            effort: ts(T),
            importance: ts(T),
            urgency: ts(T),
            recur: ts(T),
            kind: ts(T),
            archived: ts(T),
            deleted: ts(T),
        },
        authors: taskboard_domain::entities::TaskAuthors::default(),
        remote_seen: None,
        remote_baseline: None,
    };
    state.tasks.insert(task.id, task);
    let clock: Arc<dyn Clock> = Arc::new(SteppingClock::new(ts(T + 1000), 1));
    let stack = boot(Arc::new(server), state, clock).await;
    let (commands, _actor) = spawn_actor(&stack);
    wait_subscribed(&stack.system_tx).await;
    nudge_cycle(&commands).await;

    // The twin adopts as its own task; the local draft stays unbound.
    eventually(&stack.engine, |app| {
        app.tasks.len() == 2
            && app
                .tasks
                .get(&taskboard_domain::TaskId::from(uuid(100)))
                .is_some_and(|t| t.remote.is_none())
    })
    .await;
}

// ---- DR-6/3: adopt an occurrence card, learn the series ----------------

/// The reserved anchor title the document lane scans for.
const ANCHOR_TITLE: &str = "[taskboard] app-data";

/// I8: an occurrence card adopts as an ordinary task, and the other
/// device's sidecar document teaches it the series — the recur fact
/// rides the existing per-task appdata row (no new section). The fold
/// lands on the cycle *after* the adoption (§5b resolves cards through
/// the binding map, and a fresh adoption binds at §5) — the second nudge
/// pins that convergence, plus the repo persistence and the lane's
/// idempotence. No `SystemEvent` anywhere on the path.
#[tokio::test(start_paused = true)]
#[allow(clippy::too_many_lines)] // the mounts and the seed are the journey
async fn i8_adopting_an_occurrence_card_learns_its_series() {
    use taskboard_domain::appdata::{
        APPDATA_FORMAT, AppDataDocument, AppDataSections, ModeledValue, TaskEntry, TaskFieldWrite,
    };
    use taskboard_domain::ids::{RemoteBoardId, SeriesId};
    use taskboard_domain::recur::{EveryDays, RecurBound, RecurPattern, RecurRule};

    let server = Arc::new(start_server().await);
    let rule = RecurRule {
        series: SeriesId::from_uuid(uuid(300)),
        anchor: chrono::NaiveDate::from_ymd_opt(2026, 3, 1).unwrap(),
        pattern: RecurPattern::Daily {
            every: EveryDays::new(2).unwrap(),
        },
        bound: RecurBound::None,
        continued_from: None,
        paused_until: None,
    };
    // The document the other device flushed: the occurrence card's row
    // carries the rule under its write clock. Built through the real
    // document types — this is the JSON shape the flush uploads.
    let document = AppDataDocument {
        format: APPDATA_FORMAT,
        board: RemoteBoardId::deck(BOARD),
        sections: AppDataSections {
            habits: None,
            habit_log: None,
            tasks: Some(
                [(
                    "56".to_owned(),
                    TaskEntry {
                        fields: [(
                            "recur".to_owned(),
                            TaskFieldWrite {
                                value: ModeledValue::Recur(Some(rule.clone())),
                                clock: ts(T + 500),

                                by: None,
                            },
                        )]
                        .into(),
                        unmodeled: std::collections::BTreeMap::new(),
                    },
                )]
                .into(),
            ),
            ..Default::default()
        },
        unknown: std::collections::BTreeMap::new(),
    };

    // The pull: the anchor card (bound below) + the occurrence card.
    let anchor_card = serde_json::json!({
        "id": 55, "title": ANCHOR_TITLE, "stackId": STACK, "type": "plain",
        "order": 0, "lastModified": T, "labels": [], "archived": false,
        "duedate": null, "done": null
    });
    mount_pull(
        &server,
        serde_json::json!([anchor_card, card_json(56, "water the plants")]),
    )
    .await;
    // The anchor detail carries the reserved-filename attachment; the
    // download serves the document.
    let detail = serde_json::json!({
        "id": 55, "title": ANCHOR_TITLE, "stackId": STACK, "type": "plain",
        "order": 0, "lastModified": T, "labels": [], "archived": false,
        "duedate": null, "done": null,
        "attachments": [{
            "id": 11, "type": "deck_file", "cardId": 55,
            "createdAt": T, "lastModified": T, "deletedAt": 0,
            "extendedData": {
                "fileName": "taskboard-appdata.json", "filesize": 25,
                "mimetype": "application/json", "mtime": T
            }
        }]
    });
    Mock::given(method("GET"))
        .and(path(format!(
            "/index.php/apps/deck/api/v1.0/boards/{BOARD}/stacks/{STACK}/cards/55"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(detail))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!(
            "/index.php/apps/deck/api/v1.0/boards/{BOARD}/stacks/{STACK}/cards/55/attachments/11"
        )))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(serde_json::to_string(&document).unwrap()),
        )
        .mount(&server)
        .await;
    // The sidecar lane probes Deck's capabilities.
    Mock::given(method("GET"))
        .and(path("/ocs/v2.php/cloud/capabilities"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "ocs": {
                "meta": { "statuscode": 200 },
                "data": {
                    "version": { "major": 33 },
                    "capabilities": { "deck": { "version": "1.19.0" } },
                },
            }
        })))
        .mount(&server)
        .await;

    // The seed: bound board/stack + the anchor task bound to card 55.
    // No recur state anywhere.
    let mut state = seeded_state();
    state.tasks.insert(
        taskboard_domain::TaskId::from(uuid(100)),
        taskboard_domain::Task {
            id: taskboard_domain::TaskId::from(uuid(100)),
            remote: Some(taskboard_domain::RemoteCardRef {
                board: RemoteBoardId::deck(BOARD),
                stack: taskboard_domain::RemoteStackId::deck(STACK),
                card: taskboard_domain::RemoteCardId::deck(55),
            }),
            title: ANCHOR_TITLE.into(),
            description: String::new(),
            duedate: None,
            scheduled: None,
            done: None,
            stack: taskboard_domain::StackId::from(uuid(2)),
            order: 0,
            label_memberships: std::collections::BTreeMap::new(),
            effort: None,
            importance: None,
            urgency: None,
            recur: None,
            kind: taskboard_domain::entities::TaskKind::Standard,
            archived: false,
            deleted: false,
            clocks: taskboard_domain::TaskClocks {
                title: ts(T),
                description: ts(T),
                duedate: ts(T),
                scheduled: ts(T),
                done: ts(T),
                position: ts(T),
                effort: ts(T),
                importance: ts(T),
                urgency: ts(T),
                recur: ts(T),
                kind: ts(T),
                archived: ts(T),
                deleted: ts(T),
            },
            authors: taskboard_domain::entities::TaskAuthors::default(),
            remote_seen: Some(ts(T)),
            remote_baseline: None,
        },
    );
    let clock: Arc<dyn Clock> = Arc::new(SteppingClock::new(ts(T + 1000), 1));
    let stack = boot(server, state, clock).await;
    let (commands, _actor) = spawn_actor(&stack);
    wait_subscribed(&stack.system_tx).await;
    let mut events_rx = stack.system_tx.subscribe();

    // Cycle 1: the occurrence card adopts as its own task.
    nudge_cycle(&commands).await;
    eventually(&stack.engine, |app| {
        app.tasks
            .values()
            .any(|t| t.title == "water the plants" && t.remote.is_some())
    })
    .await;

    // Cycle 2: the document folds — the adopted task learns the series.
    nudge_cycle(&commands).await;
    eventually(&stack.engine, |app| {
        app.tasks
            .values()
            .any(|t| t.title == "water the plants" && t.recur.as_ref() == Some(&rule))
    })
    .await;

    // The engine's published state and the repository agree (the fold's
    // batch persisted through the real apply path).
    let persisted = stack.repo.load().await.expect("repo load");
    assert!(
        persisted
            .tasks
            .values()
            .any(|t| t.recur.as_ref() == Some(&rule))
    );

    // Honesty: the whole journey broadcast nothing.
    assert!(
        events_rx.try_recv().is_err(),
        "the document lane never broadcasts"
    );
}

// ---- I9/I10: the wiped-row strand (#1150) ------------------------------
//
// Board-carried ops whose board row was wiped strand outside both the
// cycle (no binding ⇒ no cycle) and the orphan lane (`owning_board`
// returned `Some` unconditionally). These pins watch the connector tier:
// the ops drain after exactly one cycle, and a `RemoteMissing` drain
// never re-pushes.

/// The strand harness: the healthy seeded board/stack (so a cycle
/// exists) plus five outbox ops addressing a board id whose row is
/// absent, and the wire routes the drain needs — the pull, the
/// capabilities probe, the flush's anchor create + first upload (the
/// wiped board's flush trigger rides the healthy board's sidecar lane),
/// and the two ref-delete DELETEs.
mod strand_seed {
    use super::*;
    use taskboard_domain::{LocalOp, OpId, PendingOp};

    /// The anchor-card create response (the convention-titled carrier).
    fn anchor_card_json(card: u64, stamp: i64) -> serde_json::Value {
        serde_json::json!({
            "id": card, "title": "[taskboard] app-data",
            "stackId": STACK, "type": "plain",
            "order": 0, "lastModified": stamp, "labels": [],
            "archived": false, "duedate": null, "done": null
        })
    }

    fn attachment_json(stamp: i64) -> serde_json::Value {
        serde_json::json!({
            "id": 11, "type": "deck_file", "cardId": 70,
            "createdAt": stamp, "lastModified": stamp, "deletedAt": 0,
            "extendedData": {
                "fileName": "taskboard-appdata.json", "filesize": 42,
                "mimetype": "application/json", "mtime": stamp
            }
        })
    }

    /// The wiped board: an id no row in `state.boards` carries.
    pub(crate) fn wiped_board() -> taskboard_domain::BoardId {
        taskboard_domain::BoardId::from(uuid(50))
    }

    /// The healthy seed plus the five stranded ops (op ids 300–304),
    /// all addressing the wiped board.
    pub(crate) fn state_with_stranded_ops() -> taskboard_domain::PersistedState {
        let mut state = seeded_state();
        let wiped = wiped_board();
        let stranded: [(u128, LocalOp); 5] = [
            (300, LocalOp::UpdateBoard(wiped)),
            (301, LocalOp::DeleteBoard(wiped)),
            (302, LocalOp::FlushBoardAppData(wiped)),
            (
                303,
                LocalOp::DeleteRemoteCard {
                    board: wiped,
                    card: taskboard_domain::RemoteCardRef {
                        board: RemoteBoardId0::deck(BOARD),
                        stack: taskboard_domain::RemoteStackId::deck(9),
                        card: taskboard_domain::RemoteCardId::deck(777),
                    },
                },
            ),
            (
                304,
                LocalOp::DeleteRemoteStack {
                    board: wiped,
                    stack: taskboard_domain::RemoteStackRef {
                        board: RemoteBoardId0::deck(BOARD),
                        stack: taskboard_domain::RemoteStackId::deck(8),
                    },
                },
            ),
        ];
        for (raw, op) in stranded {
            state.outbox.push(PendingOp {
                op_id: OpId(uuid(raw)),
                op,
                queued_at: ts(T),
            });
        }
        state
    }

    /// Two stranded ref-deletes only (the no-double-apply pin's seed).
    pub(crate) fn state_with_stranded_ref_deletes() -> taskboard_domain::PersistedState {
        let mut state = seeded_state();
        let wiped = wiped_board();
        for (raw, op) in [
            (
                303,
                LocalOp::DeleteRemoteCard {
                    board: wiped,
                    card: taskboard_domain::RemoteCardRef {
                        board: RemoteBoardId0::deck(BOARD),
                        stack: taskboard_domain::RemoteStackId::deck(9),
                        card: taskboard_domain::RemoteCardId::deck(777),
                    },
                },
            ),
            (
                304,
                LocalOp::DeleteRemoteStack {
                    board: wiped,
                    stack: taskboard_domain::RemoteStackRef {
                        board: RemoteBoardId0::deck(BOARD),
                        stack: taskboard_domain::RemoteStackId::deck(8),
                    },
                },
            ),
        ] {
            state.outbox.push(PendingOp {
                op_id: OpId(uuid(raw)),
                op,
                queued_at: ts(T),
            });
        }
        state
    }

    /// The routes a drain cycle addresses. `delete_status` is the
    /// ref-delete answer (200 = the remote row existed; 404 = already
    /// gone ⇒ `RemoteMissing`).
    pub(crate) async fn mount_drain_routes(server: &MockServer, delete_status: u16) {
        mount_pull(server, serde_json::json!([])).await;
        // The capabilities probe (the flush lane's feature gates).
        let body = serde_json::json!({
            "ocs": {
                "meta": { "statuscode": 200 },
                "data": {
                    "version": { "major": 33 },
                    "capabilities": { "deck": { "version": "1.19.0" } },
                },
            }
        });
        Mock::given(method("GET"))
            .and(path("/ocs/v2.php/cloud/capabilities"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(server)
            .await;
        // The flush's Create ladder on the healthy carrier: anchor card
        // create, then the first attachment upload.
        Mock::given(method("POST"))
            .and(path(format!(
                "/index.php/apps/deck/api/v1.0/boards/{BOARD}/stacks/{STACK}/cards"
            )))
            .respond_with(ResponseTemplate::new(200).set_body_json(anchor_card_json(70, T + 100)))
            .mount(server)
            .await;
        Mock::given(method("POST"))
            .and(path(format!(
                "/index.php/apps/deck/api/v1.0/boards/{BOARD}/stacks/{STACK}/cards/70/attachments"
            )))
            .respond_with(ResponseTemplate::new(200).set_body_json(attachment_json(T + 100)))
            .mount(server)
            .await;
        // The doomed remote card (stack 9, card 777) and stack (8): the
        // delete answers `delete_status`; the 404 re-resolve reads the
        // same listings (card 777 absent ⇒ `RemoteMissing`).
        Mock::given(method("DELETE"))
            .and(path(format!(
                "/index.php/apps/deck/api/v1.0/boards/{BOARD}/stacks/9/cards/777"
            )))
            .respond_with(
                ResponseTemplate::new(delete_status).set_body_json(serde_json::json!({
                    "id": 777, "title": "doomed", "stackId": 9, "type": "plain",
                    "order": 0, "lastModified": T, "labels": [], "archived": false,
                    "duedate": null, "done": null
                })),
            )
            .mount(server)
            .await;
        Mock::given(method("DELETE"))
            .and(path(format!(
                "/index.php/apps/deck/api/v1.0/boards/{BOARD}/stacks/8"
            )))
            .respond_with(
                ResponseTemplate::new(delete_status).set_body_json(serde_json::json!({
                    "id": 8, "title": "doomed stack", "boardId": BOARD, "order": 0,
                    "lastModified": T, "deletedAt": 0, "archived": false, "cards": []
                })),
            )
            .mount(server)
            .await;
    }

    /// The cycle's DELETE hit count (the ref-deletes' wire observable).
    pub(crate) async fn delete_hits(server: &MockServer) -> usize {
        server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .filter(|r| r.method == "DELETE")
            .count()
    }
}

/// I9 — the #1150 strand: five board-carried ops whose board row was
/// wiped all drain after exactly one cycle — the row-targeting ones
/// synthetically complete (the planner's missing-entity no-op), the
/// ref-deletes land on the wire. Nothing strands, and the healthy
/// board's cycle is otherwise unchanged.
#[tokio::test(start_paused = true)]
async fn i9_wiped_board_ops_drain_after_one_cycle() {
    let server = std::sync::Arc::new(start_server().await);
    strand_seed::mount_drain_routes(&server, 200).await;
    let clock: Arc<dyn Clock> = Arc::new(SteppingClock::new(ts(T + 1000), 1));
    let stack = boot(server, strand_seed::state_with_stranded_ops(), clock).await;
    let (commands, _actor) = spawn_actor(&stack);
    wait_subscribed(&stack.system_tx).await;
    nudge_cycle(&commands).await;

    // One cycle drains all five: no strand (this is the red→green
    // observable — pre-fix the ops sit in the outbox forever).
    eventually(&stack.engine, |app| app.sync.aggregate().pending_ops == 0).await;
    assert!(
        stack.repo.snapshot().outbox.is_empty(),
        "the engine persisted every completion"
    );
    // The ref-deletes reached the wire exactly once each.
    assert_eq!(strand_seed::delete_hits(&stack.server).await, 2);
}

/// I10 — the no-double-apply pin: a stranded ref-delete the server
/// answers 404 for completes as `RemoteMissing` and drains exactly
/// once — the second cycle performs no further pushes for it.
#[tokio::test(start_paused = true)]
async fn i10_remote_missing_ref_delete_drains_exactly_once() {
    let server = std::sync::Arc::new(start_server().await);
    strand_seed::mount_drain_routes(&server, 404).await;
    let clock: Arc<dyn Clock> = Arc::new(SteppingClock::new(ts(T + 1000), 1));
    let stack = boot(
        server,
        strand_seed::state_with_stranded_ref_deletes(),
        clock,
    )
    .await;
    let (commands, _actor) = spawn_actor(&stack);
    wait_subscribed(&stack.system_tx).await;
    nudge_cycle(&commands).await;

    // Cycle 1: both ref-deletes drain (each answered 404 ⇒ RemoteMissing).
    eventually(&stack.engine, |app| app.sync.aggregate().pending_ops == 0).await;
    let deletes_after_one = strand_seed::delete_hits(&stack.server).await;
    assert_eq!(deletes_after_one, 2, "one delete attempt per ref");

    // Cycle 2: nothing re-pushes — the drained ops are gone.
    nudge_cycle(&commands).await;
    eventually(&stack.engine, |app| {
        app.sync.aggregate().phase == SyncPhase::Idle
    })
    .await;
    assert_eq!(
        strand_seed::delete_hits(&stack.server).await,
        deletes_after_one,
        "a drained RemoteMissing op never re-pushes"
    );
}

// ---- C2: the orphan lane's zero-board leg (#1256) -----------------------
//
// The orphan law (plan 2026-11-24-sync-orphan-lanes §1): an outbox op
// whose owning entity row was wiped is cycle-level bookkeeping — the
// planner synthesizes a no-op completion for it, so it drains every
// cycle. #1256: a connector that owns zero boards still runs the
// standalone orphan drain; the ops never strand behind the empty-set
// early return.

/// The zero-board seed: no boards at all, one orphan op whose task row
/// is absent (its `owning_board` is `None`).
fn zero_board_state_with_orphan(op_raw: u128) -> taskboard_domain::PersistedState {
    let mut state = taskboard_domain::PersistedState::default();
    state.outbox.push(taskboard_domain::PendingOp {
        op_id: taskboard_domain::OpId(uuid(op_raw)),
        op: taskboard_domain::LocalOp::UpdateTask(taskboard_domain::TaskId::from(uuid(0xCAFE))),
        queued_at: ts(T),
    });
    state
}

/// #1256 — the zero-board standalone drain: one cycle drains the
/// orphan op with no board bound and without a single wire request
/// (the planner's synthetic completion contacts nothing — and the
/// code must execute whatever the planner produces, never assume it).
#[tokio::test(start_paused = true)]
async fn orphan_ops_drain_with_zero_owned_boards() {
    let server = std::sync::Arc::new(start_server().await);
    let clock: Arc<dyn Clock> = Arc::new(SteppingClock::new(ts(T + 1000), 1));
    let stack = boot(server.clone(), zero_board_state_with_orphan(300), clock).await;
    let (commands, _actor) = spawn_actor(&stack);
    wait_subscribed(&stack.system_tx).await;
    nudge_cycle(&commands).await;

    // One cycle drains the orphan: the engine persisted the completion
    // (the outbox is the oracle — with zero boards the sync aggregate
    // has no rows to count).
    eventually_no_state(|| stack.repo.snapshot().outbox.is_empty()).await;
    // A synthetic completion never addresses a write endpoint (the
    // boot-time capabilities probe is the only traffic).
    let requests = server.received_requests().await.unwrap_or_default();
    assert!(
        requests.iter().all(|req| req.method == "GET"),
        "the standalone drain is a local completion, not a wire cycle"
    );
}

/// #1256 — the empty-orphan guard: a zero-board cycle with no orphan
/// ops publishes nothing and changes nothing (the additive drain is a
/// no-op; the commanded `NoBoard` verdict keeps its shape).
#[tokio::test(start_paused = true)]
async fn zero_board_cycle_with_empty_orphan_lane_publishes_nothing() {
    let server = std::sync::Arc::new(start_server().await);
    let clock: Arc<dyn Clock> = Arc::new(SteppingClock::new(ts(T + 1000), 1));
    let stack = boot(server, taskboard_domain::PersistedState::default(), clock).await;
    let (commands, _actor) = spawn_actor(&stack);
    wait_subscribed(&stack.system_tx).await;
    nudge_cycle(&commands).await;

    // Structural: the outbox is (and stays) empty, the connector's
    // aggregate settles idle — nothing was published for the empty
    // orphan lane.
    eventually(&stack.engine, |app| {
        app.sync.aggregate().pending_ops == 0 && app.sync.aggregate().phase == SyncPhase::Idle
    })
    .await;
    assert_eq!(stack.repo.snapshot().outbox, Vec::new());
}

/// Pull listings for BOTH owned boards (42 and 43): the second board's
/// detail and stacks payloads mirror the seeded helpers' shapes.
async fn mount_two_board_pulls(server: &MockServer) {
    let two_boards = serde_json::json!([
        {"id": BOARD, "title": "board", "color": "00ff00",
         "lastModified": T, "deletedAt": 0, "archived": false, "labels": []},
        {"id": 43, "title": "second", "color": "00ff00",
         "lastModified": T, "deletedAt": 0, "archived": false, "labels": []}
    ])
    .to_string();
    let second_board = serde_json::json!({
        "id": 43, "title": "second", "color": "00ff00",
        "lastModified": T, "deletedAt": 0, "archived": false, "labels": []
    })
    .to_string();
    let second_stacks = serde_json::json!([{
        "id": STACK, "title": "col", "boardId": 43, "order": 0,
        "lastModified": T, "deletedAt": 0, "archived": false, "cards": []
    }])
    .to_string();
    for (p, body) in [
        (
            "/index.php/apps/deck/api/v1.0/boards".to_string(),
            two_boards,
        ),
        (
            "/index.php/apps/deck/api/v1.0/boards/43".to_string(),
            second_board,
        ),
        (
            "/index.php/apps/deck/api/v1.0/boards/43/stacks".to_string(),
            second_stacks,
        ),
        (
            "/index.php/apps/deck/api/v1.0/boards/43/stacks/archived".to_string(),
            "[]".to_string(),
        ),
    ] {
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path(p))
            .respond_with(
                wiremock::ResponseTemplate::new(200)
                    .insert_header("ETag", "\"gen-1\"")
                    .set_body_string(body),
            )
            .mount(server)
            .await;
    }
}

/// The seeded board plus a second owned board (deck 43), and one
/// orphan ref-delete whose board row is wiped (only the ref carries
/// the remote address).
fn two_board_state_with_orphan_ref_delete() -> taskboard_domain::PersistedState {
    let mut state = seeded_state();
    let board2 = taskboard_domain::Board {
        id: taskboard_domain::BoardId::from(uuid(3)),
        remote: Some(taskboard_domain::RemoteBoardBinding {
            connector: taskboard_domain::ConnectorId::from("nextcloud"),
            board: RemoteBoardId0::deck(43),
        }),
        title: "second".into(),
        color: taskboard_domain::Color::new("00ff00"),
        archived: false,
        deleted: false,
        clocks: taskboard_domain::BoardClocks {
            title: ts(T),
            color: taskboard_domain::BoardClocks::epoch(),
            archived: taskboard_domain::BoardClocks::epoch(),
            deleted: taskboard_domain::BoardClocks::epoch(),
        },
        authors: taskboard_domain::entities::BoardAuthors::default(),
        remote_seen: Some(ts(T)),
    };
    let stack2 = taskboard_domain::Stack {
        id: taskboard_domain::StackId::from(uuid(4)),
        remote: Some(taskboard_domain::RemoteStackRef {
            board: RemoteBoardId0::deck(43),
            stack: taskboard_domain::RemoteStackId::deck(STACK),
        }),
        board: board2.id,
        title: "col".into(),
        order: 0,
        archived: false,
        deleted: false,
        clocks: taskboard_domain::StackClocks {
            title: ts(T),
            order: ts(T),
            deleted: ts(T),
        },
        authors: taskboard_domain::entities::StackAuthors::default(),
        remote_seen: Some(ts(T)),
    };
    state.boards.insert(board2.id, board2);
    state.stacks.insert(stack2.id, stack2);
    let wiped = strand_seed::wiped_board();
    state.outbox.push(taskboard_domain::PendingOp {
        op_id: taskboard_domain::OpId(uuid(303)),
        op: taskboard_domain::LocalOp::DeleteRemoteCard {
            board: wiped,
            card: taskboard_domain::RemoteCardRef {
                board: RemoteBoardId0::deck(BOARD),
                stack: taskboard_domain::RemoteStackId::deck(9),
                card: taskboard_domain::RemoteCardId::deck(777),
            },
        },
        queued_at: ts(T),
    });
    state
}

/// #1256 — the existing ride is preserved: with two owned boards, the
/// orphan rides exactly the *first* board's pass — one drain, not one
/// per board (the ref-delete's wire observable counts the attempts).
#[tokio::test(start_paused = true)]
async fn owned_boards_cycle_still_rides_orphans_on_first_board() {
    let server = std::sync::Arc::new(start_server().await);
    mount_two_board_pulls(&server).await;
    strand_seed::mount_drain_routes(&server, 200).await;

    let state = two_board_state_with_orphan_ref_delete();
    let clock: Arc<dyn Clock> = Arc::new(SteppingClock::new(ts(T + 1000), 1));
    let stack = boot(server, state, clock).await;
    let (commands, _actor) = spawn_actor(&stack);
    wait_subscribed(&stack.system_tx).await;
    nudge_cycle(&commands).await;

    eventually(&stack.engine, |app| app.sync.aggregate().pending_ops == 0).await;
    let deletes_after_one = strand_seed::delete_hits(&stack.server).await;
    assert_eq!(
        deletes_after_one, 1,
        "the orphan rode the first board's pass exactly once"
    );

    // Cycle 2: drained ops never re-push.
    nudge_cycle(&commands).await;
    eventually(&stack.engine, |app| {
        app.sync.aggregate().phase == SyncPhase::Idle
    })
    .await;
    assert_eq!(
        strand_seed::delete_hits(&stack.server).await,
        deletes_after_one,
        "a drained orphan never re-pushes"
    );
}
