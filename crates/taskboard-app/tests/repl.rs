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
use taskboard_app::repl::{EditorIo, LineEvent, ReplSession, repl_loop, run_session_with_lines};
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
    // No prompts are asked on these journeys: the verdict channel's
    // receiver is dropped immediately, and the loop's `Next` sends fail
    // harmlessly (the editor is already gone).
    let (verdicts, _) = mpsc::channel(1);
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut io = CommandIo {
        out: &mut out,
        err: &mut err,
        json: json_default,
    };
    let code = run_session_with_lines(
        standalone_config(db),
        None,
        json_default,
        |_, _| EditorIo {
            lines: rx,
            dispositions: verdicts,
        },
        &mut io,
    )
    .await;
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
/// R14: `boards use` is the session's board selection (D6-6): the
/// receipt is typed, the query answers the selection (`board: null`
/// when none), later board-scoped lines pre-fill from it — in a
/// two-board world where the shared single-live default would refuse
/// ambiguous — and unknown/deleted ids refuse with the engine's named
/// kinds. A stale selection survives its board's death untouched (no
/// handler silently mutates session state).
#[tokio::test]
async fn r14_boards_use_selects_prefills_and_refuses_typed() {
    let dir = tempdir();
    let db = dir.path().join("r14.db");
    // Seed two boards one-shot so the session starts ambiguous to the
    // shared default rule.
    let (code, out, err) = run_session(
        &db,
        false,
        vec![
            line("boards create Alpha --json"),
            line("boards create Beta --json"),
            line("exit"),
        ],
    )
    .await;
    assert_eq!(code, 0, "stderr: {}", String::from_utf8_lossy(&err));
    let receipts = json_lines(&out);
    let alpha = receipts[0]["id"].as_str().expect("alpha uuid").to_owned();
    let beta = receipts[1]["id"].as_str().expect("beta uuid").to_owned();

    // A second session: select Alpha, rename without --board (the
    // ambiguous-world proof the pre-fill works), query, then refuse.
    let (code, out, err) = run_session(
        &db,
        false,
        vec![
            line("boards rename Renamed --json"),
            line(&format!("boards use {alpha} --json")),
            line("boards rename Renamed --json"),
            line("boards use --json"),
            line(&format!("boards use {beta} --json")),
            line("boards use 00000000-0000-0000-0000-00000000dead --json"),
            line(&format!("boards delete --board {beta} --confirm --json")),
            line(&format!("boards use {beta} --json")),
            line("boards use --json"),
            line("exit"),
        ],
    )
    .await;
    assert_eq!(code, 0, "stderr: {}", String::from_utf8_lossy(&err));
    let lines = json_lines(&out);
    let refusals = json_lines(&err);
    assert_eq!(refusals.len(), 3);

    // Line 1 refused ambiguous — the two-board world has no shared
    // default, so the pre-fill is what made the later rename land.
    assert_eq!(refusals[0]["error"]["kind"], "ambiguous-board");

    // The selection journey: use → pre-filled rename lands on Alpha.
    assert_eq!(lines[0]["board"], alpha, "the use receipt names the board");
    assert_eq!(lines[1]["id"], alpha, "the pre-filled rename hit Alpha");
    assert_eq!(lines[1]["title"], "Renamed");
    // The query answers the selection.
    assert_eq!(lines[2]["board"], alpha);
    assert_eq!(lines[2]["title"], "Renamed");
    // Re-pointing answers the new selection.
    assert_eq!(lines[3]["board"], beta);
    // Unknown id: typed, naming the uuid (the refusal rides stderr).
    assert_eq!(refusals[1]["error"]["kind"], "unknown-board");
    // A tombstoned target refuses with the same collapsed kind (the
    // engine's `BoardDeleted` maps to `unknown-board` in the CLI's
    // kind vocabulary), and the stale selection survives untouched —
    // the final query still answers it; the remedy is `boards use
    // <other>`.
    assert_eq!(refusals[2]["error"]["kind"], "unknown-board");
    assert_eq!(lines[5]["board"], beta, "the stale selection still answers");
    assert!(
        lines[5].get("title").is_none(),
        "a dead board's title is gone"
    );
}

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
    let (verdicts, _) = mpsc::channel(1);
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut io = CommandIo {
        out: &mut out,
        err: &mut err,
        json: true,
    };
    let code = run_session_with_lines(
        standalone_config(&db),
        None,
        true,
        |_, _| EditorIo {
            lines: rx,
            dispositions: verdicts,
        },
        &mut io,
    )
    .await;
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
    let session = ReplSession::headless(app.config.clone(), forbidden_store(), true);
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
                    board: None,
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
                cmd: BoardsCmd::List { connector: None },
            },
            Command::Boards {
                cmd: BoardsCmd::Select {
                    board: 3,
                    local_board: None,
                    timeout_secs: 1,
                    bind_local_board: false,
                    connector: None,
                },
            },
            Command::Boards {
                cmd: BoardsCmd::Publish {
                    board: None,
                    timeout_secs: 1,
                    connector: None,
                },
            },
            Command::Sync {
                no_wait: false,
                timeout_secs: 1,
                connector: None,
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
    // The re-vehicled typed-refusal test (the `board-exists` refusal the
    // CreateBoard lift retired): a typed engine rejection is a line
    // failure, and the session continues serving lines.
    let dir = tempdir();
    let db = dir.path().join("r9.db");
    let stranger = uuid::Uuid::new_v4().to_string();
    let (code, _out, err) = run_session(
        &db,
        true,
        vec![
            line("boards create \"Personal\""),
            line(&format!("tasks done {stranger}")),
            line("exit"),
        ],
    )
    .await;
    assert_eq!(code, 0);
    let errors = json_lines(&err);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0]["error"]["kind"], "unknown-task");
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
    // The typed verdict needs a row to land on (per-board status).
    seed_bound_board(&db).await;
    let mut config = standalone_config(&db);
    config.nextcloud.server_url = Some("http://127.0.0.1:1".to_owned());
    config.nextcloud.username = Some("alice".to_owned());
    config.nextcloud.credential_store = CredentialStoreKind::Env;
    let (tx, rx) = mpsc::channel(2);
    tx.send(line("sync")).await.expect("alive");
    tx.send(line("exit")).await.expect("alive");
    drop(tx);
    let (verdicts, _) = mpsc::channel(1);
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
    let code = run_session_with_lines(
        config,
        None,
        true,
        |_, _| EditorIo {
            lines: rx,
            dispositions: verdicts,
        },
        &mut io,
    )
    .await;
    drop(guard);
    assert_eq!(code, 0, "the session exits 0 despite the failed line");
    let errors = json_lines(&err);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0]["error"]["kind"], "sync-failed");
}

/// R11: a session on a *booted* standalone core refuses the binding
/// ceremony's migration gate typed — a live local board without
/// `--bind-local-board` surfaces `LocalBoardExists` — and the session
/// survives the failed line. (The ceremony's sync seam is now the
/// engine nudge; the standalone case is gated at the prelude, R7.)
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
    // The live local board the ceremony must not silently merge past.
    let _ = app
        .engine
        .execute(taskboard_domain::StateCommand::CreateBoard {
            title: "local".into(),
            color: taskboard_domain::Color::new("0082c9"),
        })
        .await
        .expect("minted");
    let session = ReplSession::headless(app.config.clone(), forbidden_store(), true);
    let mut err = Vec::new();
    let mut sink = Vec::new();
    let mut io = CommandIo {
        out: &mut sink,
        err: &mut err,
        json: true,
    };
    let entry = taskboard_app::commands::output::BoardsListEntry {
        id: 3,
        title: "remote".into(),
        archived: false,
        deleted: false,
    };
    let outcome = boards::select_on(
        &app,
        entry,
        Duration::from_secs(1),
        false,
        None,
        &taskboard_domain::ConnectorId::from("nextcloud"),
    )
    .await;
    let _ = dispatch_line(
        &session,
        &app,
        Command::Boards {
            cmd: BoardsCmd::Select {
                board: 3,
                local_board: None,
                timeout_secs: 1,
                bind_local_board: false,
                connector: None,
            },
        },
        &mut io,
    )
    .await;
    assert!(
        matches!(
            outcome,
            Err(taskboard_app::commands::CommandError::LocalBoardExists)
        ),
        "the migration gate refuses typed: {outcome:?}"
    );
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
    let session = ReplSession::headless(app.config.clone(), forbidden_store(), false);
    let (tx, rx) = mpsc::channel(1);
    drop(tx);
    let (verdicts, _) = mpsc::channel(1);
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut io = CommandIo {
        out: &mut out,
        err: &mut err,
        json: false,
    };
    let exit = repl_loop(
        app,
        session,
        EditorIo {
            lines: rx,
            dispositions: verdicts,
        },
        &mut io,
    )
    .await;
    assert_eq!(exit, taskboard_app::repl::ReplExit::Graceful);
    // The lock is gone the moment the loop returned: shutdown completed.
    let _next = InstanceLock::acquire(&db).expect("lock released");
}

/// R13: the core and the REPL are mode-agnostic (ADR 0011): a session
/// on a `mode = "kiosk"` config boots, serves its lines, and shuts
/// down — any mode's console. A kiosk machine's *bare* entry is the
/// appliance (ADR 0016); an explicit REPL session on a kiosk config
/// still works, which is what this pins.
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
    let (verdicts, _) = mpsc::channel(1);
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut io = CommandIo {
        out: &mut out,
        err: &mut err,
        json: true,
    };
    let code = run_session_with_lines(
        config,
        None,
        true,
        |_, _| EditorIo {
            lines: rx,
            dispositions: verdicts,
        },
        &mut io,
    )
    .await;
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
    let session = ReplSession::headless(app.config.clone(), forbidden_store(), true);
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
                        board: None,
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
                        board: None,
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
                        board: None,
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
                        board: None,
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
                    cmd: taskboard_app::cli::LabelsCmd::List { board: None },
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
                        scheduled: None,
                        effort: None,
                        importance: None,
                        urgency: None,
                        archive: false,
                        unarchive: false,
                    },
                },
                &mut io,
            )
            .await,
            0
        );
        // The field flags ride the same arm (the REPL mirror): one
        // line sets scheduled/effort/importance and clears urgency;
        // the booted core's list shows the round-trip.
        let start = chrono::NaiveDate::from_ymd_opt(2026, 10, 25)
            .and_then(|d| d.and_hms_opt(9, 0, 0))
            .expect("9am")
            .and_utc();
        assert_eq!(
            route_line(
                &session,
                &app,
                Command::Tasks {
                    cmd: TasksCmd::Edit {
                        task: "t2-renamed".to_owned(),
                        title: None,
                        description: None,
                        due: None,
                        scheduled: Some(taskboard_app::cli::DueSpec::Set(start)),
                        effort: Some(taskboard_app::cli::EffortSpec::Set(
                            taskboard_domain::Effort::new(5400)
                        )),
                        importance: Some(taskboard_app::cli::ImportanceSpec::Set(
                            taskboard_domain::Importance::High
                        )),
                        urgency: Some(taskboard_app::cli::UrgencySpec::Clear),
                        archive: false,
                        unarchive: false,
                    },
                },
                &mut io,
            )
            .await,
            0
        );
        let list = tasks::list_on(&app, None).expect("list on the session core");
        let row = list
            .stacks
            .iter()
            .flat_map(|g| g.tasks.iter())
            .find(|row| row.title == "t2-renamed")
            .expect("the edited row");
        assert_eq!(row.scheduled, Some(start));
        assert_eq!(row.effort, Some(5400));
        assert_eq!(row.importance.as_deref(), Some("high"));
        assert_eq!(row.urgency, None, "the clear sentinel left it unset");

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

// ---------------------------------------------------------------------------
// J-series: the in-session connector ceremonies (ADR 0014, chunk 3).
// Hermetic: wiremock is the loopback server (the repo's established
// hermetic-HTTP seam), the scripted disposition peer answers prompts
// (no TTY), a `FakeCredentialStore` rides the injected session store.
// No sleeps: the harness's own deadline bounds a wedged session, and
// wiremock is request-verified.
// ---------------------------------------------------------------------------

use std::collections::VecDeque;
use std::path::PathBuf;

use base64::Engine as _;

use taskboard_app::repl::{Disposition, PromptValue, run_session_with_parts};

/// The deadline bounding one ceremony journey (a scripted peer that
/// never answers would otherwise wedge the test binary).
const JOURNEY_DEADLINE: Duration = Duration::from_secs(120);

/// Answers `Prompt` dispositions from the script in order; `Next`
/// verdicts are ignored (the peer only owes prompt answers).
async fn scripted_peer(
    mut rx: mpsc::Receiver<Disposition>,
    mut answers: VecDeque<Result<PromptValue, std::io::Error>>,
) {
    while let Some(disposition) = rx.recv().await {
        if let Disposition::Prompt { reply, .. } = disposition {
            let _ = reply.send(answers.pop_front().expect("script covers every prompt"));
        }
    }
}

/// Runs one ceremony journey: pre-sent line events, a scripted
/// disposition peer, an injected store and config path — the terminal
/// never appears. Bounded by [`JOURNEY_DEADLINE`].
async fn run_with(
    config: AppConfig,
    config_path: Option<PathBuf>,
    store: Arc<dyn CredentialStore>,
    json_default: bool,
    events: Vec<LineEvent>,
    answers: Vec<Result<PromptValue, std::io::Error>>,
) -> (u8, Vec<u8>, Vec<u8>) {
    let (tx, rx) = mpsc::channel(events.len().max(1));
    for event in events {
        tx.send(event).await.expect("session channel alive");
    }
    drop(tx);
    let (vtx, vrx) = mpsc::channel(1);
    tokio::spawn(scripted_peer(vrx, VecDeque::from(answers)));
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut io = CommandIo {
        out: &mut out,
        err: &mut err,
        json: json_default,
    };
    let body = run_session_with_parts(
        config,
        config_path,
        store,
        json_default,
        |_, _| EditorIo {
            lines: rx,
            dispositions: vtx,
        },
        &mut io,
    );
    let code = tokio::time::timeout(JOURNEY_DEADLINE, body)
        .await
        .expect("journey must not wedge");
    (code, out, err)
}

/// A tempdir config file over the given db (auto backend unless the
/// body says otherwise), loaded through the same resolution the
/// session boot uses.
fn seeded_config_file(dir: &std::path::Path, db: &std::path::Path, body: &str) -> PathBuf {
    let config_path = dir.join("taskboard.toml");
    std::fs::write(
        &config_path,
        body.replace("%DB%", &db.display().to_string()),
    )
    .expect("seed config");
    config_path
}

fn minimal_toml(extra: impl std::fmt::Display) -> String {
    format!("[storage]\ndb_path = '%DB%'\n[app]\nshutdown_timeout_secs = 2\n{extra}")
}

/// J1: a standalone session's `connect` line runs the ceremony through
/// the disposition handshake, refreshes the session config, attaches
/// the connector (receipt `attached: true`), and a following `sync`
/// line routes to the engine against the (loopback) server.
/// Seeds one live, bound board so a session's sync cycle has a target
/// row for its verdict (the per-board status semantics, wave chunk 1).
async fn seed_bound_board(db: &std::path::Path) {
    use taskboard_domain::persistence::TaskRepository as _;
    let repo = Arc::new(
        taskboard_storage_sqlite::open(db, taskboard_domain::ConnectorId::from("nextcloud"))
            .await
            .expect("seed db"),
    );
    repo.apply(vec![taskboard_domain::PersistenceAction::UpsertBoard(
        taskboard_domain::Board {
            id: taskboard_domain::BoardId::from_uuid(uuid::Uuid::from_u128(0x51)),
            remote: taskboard_domain::test_support::test_binding(taskboard_domain::RemoteBoardId(
                common::BOARD,
            )),
            title: "board".into(),
            color: taskboard_domain::Color::new("00ff00"),
            archived: false,
            deleted: false,
            clocks: taskboard_domain::BoardClocks {
                title: taskboard_domain::BoardClocks::epoch(),
                color: taskboard_domain::BoardClocks::epoch(),
                archived: taskboard_domain::BoardClocks::epoch(),
                deleted: taskboard_domain::BoardClocks::epoch(),
            },
            remote_seen: None,
        },
    )])
    .await
    .expect("seed applies");
}

#[tokio::test]
async fn j1_connect_line_attaches_and_refreshes_the_session() {
    let dir = tempdir();
    let db = dir.path().join("j1.db");
    seed_bound_board(&db).await;
    let config_path = seeded_config_file(dir.path(), &db, &minimal_toml(""));
    let config = AppConfig::load(Some(config_path.clone())).expect("config resolves");
    let store: Arc<dyn CredentialStore> = Arc::new(FakeCredentialStore::default());
    let server = wiremock::MockServer::start().await;
    common::mount_standard_pull(&server, serde_json::json!([])).await;
    let url = server.uri();

    let (code, out, err) = run_with(
        config,
        Some(config_path.clone()),
        store.clone(),
        true,
        vec![
            line(&format!("connect --server {url} --user alice --json")),
            line("sync --json"),
            line("exit"),
        ],
        vec![Ok(PromptValue::Text("j1-secret".to_owned()))],
    )
    .await;

    assert_eq!(code, 0, "stderr: {}", String::from_utf8_lossy(&err));
    let receipts = json_lines(&out);
    assert_eq!(receipts.len(), 2, "out: {}", String::from_utf8_lossy(&out));
    assert_eq!(receipts[0]["server_url"], url);
    assert_eq!(receipts[0]["attached"], true);
    assert_eq!(receipts[0]["stored"], "keyring");
    // The sync line's prelude passed on the refreshed config and the
    // cycle *ran against the engine* — the seeded board's pull settles
    // idle, not the prelude's `sync-disabled`.
    assert_eq!(receipts[1]["phase"], "idle");
    let errors = json_lines(&err);
    assert!(
        errors.is_empty(),
        "stderr: {}",
        String::from_utf8_lossy(&err)
    );

    // The config file was written at the session's config path (the
    // refresh re-reads exactly it), secret-free.
    let written = std::fs::read_to_string(&config_path).expect("config written");
    assert!(written.contains(&url), "server persisted");
    assert!(written.contains("username"), "username persisted");
    assert!(!written.contains("j1-secret"), "no secret in the file");
    // The store holds the secret the attach loaded.
    store.load(&url, "alice").expect("secret stored");
}

/// J2: a failed verify is just a failed line — typed envelope, no
/// config write, no secret, and the session (still standalone)
/// survives; the following `sync` line refuses `sync-disabled` because
/// nothing about the session changed.
#[tokio::test]
async fn j2_a_failed_ceremony_changes_nothing_and_the_session_survives() {
    let dir = tempdir();
    let db = dir.path().join("j2.db");
    let config_path = seeded_config_file(dir.path(), &db, &minimal_toml(""));
    let config = AppConfig::load(Some(config_path.clone())).expect("config resolves");
    let store: Arc<dyn CredentialStore> = Arc::new(FakeCredentialStore::default());
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(
            "/index.php/apps/deck/api/v1.0/boards",
        ))
        .respond_with(wiremock::ResponseTemplate::new(401))
        .mount(&server)
        .await;

    let (code, out, err) = run_with(
        config,
        Some(config_path.clone()),
        store.clone(),
        true,
        vec![
            line(&format!(
                "connect --server {} --user alice --json",
                server.uri()
            )),
            line("boards create \"Personal\" --json"),
            line("sync --json"),
            line("exit"),
        ],
        vec![Ok(PromptValue::Text("j2-secret".to_owned()))],
    )
    .await;

    assert_eq!(code, 0, "the session survives the failed line");
    let errors = json_lines(&err);
    assert_eq!(errors.len(), 2);
    assert_eq!(errors[0]["error"]["kind"], "invalid-credentials");
    assert_eq!(errors[1]["error"]["kind"], "sync-disabled");
    // The probe line after the failure still executed.
    let receipts = json_lines(&out);
    assert_eq!(receipts.len(), 1);
    assert_eq!(receipts[0]["title"], "Personal");
    // No config write, no secret stored.
    let written = std::fs::read_to_string(&config_path).expect("config untouched");
    assert!(!written.contains("server_url"), "nothing persisted");
    assert!(!written.contains("alice"));
    assert!(
        store.load(&server.uri(), "alice").is_err(),
        "the failed verify persisted no secret"
    );
}

/// J3: the D2 guard's y/N confirm arrives as a `Confirm` disposition;
/// a scripted decline refuses typed `backend-explicit-standalone`, a
/// scripted consent proceeds (the guard matrix itself stays chunk 2's
/// test home — this pins only the disposition mapping).
#[tokio::test]
async fn j3_the_backend_guard_asks_as_a_confirm_disposition() {
    let dir = tempdir();
    let db = dir.path().join("j3.db");
    let config_path = seeded_config_file(
        dir.path(),
        &db,
        &minimal_toml("[sync]\nbackend = 'standalone'\n"),
    );
    let server = wiremock::MockServer::start().await;
    common::mount_standard_pull(&server, serde_json::json!([])).await;

    // Decline: the confirm arrives, the refusal is typed, no prompts
    // after it (the ceremony refuses before gathering).
    let config = AppConfig::load(Some(config_path.clone())).expect("config resolves");
    let store: Arc<dyn CredentialStore> = Arc::new(FakeCredentialStore::default());
    let (code, _out, err) = run_with(
        config,
        Some(config_path.clone()),
        store,
        true,
        vec![
            line(&format!(
                "connect --server {} --user alice --json",
                server.uri()
            )),
            line("exit"),
        ],
        vec![Ok(PromptValue::Flag(false))],
    )
    .await;
    assert_eq!(code, 0);
    let errors = json_lines(&err);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0]["error"]["kind"], "backend-explicit-standalone");

    // Consent: the ceremony proceeds to gather and attach.
    let config = AppConfig::load(Some(config_path.clone())).expect("config resolves");
    let store: Arc<dyn CredentialStore> = Arc::new(FakeCredentialStore::default());
    let (code, out, err) = run_with(
        config,
        Some(config_path.clone()),
        store,
        true,
        vec![
            line(&format!(
                "connect --server {} --user alice --json",
                server.uri()
            )),
            line("exit"),
        ],
        vec![
            Ok(PromptValue::Flag(true)),
            Ok(PromptValue::Text("j3-secret".to_owned())),
        ],
    )
    .await;
    assert_eq!(code, 0, "stderr: {}", String::from_utf8_lossy(&err));
    let receipts = json_lines(&out);
    assert_eq!(receipts[0]["attached"], true);
    assert_eq!(receipts[0]["backend_switched"], true);
}

/// J4: a connect line over a *live* connector detaches first and re
/// -attaches against the new identity — the second (auth-pinned) mock
/// proves the actor serves the new secret (the Basic header), and the
/// receipt reports the re-point with `attached: true`.
#[tokio::test]
async fn j4_a_repoint_swaps_the_live_connector_to_the_new_identity() {
    let dir = tempdir();
    let db = dir.path().join("j4.db");
    seed_bound_board(&db).await;
    let old = wiremock::MockServer::start().await;
    common::mount_standard_pull(&old, serde_json::json!([])).await;
    let config_path = seeded_config_file(
        dir.path(),
        &db,
        &minimal_toml(format!(
            "[nextcloud]\ncredential_store = 'keyring'\nserver_url = '{}'\nusername = 'alice'\n[sync]\npoll_interval_secs = 1\n",
            old.uri()
        )),
    );
    let config = AppConfig::load(Some(config_path.clone())).expect("config resolves");
    let store: Arc<dyn CredentialStore> = Arc::new(FakeCredentialStore::with(
        &old.uri(),
        "alice",
        "j4-old-secret",
    ));

    let new = wiremock::MockServer::start().await;
    let expected = format!("Basic {}", basic_auth_value("alice:j4-new-secret"));
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(
            "/index.php/apps/deck/api/v1.0/boards",
        ))
        .and(wiremock::matchers::header(
            "authorization",
            expected.clone(),
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .insert_header("ETag", "\"j4\"")
                .set_body_string(common::boards_json()),
        )
        .mount(&new)
        .await;

    let (code, out, err) = run_with(
        config,
        Some(config_path.clone()),
        store.clone(),
        true,
        vec![
            line(&format!(
                "connect --server {} --user alice --json",
                new.uri()
            )),
            line("sync --json"),
            line("exit"),
        ],
        vec![
            Ok(PromptValue::Text("j4-new-secret".to_owned())),
            Ok(PromptValue::Flag(true)),
        ],
    )
    .await;

    assert_eq!(code, 0, "stderr: {}", String::from_utf8_lossy(&err));
    let receipts = json_lines(&out);
    assert_eq!(receipts[0]["attached"], true);
    assert_eq!(receipts[0]["repointed"], true);
    assert_eq!(receipts[0]["server_url"], new.uri());
    // The sync line's cycle hit the new mock with the *new* secret's
    // Basic header (the pinned matcher would 404 an old-secret actor)
    // and settled the seeded board's row idle on the merged listing.
    assert_eq!(receipts[1]["phase"], "idle");
    // At least one boards read rode the new identity's header (the
    // pinned matcher would 404 an old-secret actor).
    let requests = new.received_requests().await.expect("requests recorded");
    assert!(
        requests.iter().any(|request| request
            .headers
            .get("authorization")
            .is_some_and(|value| value == &expected)),
        "the new secret reached the wire"
    );
    // The replaced identity's secret is gone (no keyring orphans).
    assert!(store.load(&old.uri(), "alice").is_err());
    assert_eq!(
        store
            .load(&new.uri(), "alice")
            .expect("new secret stored")
            .expose(),
        "j4-new-secret"
    );
}

/// `base64(user:password)` for the auth-pinned matcher (the same Basic
/// shape the `DeckClient` sends).
fn basic_auth_value(user_pass: &str) -> String {
    base64::engine::general_purpose::STANDARD.encode(user_pass)
}

/// J5: the `login` alias routes to the same ceremony with the narrower
/// surface — under an explicit `standalone` backend it refuses typed
/// (structural kind; the stderr notice is UX, never asserted).
#[tokio::test]
async fn j5_the_login_alias_refuses_typed_where_connect_would_confirm() {
    let dir = tempdir();
    let db = dir.path().join("j5.db");
    let config_path = seeded_config_file(
        dir.path(),
        &db,
        &minimal_toml("[sync]\nbackend = 'standalone'\n"),
    );
    let config = AppConfig::load(Some(config_path.clone())).expect("config resolves");
    let store: Arc<dyn CredentialStore> = Arc::new(FakeCredentialStore::default());
    let (code, _out, err) = run_with(
        config,
        Some(config_path),
        store,
        true,
        vec![
            line("login --server http://127.0.0.1:1 --user alice --json"),
            line("exit"),
        ],
        // No answers: the alias refuses before prompting anything.
        vec![],
    )
    .await;
    assert_eq!(code, 0);
    let errors = json_lines(&err);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0]["error"]["kind"], "backend-explicit-standalone");
}

/// J6: the in-session `logout` under the auto backend — the receipt
/// names the detach, the config file is stripped, the refreshed
/// session resolves standalone (`sync` refuses `sync-disabled` again),
/// and a subsequent `connect` line re-attaches (re-attach after detach
/// is permitted).
#[tokio::test]
async fn j6_logout_detaches_strips_and_a_reconnect_reattaches() {
    let dir = tempdir();
    let db = dir.path().join("j6.db");
    let server = wiremock::MockServer::start().await;
    common::mount_standard_pull(&server, serde_json::json!([])).await;
    let config_path = seeded_config_file(
        dir.path(),
        &db,
        &minimal_toml(format!(
            "[nextcloud]\ncredential_store = 'keyring'\nserver_url = '{}'\nusername = 'alice'\n[sync]\npoll_interval_secs = 1\n",
            server.uri()
        )),
    );
    let config = AppConfig::load(Some(config_path.clone())).expect("config resolves");
    let store: Arc<dyn CredentialStore> = Arc::new(FakeCredentialStore::with(
        &server.uri(),
        "alice",
        "j6-secret",
    ));

    let (code, out, err) = run_with(
        config,
        Some(config_path.clone()),
        store.clone(),
        true,
        vec![
            line("logout --json"),
            line("sync --json"),
            line(&format!(
                "connect --server {} --user alice --json",
                server.uri()
            )),
            line("exit"),
        ],
        vec![Ok(PromptValue::Text("j6-secret".to_owned()))],
    )
    .await;

    assert_eq!(code, 0, "stderr: {}", String::from_utf8_lossy(&err));
    let receipts = json_lines(&out);
    assert_eq!(receipts.len(), 2, "out: {}", String::from_utf8_lossy(&out));
    assert_eq!(receipts[0]["session_connector"], "detached");
    assert_eq!(receipts[0]["config_cleared"], true);
    // The refreshed session is honestly standalone again.
    let errors = json_lines(&err);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0]["error"]["kind"], "sync-disabled");
    // And a connect line re-attaches (the guard permits re-attach).
    assert_eq!(receipts[1]["attached"], true);
    // The config file was stripped and then re-written.
    let written = std::fs::read_to_string(&config_path).expect("config rewritten");
    assert!(written.contains(&server.uri()));
}

/// J7: the explicit-`nextcloud` mirror — the keys stay (`config_cleared`
/// false), the refreshed session stays nextcloud, so the post-logout
/// `sync` passes the prelude and refuses `sync-unavailable` (the WI-H
/// rider, now in-session); a re-connect re-attaches.
#[tokio::test]
async fn j7_logout_keeps_an_explicit_backend_and_sync_refuses_unavailable() {
    let dir = tempdir();
    let db = dir.path().join("j7.db");
    let server = wiremock::MockServer::start().await;
    common::mount_standard_pull(&server, serde_json::json!([])).await;
    let config_path = seeded_config_file(
        dir.path(),
        &db,
        &minimal_toml(format!(
            "[nextcloud]\ncredential_store = 'keyring'\nserver_url = '{}'\nusername = 'alice'\n[sync]\nbackend = 'nextcloud'\npoll_interval_secs = 1\n",
            server.uri()
        )),
    );
    let config = AppConfig::load(Some(config_path.clone())).expect("config resolves");
    let store: Arc<dyn CredentialStore> = Arc::new(FakeCredentialStore::with(
        &server.uri(),
        "alice",
        "j7-secret",
    ));

    let (code, out, err) = run_with(
        config,
        Some(config_path.clone()),
        store,
        true,
        vec![
            line("logout --json"),
            line("sync --json"),
            line(&format!(
                "connect --server {} --user alice --json",
                server.uri()
            )),
            line("exit"),
        ],
        vec![Ok(PromptValue::Text("j7-secret".to_owned()))],
    )
    .await;

    assert_eq!(code, 0, "stderr: {}", String::from_utf8_lossy(&err));
    let receipts = json_lines(&out);
    assert_eq!(receipts.len(), 2, "out: {}", String::from_utf8_lossy(&out));
    assert_eq!(receipts[0]["session_connector"], "detached");
    assert_eq!(receipts[0]["config_cleared"], false);
    let errors = json_lines(&err);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0]["error"]["kind"], "sync-unavailable");
    assert_eq!(receipts[1]["attached"], true);
    // The explicit backend word survives the logout (loud stays loud).
    let written = std::fs::read_to_string(&config_path).expect("config");
    assert!(written.contains("nextcloud"));
}

/// J8: `logout` on a standalone session refuses typed
/// (`not-configured`) and the session continues.
#[tokio::test]
async fn j8_logout_on_a_standalone_session_refuses_typed_and_survives() {
    let dir = tempdir();
    let db = dir.path().join("j8.db");
    let config_path = seeded_config_file(dir.path(), &db, &minimal_toml(""));
    let config = AppConfig::load(Some(config_path.clone())).expect("config resolves");
    let store: Arc<dyn CredentialStore> = Arc::new(FakeCredentialStore::default());
    let (code, out, err) = run_with(
        config,
        Some(config_path),
        store,
        true,
        vec![
            line("logout --json"),
            line("boards create \"Personal\" --json"),
            line("exit"),
        ],
        vec![],
    )
    .await;
    assert_eq!(code, 0);
    let errors = json_lines(&err);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0]["error"]["kind"], "not-configured");
    let receipts = json_lines(&out);
    assert_eq!(receipts.len(), 1);
    assert_eq!(receipts[0]["title"], "Personal");
}

/// R14: the in-session `status` under `standalone` omits the connector
/// fact entirely — a standalone core has no connector concept, and a
/// `false` would read as "detached (logged out)".
#[tokio::test]
async fn r14_in_session_status_under_standalone_omits_the_connector_fact() {
    let dir = tempdir();
    let db = dir.path().join("r14.db");
    let app = taskboard_app::bootstrap::bootstrap(
        standalone_config(&db),
        forbidden_store(),
        Arc::new(taskboard_app::notifications::LogSink),
    )
    .await
    .expect("session core boots");
    let session = ReplSession::headless(app.config.clone(), forbidden_store(), true);
    let mut out = Vec::new();
    let mut err = Vec::new();
    {
        let mut io = CommandIo {
            out: &mut out,
            err: &mut err,
            json: true,
        };
        let code = dispatch_line(&session, &app, Command::Status, &mut io).await;
        assert_eq!(code, 0, "stderr: {}", String::from_utf8_lossy(&err));
    }
    app.shutdown().await;
    let receipts = json_lines(&out);
    assert_eq!(receipts.len(), 1);
    assert_eq!(receipts[0]["daemon"], serde_json::Value::Bool(false));
    assert!(
        receipts[0].get("connector_attached").is_none(),
        "standalone has no connector fact: {}",
        receipts[0]
    );
}

/// J10: the in-session `status` under an explicit `nextcloud` backend
/// answers the live attach fact — `true` at boot-attach, `false` after
/// an in-session `logout` detaches (the session stays `nextcloud`:
/// the config keys survive).
#[tokio::test]
async fn j10_in_session_status_surfaces_the_connector_fact_across_logout() {
    let dir = tempdir();
    let db = dir.path().join("j10.db");
    let server = wiremock::MockServer::start().await;
    common::mount_standard_pull(&server, serde_json::json!([])).await;
    let config_path = seeded_config_file(
        dir.path(),
        &db,
        &minimal_toml(format!(
            "[nextcloud]\ncredential_store = 'keyring'\nserver_url = '{}'\nusername = 'alice'\n[sync]\nbackend = 'nextcloud'\npoll_interval_secs = 1\n",
            server.uri()
        )),
    );
    let config = AppConfig::load(Some(config_path.clone())).expect("config resolves");
    let store: Arc<dyn CredentialStore> = Arc::new(FakeCredentialStore::with(
        &server.uri(),
        "alice",
        "j10-secret",
    ));

    let (code, out, err) = run_with(
        config,
        Some(config_path),
        store,
        true,
        vec![
            line("status --json"),
            line("logout --json"),
            line("status --json"),
            line("exit"),
        ],
        vec![],
    )
    .await;

    assert_eq!(code, 0, "stderr: {}", String::from_utf8_lossy(&err));
    let receipts = json_lines(&out);
    assert_eq!(receipts.len(), 3, "out: {}", String::from_utf8_lossy(&out));
    assert_eq!(
        receipts[0]["connector_attached"],
        serde_json::Value::Bool(true),
        "boot-attached"
    );
    assert_eq!(
        receipts[1]["config_cleared"],
        serde_json::Value::Bool(false),
        "the explicit backend keeps the keys"
    );
    assert_eq!(
        receipts[2]["connector_attached"],
        serde_json::Value::Bool(false),
        "detached after the in-session logout"
    );
}

/// J9: an `Interrupted` event gets its `Next` verdict and the session
/// continues — a following line works, and the session end drops the
/// peer cleanly (the journey's own deadline guards the liveness).
#[tokio::test]
async fn j9_interrupted_gets_its_verdict_and_the_session_continues() {
    let dir = tempdir();
    let db = dir.path().join("j9.db");
    let config_path = seeded_config_file(dir.path(), &db, &minimal_toml(""));
    let config = AppConfig::load(Some(config_path.clone())).expect("config resolves");
    let store: Arc<dyn CredentialStore> = Arc::new(FakeCredentialStore::default());
    let (code, out, err) = run_with(
        config,
        Some(config_path),
        store,
        true,
        vec![
            LineEvent::Interrupted,
            line("boards create \"Personal\" --json"),
            LineEvent::Eof,
        ],
        vec![],
    )
    .await;
    assert_eq!(code, 0);
    assert!(
        json_lines(&err).is_empty(),
        "stderr: {}",
        String::from_utf8_lossy(&err)
    );
    let receipts = json_lines(&out);
    assert_eq!(receipts.len(), 1);
    assert_eq!(receipts[0]["title"], "Personal");
}
