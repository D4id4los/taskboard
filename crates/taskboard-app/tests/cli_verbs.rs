// SPDX-License-Identifier: AGPL-3.0-only
//! V-series: the CLI verbs over wiremock + real sqlite tempdirs
//! (plan §6). Each invocation is a full "process": boot the core, work,
//! shut down — durability is proven across invocations on one db.

use std::sync::Arc;
use std::time::Duration;

use serde_json::json;

use taskboard_app::commands::boards;
use taskboard_app::commands::sync::{self, SyncOutcome};
use taskboard_app::commands::tasks;
use taskboard_app::config::AppConfig;
use taskboard_app::secrets::CredentialStore;

mod common;

use common::{FakeCredentialStore, STACK, tempdir};

const SECRET: &str = "pw";

fn config_for(server: &str, dir: &std::path::Path, db: &str) -> AppConfig {
    let mut config = common::valid_config(server);
    config.sync.backoff_initial_secs = 1;
    config.sync.backoff_max_secs = 2;
    config.storage.db_path = Some(dir.join(db));
    config
}

fn store_for(server: &str) -> Arc<dyn CredentialStore> {
    Arc::new(FakeCredentialStore::with(server, "alice", SECRET))
}

const BOARDS_PATH: &str = "/index.php/apps/deck/api/v1.0/boards";
const BOARD_PATH: &str = "/index.php/apps/deck/api/v1.0/boards/3";
const STACKS_PATH: &str = "/index.php/apps/deck/api/v1.0/boards/3/stacks";
const STACKS_ARCHIVED_PATH: &str = "/index.php/apps/deck/api/v1.0/boards/3/stacks/archived";

async fn mount_get(server: &wiremock::MockServer, path: &str, body: String) {
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(path))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .insert_header("ETag", "\"gen-1\"")
                .set_body_string(body),
        )
        .mount(server)
        .await;
}

/// Full happy pull with the given stacks listing (one stack or two).
async fn mount_pull(server: &wiremock::MockServer, stacks_body: String) {
    mount_get(server, BOARDS_PATH, common::boards_json()).await;
    mount_get(server, BOARD_PATH, common::board_json()).await;
    mount_get(server, STACKS_PATH, stacks_body).await;
    mount_get(server, STACKS_ARCHIVED_PATH, "[]".to_owned()).await;
}

/// Two live stacks: `col1` (with one remote card) and `col2` (empty).
fn two_stacks_json() -> String {
    json!([
        {
            "id": STACK, "title": "col1", "boardId": 3, "order": 0,
            "lastModified": common::T, "deletedAt": 0, "archived": false,
            "cards": [common::card_json(1, "remote")]
        },
        {
            "id": 9, "title": "col2", "boardId": 3, "order": 1,
            "lastModified": common::T, "deletedAt": 0, "archived": false,
            "cards": []
        }
    ])
    .to_string()
}

/// V1: `tasks add` with no live board → the typed `ambiguous-board`
/// refusal with empty candidates (the sanctioned zero-boards mapping),
/// exit class 1.
#[tokio::test]
async fn v1_add_without_a_board_is_the_empty_ambiguity_refusal() {
    let dir = tempdir();
    let server = wiremock::MockServer::start().await;
    let config = config_for(&server.uri(), dir.path(), "v1.db");

    let err = tasks::add(config, store_for(&server.uri()), "t", None)
        .await
        .expect_err("no board bound");
    assert_eq!(err.error_kind(), "ambiguous-board");
    assert_eq!(err.error_detail().as_deref(), Some(""), "zero candidates");
}

/// V2: seeded board + pulled stack → add by exact title and by exact
/// uuid; a fresh invocation (second boot, same db) sees both tasks in
/// canonical order — the durability contract across processes.
#[tokio::test]
async fn v2_add_targets_stacks_and_survives_across_invocations() {
    let dir = tempdir();
    let server = wiremock::MockServer::start().await;
    mount_pull(&server, common::stacks_json(&json!([]))).await;
    let db = dir.path().join("v2.db");
    common::seed_board_binding(&db, 0x00DA_000E, common::BOARD, "v2").await;
    let store = store_for(&server.uri());
    let config = config_for(&server.uri(), dir.path(), "v2.db");

    // Add by exact stack title.
    let first = tasks::add(config.clone(), Arc::clone(&store), "first", Some("col"))
        .await
        .expect("add by title");
    assert_eq!(first.title, "first");
    assert_eq!(first.stack_title, "col");
    assert_eq!(first.order, 0);
    assert!(!first.done);

    // The id source for scripting: the list payload.
    let list = tasks::list(config.clone(), Arc::clone(&store), None)
        .await
        .expect("list");
    let stack_id = list.stacks[0].id.clone();
    assert_eq!(list.stacks[0].tasks.len(), 1);

    // Add by exact stack uuid.
    let second = tasks::add(
        config.clone(),
        Arc::clone(&store),
        "second",
        Some(&stack_id),
    )
    .await
    .expect("add by uuid");
    assert_eq!(second.order, 1, "appends after the canonical tail");

    // Fresh invocation, same db: both tasks durable, canonical order.
    let list = tasks::list(config, store, None).await.expect("list");
    let titles: Vec<&str> = list.stacks[0]
        .tasks
        .iter()
        .map(|t| t.title.as_str())
        .collect();
    assert_eq!(titles, vec!["first", "second"]);
}

/// V3: `done` then `done --undo` reflect in receipts; a repeat `done`
/// is an idempotent success (`NoOp` semantics, exit class 0).
#[tokio::test]
async fn v3_done_undo_and_idempotent_repeat() {
    let dir = tempdir();
    let server = wiremock::MockServer::start().await;
    mount_pull(&server, common::stacks_json(&json!([]))).await;
    let db = dir.path().join("v3.db");
    common::seed_board_binding(&db, 0x00DA_000E, common::BOARD, "v3").await;
    let store = store_for(&server.uri());
    let config = config_for(&server.uri(), dir.path(), "v3.db");

    let added = tasks::add(config.clone(), Arc::clone(&store), "t", Some("col"))
        .await
        .expect("add");
    let id = added.id.clone();

    let done = tasks::done(config.clone(), Arc::clone(&store), &id, false)
        .await
        .expect("done");
    assert!(done.done, "receipt reflects completion");
    assert_eq!(done.id, id);

    // Repeat: still a success, receipt reflects the (unchanged) state.
    let repeat = tasks::done(config.clone(), Arc::clone(&store), &id, false)
        .await
        .expect("repeat done succeeds");
    assert!(repeat.done);

    let undone = tasks::done(config.clone(), Arc::clone(&store), &id, true)
        .await
        .expect("undo");
    assert!(!undone.done, "receipt reflects reopening");
}

/// V4: `move` appends to the target stack (target max + 1) and honors
/// an explicit `--order`.
#[tokio::test]
async fn v4_move_appends_and_honors_explicit_order() {
    let dir = tempdir();
    let server = wiremock::MockServer::start().await;
    mount_pull(&server, two_stacks_json()).await;
    let db = dir.path().join("v4.db");
    common::seed_board_binding(&db, 0x00DA_000E, common::BOARD, "v4").await;
    let store = store_for(&server.uri());
    let config = config_for(&server.uri(), dir.path(), "v4.db");

    let added = tasks::add(config.clone(), Arc::clone(&store), "t1", Some("col1"))
        .await
        .expect("add to col1");
    let id = added.id.clone();

    // Move to the empty col2: appends → order 0.
    let moved = tasks::move_(config.clone(), Arc::clone(&store), &id, Some("col2"), None)
        .await
        .expect("move to col2");
    assert_eq!(moved.stack_title, "col2");
    assert_eq!(moved.order, 0);

    // Add another task to col2, then move t1 explicitly to position 0.
    let t2 = tasks::add(config.clone(), Arc::clone(&store), "t2", Some("col2"))
        .await
        .expect("add to col2");
    assert_eq!(t2.order, 1);
    let moved = tasks::move_(
        config.clone(),
        Arc::clone(&store),
        &id,
        Some("col2"),
        Some(0),
    )
    .await
    .expect("explicit order");
    assert_eq!(moved.order, 0);

    // Append after the adopted remote card in col1 (order 0) → 1.
    let t2_moved = tasks::move_(config, store, &t2.id, Some("col1"), None)
        .await
        .expect("append move");
    assert_eq!(t2_moved.stack_title, "col1");
    assert_eq!(t2_moved.order, 1);
}

use base64::Engine as _;

/// V5: the full happy cycle — a queued create is pushed by `sync`
/// (`Succeeded`, outbox drained), and the request authenticated as
/// `user:TASKBOARD_APP_PASSWORD` through the CLI seam.
#[tokio::test]
async fn v5_sync_pushes_the_queued_create_and_succeeds() {
    let dir = tempdir();
    let server = wiremock::MockServer::start().await;
    mount_pull(&server, common::stacks_json(&json!([]))).await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(
            "/index.php/apps/deck/api/v1.0/boards/3/stacks/8/cards",
        ))
        .and(wiremock::matchers::body_partial_json(json!({
            "title": "v5-task"
        })))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_string(common::card_json(42, "v5-task").to_string()),
        )
        .mount(&server)
        .await;
    let db = dir.path().join("v5.db");
    common::seed_board_binding(&db, 0x00DA_000E, common::BOARD, "v5").await;
    let store = store_for(&server.uri());
    let config = config_for(&server.uri(), dir.path(), "v5.db");

    tasks::add(config.clone(), Arc::clone(&store), "v5-task", Some("col"))
        .await
        .expect("add");

    let outcome = sync::run(config, store, false, Duration::from_secs(30))
        .await
        .expect("sync runs");
    let SyncOutcome::Succeeded(view) = outcome else {
        panic!("expected Succeeded, got {outcome:?}");
    };
    assert_eq!(view.pending_ops, 0, "the outbox drained");

    // Auth proof: every request carried Basic auth `alice:<secret>`.
    let requests = server.received_requests().await.expect("requests recorded");
    assert!(!requests.is_empty());
    let expected = format!("alice:{SECRET}");
    for request in &requests {
        let header = request
            .headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .expect("every request authenticates");
        let encoded = header.strip_prefix("Basic ").expect("basic scheme");
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .expect("base64 credentials");
        assert_eq!(
            String::from_utf8(decoded).as_deref(),
            Ok(expected.as_str()),
            "credentials are the CLI-configured user and secret"
        );
    }
}

/// V6: a failing server surfaces as `SyncFailed(server)`; an
/// unreachable server with a tiny budget surfaces as `TimedOut` (the
/// honest stale-identical-failure resolution).
#[tokio::test]
async fn v6_sync_failure_and_timeout_are_typed() {
    // 500s on every endpoint → a fresh Failed phase.
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .respond_with(wiremock::ResponseTemplate::new(500))
        .mount(&server)
        .await;
    let dir = tempdir();
    let db = dir.path().join("v6a.db");
    common::seed_board_binding(&db, 0x00DA_000E, common::BOARD, "v6").await;
    let config = config_for(&server.uri(), dir.path(), "v6a.db");
    let outcome = sync::run(
        config,
        store_for(&server.uri()),
        false,
        Duration::from_secs(30),
    )
    .await
    .expect("sync runs");
    let SyncOutcome::Failed(kind, _) = outcome else {
        panic!("expected Failed, got {outcome:?}");
    };
    assert_eq!(kind, taskboard_domain::SyncErrorKind::Server);

    // A cycle that stays in flight past the budget → TimedOut with the
    // final status reported (the honest "still running" resolution; an
    // instant transport failure would instead be a fresh `Failed`).
    let dir = tempdir();
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_delay(Duration::from_secs(10)))
        .mount(&server)
        .await;
    let db = dir.path().join("v6b.db");
    common::seed_board_binding(&db, 0x00DA_000E, common::BOARD, "v6b").await;
    let config = config_for(&server.uri(), dir.path(), "v6b.db");
    let outcome = sync::run(
        config,
        store_for(&server.uri()),
        false,
        Duration::from_secs(2),
    )
    .await
    .expect("sync runs");
    assert!(matches!(outcome, SyncOutcome::TimedOut(_)), "{outcome:?}");
}

/// V7: `boards select` pre-validates, boots, binds, waits for the
/// adoption, and the binding persists (a fresh list sees the stacks).
#[tokio::test]
async fn v7_select_binds_and_adopts() {
    let dir = tempdir();
    let server = wiremock::MockServer::start().await;
    mount_pull(&server, common::stacks_json(&json!([]))).await;
    let config = config_for(&server.uri(), dir.path(), "v7.db");
    let store = store_for(&server.uri());

    let receipt = boards::select(
        config.clone(),
        Arc::clone(&store),
        common::BOARD,
        Duration::from_secs(30),
        false,
    )
    .await
    .expect("select binds");
    assert_eq!(receipt.board.id, common::BOARD);
    assert_eq!(receipt.board.title, "board");

    // The adoption persisted: a fresh invocation sees the stacks.
    let list = tasks::list(config, store, None).await.expect("list");
    assert_eq!(list.stacks.len(), 1);
    assert_eq!(list.stacks[0].title, "col");
}

/// V8: an unknown board id is rejected *before* any core boot — the
/// mock records only the listing request, nothing deeper.
#[tokio::test]
async fn v8_select_unknown_board_never_boots() {
    let dir = tempdir();
    let server = wiremock::MockServer::start().await;
    mount_pull(&server, common::stacks_json(&json!([]))).await;
    let config = config_for(&server.uri(), dir.path(), "v8.db");

    let err = boards::select(
        config,
        store_for(&server.uri()),
        999,
        Duration::from_secs(5),
        false,
    )
    .await
    .expect_err("unknown board");
    assert_eq!(err.error_kind(), "unknown-board");

    let requests = server.received_requests().await.expect("requests recorded");
    for request in &requests {
        let path = request.url.path();
        assert_eq!(path, BOARDS_PATH, "only the listing endpoint was hit");
    }
}

/// V9: a different bound board refuses the rebind (`BoardAlreadyBound`
/// carrying the current id); re-selecting the bound id is a no-op
/// success.
#[tokio::test]
async fn v9_select_refuses_rebind_and_accepts_same_id() {
    let dir = tempdir();
    let server = wiremock::MockServer::start().await;
    mount_pull(&server, common::stacks_json(&json!([]))).await;
    let db = dir.path().join("v9.db");
    common::seed_board_binding(&db, 0x00DA_000E, 999, "v9").await;
    let config = config_for(&server.uri(), dir.path(), "v9.db");

    let err = boards::select(
        config,
        store_for(&server.uri()),
        common::BOARD,
        Duration::from_secs(5),
        false,
    )
    .await
    .expect_err("rebind refused");
    assert_eq!(err.error_kind(), "board-already-bound");
    let local = uuid::Uuid::from_u128(0x00DA_000E).to_string();
    assert_eq!(
        err.error_detail().as_deref(),
        Some(format!("{local} (bound remote 999)").as_str())
    );

    // Same id → idempotent success.
    let dir = tempdir();
    let server = wiremock::MockServer::start().await;
    mount_pull(&server, common::stacks_json(&json!([]))).await;
    let db = dir.path().join("v9b.db");
    common::seed_board_binding(&db, 0x00DA_000E, common::BOARD, "v9b").await;
    let config = config_for(&server.uri(), dir.path(), "v9b.db");
    let receipt = boards::select(
        config,
        store_for(&server.uri()),
        common::BOARD,
        Duration::from_secs(15),
        false,
    )
    .await
    .expect("same-id select succeeds");
    assert_eq!(receipt.board.id, common::BOARD);
}

/// V10: an ambiguous stack title and a stack-less board are typed
/// errors.
#[tokio::test]
async fn v10_ambiguous_and_missing_stacks_are_typed() {
    // Two live stacks sharing a title.
    let dir = tempdir();
    let server = wiremock::MockServer::start().await;
    let dup = json!([
        { "id": STACK, "title": "dup", "boardId": 3, "order": 0,
          "lastModified": common::T, "deletedAt": 0, "archived": false, "cards": [] },
        { "id": 9, "title": "dup", "boardId": 3, "order": 1,
          "lastModified": common::T, "deletedAt": 0, "archived": false, "cards": [] }
    ])
    .to_string();
    mount_pull(&server, dup).await;
    let db = dir.path().join("v10a.db");
    common::seed_board_binding(&db, 0x00DA_000E, common::BOARD, "v10a").await;
    let config = config_for(&server.uri(), dir.path(), "v10a.db");
    let err = tasks::add(config, store_for(&server.uri()), "t", Some("dup"))
        .await
        .expect_err("ambiguous");
    assert_eq!(err.error_kind(), "ambiguous-stack");
    // The candidates are structural: the two live stack titles.
    assert_eq!(err.error_detail().as_deref(), Some("dup, dup"));

    // Zero stacks, add without --stack.
    let dir = tempdir();
    let server = wiremock::MockServer::start().await;
    mount_pull(&server, "[]".to_owned()).await;
    let db = dir.path().join("v10b.db");
    common::seed_board_binding(&db, 0x00DA_000E, common::BOARD, "v10b").await;
    let config = config_for(&server.uri(), dir.path(), "v10b.db");
    let err = tasks::add(config, store_for(&server.uri()), "t", None)
        .await
        .expect_err("no stacks");
    assert_eq!(err.error_kind(), "no-stacks");
}

/// Seeds a persisted sync status into a db through the storage actor
/// (what earlier invocations leave behind — the sync status is
/// durable, unlike the binding).
async fn seed_sync_status(db: &std::path::Path, status: taskboard_domain::SyncStatus) {
    use taskboard_domain::persistence::TaskRepository as _;

    // A status row references its board (the per-board FK): the batch
    // seeds the unbound local board the row rides, board first. The
    // select ceremony's bind then adopts it, so the stale verdict sits
    // on the adopted board's row — the hydrated S0 the wait sees past.
    let board_id = taskboard_domain::BoardId::from_uuid(uuid::Uuid::from_u128(1));
    let board = taskboard_domain::Board {
        id: board_id,
        remote: None,
        title: "board".into(),
        color: taskboard_domain::Color::new("0000ff"),
        archived: false,
        deleted: false,
        clocks: taskboard_domain::BoardClocks {
            title: taskboard_domain::BoardClocks::epoch(),
            color: taskboard_domain::BoardClocks::epoch(),
            archived: taskboard_domain::BoardClocks::epoch(),
            deleted: taskboard_domain::BoardClocks::epoch(),
        },
        remote_seen: None,
    };
    let repo = Arc::new(
        taskboard_storage_sqlite::open(db, taskboard_domain::ConnectorId::from("nextcloud"))
            .await
            .expect("seed db"),
    );
    repo.apply(vec![
        taskboard_domain::PersistenceAction::UpsertBoard(board),
        taskboard_domain::PersistenceAction::UpsertSyncStatus(board_id, status),
    ])
    .await
    .expect("seed applies");
}

/// V11: a db carrying a *stale* persisted `Failed` status (e.g. from a
/// `sync` run before any binding — a board-less db never clears it)
/// must still adopt: the wait decides on fresh evidence, not the
/// hydrated snapshot.
#[tokio::test]
async fn v11_select_ignores_a_stale_persisted_failure() {
    let dir = tempdir();
    let server = wiremock::MockServer::start().await;
    mount_pull(&server, common::stacks_json(&json!([]))).await;
    let db = dir.path().join("v11.db");
    seed_sync_status(
        &db,
        taskboard_domain::SyncStatus {
            phase: taskboard_domain::SyncPhase::Failed {
                last_error: taskboard_domain::SyncErrorKind::NoBoard,
            },
            last_success: None,
            pending_ops: 0,
        },
    )
    .await;
    let config = config_for(&server.uri(), dir.path(), "v11.db");
    let store = store_for(&server.uri());

    let receipt = boards::select(config, store, common::BOARD, Duration::from_secs(30), true)
        .await
        .expect("stale failure must not block the adoption");
    assert_eq!(receipt.board.id, common::BOARD);
}

/// V12: selecting against an all-500 server with a seeded local board —
/// the bind adopts locally (no pull can run first), and the receipt
/// reports the hydrated failed status instead of pretending success.
#[tokio::test]
async fn v12_select_reports_the_hydrated_failure_status() {
    let dir = tempdir();
    let server = wiremock::MockServer::start().await;
    // Only the pre-validation listing succeeds; the binding cycle's
    // reads all fail.
    mount_get(&server, BOARDS_PATH, common::boards_json()).await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .respond_with(wiremock::ResponseTemplate::new(500))
        .mount(&server)
        .await;
    let db = dir.path().join("v12.db");
    seed_sync_status(
        &db,
        taskboard_domain::SyncStatus {
            phase: taskboard_domain::SyncPhase::Failed {
                last_error: taskboard_domain::SyncErrorKind::NoBoard,
            },
            last_success: None,
            pending_ops: 0,
        },
    )
    .await;
    let config = config_for(&server.uri(), dir.path(), "v12.db");
    let store = store_for(&server.uri());

    // With a local board present, the bind adopts it before any pull can
    // run, so the cold-adoption fresh failure of the singleton world is
    // unreachable: the receipt surfaces honestly with the hydrated
    // failed status (the fresh-failure path itself stays pinned by the
    // v6 sync-verb pair on a bound board).
    let receipt = boards::select(config, store, common::BOARD, Duration::from_secs(30), true)
        .await
        .expect("the local adoption succeeds without a pull");
    assert_eq!(receipt.board.id, common::BOARD);
    assert_eq!(receipt.sync.phase, "failed");
    assert_eq!(receipt.sync.error_kind.as_deref(), Some("no-board"));
}

/// V13: `sync --no-wait` nudges and reports the current status without
/// awaiting the cycle (`NoWait` outcome, current phase/pending visible).
#[tokio::test]
async fn v13_sync_no_wait_reports_without_awaiting() {
    let dir = tempdir();
    let server = wiremock::MockServer::start().await;
    mount_pull(&server, common::stacks_json(&json!([]))).await;
    let db = dir.path().join("v13.db");
    common::seed_board_binding(&db, 0x00DA_000E, common::BOARD, "v13").await;
    let config = config_for(&server.uri(), dir.path(), "v13.db");
    let store = store_for(&server.uri());

    tasks::add(config.clone(), Arc::clone(&store), "v13-task", Some("col"))
        .await
        .expect("add");

    let started = std::time::Instant::now();
    let outcome = sync::run(config, store, true, Duration::from_secs(30))
        .await
        .expect("sync runs");
    let SyncOutcome::NoWait(view) = outcome else {
        panic!("expected NoWait, got {outcome:?}");
    };
    assert_eq!(view.pending_ops, 1, "the create is still queued");
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "no-wait must not await the cycle"
    );
}

/// V14: task selectors are exact uuids — a non-uuid string is a typed
/// `unknown-task` before any core boots; a well-formed but unknown uuid
/// is the engine's typed rejection.
#[tokio::test]
async fn v14_done_and_move_reject_unknown_task_selectors() {
    let dir = tempdir();
    let server = wiremock::MockServer::start().await;
    mount_pull(&server, common::stacks_json(&json!([]))).await;
    let db = dir.path().join("v14.db");
    common::seed_board_binding(&db, 0x00DA_000E, common::BOARD, "v14").await;
    let config = config_for(&server.uri(), dir.path(), "v14.db");
    let store = store_for(&server.uri());

    // Since the WI-3 cascade, resolution happens against the published
    // state inside the booted core: a selector that matches nothing (be
    // it uuid-shaped or free text) surfaces as the same typed
    // `unknown-task` kind after the boot cycle.
    let err = tasks::done(config.clone(), Arc::clone(&store), "not-a-uuid", false)
        .await
        .expect_err("non-uuid selector");
    assert_eq!(err.error_kind(), "unknown-task");
    assert_eq!(err.error_detail().as_deref(), Some("not-a-uuid"));

    // Well-formed but unknown uuid: the same typed miss.
    let missing = uuid::Uuid::new_v4().to_string();
    let err = tasks::done(config.clone(), Arc::clone(&store), &missing, false)
        .await
        .expect_err("unknown uuid");
    assert_eq!(err.error_kind(), "unknown-task");

    let err = tasks::move_(config, Arc::clone(&store), &missing, Some("col"), None)
        .await
        .expect_err("unknown uuid move");
    assert_eq!(err.error_kind(), "unknown-task");
}

/// V15: `boards color` sets the color and `boards archive`/`unarchive`
/// flip the flag; the receipt carries the post-edit projection. A
/// malformed color is a clap usage error, never a dispatch.
#[tokio::test]
#[allow(clippy::items_after_statements)] // the clap boundary probe is local color
async fn v15_board_color_and_archive_edit_receipts() {
    use clap::Parser as _;
    use clap::error::ErrorKind as ClapKind;
    let dir = tempdir();
    let server = wiremock::MockServer::start().await;
    let store = store_for(&server.uri());
    mount_pull(&server, "[]".to_owned()).await;

    let config = config_for(&server.uri(), dir.path(), "v15.db");
    boards::create(config.clone(), Arc::clone(&store), "mine", Some("3366cc"))
        .await
        .expect("create");

    let receipt = boards::color(
        config_for(&server.uri(), dir.path(), "v15.db"),
        Arc::clone(&store),
        None,
        taskboard_domain::Color::new("00c2e0"),
    )
    .await
    .expect("color set");
    assert_eq!(receipt.color, "00c2e0");

    let receipt = boards::archive(
        config_for(&server.uri(), dir.path(), "v15.db"),
        Arc::clone(&store),
        None,
        true,
    )
    .await
    .expect("archive");
    assert_eq!(receipt.title, "mine");

    let receipt = boards::archive(
        config_for(&server.uri(), dir.path(), "v15.db"),
        Arc::clone(&store),
        None,
        false,
    )
    .await
    .expect("unarchive");

    // Unarchive on a live board is an accepted no-op receipt (exit 0).
    let again = boards::archive(
        config_for(&server.uri(), dir.path(), "v15.db"),
        store,
        None,
        false,
    )
    .await
    .expect("repeat unarchive");
    assert_eq!(again, receipt);

    // The malformed color is a usage error at the boundary; the valid
    // form parses (normalized to lowercase) into the dispatch payload.
    let err = taskboard_app::cli::Cli::try_parse_from(["taskboard", "boards", "color", "not-hex"])
        .expect_err("malformed color");
    assert_eq!(err.kind(), ClapKind::ValueValidation);
    let ok = taskboard_app::cli::Cli::try_parse_from(["taskboard", "boards", "color", "00C2E0"])
        .expect("valid color parses");
    let Some(taskboard_app::cli::Command::Boards {
        cmd: taskboard_app::cli::BoardsCmd::Color { board, color },
    }) = ok.command
    else {
        panic!("wrong command");
    };
    assert_eq!(board, None, "the flag stays optional");
    assert_eq!(color, "00c2e0", "the parser normalizes to lowercase");
}

/// V16: `boards delete` refuses without `--confirm` (typed, with the
/// counts) leaving the content intact, then lands the cascade with the
/// confirmed run: the second ceremony finds nothing to destroy.
#[tokio::test]
async fn v16_delete_confirmation_ceremony() {
    let dir = tempdir();
    let server = wiremock::MockServer::start().await;
    let store = store_for(&server.uri());
    mount_pull(&server, "[]".to_owned()).await;

    let config = config_for(&server.uri(), dir.path(), "v16.db");
    boards::create(config.clone(), Arc::clone(&store), "doomed", None)
        .await
        .expect("create");
    let stack = taskboard_app::commands::stacks::add(
        config_for(&server.uri(), dir.path(), "v16.db"),
        Arc::clone(&store),
        None,
        "col",
        None,
    )
    .await
    .expect("stack");
    tasks::add(
        config_for(&server.uri(), dir.path(), "v16.db"),
        Arc::clone(&store),
        "doomed task",
        Some(&stack.title),
    )
    .await
    .expect("task");

    // Unconfirmed: typed refusal; the content survives (the confirmed
    // run below still finds the stack and the task).
    let err = boards::delete(
        config_for(&server.uri(), dir.path(), "v16.db"),
        Arc::clone(&store),
        None,
        false,
    )
    .await
    .expect_err("unconfirmed delete");
    assert_eq!(err.error_kind(), "confirmation-required");
    assert_eq!(err.error_detail().as_deref(), Some("delete"));

    let receipt = boards::delete(
        config_for(&server.uri(), dir.path(), "v16.db"),
        Arc::clone(&store),
        None,
        true,
    )
    .await
    .expect("confirmed delete");
    assert_eq!(receipt.verb, "delete");
    assert_eq!(receipt.stacks, 1);
    assert_eq!(receipt.tasks, 1);
    assert_eq!(receipt.labels, 0);
    assert_eq!(receipt.bound_remote, None, "local-only board");
    assert!(
        receipt.pending_ops >= 1,
        "the doomed entities' queued ops are counted, not zero by accident"
    );
}

/// V17: `boards reset` runs the same two-sided ceremony; the confirmed
/// repair clears the content and a repeat reports an all-zero scope.
#[tokio::test]
async fn v17_reset_confirmation_ceremony() {
    let dir = tempdir();
    let server = wiremock::MockServer::start().await;
    let store = store_for(&server.uri());
    mount_pull(&server, "[]".to_owned()).await;

    let config = config_for(&server.uri(), dir.path(), "v17.db");
    boards::create(config.clone(), Arc::clone(&store), "repair", None)
        .await
        .expect("create");
    let stack = taskboard_app::commands::stacks::add(
        config_for(&server.uri(), dir.path(), "v17.db"),
        Arc::clone(&store),
        None,
        "col",
        None,
    )
    .await
    .expect("stack");
    tasks::add(
        config_for(&server.uri(), dir.path(), "v17.db"),
        Arc::clone(&store),
        "task",
        Some(&stack.title),
    )
    .await
    .expect("task");

    let err = boards::reset(
        config_for(&server.uri(), dir.path(), "v17.db"),
        Arc::clone(&store),
        None,
        false,
    )
    .await
    .expect_err("unconfirmed reset");
    assert_eq!(err.error_kind(), "confirmation-required");

    let receipt = boards::reset(
        config_for(&server.uri(), dir.path(), "v17.db"),
        Arc::clone(&store),
        None,
        true,
    )
    .await
    .expect("confirmed reset");
    assert_eq!(receipt.verb, "reset");
    assert_eq!((receipt.stacks, receipt.tasks), (1, 1));

    // The repeat sees the already-empty scope (the engine's no-op) with
    // nothing left to lose.
    let repeat = boards::reset(
        config_for(&server.uri(), dir.path(), "v17.db"),
        store,
        None,
        true,
    )
    .await
    .expect("confirmed repeat");
    assert_eq!(
        (
            repeat.stacks,
            repeat.tasks,
            repeat.labels,
            repeat.pending_ops
        ),
        (0, 0, 0, 0)
    );
}

// ---- V18: the board-scoped vocabulary (`--board` flag, invocation
// default, typed refusals) over two-board cores -------------------------

/// Seeds two live boards on one db through `boards create` (which always
/// mints now), returning their uuids in creation (board-id) order.
async fn two_boards(
    server: &wiremock::MockServer,
    dir: &std::path::Path,
    db: &str,
) -> (AppConfig, String, String) {
    let config = config_for(&server.uri(), dir, db);
    let store = store_for(&server.uri());
    let a = boards::create(config.clone(), Arc::clone(&store), "alpha", None)
        .await
        .expect("board a");
    let b = boards::create(config.clone(), Arc::clone(&store), "beta", None)
        .await
        .expect("board b");
    (config, a.id, b.id)
}

/// The default resolution refuses typed when ≥2 live boards make the
/// pick ambiguous, listing both candidates in board-id order.
#[tokio::test]
async fn v18_unflagged_two_board_rename_is_the_ambiguity_refusal() {
    let dir = tempdir();
    let server = wiremock::MockServer::start().await;
    mount_pull(&server, "[]".to_owned()).await;
    let (config, a, b) = two_boards(&server, dir.path(), "v18a.db").await;

    let err = boards::rename(config, store_for(&server.uri()), None, "x")
        .await
        .expect_err("two live boards");
    assert_eq!(err.error_kind(), "ambiguous-board");
    assert_eq!(
        err.error_detail().as_deref(),
        Some(format!("{a}, {b}").as_str())
    );
}

/// The `--board` flag targets the named board: the rename lands on it
/// and the other board is untouched.
#[tokio::test]
async fn v18_flagged_rename_targets_the_named_board() {
    let dir = tempdir();
    let server = wiremock::MockServer::start().await;
    mount_pull(&server, "[]".to_owned()).await;
    let (config, a, b) = two_boards(&server, dir.path(), "v18b.db").await;
    let store = store_for(&server.uri());

    let receipt = boards::rename(
        config.clone(),
        Arc::clone(&store),
        Some(taskboard_domain::BoardId::from_uuid(
            uuid::Uuid::parse_str(&a).unwrap(),
        )),
        "renamed alpha",
    )
    .await
    .expect("flagged rename");
    assert_eq!(receipt.id, a);
    assert_eq!(receipt.title, "renamed alpha");

    // A second invocation resolves each board independently: beta kept
    // its title, alpha kept its rename.
    let a_receipt = boards::rename(
        config.clone(),
        Arc::clone(&store),
        Some(taskboard_domain::BoardId::from_uuid(
            uuid::Uuid::parse_str(&a).unwrap(),
        )),
        "renamed alpha",
    )
    .await
    .expect("alpha re-read");
    assert_eq!(a_receipt.title, "renamed alpha");
    let b_receipt = boards::rename(
        config,
        Arc::clone(&store),
        Some(taskboard_domain::BoardId::from_uuid(
            uuid::Uuid::parse_str(&b).unwrap(),
        )),
        "beta",
    )
    .await
    .expect("beta re-read");
    assert_eq!(b_receipt.title, "beta");
}

/// A flagged id that names nothing / a tombstone refuses with the
/// engine's typed rejections before any command is constructed.
#[tokio::test]
async fn v18_flagged_unknown_and_deleted_boards_refuse_typed() {
    use taskboard_app::commands::CommandError;
    use taskboard_domain::CommandError as D;
    use taskboard_state::ExecuteError;

    let dir = tempdir();
    let server = wiremock::MockServer::start().await;
    mount_pull(&server, "[]".to_owned()).await;
    let (config, _a, b) = two_boards(&server, dir.path(), "v18c.db").await;
    let store = store_for(&server.uri());
    let stranger = taskboard_domain::BoardId::from_uuid(uuid::Uuid::from_u128(0x57_414_1E5));

    let err = boards::rename(config.clone(), Arc::clone(&store), Some(stranger), "x")
        .await
        .expect_err("unknown board");
    assert!(matches!(
        err,
        CommandError::Engine(ref boxed)
            if matches!(&**boxed, ExecuteError::Rejected(D::UnknownBoard(id)) if *id == stranger)
    ));

    // Tombstone beta, then the same flag refuses the deleted class.
    boards::delete(
        config.clone(),
        Arc::clone(&store),
        Some(taskboard_domain::BoardId::from_uuid(
            uuid::Uuid::parse_str(&b).unwrap(),
        )),
        true,
    )
    .await
    .expect("beta deleted");
    let err = boards::rename(
        config,
        store_for(&server.uri()),
        Some(taskboard_domain::BoardId::from_uuid(
            uuid::Uuid::parse_str(&b).unwrap(),
        )),
        "x",
    )
    .await
    .expect_err("deleted board");
    assert_eq!(err.error_kind(), "unknown-board");
    assert!(matches!(
        err,
        CommandError::Engine(ref boxed)
            if matches!(&**boxed, ExecuteError::Rejected(D::BoardDeleted(_)))
    ));
}

/// `stacks add` scopes the append order to the named board: two boards
/// with different stack maxima append independently.
#[tokio::test]
async fn v18_stack_add_scopes_the_append_order_to_the_named_board() {
    use taskboard_app::commands::stacks;
    let dir = tempdir();
    let server = wiremock::MockServer::start().await;
    mount_pull(&server, "[]".to_owned()).await;
    let (config, a, b) = two_boards(&server, dir.path(), "v18d.db").await;
    let store = store_for(&server.uri());
    let board = |uuid: &str| {
        Some(taskboard_domain::BoardId::from_uuid(
            uuid::Uuid::parse_str(uuid).unwrap(),
        ))
    };

    // alpha grows two stacks (orders 0, 1); beta only one (order 0).
    for title in ["a1", "a2"] {
        stacks::add(config.clone(), Arc::clone(&store), board(&a), title, None)
            .await
            .expect("alpha stack");
    }
    stacks::add(config.clone(), Arc::clone(&store), board(&b), "b1", None)
        .await
        .expect("beta stack");

    // The scoping pin: beta's append continues from beta's maximum, not
    // alpha's.
    let appended = stacks::add(config, Arc::clone(&store), board(&b), "b2", None)
        .await
        .expect("beta append");
    assert_eq!(appended.order, 1, "beta's own maximum + 1");
}

/// `labels add`/`labels list` scope by the named board.
#[tokio::test]
async fn v18_labels_scope_by_the_named_board() {
    use taskboard_app::commands::labels;
    let dir = tempdir();
    let server = wiremock::MockServer::start().await;
    mount_pull(&server, "[]".to_owned()).await;
    let (config, a, b) = two_boards(&server, dir.path(), "v18e.db").await;
    let store = store_for(&server.uri());
    let board = |uuid: &str| {
        Some(taskboard_domain::BoardId::from_uuid(
            uuid::Uuid::parse_str(uuid).unwrap(),
        ))
    };

    let _ = labels::add(
        config.clone(),
        Arc::clone(&store),
        board(&b),
        "beta-only",
        None,
    )
    .await
    .expect("beta label");

    let alpha_rows = labels::list(config.clone(), Arc::clone(&store), board(&a))
        .await
        .expect("alpha list");
    assert!(alpha_rows.is_empty(), "alpha does not see beta's label");
    let beta_rows = labels::list(config, Arc::clone(&store), board(&b))
        .await
        .expect("beta list");
    assert_eq!(beta_rows.len(), 1);
    assert_eq!(beta_rows[0].title, "beta-only");
}

/// The clap surface: `--board` parses on the board-scoped verbs, a
/// malformed uuid is a usage error (exit-2 class).
#[test]
fn v18_board_flag_parses_uuids_and_rejects_garbage() {
    use clap::Parser as _;
    use taskboard_app::cli::{BoardsCmd, Cli, LabelsCmd, StacksCmd};
    let good = "00000000-0000-0000-0000-000000000001";

    let verbs: Vec<Vec<&str>> = vec![
        vec!["taskboard", "boards", "rename", "--board", good, "t"],
        vec!["taskboard", "boards", "publish", "--board", good],
        vec!["taskboard", "boards", "color", "--board", good, "3366cc"],
        vec!["taskboard", "boards", "archive", "--board", good],
        vec!["taskboard", "boards", "unarchive", "--board", good],
        vec![
            "taskboard",
            "boards",
            "delete",
            "--board",
            good,
            "--confirm",
        ],
        vec!["taskboard", "boards", "reset", "--board", good, "--confirm"],
        vec!["taskboard", "stacks", "add", "--board", good, "t"],
        vec!["taskboard", "labels", "list", "--board", good],
        vec!["taskboard", "labels", "add", "--board", good, "t"],
    ];
    for argv in &verbs {
        let parsed = Cli::try_parse_from(argv).unwrap_or_else(|e| {
            panic!("{argv:?} must parse: {e}");
        });
        let has_board = match &parsed.command {
            Some(taskboard_app::cli::Command::Boards { cmd }) => matches!(
                cmd,
                BoardsCmd::Rename { board: Some(_), .. }
                    | BoardsCmd::Publish { board: Some(_), .. }
                    | BoardsCmd::Color { board: Some(_), .. }
                    | BoardsCmd::Archive { board: Some(_) }
                    | BoardsCmd::Unarchive { board: Some(_) }
                    | BoardsCmd::Delete { board: Some(_), .. }
                    | BoardsCmd::Reset { board: Some(_), .. }
            ),
            Some(taskboard_app::cli::Command::Stacks { cmd }) => {
                matches!(cmd, StacksCmd::Add { board: Some(_), .. })
            }
            Some(taskboard_app::cli::Command::Labels { cmd }) => {
                matches!(
                    cmd,
                    LabelsCmd::List { board: Some(_) } | LabelsCmd::Add { board: Some(_), .. }
                )
            }
            _ => false,
        };
        assert!(has_board, "{argv:?} must carry the parsed board");
    }

    for bad in ["not-a-uuid", "3"] {
        let err = Cli::try_parse_from(["taskboard", "boards", "rename", "--board", bad, "t"])
            .expect_err("malformed uuid");
        assert_eq!(err.exit_code(), 2, "{bad} is a usage error");
    }
}
