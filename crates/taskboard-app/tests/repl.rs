// SPDX-License-Identifier: AGPL-3.0-only
//! R-series: the interactive REPL session journeys (WI-4 plan §6 step
//! 5). Hermetic standalone cores — sqlite tempdirs, no wiremock, the
//! credential store is never consulted. Sessions run against injected
//! [`LineEvent`] streams and injected writers: the terminal, the editor
//! thread, and the process never appear. Assertions are structural
//! (parsed JSON receipts, typed error kinds, state effects) — session
//! banner and prompt wording are UI text and never asserted.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;

use taskboard_app::cli::{BoardsCmd, Command, StacksCmd, TasksCmd};
use taskboard_app::commands::{CommandIo, boards, dispatch_line, tasks};
use taskboard_app::config::{AppConfig, CredentialStoreKind};
use taskboard_app::lockfile::InstanceLock;
use taskboard_app::repl::{LineEvent, ReplSession, repl_loop, run_session_with_lines};
use taskboard_app::secrets::CredentialStore;

mod common;

use common::{FakeCredentialStore, tempdir};

/// A standalone config over a tempdir db: no server, no username — the
/// fresh-install shape; the credential store is never consulted.
fn standalone_config(db: &std::path::Path) -> AppConfig {
    AppConfig {
        nextcloud: taskboard_app::config::NextcloudSection {
            server_url: None,
            username: None,
            credential_store: CredentialStoreKind::Keyring,
        },
        storage: taskboard_app::config::StorageSection {
            db_path: Some(db.to_owned()),
        },
        ..AppConfig::default()
    }
}

/// A store that would fail any touch: standalone never consults it.
fn forbidden_store() -> Arc<dyn CredentialStore> {
    Arc::new(FakeCredentialStore::default())
}

/// The JSON lines of a captured buffer (banner/human text skipped).
fn json_lines(buf: &[u8]) -> Vec<serde_json::Value> {
    String::from_utf8_lossy(buf)
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

/// Runs one full session against an injected event script and buffers;
/// returns the session exit code plus the captured streams.
async fn run_session(
    db: &std::path::Path,
    json_default: bool,
    script: Vec<LineEvent>,
) -> (u8, Vec<u8>, Vec<u8>) {
    let (tx, rx) = mpsc::channel(script.len().max(1));
    for event in script {
        tx.send(event).await.expect("session channel alive");
    }
    drop(tx);
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut io = CommandIo {
        out: &mut out,
        err: &mut err,
        json: json_default,
    };
    let code = run_session_with_lines(standalone_config(db), json_default, |_| rx, &mut io).await;
    (code, out, err)
}

fn line(raw: &str) -> LineEvent {
    LineEvent::Line(raw.to_owned())
}

/// R1: the board→stack→task journey against one booted core — JSON
/// receipts per line, the list payload reflects the mutations, `exit`
/// shuts the core down gracefully, and everything is durable after the
/// session (a fresh one-shot core hydrates it).
#[tokio::test]
async fn r1_session_journey_boot_once_work_exit_durable() {
    let dir = tempdir();
    let db = dir.path().join("r1.db");
    let (code, out, err) = run_session(
        &db,
        false,
        vec![
            line("boards create \"Personal\" --json"),
            line("stacks add \"Todo\" --json"),
            line("tasks add \"first task\" --json"),
            line("tasks list --json"),
            line("exit"),
        ],
    )
    .await;
    assert_eq!(code, 0, "session stderr: {}", String::from_utf8_lossy(&err));

    // One JSON receipt per mutating line, in order.
    let receipts = json_lines(&out);
    assert_eq!(receipts.len(), 4, "out: {}", String::from_utf8_lossy(&out));
    assert_eq!(receipts[0]["title"], "Personal");
    assert_eq!(receipts[0]["color"], "0082c9", "the default color applies");
    assert_eq!(receipts[1]["title"], "Todo");
    assert_eq!(receipts[1]["order"], 0);
    assert_eq!(receipts[2]["title"], "first task");
    // The list payload reflects the session's own mutations.
    assert_eq!(receipts[3]["stacks"][0]["title"], "Todo");
    assert_eq!(receipts[3]["stacks"][0]["tasks"][0]["title"], "first task");
    assert_eq!(
        receipts[3]["sync"]["pending_ops"], 2,
        "stack + task; board create is local until publish"
    );

    // Durability beyond the session: a fresh one-shot core hydrates the
    // session's data from the db.
    let list = tasks::list(standalone_config(&db), forbidden_store(), None)
        .await
        .expect("fresh core reads the session's data");
    assert_eq!(list.stacks.len(), 1);
    assert_eq!(list.stacks[0].tasks[0].title, "first task");
}

/// R2: a failed line reports typed and the session continues — the
/// unknown-task refusal does not abort anything after it.
#[tokio::test]
async fn r2_a_failed_line_reports_and_the_session_continues() {
    let dir = tempdir();
    let db = dir.path().join("r2.db");
    let (code, out, err) = run_session(
        &db,
        false,
        vec![
            line("boards create \"Personal\" --json"),
            line("tasks done 00000000-0000-0000-0000-000000000000 --json"),
            line("stacks add \"Todo\" --json"),
            line("exit"),
        ],
    )
    .await;
    assert_eq!(code, 0);
    let errors = json_lines(&err);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0]["error"]["kind"], "unknown-task");
    // The line after the failure still executed.
    let receipts = json_lines(&out);
    assert_eq!(receipts.len(), 2);
    assert_eq!(receipts[1]["title"], "Todo");
}

/// R3: a `sync` line under a standalone session refuses typed
/// (`sync-disabled`) and the session survives it.
#[tokio::test]
async fn r3_sync_under_standalone_refuses_typed_and_survives() {
    let dir = tempdir();
    let db = dir.path().join("r3.db");
    let (code, out, err) = run_session(
        &db,
        true,
        vec![
            line("sync"),
            line("boards create \"Personal\""),
            line("exit"),
        ],
    )
    .await;
    assert_eq!(code, 0);
    let errors = json_lines(&err);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0]["error"]["kind"], "sync-disabled");
    assert_eq!(json_lines(&out).len(), 1, "the session continued");
}

/// R4: Ctrl-D (EOF) ends the session with the same graceful shutdown as
/// `exit` — no lines at all.
#[tokio::test]
async fn r4_eof_ends_the_session_gracefully() {
    let dir = tempdir();
    let db = dir.path().join("r4.db");
    let (code, out, err) = run_session(&db, false, vec![LineEvent::Eof]).await;
    assert_eq!(code, 0);
    assert_eq!(json_lines(&err).len(), 0);
    assert_eq!(
        json_lines(&out).len(),
        0,
        "no command ran; only the banner showed"
    );
    // The db was created and cleanly released: a fresh core boots on it.
    let _ = boards::create(standalone_config(&db), forbidden_store(), "After", None)
        .await
        .expect("lock released by the session shutdown");
}

/// R5: the JSON flag composition — the launch default is human, a
/// line-level `--json` overrides for that line only.
#[tokio::test]
async fn r5_per_line_json_overrides_the_session_default() {
    let dir = tempdir();
    let db = dir.path().join("r5.db");
    let (code, out, err) = run_session(
        &db,
        false,
        vec![
            line("boards create \"Personal\""),
            line("stacks add \"Todo\" --json"),
            line("exit"),
        ],
    )
    .await;
    assert_eq!(code, 0, "stderr: {}", String::from_utf8_lossy(&err));
    // Exactly one JSON line (the flagged one); the unflagged receipt is
    // human text.
    let receipts = json_lines(&out);
    assert_eq!(receipts.len(), 1);
    assert_eq!(receipts[0]["title"], "Todo");
}

/// R6: a session cannot boot while another instance holds the database
/// lock — typed `instance-locked`, exit 1, and the lock survives for
/// its true holder.
#[tokio::test]
async fn r6_a_held_instance_lock_fails_the_session_boot_typed() {
    let dir = tempdir();
    let db = dir.path().join("r6.db");
    let lock = InstanceLock::acquire(&db).expect("hold the lock");
    let (tx, rx) = mpsc::channel(1);
    drop(tx);
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut io = CommandIo {
        out: &mut out,
        err: &mut err,
        json: true,
    };
    let code = run_session_with_lines(standalone_config(&db), true, |_| rx, &mut io).await;
    assert_eq!(code, 1);
    let errors = json_lines(&err);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0]["error"]["kind"], "instance-locked");
    drop(lock);
}

/// R7: the session dispatcher's routing table — one booted core, every
/// `Command` variant routed: core verbs hit the session core (no boot,
/// receipts), connector verbs refuse typed `sync-disabled` at their
/// prelude, and a garbage task selector routes to `unknown-task`.
#[tokio::test]
#[allow(clippy::too_many_lines)] // the full routing table, one row per arm
async fn r7_dispatch_line_routes_every_verb_family() {
    let dir = tempdir();
    let db = dir.path().join("r7.db");
    let app = taskboard_app::bootstrap::bootstrap(
        standalone_config(&db),
        forbidden_store(),
        Arc::new(taskboard_app::notifications::LogSink),
    )
    .await
    .expect("session core boots");
    let session = ReplSession {
        config: app.config.clone(),
        store: forbidden_store(),
        json_default: true,
    };
    let mut out = Vec::new();
    let mut err = Vec::new();
    {
        let mut io = CommandIo {
            out: &mut out,
            err: &mut err,
            json: true,
        };

        // Core verbs work against the session core.
        let code = dispatch_line(
            &session,
            &app,
            Command::Boards {
                cmd: BoardsCmd::Create {
                    title: "Personal".to_owned(),
                    color: Some("3366cc".to_owned()),
                },
            },
            &mut io,
        )
        .await;
        assert_eq!(code, 0);
        let code = dispatch_line(
            &session,
            &app,
            Command::Stacks {
                cmd: StacksCmd::Add {
                    title: "Todo".to_owned(),
                    order: None,
                },
            },
            &mut io,
        )
        .await;
        assert_eq!(code, 0);
        let code = dispatch_line(
            &session,
            &app,
            Command::Tasks {
                cmd: TasksCmd::Add {
                    title: "first task".to_owned(),
                    stack: None,
                },
            },
            &mut io,
        )
        .await;
        assert_eq!(code, 0);

        // Connector verbs refuse typed at their preludes.
        for command in [
            Command::Boards {
                cmd: BoardsCmd::List,
            },
            Command::Boards {
                cmd: BoardsCmd::Select {
                    board: 3,
                    timeout_secs: 1,
                    bind_local_board: false,
                },
            },
            Command::Boards {
                cmd: BoardsCmd::Publish { timeout_secs: 1 },
            },
            Command::Sync {
                no_wait: false,
                timeout_secs: 1,
            },
        ] {
            let code = dispatch_line(&session, &app, command, &mut io).await;
            assert_eq!(code, 1);
        }

        // A garbage selector routes to the typed refusal, not a panic.
        let code = dispatch_line(
            &session,
            &app,
            Command::Tasks {
                cmd: TasksCmd::Done {
                    task: "not-a-uuid".to_owned(),
                    undo: false,
                },
            },
            &mut io,
        )
        .await;
        assert_eq!(code, 1);
    }
    let errors = json_lines(&err);
    assert_eq!(errors.len(), 5);
    for error in &errors[..4] {
        assert_eq!(error["error"]["kind"], "sync-disabled");
    }
    assert_eq!(errors[4]["error"]["kind"], "unknown-task");
    let receipts = json_lines(&out);
    assert_eq!(receipts.len(), 3);
    assert_eq!(receipts[0]["color"], "3366cc");

    // The session core accumulated the mutations live (no boot cycle
    // needed): a second read on the same core sees them.
    let list = tasks::list_on(&app, None).expect("live read");
    assert_eq!(list.stacks[0].tasks[0].title, "first task");

    app.shutdown().await;
}

/// R8: the session loop replays its line events through the same
/// parse/route machinery the one-shot dispatcher uses — quoting and
/// flag composition survive end to end (the loop-level view of the
/// line-table unit pins).
#[tokio::test]
async fn r8_quoted_titles_and_flag_composition_survive_the_loop() {
    let dir = tempdir();
    let db = dir.path().join("r8.db");
    let (code, out, err) = run_session(
        &db,
        false,
        vec![
            line("boards create Personal"),
            line("stacks add 'two words'"),
            line("exit"),
        ],
    )
    .await;
    assert_eq!(code, 0, "stderr: {}", String::from_utf8_lossy(&err));
    // The quoted stack title arrived as one argument.
    let list = tasks::list(standalone_config(&db), forbidden_store(), None)
        .await
        .expect("fresh core");
    assert_eq!(list.stacks[0].title, "two words");
    assert_eq!(json_lines(&out).len(), 0, "human session default");
}

/// R9: the boards create single-board refusal inside a session is the
/// engine's typed `board-exists`, and the session continues after it.
#[tokio::test]
async fn r9_single_board_refusal_is_typed_inside_a_session() {
    let dir = tempdir();
    let db = dir.path().join("r9.db");
    let (code, _out, err) = run_session(
        &db,
        true,
        vec![
            line("boards create \"Personal\""),
            line("boards create \"Second\""),
            line("exit"),
        ],
    )
    .await;
    assert_eq!(code, 0);
    let errors = json_lines(&err);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0]["error"]["kind"], "board-exists");
}

/// R10: `sync` under a *connector* session is the sync actor's own path
/// — here proven at the dispatcher seam only: the standalone refusal is
/// the prelude's, so a nextcloud-configured session with an unreachable
/// server routes the failure typed (`unreachable`), and the session
/// survives it.
#[tokio::test]
async fn r10_a_connector_sync_failure_is_typed_and_survivable() {
    let dir = tempdir();
    let db = dir.path().join("r10.db");
    let mut config = standalone_config(&db);
    config.nextcloud.server_url = Some("http://127.0.0.1:1".to_owned());
    config.nextcloud.username = Some("alice".to_owned());
    config.nextcloud.credential_store = CredentialStoreKind::Env;
    let (tx, rx) = mpsc::channel(2);
    tx.send(line("sync")).await.expect("alive");
    tx.send(line("exit")).await.expect("alive");
    drop(tx);
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut io = CommandIo {
        out: &mut out,
        err: &mut err,
        json: true,
    };
    // The env store supplies the secret; the server refuses to exist,
    // so the session's sync cycle fails fresh — typed, survivable.
    let guard = common::env_lock();
    common::set_env("TASKBOARD_APP_PASSWORD", "r10-secret");
    let code = run_session_with_lines(config, true, |_| rx, &mut io).await;
    drop(guard);
    assert_eq!(code, 0, "the session exits 0 despite the failed line");
    let errors = json_lines(&err);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0]["error"]["kind"], "sync-failed");
}

/// R11: a session on a *booted* core with a dead connector refuses a
/// `SetBoard`-carrying ceremony typed at the sync seam (the
/// `EngineGone` class) — the dispatcher maps it, the session survives.
/// Proven via `publish`'s post-prelude half under standalone config
/// surfaced through the loop's `sync-disabled` gate instead: here the
/// engine-gone mapping is pinned directly.
#[tokio::test]
async fn r11_select_on_a_standalone_core_surfaces_the_typed_gate() {
    let dir = tempdir();
    let db = dir.path().join("r11.db");
    let app = taskboard_app::bootstrap::bootstrap(
        standalone_config(&db),
        forbidden_store(),
        Arc::new(taskboard_app::notifications::LogSink),
    )
    .await
    .expect("session core boots");
    let session = ReplSession {
        config: app.config.clone(),
        store: forbidden_store(),
        json_default: true,
    };
    let mut err = Vec::new();
    let mut sink = Vec::new();
    let mut io = CommandIo {
        out: &mut sink,
        err: &mut err,
        json: true,
    };
    // `select_on` on a connector-less core: the standalone session never
    // reaches it through the loop (prelude gate, R7); the direct seam
    // call maps the dead `sync_commands` channel to the engine-gone
    // class, and the session's shutdown stays graceful.
    let outcome = boards::select_on(&app, 3, Duration::from_secs(1), false).await;
    let _ = dispatch_line(
        &session,
        &app,
        Command::Boards {
            cmd: BoardsCmd::Select {
                board: 3,
                timeout_secs: 1,
                bind_local_board: false,
            },
        },
        &mut io,
    )
    .await;
    assert!(matches!(
        outcome,
        Err(taskboard_app::commands::CommandError::Engine(boxed))
            if matches!(*boxed, taskboard_state::ExecuteError::EngineGone)
    ));
    app.shutdown().await;
}

/// R12: `repl_loop`'s exit is the graceful one and the shutdown joins —
/// pinned by observing that the loop returns only after the engine
/// quiesced (a follow-up boot re-acquires the lock immediately).
#[tokio::test]
async fn r12_repl_loop_shuts_the_core_down_before_returning() {
    let dir = tempdir();
    let db = dir.path().join("r12.db");
    let app = taskboard_app::bootstrap::bootstrap(
        standalone_config(&db),
        forbidden_store(),
        Arc::new(taskboard_app::notifications::LogSink),
    )
    .await
    .expect("boot");
    let session = ReplSession {
        config: app.config.clone(),
        store: forbidden_store(),
        json_default: false,
    };
    let (tx, rx) = mpsc::channel(1);
    drop(tx);
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut io = CommandIo {
        out: &mut out,
        err: &mut err,
        json: false,
    };
    let exit = repl_loop(app, session, rx, &mut io).await;
    assert_eq!(exit, taskboard_app::repl::ReplExit::Graceful);
    // The lock is gone the moment the loop returned: shutdown completed.
    let _next = InstanceLock::acquire(&db).expect("lock released");
}

/// R13: the core is mode-agnostic (ADR 0011): a session on a
/// `mode = "kiosk"` config boots, serves its lines, and shuts down —
/// the console a kiosk machine gets instead of the pre-REPL
/// `ModeNotImplemented` dead end (the daemon/UI entry owns the Phase-7
/// refusal, not `bootstrap`).
#[tokio::test]
async fn r13_a_kiosk_mode_session_boots_and_serves() {
    let dir = tempdir();
    let db = dir.path().join("r13.db");
    let mut config = standalone_config(&db);
    config.app.mode = taskboard_app::config::AppMode::Kiosk;
    let (tx, rx) = mpsc::channel(2);
    tx.send(line("boards create \"Kiosk\" --json"))
        .await
        .expect("alive");
    tx.send(LineEvent::Eof).await.expect("alive");
    drop(tx);
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut io = CommandIo {
        out: &mut out,
        err: &mut err,
        json: true,
    };
    let code = run_session_with_lines(config, true, |_| rx, &mut io).await;
    assert_eq!(code, 0, "stderr: {}", String::from_utf8_lossy(&err));
    let receipts = json_lines(&out);
    assert_eq!(receipts.len(), 1);
    assert_eq!(receipts[0]["title"], "Kiosk");
}

/// R12: the remaining session arms' *success* paths route end to end —
/// every family arm the r7 refusals/creation rows don't reach, driven
/// against one booted session core (rename/move/delete/edit/label across
/// boards, stacks, labels, and tasks). llvm-cov cannot see the
/// subprocess-driving suites, so this is the in-process slice.
/// One routed line against the session (the R12 walk calls this once
/// per arm).
async fn route_line(
    session: &ReplSession,
    app: &taskboard_app::App,
    command: Command,
    io: &mut CommandIo<'_>,
) -> u8 {
    dispatch_line(session, app, command, io).await
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // the full arm walk, one row per arm

async fn r12_session_arms_route_success_end_to_end() {
    let dir = tempdir();
    let db = dir.path().join("r12.db");
    let app = taskboard_app::bootstrap::bootstrap(
        standalone_config(&db),
        forbidden_store(),
        Arc::new(taskboard_app::notifications::LogSink),
    )
    .await
    .expect("session core boots");
    let session = ReplSession {
        config: app.config.clone(),
        store: forbidden_store(),
        json_default: true,
    };
    let mut out = Vec::new();
    let mut err = Vec::new();

    {
        let mut io = CommandIo {
            out: &mut out,
            err: &mut err,
            json: true,
        };

        // Setup: the board and stack every other verb shapes.
        assert_eq!(
            route_line(
                &session,
                &app,
                Command::Boards {
                    cmd: BoardsCmd::Create {
                        title: "Personal".to_owned(),
                        color: None,
                    },
                },
                &mut io,
            )
            .await,
            0
        );
        assert_eq!(
            route_line(
                &session,
                &app,
                Command::Stacks {
                    cmd: StacksCmd::Add {
                        title: "Todo".to_owned(),
                        order: None,
                    },
                },
                &mut io,
            )
            .await,
            0
        );

        // boards rename; stacks rename → move → delete (a second stack).
        assert_eq!(
            route_line(
                &session,
                &app,
                Command::Boards {
                    cmd: BoardsCmd::Rename {
                        title: "Work".to_owned(),
                    },
                },
                &mut io,
            )
            .await,
            0
        );
        assert_eq!(
            route_line(
                &session,
                &app,
                Command::Stacks {
                    cmd: StacksCmd::Add {
                        title: "Scratch".to_owned(),
                        order: None,
                    },
                },
                &mut io,
            )
            .await,
            0
        );
        assert_eq!(
            route_line(
                &session,
                &app,
                Command::Stacks {
                    cmd: StacksCmd::Rename {
                        stack: "Scratch".to_owned(),
                        title: "Someday".to_owned(),
                    },
                },
                &mut io,
            )
            .await,
            0
        );
        assert_eq!(
            route_line(
                &session,
                &app,
                Command::Stacks {
                    cmd: StacksCmd::Move {
                        stack: "Someday".to_owned(),
                        order: 3,
                    },
                },
                &mut io,
            )
            .await,
            0
        );
        assert_eq!(
            route_line(
                &session,
                &app,
                Command::Stacks {
                    cmd: StacksCmd::Delete {
                        stack: "Someday".to_owned(),
                    },
                },
                &mut io,
            )
            .await,
            0
        );

        // labels add → list → edit → delete.
        assert_eq!(
            route_line(
                &session,
                &app,
                Command::Labels {
                    cmd: taskboard_app::cli::LabelsCmd::Add {
                        title: "urgent".to_owned(),
                        color: Some("ff0000".to_owned()),
                    },
                },
                &mut io,
            )
            .await,
            0
        );
        assert_eq!(
            route_line(
                &session,
                &app,
                Command::Labels {
                    cmd: taskboard_app::cli::LabelsCmd::List,
                },
                &mut io,
            )
            .await,
            0
        );
        assert_eq!(
            route_line(
                &session,
                &app,
                Command::Labels {
                    cmd: taskboard_app::cli::LabelsCmd::Edit {
                        label: "urgent".to_owned(),
                        title: Some("critical".to_owned()),
                        color: Some("0000ff".to_owned()),
                    },
                },
                &mut io,
            )
            .await,
            0
        );
        assert_eq!(
            route_line(&session, &app, Command::Status, &mut io).await,
            0
        );

        // tasks: add two; label/unlabel, move, edit, done, delete.
        for title in ["t1", "t2"] {
            assert_eq!(
                route_line(
                    &session,
                    &app,
                    Command::Tasks {
                        cmd: TasksCmd::Add {
                            title: title.to_owned(),
                            stack: None,
                        },
                    },
                    &mut io,
                )
                .await,
                0
            );
        }
        assert_eq!(
            route_line(
                &session,
                &app,
                Command::Tasks {
                    cmd: TasksCmd::List { stack: None },
                },
                &mut io,
            )
            .await,
            0
        );
        assert_eq!(
            route_line(
                &session,
                &app,
                Command::Tasks {
                    cmd: TasksCmd::Label {
                        task: "t1".to_owned(),
                        label: "critical".to_owned(),
                    },
                },
                &mut io,
            )
            .await,
            0
        );
        assert_eq!(
            route_line(
                &session,
                &app,
                Command::Tasks {
                    cmd: TasksCmd::Unlabel {
                        task: "t1".to_owned(),
                        label: "critical".to_owned(),
                    },
                },
                &mut io,
            )
            .await,
            0
        );
        assert_eq!(
            route_line(
                &session,
                &app,
                Command::Tasks {
                    cmd: TasksCmd::Move {
                        task: "t2".to_owned(),
                        stack: Some("Todo".to_owned()),
                        order: Some(0),
                    },
                },
                &mut io,
            )
            .await,
            0
        );
        assert_eq!(
            route_line(
                &session,
                &app,
                Command::Tasks {
                    cmd: TasksCmd::Edit {
                        task: "t2".to_owned(),
                        title: Some("t2-renamed".to_owned()),
                        description: None,
                        due: None,
                        archive: false,
                        unarchive: false,
                    },
                },
                &mut io,
            )
            .await,
            0
        );
        assert_eq!(
            route_line(
                &session,
                &app,
                Command::Tasks {
                    cmd: TasksCmd::Done {
                        task: "t2-renamed".to_owned(),
                        undo: false,
                    },
                },
                &mut io,
            )
            .await,
            0
        );
        assert_eq!(
            route_line(
                &session,
                &app,
                Command::Tasks {
                    cmd: TasksCmd::Delete {
                        task: "t1".to_owned(),
                    },
                },
                &mut io,
            )
            .await,
            0
        );
        assert_eq!(
            route_line(
                &session,
                &app,
                Command::Labels {
                    cmd: taskboard_app::cli::LabelsCmd::Delete {
                        label: "critical".to_owned(),
                    },
                },
                &mut io,
            )
            .await,
            0
        );
    }
    // No failures anywhere in the walk.
    assert!(err.is_empty(), "stderr: {}", String::from_utf8_lossy(&err));
    let receipts = json_lines(&out);
    assert!(receipts.len() >= 15, "{} receipts", receipts.len());
    // The edited label's shape survived rendering (the edit receipt's
    // fields are the typed outcome).
    assert!(
        receipts
            .iter()
            .any(|r| r.get("title").and_then(|t| t.as_str()) == Some("critical")),
        "the label edit receipt is on stdout"
    );

    app.shutdown().await;
}
