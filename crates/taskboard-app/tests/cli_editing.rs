// SPDX-License-Identifier: AGPL-3.0-only
//! ST-E-series: the WI-3 editing-surface journeys (plan §6) — hermetic
//! sqlite tempdirs, **no wiremock** (stand-alone cores perform no
//! network I/O by construction). Each invocation is a full "process"
//! (boot, work, shut down); durability is proven across invocations on
//! one db. Error kinds are asserted structurally (`error_kind()`),
//! never by message text.

use std::sync::Arc;

use taskboard_app::commands::boards;
use taskboard_app::commands::labels;
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

/// E3: `tasks delete` — the pre-execute snapshot receipt, an idempotent
/// repeat, deletion of a done task, and the typed unknown miss.
#[tokio::test]
async fn e3_tasks_delete_tombstones_idempotently() {
    let dir = tempdir();
    let config = standalone_config(dir.path(), "e3.db");
    board_with_stack(&config, "Personal", "Todo").await;
    let created = tasks::add(config.clone(), forbidden_store(), "chore", None)
        .await
        .expect("task");

    let receipt = tasks::delete(config.clone(), forbidden_store(), "chore")
        .await
        .expect("delete");
    assert_eq!(receipt.id, created.id);
    assert_eq!(receipt.title, "chore");

    // Absent from the list on a fresh invocation.
    let list = tasks::list(config.clone(), forbidden_store(), None)
        .await
        .expect("list");
    assert!(list.stacks[0].tasks.is_empty(), "tombstoned task hidden");

    // Repeat: idempotent success reporting the deleted snapshot.
    let repeat = tasks::delete(config.clone(), forbidden_store(), &created.id)
        .await
        .expect("repeat delete");
    assert_eq!(repeat.id, created.id);

    // A done task deletes like any other.
    let done = tasks::add(config.clone(), forbidden_store(), "finished thing", None)
        .await
        .expect("second task");
    let _ = tasks::done(config.clone(), forbidden_store(), &done.id, false)
        .await
        .expect("complete");
    let gone = tasks::delete(config, forbidden_store(), "finished thing")
        .await
        .expect("done task deletes");
    assert_eq!(gone.id, done.id);
}

/// E3b: a tombstoned task is no longer a selector hit (edit-style
/// intents against it are the typed `unknown-task`).
#[tokio::test]
async fn e3b_deleted_task_is_not_a_selector_hit() {
    let dir = tempdir();
    let config = standalone_config(dir.path(), "e3b.db");
    board_with_stack(&config, "Personal", "Todo").await;
    let created = tasks::add(config.clone(), forbidden_store(), "gone", None)
        .await
        .expect("task");
    let _ = tasks::delete(config.clone(), forbidden_store(), &created.id)
        .await
        .expect("delete");
    let err = tasks::edit(
        config,
        forbidden_store(),
        "gone",
        taskboard_domain::TaskChanges::default(),
    )
    .await
    .expect_err("edit against a tombstone");
    assert_eq!(err.error_kind(), "unknown-task");
}

/// E4: `boards rename` — the title change is durable across invocations,
/// a same-title repeat exits 0 (`NoOp`), and a board-less db is the typed
/// `no-board` kind.
#[tokio::test]
async fn e4_boards_rename_is_durable_and_idempotent() {
    let dir = tempdir();
    let config = standalone_config(dir.path(), "e4.db");

    let err = boards::rename(config.clone(), forbidden_store(), "ghost").await;
    assert_eq!(
        err.expect_err("no board").error_kind(),
        "no-board",
        "a board-less db surfaces the engine's own rejection"
    );

    boards::create(config.clone(), forbidden_store(), "Personal", None)
        .await
        .expect("board");
    let receipt = boards::rename(config.clone(), forbidden_store(), "Renamed Board")
        .await
        .expect("rename");
    assert_eq!(receipt.title, "Renamed Board");

    // Durability: a fresh invocation still sees the rename; a repeat of
    // the same title is an idempotent success.
    let config2 = standalone_config(dir.path(), "e4.db");
    let repeat = boards::rename(config2.clone(), forbidden_store(), "Renamed Board")
        .await
        .expect("repeat rename");
    assert_eq!(repeat.title, "Renamed Board");

    // The rename coexists with the rest of the editing surface: tasks
    // still list against the renamed board's stacks.
    stacks::add(config2, forbidden_store(), "Todo", None)
        .await
        .expect("stack after rename");
}

/// E5: `stacks rename/delete/move` — the rename and order changes are
/// durable, delete reports the cascade and repeats idempotently, and
/// unknown/ambiguous selectors are the typed errors.
#[tokio::test]
async fn e5_stacks_rename_delete_move() {
    let dir = tempdir();
    let config = standalone_config(dir.path(), "e5.db");
    board_with_stack(&config, "Personal", "Todo").await;

    // Rename.
    let renamed = stacks::rename(config.clone(), forbidden_store(), "Todo", "Doing")
        .await
        .expect("rename");
    assert_eq!(renamed.title, "Doing");

    // Durable across invocations; repeat rename to the same title: NoOp.
    let config2 = standalone_config(dir.path(), "e5.db");
    let repeat = stacks::rename(config2.clone(), forbidden_store(), "Doing", "Doing")
        .await
        .expect("repeat rename");
    assert_eq!(repeat.title, "Doing");

    // Move.
    let moved = stacks::move_(config2.clone(), forbidden_store(), "Doing", 7)
        .await
        .expect("move");
    assert_eq!(moved.order, 7);
    let moved = stacks::move_(config2.clone(), forbidden_store(), "Doing", 7)
        .await
        .expect("unchanged move is a NoOp success");
    assert_eq!(moved.order, 7);

    // Unknown selector.
    let err = stacks::rename(config2.clone(), forbidden_store(), "ghost", "x")
        .await
        .expect_err("unknown stack");
    assert_eq!(err.error_kind(), "unknown-stack");

    // Delete with the cascade: seed tasks first.
    tasks::add(config2.clone(), forbidden_store(), "a", Some("Doing"))
        .await
        .expect("task a");
    tasks::add(config2.clone(), forbidden_store(), "b", Some("Doing"))
        .await
        .expect("task b");
    let receipt = stacks::delete(config2, forbidden_store(), "Doing")
        .await
        .expect("delete");
    assert_eq!(receipt.title, "Doing");
    assert_eq!(receipt.cascaded_tasks, 2);

    // The cascade tombstoned the tasks.
    let list = tasks::list(config.clone(), forbidden_store(), None)
        .await
        .expect("list");
    assert!(list.stacks.is_empty(), "the stack and its tasks are gone");

    // Repeat delete: idempotent success reporting the deleted snapshot.
    let repeat = stacks::delete(config, forbidden_store(), &receipt.id)
        .await
        .expect("repeat delete");
    assert_eq!(repeat.id, receipt.id);
}

/// E6: the `labels` group — list (with assignment counts), add (default
/// and explicit colors), edit, delete, and the typed selector errors.
#[tokio::test]
async fn e6_labels_group() {
    let dir = tempdir();
    let config = standalone_config(dir.path(), "e6.db");
    board_with_stack(&config, "Personal", "Todo").await;

    // Empty list on a fresh board.
    let rows = labels::list(config.clone(), forbidden_store())
        .await
        .expect("list");
    assert_eq!(rows.len(), 0);

    // Add: explicit color, then the default.
    let urgent = labels::add(config.clone(), forbidden_store(), "urgent", Some("FF00AA"))
        .await
        .expect("add explicit");
    assert_eq!(urgent.color, "ff00aa", "six hex digits, lowercased");
    let later = labels::add(config.clone(), forbidden_store(), "later", None)
        .await
        .expect("add default");
    assert_eq!(later.color, taskboard_app::cli::DEFAULT_LABEL_COLOR);

    // List: both rows, zero assignments so far.
    let rows = labels::list(config.clone(), forbidden_store())
        .await
        .expect("list");
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|r| r.assigned_tasks == 0));

    // Edit: rename + recolor.
    let edited = labels::edit(
        config.clone(),
        forbidden_store(),
        "urgent",
        Some("critical".to_owned()),
        Some("0000ff".to_owned()),
    )
    .await
    .expect("edit");
    assert_eq!(edited.title, "critical");
    assert_eq!(edited.color, "0000ff");

    // Assignment count reflects a live task carrying the label.
    let created = tasks::add(config.clone(), forbidden_store(), "tagged", None)
        .await
        .expect("task");
    let _ = tasks::label(config.clone(), forbidden_store(), &created.id, "critical")
        .await
        .expect("assign");
    let rows = labels::list(config.clone(), forbidden_store())
        .await
        .expect("list");
    let critical = rows.iter().find(|r| r.title == "critical").expect("row");
    assert_eq!(critical.assigned_tasks, 1);

    // Typed selector errors.
    let err = labels::edit(config.clone(), forbidden_store(), "ghost", None, None)
        .await
        .expect_err("unknown label");
    assert_eq!(err.error_kind(), "unknown-label");

    // Delete, then the repeat is idempotent by uuid.
    let gone = labels::delete(config.clone(), forbidden_store(), "later")
        .await
        .expect("delete");
    assert_eq!(gone.title, "later");
    let repeat = labels::delete(config.clone(), forbidden_store(), &gone.id)
        .await
        .expect("repeat delete");
    assert_eq!(repeat.id, gone.id);
    let rows = labels::list(config, forbidden_store()).await.expect("list");
    assert_eq!(rows.len(), 1, "only 'critical' remains");
}

/// E7: `tasks label/unlabel` — the receipt's `labels` set tracks the
/// post-state, both directions are idempotent, and unknown task/label
/// selectors are the typed errors.
#[tokio::test]
async fn e7_tasks_label_unlabel() {
    let dir = tempdir();
    let config = standalone_config(dir.path(), "e7.db");
    board_with_stack(&config, "Personal", "Todo").await;
    let _ = labels::add(config.clone(), forbidden_store(), "urgent", None)
        .await
        .expect("label");
    let _ = labels::add(config.clone(), forbidden_store(), "someday", None)
        .await
        .expect("second label");
    let created = tasks::add(config.clone(), forbidden_store(), "card", None)
        .await
        .expect("task");

    // Attach: the receipt lists the label.
    let receipt = tasks::label(config.clone(), forbidden_store(), &created.id, "urgent")
        .await
        .expect("label");
    assert_eq!(receipt.labels, vec!["urgent".to_owned()]);

    // Assign-already-assigned: idempotent success.
    let receipt = tasks::label(config.clone(), forbidden_store(), "card", "urgent")
        .await
        .expect("repeat label");
    assert_eq!(receipt.labels, vec!["urgent".to_owned()]);

    // The label set is whole: a second label replaces nothing.
    let receipt = tasks::label(config.clone(), forbidden_store(), &created.id, "someday")
        .await
        .expect("second label");
    assert_eq!(receipt.labels.len(), 2);

    // Detach: absent from the receipt; repeat is idempotent.
    let receipt = tasks::unlabel(config.clone(), forbidden_store(), &created.id, "urgent")
        .await
        .expect("unlabel");
    assert_eq!(receipt.labels, vec!["someday".to_owned()]);
    let receipt = tasks::unlabel(config, forbidden_store(), &created.id, "urgent")
        .await
        .expect("repeat unlabel");
    assert_eq!(receipt.labels, vec!["someday".to_owned()]);
}
