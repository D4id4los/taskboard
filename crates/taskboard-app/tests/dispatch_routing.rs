// SPDX-License-Identifier: AGPL-3.0-only
//! D-series: hermetic dispatch-routing suite (M1-exit report §2, gap 2).
//! llvm-cov does not instrument the subprocess-driving J/K suites, so
//! `dispatch()`'s match arms were invisible to the coverage report. One
//! test per `Command` variant proves the arm routes to a typed failure
//! outcome (exit 1) — routing only; the verbs' success paths are the
//! L/V/K-series' job, not duplicated here.

mod common;

use taskboard_app::cli::{BoardsCmd, Command, ConfigCmd, LabelsCmd, StacksCmd, TasksCmd};
use taskboard_app::commands::{CommandCtx, CommandIo, dispatch};

use common::{env_lock, remove_env, tempdir};

/// The dispatcher's per-invocation context over a per-test config file,
/// writing into the test's buffers.
fn ctx<'a>(dir: &std::path::Path, out: &'a mut Vec<u8>, err: &'a mut Vec<u8>) -> CommandCtx<'a> {
    CommandCtx {
        config_path: Some(dir.join("taskboard.toml")),
        io: CommandIo {
            out,
            err,
            json: true,
        },
    }
}

/// Writes the minimal valid config and guarantees the env credential
/// store finds nothing: every core-booting arm then fails fast at the
/// credential load, deterministically routing to exit 1.
fn failing_setup() -> (common::EnvGuard, tempfile::TempDir) {
    let dir = tempdir();
    common::write_test_config(
        &dir.path().join("taskboard.toml"),
        Some("http://127.0.0.1:1"),
        Some("alice"),
    )
    .expect("write config");
    let guard = env_lock();
    remove_env("TASKBOARD_APP_PASSWORD");
    (guard, dir)
}

/// An undecodable config file: every arm (login's `extract_raw`
/// included) must reject it and report exit 1.
fn garbage_setup() -> (common::EnvGuard, tempfile::TempDir) {
    let dir = tempdir();
    std::fs::write(dir.path().join("taskboard.toml"), "= not toml =").expect("write garbage");
    (env_lock(), dir)
}

#[tokio::test]
async fn login_with_an_undecodable_config_routes_to_exit_one() {
    let (_guard, dir) = garbage_setup();
    let mut out = Vec::new();
    let mut err = Vec::new();
    let code = dispatch(
        Command::Login {
            server: None,
            user: None,
        },
        ctx(dir.path(), &mut out, &mut err),
    )
    .await;
    assert_eq!(code, 1);
}

/// Piped stdin (nextest) feeds an empty app password; the verification
/// call then hits the refused server and login reports exit 1.
#[tokio::test]
async fn login_with_unverifiable_credentials_routes_to_exit_one() {
    let (_guard, dir) = failing_setup();
    let mut out = Vec::new();
    let mut err = Vec::new();
    let code = dispatch(
        Command::Login {
            server: Some("http://127.0.0.1:1".to_owned()),
            user: Some("alice".to_owned()),
        },
        ctx(dir.path(), &mut out, &mut err),
    )
    .await;
    assert_eq!(code, 1);
}

#[tokio::test]
async fn boards_list_routes_a_failed_credential_load_to_exit_one() {
    let (_guard, dir) = failing_setup();
    let mut out = Vec::new();
    let mut err = Vec::new();
    let code = dispatch(
        Command::Boards {
            cmd: BoardsCmd::List { connector: None },
        },
        ctx(dir.path(), &mut out, &mut err),
    )
    .await;
    assert_eq!(code, 1);
}

#[tokio::test]
async fn boards_select_routes_a_failed_credential_load_to_exit_one() {
    let (_guard, dir) = failing_setup();
    let mut out = Vec::new();
    let mut err = Vec::new();
    let code = dispatch(
        Command::Boards {
            cmd: BoardsCmd::Select {
                board: 3,
                local_board: None,
                timeout_secs: 1,
                bind_local_board: false,
                connector: None,
            },
        },
        ctx(dir.path(), &mut out, &mut err),
    )
    .await;
    assert_eq!(code, 1);
}

#[tokio::test]
async fn tasks_list_routes_a_failed_credential_load_to_exit_one() {
    let (_guard, dir) = failing_setup();
    let mut out = Vec::new();
    let mut err = Vec::new();
    let code = dispatch(
        Command::Tasks {
            cmd: TasksCmd::List { stack: None },
        },
        ctx(dir.path(), &mut out, &mut err),
    )
    .await;
    assert_eq!(code, 1);
}

#[tokio::test]
async fn tasks_add_routes_a_failed_credential_load_to_exit_one() {
    let (_guard, dir) = failing_setup();
    let mut out = Vec::new();
    let mut err = Vec::new();
    let code = dispatch(
        Command::Tasks {
            cmd: TasksCmd::Add {
                title: "x".to_owned(),
                stack: None,
            },
        },
        ctx(dir.path(), &mut out, &mut err),
    )
    .await;
    assert_eq!(code, 1);
}

#[tokio::test]
async fn tasks_done_routes_a_failed_credential_load_to_exit_one() {
    let (_guard, dir) = failing_setup();
    let mut out = Vec::new();
    let mut err = Vec::new();
    let code = dispatch(
        Command::Tasks {
            cmd: TasksCmd::Done {
                task: "some-task".to_owned(),
                undo: false,
            },
        },
        ctx(dir.path(), &mut out, &mut err),
    )
    .await;
    assert_eq!(code, 1);
}

#[tokio::test]
async fn tasks_move_routes_a_failed_credential_load_to_exit_one() {
    let (_guard, dir) = failing_setup();
    let mut out = Vec::new();
    let mut err = Vec::new();
    let code = dispatch(
        Command::Tasks {
            cmd: TasksCmd::Move {
                task: "some-task".to_owned(),
                stack: None,
                board: None,
                order: None,
            },
        },
        ctx(dir.path(), &mut out, &mut err),
    )
    .await;
    assert_eq!(code, 1);
}

#[tokio::test]
async fn sync_routes_a_failed_credential_load_to_exit_one() {
    let (_guard, dir) = failing_setup();
    let mut out = Vec::new();
    let mut err = Vec::new();
    let code = dispatch(
        Command::Sync {
            no_wait: false,
            timeout_secs: 1,
            connector: None,
        },
        ctx(dir.path(), &mut out, &mut err),
    )
    .await;
    assert_eq!(code, 1);
}

/// WI-3 editing verbs: one routing test per new `Command` variant, in
/// the `failing_setup` shape (exit 1 at the credential load).
#[tokio::test]
async fn tasks_edit_routes_a_failed_credential_load_to_exit_one() {
    let (_guard, dir) = failing_setup();
    let mut out = Vec::new();
    let mut err = Vec::new();
    let code = dispatch(
        Command::Tasks {
            cmd: TasksCmd::Edit {
                task: "x".to_owned(),
                title: Some("t".to_owned()),
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
        ctx(dir.path(), &mut out, &mut err),
    )
    .await;
    assert_eq!(code, 1);
}

#[tokio::test]
async fn tasks_delete_routes_a_failed_credential_load_to_exit_one() {
    let (_guard, dir) = failing_setup();
    let mut out = Vec::new();
    let mut err = Vec::new();
    let code = dispatch(
        Command::Tasks {
            cmd: TasksCmd::Delete {
                task: "x".to_owned(),
            },
        },
        ctx(dir.path(), &mut out, &mut err),
    )
    .await;
    assert_eq!(code, 1);
}

#[tokio::test]
async fn boards_rename_routes_a_failed_credential_load_to_exit_one() {
    let (_guard, dir) = failing_setup();
    let mut out = Vec::new();
    let mut err = Vec::new();
    let code = dispatch(
        Command::Boards {
            cmd: BoardsCmd::Rename {
                board: None,
                title: "t".to_owned(),
            },
        },
        ctx(dir.path(), &mut out, &mut err),
    )
    .await;
    assert_eq!(code, 1);
}

#[tokio::test]
async fn stacks_rename_routes_a_failed_credential_load_to_exit_one() {
    let (_guard, dir) = failing_setup();
    let mut out = Vec::new();
    let mut err = Vec::new();
    let code = dispatch(
        Command::Stacks {
            cmd: StacksCmd::Rename {
                stack: "s".to_owned(),
                title: "t".to_owned(),
            },
        },
        ctx(dir.path(), &mut out, &mut err),
    )
    .await;
    assert_eq!(code, 1);
}

#[tokio::test]
async fn stacks_delete_routes_a_failed_credential_load_to_exit_one() {
    let (_guard, dir) = failing_setup();
    let mut out = Vec::new();
    let mut err = Vec::new();
    let code = dispatch(
        Command::Stacks {
            cmd: StacksCmd::Delete {
                stack: "s".to_owned(),
            },
        },
        ctx(dir.path(), &mut out, &mut err),
    )
    .await;
    assert_eq!(code, 1);
}

#[tokio::test]
async fn stacks_move_routes_a_failed_credential_load_to_exit_one() {
    let (_guard, dir) = failing_setup();
    let mut out = Vec::new();
    let mut err = Vec::new();
    let code = dispatch(
        Command::Stacks {
            cmd: StacksCmd::Move {
                stack: "s".to_owned(),
                order: 1,
            },
        },
        ctx(dir.path(), &mut out, &mut err),
    )
    .await;
    assert_eq!(code, 1);
}

#[tokio::test]
async fn labels_list_routes_a_failed_credential_load_to_exit_one() {
    let (_guard, dir) = failing_setup();
    let mut out = Vec::new();
    let mut err = Vec::new();
    let code = dispatch(
        Command::Labels {
            cmd: LabelsCmd::List { board: None },
        },
        ctx(dir.path(), &mut out, &mut err),
    )
    .await;
    assert_eq!(code, 1);
}

#[tokio::test]
async fn labels_add_routes_a_failed_credential_load_to_exit_one() {
    let (_guard, dir) = failing_setup();
    let mut out = Vec::new();
    let mut err = Vec::new();
    let code = dispatch(
        Command::Labels {
            cmd: LabelsCmd::Add {
                board: None,
                title: "l".to_owned(),
                color: None,
            },
        },
        ctx(dir.path(), &mut out, &mut err),
    )
    .await;
    assert_eq!(code, 1);
}

#[tokio::test]
async fn labels_edit_routes_a_failed_credential_load_to_exit_one() {
    let (_guard, dir) = failing_setup();
    let mut out = Vec::new();
    let mut err = Vec::new();
    let code = dispatch(
        Command::Labels {
            cmd: LabelsCmd::Edit {
                label: "l".to_owned(),
                title: Some("t".to_owned()),
                color: None,
            },
        },
        ctx(dir.path(), &mut out, &mut err),
    )
    .await;
    assert_eq!(code, 1);
}

#[tokio::test]
async fn labels_delete_routes_a_failed_credential_load_to_exit_one() {
    let (_guard, dir) = failing_setup();
    let mut out = Vec::new();
    let mut err = Vec::new();
    let code = dispatch(
        Command::Labels {
            cmd: LabelsCmd::Delete {
                label: "l".to_owned(),
            },
        },
        ctx(dir.path(), &mut out, &mut err),
    )
    .await;
    assert_eq!(code, 1);
}

#[tokio::test]
async fn tasks_label_routes_a_failed_credential_load_to_exit_one() {
    let (_guard, dir) = failing_setup();
    let mut out = Vec::new();
    let mut err = Vec::new();
    let code = dispatch(
        Command::Tasks {
            cmd: TasksCmd::Label {
                task: "t".to_owned(),
                label: "l".to_owned(),
            },
        },
        ctx(dir.path(), &mut out, &mut err),
    )
    .await;
    assert_eq!(code, 1);
}

#[tokio::test]
async fn tasks_unlabel_routes_a_failed_credential_load_to_exit_one() {
    let (_guard, dir) = failing_setup();
    let mut out = Vec::new();
    let mut err = Vec::new();
    let code = dispatch(
        Command::Tasks {
            cmd: TasksCmd::Unlabel {
                task: "t".to_owned(),
                label: "l".to_owned(),
            },
        },
        ctx(dir.path(), &mut out, &mut err),
    )
    .await;
    assert_eq!(code, 1);
}

/// Every core-booting arm also rejects an undecodable config before any
/// boot work (the `core_ctx` Err branch each arm shares).
#[tokio::test]
async fn a_core_command_with_an_undecodable_config_routes_to_exit_one() {
    let (_guard, dir) = garbage_setup();
    let mut out = Vec::new();
    let mut err = Vec::new();
    let code = dispatch(
        Command::Tasks {
            cmd: TasksCmd::List { stack: None },
        },
        ctx(dir.path(), &mut out, &mut err),
    )
    .await;
    assert_eq!(code, 1);
}

/// `logout` routing: the env-selected store cannot delete (the typed
/// `credential` refusal), and the secret-first ordering means the config
/// file is untouched — deterministic exit 1, no keyring, no writes.
#[tokio::test]
async fn logout_routes_an_unsupportable_credential_store_to_exit_one() {
    let (_guard, dir) = failing_setup();
    let mut out = Vec::new();
    let mut err = Vec::new();
    let code = dispatch(Command::Logout, ctx(dir.path(), &mut out, &mut err)).await;
    assert_eq!(code, 1);
    assert!(
        dir.path().join("taskboard.toml").is_file(),
        "the config file survives the refused delete"
    );
}

/// `config print` routing: an undecodable config fails the decode-only
/// gate (the one exit-1 case of that arm).
#[tokio::test]
async fn config_print_routes_an_undecodable_config_to_exit_one() {
    let (_guard, dir) = garbage_setup();
    let mut out = Vec::new();
    let mut err = Vec::new();
    let code = dispatch(
        Command::Config {
            cmd: ConfigCmd::Print,
        },
        ctx(dir.path(), &mut out, &mut err),
    )
    .await;
    assert_eq!(code, 1);
}

/// D-new: `config path` never decodes — the same garbage file that
/// fails `config print` with exit 1 gets its paths named with exit 0
/// (the decode-vs-no-decode contrast between the two verbs).
#[tokio::test]
async fn config_path_is_infallible_even_for_a_garbage_config() {
    let (_guard, dir) = garbage_setup();
    let mut out = Vec::new();
    let mut err = Vec::new();
    let code = dispatch(
        Command::Config {
            cmd: ConfigCmd::Path,
        },
        ctx(dir.path(), &mut out, &mut err),
    )
    .await;
    assert_eq!(code, 0, "stderr: {}", String::from_utf8_lossy(&err));
    let view: serde_json::Value =
        serde_json::from_slice(&out).expect("the payload is one json line");
    let named = dir.path().join("taskboard.toml").display().to_string();
    assert_eq!(view["config_path"], named);
    assert_eq!(view["write_path"], named);
}

/// D-new: `status` routes client-side — never a core boot, exit 0, and
/// (under `--json`) the `daemon: false` answer on stdout. A rejected
/// config is the one typed failure (asserted via the garbage file).
#[tokio::test]
async fn status_routes_client_side_without_booting() {
    let dir = tempdir();
    common::write_test_config(&dir.path().join("taskboard.toml"), None, None)
        .expect("write config");
    let mut out = Vec::new();
    let mut err = Vec::new();
    let code = dispatch(Command::Status, ctx(dir.path(), &mut out, &mut err)).await;
    assert_eq!(code, 0);
    let view: serde_json::Value =
        serde_json::from_slice(&out).expect("status payload is one json line");
    assert_eq!(view["daemon"], serde_json::Value::Bool(false));

    // A rejected config reports typed, exit 1.
    let bad = tempdir();
    std::fs::write(bad.path().join("taskboard.toml"), "= not toml =").expect("garbage");
    let mut out = Vec::new();
    let mut err = Vec::new();
    let code = dispatch(Command::Status, ctx(bad.path(), &mut out, &mut err)).await;
    assert_eq!(code, 1);
    let envelope: serde_json::Value = serde_json::from_slice(&err).expect("envelope");
    assert_eq!(envelope["error"]["kind"], "decode");
}

/// D-success: the one-shot arms' success paths route end to end — a
/// standalone install's core-verb chain through `dispatch()`, each
/// invocation booting its own core and printing the JSON receipt
/// (llvm-cov cannot see the subprocess-driving V-series, so this is the
/// in-process slice for the `run_core` bodies). The connector-gated
/// verbs (boards select/publish, a real sync) keep their wiremock- and
/// binary-level suites.
#[tokio::test]
#[allow(clippy::too_many_lines)] // one verb chain, one row per arm
async fn one_shot_core_verbs_route_success_end_to_end() {
    let dir = tempdir();
    common::write_test_config(&dir.path().join("taskboard.toml"), None, None).expect("config");
    let mut out = Vec::new();
    let mut err = Vec::new();

    macro_rules! run {
        ($command:expr) => {{
            let code = dispatch($command, ctx(dir.path(), &mut out, &mut err)).await;
            assert_eq!(code, 0, "stderr: {}", String::from_utf8_lossy(&err));
        }};
    }

    run!(Command::Boards {
        cmd: BoardsCmd::Create {
            title: "Personal".to_owned(),
            color: None,
        },
    });
    run!(Command::Boards {
        cmd: BoardsCmd::Rename {
            board: None,
            title: "Work".to_owned(),
        },
    });
    run!(Command::Stacks {
        cmd: StacksCmd::Add {
            board: None,
            title: "Todo".to_owned(),
            order: None,
        },
    });
    run!(Command::Stacks {
        cmd: StacksCmd::Add {
            board: None,
            title: "Scratch".to_owned(),
            order: None,
        },
    });
    run!(Command::Stacks {
        cmd: StacksCmd::Rename {
            stack: "Scratch".to_owned(),
            title: "Someday".to_owned(),
        },
    });
    run!(Command::Stacks {
        cmd: StacksCmd::Move {
            stack: "Someday".to_owned(),
            order: 3,
        },
    });
    run!(Command::Stacks {
        cmd: StacksCmd::Delete {
            stack: "Someday".to_owned(),
        },
    });
    run!(Command::Labels {
        cmd: LabelsCmd::Add {
            board: None,
            title: "urgent".to_owned(),
            color: Some("ff0000".to_owned()),
        },
    });
    run!(Command::Labels {
        cmd: LabelsCmd::List { board: None },
    });
    run!(Command::Labels {
        cmd: LabelsCmd::Edit {
            label: "urgent".to_owned(),
            title: Some("critical".to_owned()),
            color: Some("0000ff".to_owned()),
        },
    });
    run!(Command::Tasks {
        cmd: TasksCmd::Add {
            title: "t1".to_owned(),
            stack: None,
        },
    });
    run!(Command::Tasks {
        cmd: TasksCmd::Add {
            title: "t2".to_owned(),
            stack: None,
        },
    });
    run!(Command::Tasks {
        cmd: TasksCmd::List { stack: None },
    });
    run!(Command::Tasks {
        cmd: TasksCmd::Label {
            task: "t1".to_owned(),
            label: "critical".to_owned(),
        },
    });
    run!(Command::Tasks {
        cmd: TasksCmd::Unlabel {
            task: "t1".to_owned(),
            label: "critical".to_owned(),
        },
    });
    run!(Command::Labels {
        cmd: LabelsCmd::Delete {
            label: "critical".to_owned(),
        },
    });
    run!(Command::Tasks {
        cmd: TasksCmd::Move {
            task: "t2".to_owned(),
            stack: Some("Todo".to_owned()),
            board: None,
            order: Some(0),
        },
    });
    run!(Command::Tasks {
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
    });
    run!(Command::Tasks {
        cmd: TasksCmd::Done {
            task: "t2-renamed".to_owned(),
            undo: false,
        },
    });
    run!(Command::Tasks {
        cmd: TasksCmd::Delete {
            task: "t1".to_owned(),
        },
    });

    // Every arm printed its JSON receipt; stderr stayed clean.
    assert_eq!(err.len(), 0, "stderr: {}", String::from_utf8_lossy(&err));
    let receipts: Vec<serde_json::Value> = String::from_utf8_lossy(&out)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("one JSON payload per line"))
        .collect();
    assert!(receipts.len() >= 20, "{} receipts", receipts.len());
}

/// D-sync: the one-shot sync arm's outcome arms route in-process — a
/// wiremock that answers slower than the budget yields the typed
/// timeout exit, and a fast auth failure yields the failed exit (both
/// envelopes on stderr, exit 1). The success arm rides the binary-level
/// V/J suites (llvm-cov cannot see subprocesses).
#[tokio::test]
async fn sync_routes_the_timeout_and_failure_outcomes() {
    use common::EnvOverride;

    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .respond_with(
            wiremock::ResponseTemplate::new(401).set_delay(std::time::Duration::from_secs(5)),
        )
        .mount(&server)
        .await;

    let dir = tempdir();
    common::write_test_config(
        &dir.path().join("taskboard.toml"),
        Some(&server.uri()),
        Some("alice"),
    )
    .expect("write config");
    let _guard = env_lock();
    let _password = EnvOverride::set("TASKBOARD_APP_PASSWORD", "pw");

    // The cycle's first request outruns the one-second budget: the
    // typed timeout, exit 1. (The board binding makes the cycle pull —
    // without one it skips silently and never touches the server.)
    common::seed_board_binding(
        &dir.path().join("taskboard.db"),
        1,
        common::BOARD,
        "d-sync-run",
    )
    .await;
    let mut out = Vec::new();
    let mut err = Vec::new();
    let code = dispatch(
        Command::Sync {
            no_wait: false,
            timeout_secs: 1,
            connector: None,
        },
        ctx(dir.path(), &mut out, &mut err),
    )
    .await;
    assert_eq!(code, 1);

    // A fast 401: the cycle fails auth within the budget.
    let fast = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .respond_with(wiremock::ResponseTemplate::new(401))
        .mount(&fast)
        .await;
    common::write_test_config(
        &dir.path().join("taskboard.toml"),
        Some(&fast.uri()),
        Some("alice"),
    )
    .expect("rewrite config");
    let mut out = Vec::new();
    let mut err = Vec::new();
    let code = dispatch(
        Command::Sync {
            no_wait: false,
            timeout_secs: 30,
            connector: None,
        },
        ctx(dir.path(), &mut out, &mut err),
    )
    .await;
    assert_eq!(code, 1);
    let envelope: serde_json::Value = serde_json::from_slice(&err).expect("one error envelope");
    assert_eq!(envelope["error"]["kind"], "sync-failed");
}

/// Piped stdin (nextest) feeds an empty app password; the verification
/// call then hits the refused server and connect reports exit 1.
#[tokio::test]
async fn connect_with_unverifiable_credentials_routes_to_exit_one() {
    let (_guard, dir) = failing_setup();
    let mut out = Vec::new();
    let mut err = Vec::new();
    let code = dispatch(
        Command::Connect {
            server: Some("http://127.0.0.1:1".to_owned()),
            user: Some("alice".to_owned()),
            switch_backend: false,
            force: false,
        },
        ctx(dir.path(), &mut out, &mut err),
    )
    .await;
    assert_eq!(code, 1);
}

/// An explicit `standalone` backend without `--switch-backend` refuses
/// typed at the ceremony's D2 guard — exit 1 through `dispatch`.
#[tokio::test]
async fn connect_under_explicit_standalone_routes_to_exit_one() {
    let dir = tempdir();
    std::fs::write(
        dir.path().join("taskboard.toml"),
        "[sync]\nbackend = 'standalone'\n",
    )
    .expect("write toml");
    let mut out = Vec::new();
    let mut err = Vec::new();
    let code = dispatch(
        Command::Connect {
            server: Some("http://127.0.0.1:1".to_owned()),
            user: Some("alice".to_owned()),
            switch_backend: false,
            force: false,
        },
        ctx(dir.path(), &mut out, &mut err),
    )
    .await;
    assert_eq!(code, 1);
}

/// A configured different identity without `--force` refuses typed at
/// the ceremony's D3 guard — exit 1 through `dispatch`.
#[tokio::test]
async fn connect_over_a_configured_identity_without_force_routes_to_exit_one() {
    let dir = tempdir();
    std::fs::write(
        dir.path().join("taskboard.toml"),
        "[nextcloud]\nserver_url = 'https://old.example.com'\nusername = 'alice'\n",
    )
    .expect("write toml");
    let mut out = Vec::new();
    let mut err = Vec::new();
    let code = dispatch(
        Command::Connect {
            server: Some("http://127.0.0.1:1".to_owned()),
            user: Some("alice".to_owned()),
            switch_backend: false,
            force: false,
        },
        ctx(dir.path(), &mut out, &mut err),
    )
    .await;
    assert_eq!(code, 1);
}

/// The one-shot `boards use` refuses typed `session-only` (D6-6): the
/// selection is session state; the one-shot dispatch has no home for it.
#[tokio::test]
async fn boards_use_one_shot_refuses_session_only() {
    let dir = tempdir();
    let mut out = Vec::new();
    let mut err = Vec::new();
    let code = dispatch(
        Command::Boards {
            cmd: BoardsCmd::Use { board: None },
        },
        ctx(dir.path(), &mut out, &mut err),
    )
    .await;
    assert_eq!(code, 1);
    let envelope: serde_json::Value =
        serde_json::from_slice(&err).expect("the error envelope is JSON");
    assert_eq!(envelope["error"]["kind"], "session-only");
}
