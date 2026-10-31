// SPDX-License-Identifier: AGPL-3.0-only
//! U-series: the `tasks undo`/`tasks redo` verbs over hermetic
//! standalone tempdirs (no wiremock — a standalone core performs no
//! network I/O). One invocation is a full boot/work/shutdown
//! "process"; receipts are asserted as typed values; refusals via
//! `matches!`/`error_kind()`, never message text.

use std::sync::Arc;

use clap::Parser;

use taskboard_app::cli::{Cli, Command, TasksCmd};
use taskboard_app::commands::boards;
use taskboard_app::commands::output::{JournalReceipt, JournalVerb};
use taskboard_app::commands::stacks;
use taskboard_app::commands::tasks;
use taskboard_app::commands::{BasicSession, CommandError, CommandIo, dispatch_line};
use taskboard_app::config::{AppConfig, CredentialStoreKind};
use taskboard_app::secrets::CredentialStore;
use taskboard_domain::TaskChanges;

mod common;

use common::tempdir;

/// A standalone config over a tempdir db: no server, no username —
/// the fresh-install shape.
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

/// Parses one `taskboard tasks …` argv through the real clap surface.
fn parse_tasks(argv: &[&str]) -> TasksCmd {
    let mut cmdline = vec!["taskboard"];
    cmdline.extend_from_slice(argv);
    let cli = Cli::try_parse_from(&cmdline).expect("the argv parses");
    let Some(Command::Tasks { cmd }) = cli.command else {
        panic!("the argv selects the tasks group");
    };
    cmd
}

/// Seeds the fresh db with its board, one stack, and one task; returns
/// the config. Each seeding step is one full boot (durability rides
/// the db).
async fn seeded_config(dir: &std::path::Path, db: &str) -> AppConfig {
    let config = standalone_config(dir, db);
    boards::create(config.clone(), forbidden_store(), "seed board", None)
        .await
        .expect("board create boots");
    stacks::add(config.clone(), forbidden_store(), None, "seed stack", None)
        .await
        .expect("stack add boots");
    tasks::add(
        config.clone(),
        forbidden_store(),
        "seed task",
        Some("seed stack"),
    )
    .await
    .expect("task add boots");
    config
}

/// The live task titles, read off a fresh boot (terminal state, not
/// receipt text).
async fn live_titles(config: &AppConfig) -> Vec<String> {
    let list = tasks::list(config.clone(), forbidden_store(), None)
        .await
        .expect("list boots");
    list.stacks
        .iter()
        .flat_map(|group| group.tasks.iter().map(|row| row.title.clone()))
        .collect()
}

/// U-1: the two verbs parse as zero-arg unit variants through the real
/// clap surface.
#[test]
fn u01_the_undo_verbs_parse_as_bare_unit_variants() {
    assert!(matches!(parse_tasks(&["tasks", "undo"]), TasksCmd::Undo));
    assert!(matches!(parse_tasks(&["tasks", "redo"]), TasksCmd::Redo));
}

/// U-2: undo on an empty journal refuses typed.
#[tokio::test]
async fn u02_undo_on_an_empty_journal_refuses_nothing_to_undo() {
    let dir = tempdir();
    let config = standalone_config(dir.path(), "u02.db");
    let err = tasks::undo(config, forbidden_store())
        .await
        .expect_err("empty line refuses");
    assert_eq!(err.error_kind(), "nothing-to-undo");
    let CommandError::Engine(rejected) = &err else {
        panic!("the refusal is the engine rejection: {err:?}");
    };
    assert!(
        matches!(
            &**rejected,
            taskboard_state::ExecuteError::Rejected(taskboard_domain::CommandError::NothingToUndo)
        ),
        "typed NothingToUndo: {rejected:?}"
    );
}

/// U-3: undo retracts the last change and the receipt names the entry;
/// the task is structurally gone from a fresh boot's terminal state.
#[tokio::test]
async fn u03_undo_retracts_the_last_change_and_names_the_entry() {
    let dir = tempdir();
    let config = seeded_config(dir.path(), "u03.db").await;

    let receipt = tasks::undo(config.clone(), forbidden_store())
        .await
        .expect("the add is undoable");
    let JournalReceipt { verb, entry, label } = receipt;
    assert_eq!(verb, JournalVerb::Undo);
    assert!(uuid::Uuid::parse_str(&entry).is_ok(), "durable uuid");
    assert!(label.is_some(), "the just-touched entry is in-window");

    assert_eq!(
        live_titles(&config).await,
        Vec::<String>::new(),
        "the created task is gone"
    );
}

/// U-4: redo re-applies the undone change; the task is back in the
/// terminal state.
#[tokio::test]
async fn u04_redo_reapplies_the_undone_change() {
    let dir = tempdir();
    let config = seeded_config(dir.path(), "u04.db").await;

    tasks::undo(config.clone(), forbidden_store())
        .await
        .expect("undo lands");
    let receipt = tasks::redo(config.clone(), forbidden_store())
        .await
        .expect("a redo target exists");
    assert_eq!(receipt.verb, JournalVerb::Redo);
    assert!(uuid::Uuid::parse_str(&receipt.entry).is_ok());
    assert!(receipt.label.is_some());

    assert_eq!(live_titles(&config).await, vec!["seed task".to_owned()]);
}

/// U-5: redo on a fresh line (no undone head) refuses typed.
#[tokio::test]
async fn u05_redo_without_an_undone_head_refuses_nothing_to_redo() {
    let dir = tempdir();
    let config = standalone_config(dir.path(), "u05.db");
    let err = tasks::redo(config, forbidden_store())
        .await
        .expect_err("empty line refuses");
    assert_eq!(err.error_kind(), "nothing-to-redo");
}

/// U-6: an interleaved write to the same field diverges the watermark
/// — the second undo refuses typed, never force-applies (R-U7).
#[tokio::test]
async fn u06_undo_refuses_typed_when_a_later_write_diverged() {
    let dir = tempdir();
    let config = seeded_config(dir.path(), "u06.db").await;
    let edit = |title: &str| TaskChanges {
        title: Some(title.to_owned()),
        ..TaskChanges::default()
    };

    // Two title writes: the second's stamp diverges the first's
    // watermark.
    tasks::edit(
        config.clone(),
        forbidden_store(),
        "seed task",
        edit("first edit"),
    )
    .await
    .expect("edit one lands");
    tasks::edit(
        config.clone(),
        forbidden_store(),
        "first edit",
        edit("second edit"),
    )
    .await
    .expect("edit two lands");

    tasks::undo(config.clone(), forbidden_store())
        .await
        .expect("the line head (edit two) undoes cleanly");
    let err = tasks::undo(config.clone(), forbidden_store())
        .await
        .expect_err("edit one's watermark diverged");
    assert_eq!(err.error_kind(), "target-not-invertible");
    let CommandError::Engine(rejected) = &err else {
        panic!("the refusal is the engine rejection: {err:?}");
    };
    assert!(
        matches!(
            &**rejected,
            taskboard_state::ExecuteError::Rejected(
                taskboard_domain::CommandError::TargetNoLongerInvertible { .. }
            )
        ),
        "typed TargetNoLongerInvertible: {rejected:?}"
    );
    // Refuse-and-name: the detail is structural — the entity uuid plus
    // the divergent field kind (D-3: ids and enum kinds, never prose).
    let detail = err.error_detail().expect("the refusal names its facts");
    let (entity, field) = detail.split_once(' ').expect("entity + field kind");
    assert!(
        uuid::Uuid::parse_str(entity).is_ok(),
        "the entity is a uuid: {entity}"
    );
    assert_eq!(field, "title");
}

/// U-7: both verbs pin the `Command::Tasks` wildcard's `IpcOrLocal`
/// lane — the daemon socket inheritance is guarded, not accidental
/// (ADR 0012).
#[test]
fn u07_the_undo_verbs_route_over_the_daemon_lane() {
    use taskboard_app::commands::{IpcRoute, route_for_ipc};
    assert_eq!(
        route_for_ipc(&Command::Tasks {
            cmd: TasksCmd::Undo,
        }),
        IpcRoute::IpcOrLocal
    );
    assert_eq!(
        route_for_ipc(&Command::Tasks {
            cmd: TasksCmd::Redo,
        }),
        IpcRoute::IpcOrLocal
    );
}

/// U-8: the session lane (REPL + daemon socket, `dispatch_line`)
/// serves both verbs end to end on one booted core.
#[tokio::test]
async fn u08_the_session_lane_serves_the_undo_verbs() {
    let dir = tempdir();
    let config = seeded_config(dir.path(), "u08.db").await;

    let store = forbidden_store();
    let app = taskboard_app::bootstrap::bootstrap(
        config.clone(),
        store.clone(),
        Arc::new(taskboard_app::notifications::LogSink),
    )
    .await
    .expect("session boots");
    let session = BasicSession {
        config: config.clone(),
        store,
        config_path: None,
    };

    let undo_cmd = || Command::Tasks {
        cmd: TasksCmd::Undo,
    };
    let redo_cmd = || Command::Tasks {
        cmd: TasksCmd::Redo,
    };

    let mut out = Vec::new();
    let mut err = Vec::new();
    {
        let mut io = CommandIo {
            out: &mut out,
            err: &mut err,
            json: true,
        };
        assert_eq!(
            dispatch_line(&session, &app, undo_cmd(), &mut io).await,
            0,
            "stderr: {}",
            String::from_utf8_lossy(&err)
        );
        assert_eq!(
            dispatch_line(&session, &app, redo_cmd(), &mut io).await,
            0,
            "stderr: {}",
            String::from_utf8_lossy(&err)
        );
    }
    app.shutdown().await;

    // Two JSON receipt lines: undo then redo, in order.
    let lines: Vec<&[u8]> = out
        .split(|b| *b == b'\n')
        .filter(|l| !l.is_empty())
        .collect();
    assert_eq!(lines.len(), 2);
    let first: JournalReceipt = serde_json::from_slice(lines[0]).expect("the undo receipt is JSON");
    let second: JournalReceipt =
        serde_json::from_slice(lines[1]).expect("the redo receipt is JSON");
    assert_eq!(first.verb, JournalVerb::Undo);
    assert_eq!(second.verb, JournalVerb::Redo);

    // Terminal state: undo removed the task, redo brought it back.
    assert_eq!(live_titles(&config).await, vec!["seed task".to_owned()]);
}
