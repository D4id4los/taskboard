// SPDX-License-Identifier: AGPL-3.0-only
//! ST-series: the stand-alone CLI journeys (plan §6 step 5) — hermetic
//! sqlite tempdirs, **no wiremock**: a standalone core performs no
//! network I/O by construction. Each invocation is a full "process"
//! (boot, work, shut down); durability is proven across invocations on
//! one db. Error kinds are asserted structurally (`error_kind()`); the
//! stand-alone note is asserted as a *fact* (the dedicated output
//! function's `Option`), never as wording.

use taskboard_app::commands::boards;
use taskboard_app::commands::output::standalone_note;
use taskboard_app::commands::require_connector;
use taskboard_app::commands::stacks;
use taskboard_app::commands::sync;
use taskboard_app::commands::tasks;

use crate::common;

use common::{forbidden_store, standalone_config, tempdir};

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

// --- The `boards publish --to-files` journeys (#764, journey plan
// `2026-11-17-boards-publish-to-files-ux-plan.md`; the step-ID tokens
// are normative). The files-lane fixtures need a wiremock Nextcloud
// anchor: the lane's spawn gate resolves the account from the
// connector set, and the Deck module's first cycle pulls against it.

use std::sync::Arc;
use std::time::Duration;

use taskboard_app::commands::CommandError;
use taskboard_app::config::AppConfig;
use taskboard_app::secrets::CredentialStore;
use wiremock::MockServer;

use common::{EnvOverride, FakeCredentialStore, env_lock, mount_standard_pull, valid_config};

/// A files-lane config over a tempdir db: one Nextcloud connector (the
/// wiremock anchor) plus `[files_sync] enabled` — the lane gate
/// resolves by construction. Callers hold [`env_lock`] and plant the
/// `XDG_DATA_HOME` sandbox for the actor's device identity.
fn files_lane_config(server: &MockServer, dir: &std::path::Path, db: &str) -> AppConfig {
    let mut config = valid_config(&server.uri());
    config.storage.db_path = Some(dir.join(db));
    config.files_sync.enabled = true;
    // The fixture's own snappiness: every invocation is a full process
    // (boot + shutdown), and the dead-end mock leaves the files actor
    // backing off — a 1s per-actor join budget keeps the journey
    // tests' wall time out of the timeout lake. Terminal states are
    // asserted, never timing.
    config.app.shutdown_timeout_secs = 1;
    config
}

/// The data-dir seam sandbox: `dirs::data_dir()` honors
/// `XDG_DATA_HOME` on Linux; the files actor's `device.json` lands in
/// the tempdir instead of the machine's live application-data
/// directory.
struct DataDirSandbox {
    _override: EnvOverride,
    _lock: common::EnvGuard,
}

impl DataDirSandbox {
    fn new(dir: &std::path::Path) -> Self {
        let lock = env_lock();
        let override_ =
            EnvOverride::set("XDG_DATA_HOME", dir.to_str().expect("utf-8 tempdir path"));
        Self {
            _override: override_,
            _lock: lock,
        }
    }
}

/// One files-lane journey fixture: the wiremock anchor, the fake
/// credential store, the sandboxed data dir, and the lane-enabled
/// config. Each journey test is a full "process" per invocation, like
/// every suite in this file.
struct FilesJourney {
    /// Holds the tempdir (the db lives in it) for the fixture's
    /// lifetime; asserted through `config`, never read directly.
    #[allow(dead_code)]
    dir: tempfile::TempDir,
    /// Holds the data-dir sandbox for the fixture's lifetime.
    #[allow(dead_code)]
    data: DataDirSandbox,
    /// Holds the wiremock anchor alive for the fixture's lifetime;
    /// reachable through `config`, never read directly.
    #[allow(dead_code)]
    server: MockServer,
    store: Arc<dyn CredentialStore>,
    config: AppConfig,
}

async fn files_journey(db: &str) -> FilesJourney {
    let dir = tempdir();
    let data = DataDirSandbox::new(dir.path());
    let server = MockServer::start().await;
    mount_standard_pull(&server, serde_json::json!([])).await;
    let store: Arc<dyn CredentialStore> =
        Arc::new(FakeCredentialStore::with(&server.uri(), "alice", "pw"));
    let config = files_lane_config(&server, dir.path(), db);
    FilesJourney {
        dir,
        data,
        server,
        store,
        config,
    }
}

/// Reads the persisted binding of `board` from a fresh boot (the
/// CLI-side twin of the GUI seam test's exact-mint assertion shape).
async fn persisted_binding_of(
    config: &AppConfig,
    store: &Arc<dyn CredentialStore>,
    board_uuid: uuid::Uuid,
    identity: taskboard_app::bootstrap::BootIdentity,
) -> Option<(String, uuid::Uuid)> {
    let app = taskboard_app::bootstrap::bootstrap_in(
        config.clone(),
        Arc::clone(store),
        Arc::new(taskboard_app::notifications::LogSink),
        identity,
    )
    .await
    .expect("boots");
    let binding = app
        .engine
        .shared_state()
        .load()
        .boards
        .get(&taskboard_domain::BoardId::from_uuid(board_uuid))
        .expect("the board exists")
        .remote
        .as_ref()
        .map(|b| (b.connector.as_str().to_owned(), b.board.as_files()));
    app.shutdown().await;
    binding.map(|(connector, remote)| (connector, remote.expect("a files payload")))
}

/// Journey US-1/S1 (`us1_s1`): the parse boundary — `--to-files`
/// resolves alone (with `--connector` as `None` and the bare Deck
/// grammar unchanged), and the R-4 `--connector` pair refuses as a
/// typed clap conflict before any boot. (The journey plan's Test-root
/// is this file; the cli.rs-side parse module carries no duplicate.)
#[test]
fn us1_s1_to_files_parses_and_the_r4_connector_conflict_refuses_before_boot() {
    use clap::Parser as _;
    use taskboard_app::cli::Command;
    use taskboard_app::cli::{BoardsCmd, Cli};

    let parsed = Cli::try_parse_from(["taskboard", "boards", "publish", "--to-files"])
        .expect("--to-files alone parses");
    let Command::Boards {
        cmd:
            BoardsCmd::Publish {
                board,
                to_files,
                connector,
                timeout_secs,
            },
    } = parsed.command.expect("a subcommand parsed")
    else {
        panic!("the publish verb parsed into a different shape");
    };
    assert!(to_files, "the direction flag resolved");
    assert_eq!(board, None);
    assert_eq!(connector, None, "--to-files alone keeps --connector None");
    assert_eq!(timeout_secs, 60, "the settling budget default holds");

    let bare = Cli::try_parse_from(["taskboard", "boards", "publish"])
        .expect("the bare Deck publish grammar is unchanged");
    let Command::Boards {
        cmd: BoardsCmd::Publish { to_files, .. },
    } = bare.command.expect("a subcommand parsed")
    else {
        panic!("the bare publish verb parsed into a different shape");
    };
    assert!(!to_files, "bare publish stays the Deck migration path");

    let conflict = Cli::try_parse_from([
        "taskboard",
        "boards",
        "publish",
        "--to-files",
        "--connector",
        "x",
    ])
    .expect_err("the R-4 conflict refuses at the parse boundary");
    assert_eq!(
        conflict.kind(),
        clap::error::ErrorKind::ArgumentConflict,
        "a typed parse conflict, exit 2, nothing booted"
    );
}

/// Journey US-1/S2 (`us1_s2`): the verb prelude — the files lane
/// configured, the single live local board resolved with `--board`
/// omitted — and the ceremony proceeds to a committed bind.
#[tokio::test]
async fn us1_s2_prelude_resolves_the_lane_and_the_single_live_board_then_binds() {
    let j = files_journey("us1s2.db").await;
    let created = boards::create(j.config.clone(), Arc::clone(&j.store), "Personal", None)
        .await
        .expect("board");

    let receipt = boards::publish_to_files(
        j.config.clone(),
        Arc::clone(&j.store),
        None,
        Duration::from_secs(60),
    )
    .await
    .expect("the prelude holds and the ceremony commits");
    assert_eq!(
        receipt.board_id, created.id,
        "the resolved board is the one bound"
    );
}

/// Journey US-1/S3 (`us1_s3`): the exact files bind mint — the
/// persisted binding is the files connector with the board's own uuid
/// as the files remote id, the GUI ceremony's payload verbatim.
#[tokio::test]
async fn us1_s3_the_bind_is_the_exact_files_mint_persisted() {
    let j = files_journey("us1s3.db").await;
    let created = boards::create(j.config.clone(), Arc::clone(&j.store), "Personal", None)
        .await
        .expect("board");
    let board_uuid = uuid::Uuid::parse_str(&created.id).expect("create mints a uuid");

    boards::publish_to_files(
        j.config.clone(),
        Arc::clone(&j.store),
        None,
        Duration::from_secs(60),
    )
    .await
    .expect("the ceremony commits");

    let binding = persisted_binding_of(
        &j.config,
        &j.store,
        board_uuid,
        taskboard_app::bootstrap::BootIdentity::At(j.dir.path().to_owned()),
    )
    .await;
    assert_eq!(
        binding,
        Some(("files-sync".to_owned(), board_uuid)),
        "the mint: the files connector owns the binding, the remote id is the board's own uuid"
    );
}

/// Journey US-1/S4 (`us1_s4`): the receipt read-back — structural
/// fields naming the Files channel (the files remote id, the board's
/// live flags, the sync row), rendered from a fresh snapshot.
#[tokio::test]
async fn us1_s4_the_receipt_reads_back_the_files_binding_structurally() {
    let j = files_journey("us1s4.db").await;
    let created = boards::create(j.config.clone(), Arc::clone(&j.store), "Personal", None)
        .await
        .expect("board");
    let board_uuid = uuid::Uuid::parse_str(&created.id).expect("create mints a uuid");

    let receipt = boards::publish_to_files(
        j.config.clone(),
        Arc::clone(&j.store),
        None,
        Duration::from_secs(60),
    )
    .await
    .expect("the ceremony commits");
    assert_eq!(receipt.board_id, board_uuid.to_string());
    assert_eq!(
        receipt.files_remote_id,
        board_uuid.to_string(),
        "the mint: the files remote id is the board's own uuid"
    );
    assert_eq!(receipt.title, "Personal");
    assert!(!receipt.archived);
    assert!(!receipt.deleted);
}

/// Journey US-2/S1 (`us2_s1`): the files lane unconfigured — the typed
/// lane-disabled refusal, and the state tree is unchanged (the board
/// stays unbound; no `BindBoard` executed).
#[tokio::test]
async fn us2_s1_lane_disabled_refuses_before_any_command_and_the_tree_is_unchanged() {
    let dir = tempdir();
    let config = standalone_config(dir.path(), "us2s1.db");

    boards::create(config.clone(), forbidden_store(), "Personal", None)
        .await
        .expect("board");
    let err = boards::publish_to_files(
        config.clone(),
        forbidden_store(),
        None,
        Duration::from_secs(1),
    )
    .await
    .expect_err("no files lane");
    assert!(
        matches!(err, CommandError::SyncDisabled),
        "typed lane-disabled refusal"
    );

    // The state tree is unchanged: the board never bound (a fresh boot
    // reads the persisted state).
    let app = taskboard_app::bootstrap::bootstrap_in(
        config,
        forbidden_store(),
        Arc::new(taskboard_app::notifications::LogSink),
        taskboard_app::bootstrap::BootIdentity::At(dir.path().to_owned()),
    )
    .await
    .expect("boots");
    let unbound = app
        .engine
        .shared_state()
        .load()
        .boards
        .values()
        .all(|b| b.remote.is_none());
    app.shutdown().await;
    assert!(unbound, "no BindBoard executed under the refusal");
}

/// Journey US-2/S2 (`us2_s2`): a board that already carries a binding
/// refuses typed (`BoardAlreadyBound`) and the existing binding is
/// untouched — the files payload named in the refusal is the persisted
/// mint.
#[tokio::test]
async fn us2_s2_already_bound_refuses_typed_and_the_binding_is_untouched() {
    let j = files_journey("us2s2.db").await;
    let created = boards::create(j.config.clone(), Arc::clone(&j.store), "Personal", None)
        .await
        .expect("board");
    let board_uuid = uuid::Uuid::parse_str(&created.id).expect("create mints a uuid");
    boards::publish_to_files(
        j.config.clone(),
        Arc::clone(&j.store),
        None,
        Duration::from_secs(60),
    )
    .await
    .expect("the first publish binds");

    let err = boards::publish_to_files(
        j.config.clone(),
        Arc::clone(&j.store),
        Some(taskboard_domain::BoardId::from_uuid(board_uuid)),
        Duration::from_secs(60),
    )
    .await
    .expect_err("a bound board refuses");
    let CommandError::Engine(err) = err else {
        panic!("the bind-refused class expected, got a different error");
    };
    let taskboard_state::ExecuteError::Rejected(
        taskboard_domain::CommandError::BoardAlreadyBound { board, bound },
    ) = &*err
    else {
        panic!("BoardAlreadyBound expected, got a different rejection");
    };
    assert_eq!(board.as_uuid(), board_uuid);
    assert_eq!(
        bound.as_files(),
        Some(board_uuid),
        "the files payload named"
    );

    // The existing binding is untouched by the refusal.
    let binding = persisted_binding_of(
        &j.config,
        &j.store,
        board_uuid,
        taskboard_app::bootstrap::BootIdentity::At(j.dir.path().to_owned()),
    )
    .await;
    assert_eq!(
        binding,
        Some(("files-sync".to_owned(), board_uuid)),
        "the existing binding is untouched by the refusal"
    );
}

/// Journey US-2/S3 (`us2_s3`, parameterized: unknown, soft-deleted,
/// archived): the typed gone refusals — each class distinct, no
/// partial bind.
#[tokio::test]
async fn us2_s3_gone_boards_refuse_typed_unknown_deleted_archived() {
    let j = files_journey("us2s3.db").await;

    // Unknown: a uuid no board carries.
    let unknown = uuid::Uuid::now_v7();
    let err = boards::publish_to_files(
        j.config.clone(),
        Arc::clone(&j.store),
        Some(taskboard_domain::BoardId::from_uuid(unknown)),
        Duration::from_secs(1),
    )
    .await
    .expect_err("unknown board");
    assert!(
        matches!(err, CommandError::Engine(ref e)
            if matches!(&**e, taskboard_state::ExecuteError::Rejected(
                taskboard_domain::CommandError::UnknownBoard(_)))),
        "typed unknown-board refusal"
    );

    // Soft-deleted: the tombstoned board's class.
    let created = boards::create(j.config.clone(), Arc::clone(&j.store), "Doomed", None)
        .await
        .expect("board");
    let doomed =
        taskboard_domain::BoardId::from_uuid(uuid::Uuid::parse_str(&created.id).expect("uuid"));
    boards::delete(j.config.clone(), Arc::clone(&j.store), Some(doomed), true)
        .await
        .expect("delete");
    let err = boards::publish_to_files(
        j.config.clone(),
        Arc::clone(&j.store),
        Some(doomed),
        Duration::from_secs(1),
    )
    .await
    .expect_err("deleted board");
    assert!(
        matches!(err, CommandError::Engine(ref e)
            if matches!(&**e, taskboard_state::ExecuteError::Rejected(
                taskboard_domain::CommandError::BoardDeleted(_)))),
        "typed board-deleted refusal"
    );

    // Archived: the app-layer live-row limb (the engine keeps archived
    // boards commandable; the GUI ceremony's gone ruling does not).
    let created = boards::create(j.config.clone(), Arc::clone(&j.store), "Shelved", None)
        .await
        .expect("board");
    let shelved =
        taskboard_domain::BoardId::from_uuid(uuid::Uuid::parse_str(&created.id).expect("uuid"));
    boards::archive(j.config.clone(), Arc::clone(&j.store), Some(shelved), true)
        .await
        .expect("archive");
    let err = boards::publish_to_files(
        j.config,
        Arc::clone(&j.store),
        Some(shelved),
        Duration::from_secs(1),
    )
    .await
    .expect_err("archived board");
    assert!(
        matches!(err, CommandError::BoardArchived { board } if board == shelved),
        "typed board-archived refusal"
    );
}

/// Journey US-2/S4 (`us2_s4`): the omitted `--board` with zero or
/// several live boards rides the shared exactly-one-live-board
/// resolution — the same `ambiguous-board` class the other boards
/// verbs use, no new semantics.
#[tokio::test]
async fn us2_s4_resolution_is_the_shared_exactly_one_live_board_rule() {
    let j = files_journey("us2s4.db").await;

    // Zero live boards: the ambiguity refusal with empty candidates.
    let err = boards::publish_to_files(
        j.config.clone(),
        Arc::clone(&j.store),
        None,
        Duration::from_secs(1),
    )
    .await
    .expect_err("zero live boards");
    let CommandError::AmbiguousBoard { candidates } = err else {
        panic!("the shared resolution class expected, got a different error");
    };
    assert!(candidates.is_empty(), "zero candidates");

    // Several live boards: the same class, every live uuid a candidate.
    boards::create(j.config.clone(), Arc::clone(&j.store), "One", None)
        .await
        .expect("board");
    boards::create(j.config.clone(), Arc::clone(&j.store), "Two", None)
        .await
        .expect("board");
    let err =
        boards::publish_to_files(j.config, Arc::clone(&j.store), None, Duration::from_secs(1))
            .await
            .expect_err("several live boards");
    let CommandError::AmbiguousBoard { candidates } = err else {
        panic!("the shared resolution class expected, got a different error");
    };
    assert_eq!(candidates.len(), 2, "every live board is a candidate");
}

/// The board/label/stack/find render families' structural pins
/// (#1229): list renderers are one line per row plus the empty
/// family's line; receipts are single-line and distinguish their
/// branch fields (the wiped-refs arm, the bound-remote arms, the
/// archived/deleted markers, the destruct verb and bound-remote arm).
#[test]
#[allow(clippy::too_many_lines)] // one table per renderer family, kept flat for review
fn board_label_stack_find_render_arity_and_distinguishment() {
    use taskboard_app::commands::find::{self, FindRow};
    use taskboard_app::commands::output::{
        BoardDestructReceipt, BoardReceipt, BoardsListEntry, FilesPublishReceipt, LabelReceipt,
        LabelRow, RebindReceipt, ReconcileReceipt, SelectReceipt, StackDeleteReceipt, StackReceipt,
        SyncStatusView, UnbindReceipt,
    };
    use taskboard_app::commands::{labels, stacks};

    let sync_view = SyncStatusView {
        phase: "idle".to_owned(),
        last_success: None,
        pending_ops: 0,
        error_kind: None,
        cycles: 0,
        pushes: 0,
        pulls: 0,
        failures: 0,
        offline_episodes: 0,
        last_cycle_ms: None,
        sidecar: None,
    };
    let entry = |id: u64, title: &str, archived: bool, deleted: bool| BoardsListEntry {
        id,
        title: title.to_owned(),
        archived,
        deleted,
    };
    let board_list = |entries: Vec<BoardsListEntry>| boards::render_text_list(&entries);
    assert_eq!(
        board_list(vec![
            entry(1, "a", false, false),
            entry(2, "b", true, false)
        ])
        .split('\n')
        .count(),
        2,
        "one line per board"
    );
    assert_eq!(
        board_list(Vec::new()).split('\n').count(),
        1,
        "the empty family renders its own line"
    );
    assert_ne!(
        board_list(vec![entry(1, "a", false, false)]),
        board_list(vec![entry(1, "a", true, false)]),
        "the archived marker must surface"
    );
    assert_ne!(
        board_list(vec![entry(1, "a", false, false)]),
        board_list(vec![entry(1, "a", false, true)]),
        "the deleted marker must surface"
    );

    let select = |wiped: Option<usize>| {
        boards::render_text_select(&SelectReceipt {
            board: entry(1, "a", false, false),
            sync: sync_view.clone(),
            wiped_refs: wiped,
        })
    };
    assert_eq!(select(None).split('\n').count(), 1);
    assert_ne!(
        select(None),
        select(Some(3)),
        "the wiped-refs arm must surface"
    );

    let publish = |remote: &str| {
        boards::render_text_files_publish(&FilesPublishReceipt {
            board_id: "b".to_owned(),
            files_remote_id: remote.to_owned(),
            title: "a".to_owned(),
            archived: false,
            deleted: false,
            sync: sync_view.clone(),
        })
    };
    assert_eq!(publish("f1").split('\n').count(), 1);
    assert_ne!(
        publish("f1"),
        publish("f2"),
        "the files remote id must surface"
    );

    let unbind = |bound: Option<u64>| {
        boards::render_text_unbind(&UnbindReceipt {
            board: "b".to_owned(),
            bound_remote: bound,
            stranded_ops: 2,
            kept_refs: 1,
        })
    };
    assert_eq!(unbind(None).split('\n').count(), 1);
    assert_ne!(
        unbind(None),
        unbind(Some(7)),
        "the bound-remote arm must surface"
    );

    let reconcile = |tasks_merged: usize| {
        boards::render_text_reconcile(&ReconcileReceipt {
            board: "b".to_owned(),
            stacks: 1,
            labels: 2,
            tasks: tasks_merged,
            cancelled_ops: 0,
        })
    };
    assert_eq!(reconcile(3).split('\n').count(), 1);
    assert_ne!(reconcile(3), reconcile(4), "the merge count must surface");

    let rebind = |connection: &str| {
        boards::render_text_rebind(&RebindReceipt {
            board: "b".to_owned(),
            connection: connection.to_owned(),
        })
    };
    assert_eq!(rebind("c1").split('\n').count(), 1);
    assert_ne!(rebind("c1"), rebind("c2"));

    let board_receipt = |title: &str| BoardReceipt {
        id: "b".to_owned(),
        title: title.to_owned(),
        color: "0082c9".to_owned(),
    };
    assert_eq!(
        boards::render_text_create(&board_receipt("a"))
            .split('\n')
            .count(),
        1
    );
    assert_ne!(
        boards::render_text_create(&board_receipt("a")),
        boards::render_text_create(&board_receipt("c")),
    );
    assert_ne!(
        boards::render_text_board_edit(&board_receipt("a")),
        boards::render_text_board_edit(&BoardReceipt {
            color: "ff0000".to_owned(),
            ..board_receipt("a")
        }),
        "the color must surface"
    );

    let destruct = |verb: &str, bound: Option<u64>| {
        boards::render_text_destruct(&BoardDestructReceipt {
            board: "b".to_owned(),
            verb: verb.to_owned(),
            stacks: 1,
            tasks: 2,
            labels: 3,
            pending_ops: 4,
            bound_remote: bound,
        })
    };
    assert_eq!(destruct("deleted", None).split('\n').count(), 1);
    assert_ne!(
        destruct("deleted", None),
        destruct("reset", None),
        "the verb must surface"
    );
    assert_ne!(
        destruct("deleted", None),
        destruct("deleted", Some(9)),
        "the bound-remote arm must surface"
    );

    let label_rows = |rows: Vec<LabelRow>| labels::render_text_list(&rows);
    let label_row = |title: &str| LabelRow {
        id: "l".to_owned(),
        title: title.to_owned(),
        color: "red".to_owned(),
        assigned_tasks: 1,
    };
    assert_eq!(
        label_rows(vec![label_row("a"), label_row("b")])
            .split('\n')
            .count(),
        2
    );
    assert_eq!(
        label_rows(Vec::new()).split('\n').count(),
        1,
        "the empty family renders its own line"
    );
    assert_ne!(
        label_rows(vec![label_row("a")]),
        label_rows(vec![label_row("b")])
    );

    let label_receipt = |color: &str| {
        labels::render_text_receipt(&LabelReceipt {
            id: "l".to_owned(),
            title: "a".to_owned(),
            color: color.to_owned(),
        })
    };
    assert_eq!(label_receipt("red").split('\n').count(), 1);
    assert_ne!(
        label_receipt("red"),
        label_receipt("blue"),
        "the color must surface"
    );

    let stack_add = |order: i64| {
        stacks::render_text_add(&StackReceipt {
            id: "s".to_owned(),
            title: "stack".to_owned(),
            order,
        })
    };
    assert_eq!(stack_add(1).split('\n').count(), 1);
    assert_ne!(stack_add(1), stack_add(2), "the position must surface");

    let stack_delete = |cascaded: usize| {
        stacks::render_text_delete(&StackDeleteReceipt {
            id: "s".to_owned(),
            title: "stack".to_owned(),
            cascaded_tasks: cascaded,
        })
    };
    assert_eq!(stack_delete(2).split('\n').count(), 1);
    assert_ne!(
        stack_delete(2),
        stack_delete(3),
        "the cascade count must surface"
    );

    let find_rows = |rows: Vec<FindRow>| find::render_text_list(&rows);
    let find_row = |kind: &str, title: &str| FindRow {
        kind: kind.to_owned(),
        id: "x".to_owned(),
        title: title.to_owned(),
        context: None,
    };
    assert_eq!(
        find_rows(vec![find_row("task", "a"), find_row("board", "b")])
            .split('\n')
            .count(),
        2,
        "one line per hit"
    );
    assert_eq!(
        find_rows(Vec::new()).split('\n').count(),
        1,
        "the empty family renders its own line"
    );
    assert_ne!(
        find_rows(vec![find_row("task", "a")]),
        find_rows(vec![find_row("board", "a")]),
        "the kind column must surface"
    );
    assert_ne!(
        find_rows(vec![find_row("task", "a")]),
        find_rows(vec![find_row("task", "b")]),
        "the title must surface"
    );
}
