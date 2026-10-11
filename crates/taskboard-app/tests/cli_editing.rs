// SPDX-License-Identifier: AGPL-3.0-only
//! ST-E-series: the WI-3 editing-surface journeys (plan §6) — hermetic
//! sqlite tempdirs, **no wiremock** (stand-alone cores perform no
//! network I/O by construction). Each invocation is a full "process"
//! (boot, work, shut down); durability is proven across invocations on
//! one db. Error kinds are asserted structurally (`error_kind()`),
//! never by message text.

use std::sync::Arc;

use taskboard_app::commands::boards;
use taskboard_app::commands::stacks;
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

/// The one-board one-stack fixture every task journey starts from;
/// returns the config bound to the db.
async fn board_with_stack(config: &AppConfig, board: &str, stack: &str) {
    boards::create(config.clone(), forbidden_store(), board, None)
        .await
        .expect("board");
    stacks::add(config.clone(), forbidden_store(), stack, None)
        .await
        .expect("stack");
}

/// E1: the task selector cascade over a live db — exact uuid wins,
/// a unique ≥8-char prefix resolves, an exact title resolves, a miss is
/// the typed `unknown-task`, and duplicate titles are `ambiguous-task`.
#[tokio::test]
async fn e1_task_selector_cascade() {
    let dir = tempdir();
    let config = standalone_config(dir.path(), "e1.db");
    board_with_stack(&config, "Personal", "Todo").await;

    let created = tasks::add(config.clone(), forbidden_store(), "buy milk", None)
        .await
        .expect("task");
    let full_id = created.id.clone();
    let prefix = &full_id[..8];

    // Exact uuid.
    let done = tasks::done(config.clone(), forbidden_store(), &full_id, false)
        .await
        .expect("exact uuid");
    assert_eq!(done.id, full_id);
    assert!(done.done);

    // Unique 8-char prefix (undo via the same selector).
    let undone = tasks::done(config.clone(), forbidden_store(), prefix, true)
        .await
        .expect("unique prefix");
    assert_eq!(undone.id, full_id);
    assert!(!undone.done);

    // Exact title.
    let moved = tasks::move_(config.clone(), forbidden_store(), "buy milk", None, Some(0))
        .await
        .expect("exact title");
    assert_eq!(moved.id, full_id);

    // A miss is typed.
    let err = tasks::done(config.clone(), forbidden_store(), "no such thing", false)
        .await
        .expect_err("unknown");
    assert_eq!(err.error_kind(), "unknown-task");
}

/// E1b: duplicate titles resolve to `ambiguous-task` listing the
/// candidates; a longer (unique) prefix still resolves.
#[tokio::test]
async fn e1b_duplicate_titles_are_ambiguous_but_prefixes_disambiguate() {
    let dir = tempdir();
    let config = standalone_config(dir.path(), "e1b.db");
    board_with_stack(&config, "Personal", "Todo").await;

    let first = tasks::add(config.clone(), forbidden_store(), "dupe", None)
        .await
        .expect("first");
    let second = tasks::add(config.clone(), forbidden_store(), "dupe", None)
        .await
        .expect("second");

    let err = tasks::done(config.clone(), forbidden_store(), "dupe", false)
        .await
        .expect_err("ambiguous title");
    assert_eq!(err.error_kind(), "ambiguous-task");

    // A full uuid still targets exactly one task (scripts keep working).
    let done = tasks::done(config.clone(), forbidden_store(), &second.id, false)
        .await
        .expect("exact uuid beats the ambiguity");
    assert_eq!(done.id, second.id);

    // A prefix unique to the first task resolves it (the two were
    // created within the same run; their full uuids differ).
    let mut unique = String::new();
    for (a, b) in first.id.chars().zip(second.id.chars()) {
        unique.push(a);
        if a != b {
            break;
        }
    }
    let _ = tasks::done(config.clone(), forbidden_store(), &unique, false)
        .await
        .expect("unique prefix resolves the first task");
}

/// E2: `tasks edit` — title/description/due set + clear, archive
/// toggle, no-op repeat (exit-0 semantics: `NoOp` is not an error),
/// and the typed unknown-task miss.
#[tokio::test]
async fn e2_tasks_edit_sets_and_clears_fields() {
    use taskboard_domain::TaskChanges;

    let dir = tempdir();
    let config = standalone_config(dir.path(), "e2.db");
    board_with_stack(&config, "Personal", "Todo").await;
    let created = tasks::add(config.clone(), forbidden_store(), "draft", None)
        .await
        .expect("task");
    let _id = created.id.clone();

    // Title + description + due (bare date → midnight UTC).
    let due: chrono::DateTime<chrono::Utc> = chrono::NaiveDate::from_ymd_opt(2026, 10, 20)
        .and_then(|d| d.and_hms_opt(0, 0, 0))
        .expect("midnight")
        .and_utc();
    let receipt = tasks::edit(
        config.clone(),
        forbidden_store(),
        "draft",
        TaskChanges {
            title: Some("renamed".to_owned()),
            description: Some("the body".to_owned()),
            duedate: Some(Some(due)),
            archived: None,
        },
    )
    .await
    .expect("edit");
    assert_eq!(receipt.title, "renamed");
    assert!(receipt.labels.is_empty(), "receipt carries the label set");

    // Durability: a fresh invocation sees the edit (list view).
    let list = tasks::list(config.clone(), forbidden_store(), None)
        .await
        .expect("list");
    let row = &list.stacks[0].tasks[0];
    assert_eq!(row.title, "renamed");
    assert_eq!(row.duedate, Some(due));
    assert_eq!(row.stack_title, "Todo");

    // Clear the due date.
    let _cleared = tasks::edit(
        config.clone(),
        forbidden_store(),
        "renamed",
        TaskChanges {
            title: None,
            description: None,
            duedate: Some(None),
            archived: None,
        },
    )
    .await
    .expect("clear due");
    let list = tasks::list(config.clone(), forbidden_store(), None)
        .await
        .expect("list after clear");
    assert_eq!(list.stacks[0].tasks[0].duedate, None);

    // Identical repeat: an accepted no-op, not an error.
    let noop = tasks::edit(
        config.clone(),
        forbidden_store(),
        "renamed",
        TaskChanges {
            title: None,
            description: None,
            duedate: Some(None),
            archived: None,
        },
    )
    .await
    .expect("repeat edit is idempotent");
    assert_eq!(noop.title, "renamed");

    // Unknown selector: the typed miss.
    let err = tasks::edit(config, forbidden_store(), "ghost", TaskChanges::default())
        .await
        .expect_err("unknown task");
    assert_eq!(err.error_kind(), "unknown-task");
}
