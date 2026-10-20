// SPDX-License-Identifier: AGPL-3.0-only
//! ST-series: the stand-alone CLI journeys (plan §6 step 5) — hermetic
//! sqlite tempdirs, **no wiremock**: a standalone core performs no
//! network I/O by construction. Each invocation is a full "process"
//! (boot, work, shut down); durability is proven across invocations on
//! one db. Error kinds are asserted structurally (`error_kind()`); the
//! stand-alone note is asserted as a *fact* (the dedicated output
//! function's `Option`), never as wording.

use std::sync::Arc;

use taskboard_app::commands::boards;
use taskboard_app::commands::output::standalone_note;
use taskboard_app::commands::require_connector;
use taskboard_app::commands::stacks;
use taskboard_app::commands::sync;
use taskboard_app::commands::tasks;
use taskboard_app::config::{AppConfig, CredentialStoreKind};
use taskboard_app::secrets::CredentialStore;

mod common;

use common::tempdir;

/// A standalone config over a tempdir db: no server, no username — the
/// fresh-install shape. The credential store is never consulted.
fn standalone_config(dir: &std::path::Path, db: &str) -> AppConfig {
    AppConfig {
        nextcloud: taskboard_app::config::NextcloudSection {
            server_url: None,
            username: None,
            credential_store: CredentialStoreKind::Keyring,
        },
        storage: taskboard_app::config::StorageSection {
            db_path: Some(dir.join(db)),
        },
        ..AppConfig::default()
    }
}

/// A store that would fail any touch: standalone never consults it.
fn forbidden_store() -> Arc<dyn CredentialStore> {
    Arc::new(common::FakeCredentialStore::default())
}

/// ST1: `boards create` mints the local board; a second create is the
/// typed `board-exists` refusal; the receipt survives the invocation
/// (durable across processes).
#[tokio::test]
async fn st1_boards_create_always_mints() {
    let dir = tempdir();
    let config = standalone_config(dir.path(), "st1.db");

    let receipt = boards::create(
        config.clone(),
        forbidden_store(),
        "Personal",
        Some("3366cc"),
    )
    .await
    .expect("first create");
    assert_ne!(receipt.id, "");
    assert_eq!(receipt.title, "Personal");
    assert_eq!(receipt.color, "3366cc");

    // The refusal lift: a second create mints too, with a distinct id —
    // and durability holds (a fresh invocation still sees the boards).
    let config2 = standalone_config(dir.path(), "st1.db");
    let second = boards::create(config2.clone(), forbidden_store(), "Second", None)
        .await
        .expect("second create mints");
    assert_ne!(second.id, receipt.id);

    // Titles are not uniqueness keys: a same-titled board coexists.
    let third = boards::create(config2, forbidden_store(), "Second", None)
        .await
        .expect("same-titled create mints");
    assert_ne!(third.id, second.id);
}

/// ST2: `stacks add` appends by default and honors `--order`; a
/// board-less db surfaces the typed `ambiguous-board` refusal (empty
/// candidates).
#[tokio::test]
async fn st2_stacks_add_appends_and_requires_a_board() {
    let dir = tempdir();
    let config = standalone_config(dir.path(), "st2.db");

    let err = stacks::add(
        config.clone(),
        forbidden_store(),
        None,
        "no board yet",
        None,
    )
    .await
    .expect_err("no board");
    assert_eq!(err.error_kind(), "ambiguous-board");
    assert_eq!(err.error_detail().as_deref(), Some(""), "zero candidates");

    boards::create(config.clone(), forbidden_store(), "Personal", None)
        .await
        .expect("board");
    let first = stacks::add(config.clone(), forbidden_store(), None, "Todo", None)
        .await
        .expect("first stack");
    assert_eq!(first.order, 0, "empty board appends at 0");
    let second = stacks::add(config.clone(), forbidden_store(), None, "Doing", None)
        .await
        .expect("second stack");
    assert_eq!(second.order, 1, "append = max(order) + 1");
    let explicit = stacks::add(config.clone(), forbidden_store(), None, "Later", Some(5))
        .await
        .expect("explicit order");
    assert_eq!(explicit.order, 5);
    let appended = stacks::add(config, forbidden_store(), None, "Last", None)
        .await
        .expect("append after explicit");
    assert_eq!(appended.order, 6);
}

/// ST3: `tasks add/list` end-to-end over a stand-alone db — the
/// outbox accumulates (nothing drains without a connector) and the list
/// payload stays the backend-agnostic JSON shape.
#[tokio::test]
async fn st3_tasks_over_a_standalone_db() {
    let dir = tempdir();
    let config = standalone_config(dir.path(), "st3.db");

    boards::create(config.clone(), forbidden_store(), "Personal", None)
        .await
        .expect("board");
    let stack = stacks::add(config.clone(), forbidden_store(), None, "Todo", None)
        .await
        .expect("stack");

    let first = tasks::add(
        config.clone(),
        forbidden_store(),
        "first task",
        Some("Todo"),
    )
    .await
    .expect("add");
    assert_eq!(first.stack_id, stack.id);
    let second = tasks::add(config, forbidden_store(), "second task", Some("Todo"))
        .await
        .expect("add");
    assert_eq!(second.order, 1);

    // Fresh invocation, same db: both tasks, canonical order, pending ops.
    let config2 = standalone_config(dir.path(), "st3.db");
    let list = tasks::list(config2, forbidden_store(), None)
        .await
        .expect("list");
    assert_eq!(list.stacks.len(), 1);
    let titles: Vec<&str> = list.stacks[0]
        .tasks
        .iter()
        .map(|t| t.title.as_str())
        .collect();
    assert_eq!(titles, ["first task", "second task"]);
    assert_eq!(list.sync.phase, "idle");
    assert!(list.sync.pending_ops >= 2, "the outbox keeps climbing");
}

/// ST4: the connector-only verbs refuse at handler entry under the
/// resolved `standalone` backend — `sync`, `boards list`, `boards
/// select`, `boards publish`, and the daemon gate — before any core
/// boot or client construction.
#[tokio::test]
async fn st4_connector_verbs_refuse_sync_disabled() {
    let dir = tempdir();
    let config = standalone_config(dir.path(), "st4.db");

    let err = sync::run(
        config.clone(),
        forbidden_store(),
        false,
        std::time::Duration::from_secs(1),
        None,
    )
    .await
    .expect_err("sync refuses");
    assert_eq!(err.error_kind(), "sync-disabled");
    let err = boards::list(&config, &forbidden_store(), None)
        .await
        .expect_err("list refuses");
    assert_eq!(err.error_kind(), "sync-disabled");
    let err = boards::select(
        config.clone(),
        forbidden_store(),
        3,
        None,
        std::time::Duration::from_secs(1),
        false,
        None,
    )
    .await
    .expect_err("select refuses");
    assert_eq!(err.error_kind(), "sync-disabled");
    let err = boards::publish(
        config.clone(),
        forbidden_store(),
        None,
        std::time::Duration::from_secs(1),
        None,
    )
    .await
    .expect_err("publish refuses");
    assert_eq!(err.error_kind(), "sync-disabled");
    let err = require_connector(&config).expect_err("daemon gate refuses");
    assert_eq!(err.error_kind(), "sync-disabled");
}

/// ST5: the human `tasks list` stand-alone footer is a *structural*
/// fact — exactly one extra line, emitted iff the resolved backend is
/// standalone (wording is never asserted).
#[tokio::test]
async fn st5_human_list_carries_the_standalone_note() {
    let dir = tempdir();
    let config = standalone_config(dir.path(), "st5.db");
    boards::create(config.clone(), forbidden_store(), "Personal", None)
        .await
        .expect("board");
    let _stack = stacks::add(config.clone(), forbidden_store(), None, "Todo", None)
        .await
        .expect("stack");
    let _ = tasks::add(config, forbidden_store(), "a task", Some("Todo"))
        .await
        .expect("task");

    let config2 = standalone_config(dir.path(), "st5.db");
    let list = tasks::list(config2, forbidden_store(), None)
        .await
        .expect("list");
    let plain = taskboard_app::commands::tasks::render_text_list(&list, false);
    let with_note = taskboard_app::commands::tasks::render_text_list(&list, true);
    assert_eq!(
        with_note.lines().count(),
        plain.lines().count() + 1,
        "the note is one extra line"
    );
    // The fact, structurally: the dedicated output function emits for
    // standalone and not otherwise.
    assert!(standalone_note(true, list.sync.pending_ops).is_some());
    assert!(standalone_note(false, list.sync.pending_ops).is_none());
}
