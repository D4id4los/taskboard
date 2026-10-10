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

/// V1: `tasks add` with no bound board → typed `no-board`, exit class 1.
#[tokio::test]
async fn v1_add_without_a_board_is_no_board() {
    let dir = tempdir();
    let server = wiremock::MockServer::start().await;
    let config = config_for(&server.uri(), dir.path(), "v1.db");

    let err = tasks::add(config, store_for(&server.uri()), "t", None)
        .await
        .expect_err("no board bound");
    assert_eq!(err.error_kind(), "no-board");
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
    )
    .await
    .expect_err("rebind refused");
    assert_eq!(err.error_kind(), "board-already-bound");
    assert_eq!(err.error_detail().as_deref(), Some("999"));

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
