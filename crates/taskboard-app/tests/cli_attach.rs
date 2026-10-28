// SPDX-License-Identifier: AGPL-3.0-only
//! V-series extension: the attach ceremonies over wiremock (plan §6
//! step 7) — `boards select --bind-local-board` (migration onto an
//! existing remote board, union semantics visible locally) and `boards
//! publish` (fresh remote board; color fallback; typed refusals).

use std::sync::Arc;
use std::time::Duration;

use serde_json::json;

use taskboard_app::commands::boards;
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

async fn mount_pull_with_remote_stack(server: &wiremock::MockServer, stack_title: &str) {
    let stacks = json!([{
        "id": STACK, "title": stack_title, "boardId": 3, "order": 0,
        "lastModified": common::T, "deletedAt": 0, "archived": false,
        "cards": [common::card_json(1, "remote card")]
    }])
    .to_string();
    for (p, body) in [
        (BOARDS_PATH.to_owned(), common::boards_json()),
        (BOARD_PATH.to_owned(), common::board_json()),
        (STACKS_PATH.to_owned(), stacks),
        (STACKS_ARCHIVED_PATH.to_owned(), "[]".to_owned()),
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

/// Seeds an *unbound* local board plus one live unbound stack with its
/// `CreateStack` op already queued (the stand-alone shape: content the
/// server never knew).
///
/// # Panics
///
/// Never in practice: the fixtures use valid tempdir paths, uuids, and
/// timestamps; `expect` failures are test bugs.
#[allow(clippy::missing_panics_doc)]
pub async fn seed_local_board(db_path: &std::path::Path, local_id: u128, run: &str, color: &str) {
    let repo = Arc::new(
        taskboard_storage_sqlite::open(db_path, taskboard_domain::ConnectorId::from("nextcloud"))
            .await
            .expect("seed db"),
    );
    let (handle, join) = taskboard_storage_sqlite::spawn_storage_actor(repo);
    let board_id = taskboard_domain::BoardId::from_uuid(uuid::Uuid::from_u128(local_id));
    let stack_id = taskboard_domain::StackId::from_uuid(uuid::Uuid::from_u128(local_id + 1));
    let op_id = taskboard_domain::OpId(uuid::Uuid::from_u128(local_id + 2));
    let now = chrono::DateTime::from_timestamp(common::T, 0).expect("fixture timestamp");
    handle
        .apply(vec![
            taskboard_domain::PersistenceAction::UpsertBoard(taskboard_domain::Board {
                id: board_id,
                remote: None,
                title: run.to_owned(),
                color: taskboard_domain::Color::new(color),
                archived: false,
                deleted: false,
                clocks: taskboard_domain::BoardClocks {
                    title: taskboard_domain::BoardClocks::epoch(),
                    color: taskboard_domain::BoardClocks::epoch(),
                    archived: taskboard_domain::BoardClocks::epoch(),
                    deleted: taskboard_domain::BoardClocks::epoch(),
                },
                remote_seen: None,
            }),
            taskboard_domain::PersistenceAction::UpsertStack(taskboard_domain::Stack {
                id: stack_id,
                remote: None,
                board: board_id,
                title: format!("{run}-local-col"),
                order: 0,
                archived: false,
                deleted: false,
                clocks: taskboard_domain::StackClocks {
                    title: now,
                    order: now,
                    deleted: now,
                },
                remote_seen: None,
            }),
            taskboard_domain::PersistenceAction::EnqueueOp(taskboard_domain::PendingOp {
                op_id,
                op: taskboard_domain::LocalOp::CreateStack(stack_id),
                queued_at: now,
            }),
        ])
        .await
        .expect("seed applies");
    drop(handle);
    join.await.expect("seed actor exits");
}

/// V15: plain `boards select` refuses while a live local-only board
/// exists (`local-board-exists`); with `--bind-local-board` the
/// ceremony binds, the first cycle merges the remote stack alongside
/// the pushed local one, and the binding survives a fresh invocation.
#[tokio::test]
async fn v15_select_bind_local_board_migrates_and_refuses_without_the_flag() {
    let dir = tempdir();
    let server = wiremock::MockServer::start().await;
    mount_pull_with_remote_stack(&server, "remote col").await;
    // The queued local create drains onto the remote board.
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(STACKS_PATH))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_string(
                json!([{
                    "id": 9, "title": "v15-local-col", "boardId": 3, "order": 1,
                    "lastModified": common::T, "deletedAt": 0, "archived": false,
                    "cards": []
                }])
                .to_string(),
            ),
        )
        .mount(&server)
        .await;
    let db = dir.path().join("v15.db");
    seed_local_board(&db, 0x00DA_000E, "v15", "00c2e0").await;
    let config = config_for(&server.uri(), dir.path(), "v15.db");

    // Refusal without the flag (after pre-validation).
    let err = boards::select(
        config.clone(),
        store_for(&server.uri()),
        common::BOARD,
        None,
        Duration::from_secs(10),
        false,
        None,
    )
    .await
    .expect_err("plain select refuses");
    assert_eq!(err.error_kind(), "local-board-exists");

    // The migration path binds and merges (union by identity).
    let receipt = boards::select(
        config.clone(),
        store_for(&server.uri()),
        common::BOARD,
        None,
        Duration::from_secs(60),
        true,
        None,
    )
    .await
    .expect("bind ceremony succeeds");
    assert_eq!(receipt.board.id, common::BOARD);

    // Union visible in a fresh invocation's local state: the remote
    // stack was adopted alongside the (now-bound) local one.
    let list = tasks::list(config, store_for(&server.uri()), None)
        .await
        .expect("list");
    let titles: Vec<&str> = list.stacks.iter().map(|s| s.title.as_str()).collect();
    assert!(
        titles.contains(&"remote col") && titles.contains(&"v15-local-col"),
        "both identities survive the attach: {titles:?}"
    );
    assert_eq!(list.stacks.len(), 2, "identity, not titles, is the key");
}

/// V16: `boards publish` creates a fresh remote board from the local
/// one (defensive color fallback verified in the request body), binds,
/// and reports the cycle outcome; a board-less db surfaces `no-board`
/// and a bound board `board-already-bound`.
#[tokio::test]
async fn v16_publish_creates_binds_and_reports() {
    let dir = tempdir();
    let server = wiremock::MockServer::start().await;
    mount_pull_with_remote_stack(&server, "remote col").await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(BOARDS_PATH))
        .and(wiremock::matchers::body_partial_json(json!({
            "title": "v16",
            // The seeded local color is not valid hex → the fallback
            // constant must be what reaches the server.
            "color": "0082c9"
        })))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_string(common::board_json()))
        .mount(&server)
        .await;
    // The queued local create drains onto the fresh board.
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(STACKS_PATH))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_string(
                json!([{
                    "id": 9, "title": "v16-local-col", "boardId": 3, "order": 1,
                    "lastModified": common::T, "deletedAt": 0, "archived": false,
                    "cards": []
                }])
                .to_string(),
            ),
        )
        .mount(&server)
        .await;
    let db = dir.path().join("v16.db");
    seed_local_board(&db, 0x00DA_000E, "v16", "not-hex").await;
    let config = config_for(&server.uri(), dir.path(), "v16.db");

    let receipt = boards::publish(
        config.clone(),
        store_for(&server.uri()),
        None,
        Duration::from_secs(60),
        None,
    )
    .await
    .expect("publish succeeds");
    assert_eq!(receipt.board.id, common::BOARD);

    // A fresh invocation sees the bound board with both identities:
    // the adopted remote stack and the pushed local one.
    let list = tasks::list(config, store_for(&server.uri()), None)
        .await
        .expect("list");
    let titles: Vec<&str> = list.stacks.iter().map(|s| s.title.as_str()).collect();
    assert_eq!(list.stacks.len(), 2);
    assert!(titles.contains(&"remote col") && titles.contains(&"v16-local-col"));
}

/// V17: `boards publish` typed refusals — no live board is the
/// `ambiguous-board` refusal (empty candidates); a bound board is
/// `board-already-bound` with the local board in the detail. Neither
/// boots a cycle (no mounts needed beyond client construction).
#[tokio::test]
async fn v17_publish_refusals_are_typed() {
    let dir = tempdir();
    let server = wiremock::MockServer::start().await;

    // No board at all.
    let config = config_for(&server.uri(), dir.path(), "v17a.db");
    let err = boards::publish(
        config,
        store_for(&server.uri()),
        None,
        Duration::from_secs(5),
        None,
    )
    .await
    .expect_err("no board");
    assert_eq!(err.error_kind(), "ambiguous-board");
    assert_eq!(err.error_detail().as_deref(), Some(""));

    // Already bound.
    let db = dir.path().join("v17b.db");
    common::seed_board_binding(&db, 0x00DA_000E, common::BOARD, "v17b").await;
    let config = config_for(&server.uri(), dir.path(), "v17b.db");
    let err = boards::publish(
        config,
        store_for(&server.uri()),
        None,
        Duration::from_secs(5),
        None,
    )
    .await
    .expect_err("bound board");
    assert_eq!(err.error_kind(), "board-already-bound");
    let local = uuid::Uuid::from_u128(0x00DA_000E).to_string();
    assert_eq!(
        err.error_detail().as_deref(),
        Some(format!("{local} (bound remote 3)").as_str())
    );
}

// ---- V18–V21: the publish ceremony's W2 orphan sweep (#150) ---------

/// The created-board payload for the sweep fixtures: `id`/`lastModified`
/// controlled, the color the sweep matches on.
fn created_board_json(id: u64, last_modified: i64) -> String {
    json!({
        "id": id, "title": "v18", "color": "00ff00",
        "lastModified": last_modified, "deletedAt": 0,
        "archived": false, "labels": []
    })
    .to_string()
}

fn orphan_board_json(id: u64, last_modified: i64) -> String {
    json!([{
        "id": 3, "title": "unrelated", "color": "ffffff",
        "lastModified": common::T, "deletedAt": 0, "archived": false,
        "labels": []
    }, {
        "id": id, "title": "  V18 ", "color": "00ff00",
        "lastModified": last_modified, "deletedAt": 0,
        "archived": false, "labels": []
    }])
    .to_string()
}

/// The pull endpoints for the board the ceremony ends bound to.
async fn mount_pull_for(server: &wiremock::MockServer, board: u64, last_modified: i64) {
    let base = format!("/index.php/apps/deck/api/v1.0/boards/{board}");
    let detail = json!({
        "id": board, "title": "v18", "color": "00ff00",
        "lastModified": last_modified, "deletedAt": 0,
        "archived": false, "labels": []
    })
    .to_string();
    for (p, body) in [
        (base.clone(), detail),
        (format!("{base}/stacks"), "[]".to_owned()),
        (format!("{base}/stacks/archived"), "[]".to_owned()),
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

async fn delete_expectation(server: &wiremock::MockServer, board: u64) {
    wiremock::Mock::given(wiremock::matchers::method("DELETE"))
        .and(wiremock::matchers::path(format!(
            "/index.php/apps/deck/api/v1.0/boards/{board}"
        )))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_string(created_board_json(board, common::T)),
        )
        .expect(1)
        .mount(server)
        .await;
}

/// V18: an orphan from a crashed earlier publish attempt — same title
/// (normalized), same color, stamped inside the window but OLDER — is
/// adopted: the ceremony rebinds to it and deletes the just-created
/// board (keep-older, DEC-12).
#[tokio::test]
async fn v18_publish_adopts_an_older_orphan_and_deletes_the_newer() {
    let dir = tempdir();
    let server = wiremock::MockServer::start().await;
    // The just-created board (newer) and the crashed attempt's orphan
    // (older, inside the 10-minute window).
    let created_lm = common::T + 1_200;
    let orphan_lm = common::T + 900;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(BOARDS_PATH))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_string(created_board_json(30, created_lm)),
        )
        .mount(&server)
        .await;
    // One listing serves both the sweep and the cycle's boards read.
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(BOARDS_PATH))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_string(orphan_board_json(31, orphan_lm)),
        )
        .mount(&server)
        .await;
    mount_pull_for(&server, 31, orphan_lm).await;
    delete_expectation(&server, 30).await;
    // The queued CreateStack drains onto the adopted board (31).
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(
            "/index.php/apps/deck/api/v1.0/boards/31/stacks",
        ))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_string("[]"))
        .mount(&server)
        .await;

    let db = dir.path().join("v18.db");
    seed_local_board(&db, 0x00DA_0012, "v18", "00ff00").await;
    let config = config_for(&server.uri(), dir.path(), "v18.db");
    let receipt = boards::publish(
        config,
        store_for(&server.uri()),
        None,
        Duration::from_secs(60),
        None,
    )
    .await
    .expect("publish succeeds");
    assert_eq!(receipt.board.id, 31, "the older orphan wins identity");
    server.verify().await;
}

/// V19: an exact-stamp tie keeps the just-created board bound (the
/// DEC-5 tie rule, board vocabulary) and the orphan is deleted. (An
/// in-window orphan can never be strictly NEWER — the window's upper
/// bound is the created board's stamp — so the tie is the only
/// keep-the-binding case.)
#[tokio::test]
async fn v19_publish_keeps_the_binding_when_the_created_board_is_older() {
    let dir = tempdir();
    let server = wiremock::MockServer::start().await;
    let created_lm = common::T + 900;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(BOARDS_PATH))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_string(created_board_json(30, created_lm)),
        )
        .mount(&server)
        .await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(BOARDS_PATH))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_string(orphan_board_json(31, created_lm)),
        )
        .mount(&server)
        .await;
    mount_pull_for(&server, 30, created_lm).await;
    delete_expectation(&server, 31).await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(
            "/index.php/apps/deck/api/v1.0/boards/30/stacks",
        ))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_string("[]"))
        .mount(&server)
        .await;

    let db = dir.path().join("v19.db");
    seed_local_board(&db, 0x00DA_0013, "v18", "00ff00").await;
    let config = config_for(&server.uri(), dir.path(), "v19.db");
    let receipt = boards::publish(
        config,
        store_for(&server.uri()),
        None,
        Duration::from_secs(60),
        None,
    )
    .await
    .expect("publish succeeds");
    assert_eq!(receipt.board.id, 30, "the binding keeps the created board");
    server.verify().await;
}

/// V20: a same-title same-color board OUTSIDE the window is a near-miss
/// negative — neither board is deleted, the publish is unaffected.
#[tokio::test]
async fn v20_an_outside_window_orphan_is_left_alone() {
    let dir = tempdir();
    let server = wiremock::MockServer::start().await;
    let created_lm = common::T + 1_200;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(BOARDS_PATH))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_string(created_board_json(30, created_lm)),
        )
        .mount(&server)
        .await;
    // Same title/color, but stamped long before the window.
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(BOARDS_PATH))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_string(orphan_board_json(31, common::T - 100_000)),
        )
        .mount(&server)
        .await;
    mount_pull_for(&server, 30, created_lm).await;
    wiremock::Mock::given(wiremock::matchers::method("DELETE"))
        .and(wiremock::matchers::path(BOARDS_PATH.to_owned() + "/31"))
        .respond_with(wiremock::ResponseTemplate::new(200))
        .expect(0)
        .mount(&server)
        .await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(
            "/index.php/apps/deck/api/v1.0/boards/30/stacks",
        ))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_string("[]"))
        .mount(&server)
        .await;

    let db = dir.path().join("v20.db");
    seed_local_board(&db, 0x00DA_0014, "v18", "00ff00").await;
    let config = config_for(&server.uri(), dir.path(), "v20.db");
    let receipt = boards::publish(
        config,
        store_for(&server.uri()),
        None,
        Duration::from_secs(60),
        None,
    )
    .await
    .expect("publish succeeds");
    assert_eq!(receipt.board.id, 30);
    server.verify().await;
}

/// V21: a sweep failure (the listing read fails once) degrades to
/// today's behavior — the publish still succeeds.
#[tokio::test]
async fn v21_a_sweep_failure_never_fails_the_publish() {
    let dir = tempdir();
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(BOARDS_PATH))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_string(created_board_json(30, common::T)),
        )
        .mount(&server)
        .await;
    // The sweep's listing read fails once; the later reads (the cycle's
    // boards pull) answer fresh — wiremock prefers the latest mount, so
    // the 500-once mock sits in front of the healthy listing.
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(BOARDS_PATH))
        .respond_with(wiremock::ResponseTemplate::new(500))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(BOARDS_PATH))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_string(orphan_board_json(31, common::T)),
        )
        .mount(&server)
        .await;
    mount_pull_for(&server, 30, common::T).await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(
            "/index.php/apps/deck/api/v1.0/boards/30/stacks",
        ))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_string("[]"))
        .mount(&server)
        .await;

    let db = dir.path().join("v21.db");
    seed_local_board(&db, 0x00DA_0015, "v18", "00ff00").await;
    let config = config_for(&server.uri(), dir.path(), "v21.db");
    let receipt = boards::publish(
        config,
        store_for(&server.uri()),
        None,
        Duration::from_secs(60),
        None,
    )
    .await
    .expect("the publish survives the sweep failure");
    assert_eq!(receipt.board.id, 30);
}
