// SPDX-License-Identifier: AGPL-3.0-only
//! D-series: hermetic dispatch-routing suite (M1-exit report §2, gap 2).
//! llvm-cov does not instrument the subprocess-driving J/K suites, so
//! `dispatch()`'s match arms were invisible to the coverage report. One
//! test per `Command` variant proves the arm routes to a typed failure
//! outcome (exit 1) — routing only; the verbs' success paths are the
//! L/V/K-series' job, not duplicated here.

mod common;

use taskboard_app::cli::{BoardsCmd, Command, StacksCmd, TasksCmd};
use taskboard_app::commands::{CommandCtx, dispatch};

use common::{env_lock, remove_env, tempdir};

/// The dispatcher's per-invocation context over a per-test config file.
fn ctx(dir: &std::path::Path) -> CommandCtx {
    CommandCtx {
        json: true,
        config_path: Some(dir.join("taskboard.toml")),
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
    let code = dispatch(
        Command::Login {
            server: None,
            user: None,
        },
        ctx(dir.path()),
    )
    .await;
    assert_eq!(code, 1);
}

/// Piped stdin (nextest) feeds an empty app password; the verification
/// call then hits the refused server and login reports exit 1.
#[tokio::test]
async fn login_with_unverifiable_credentials_routes_to_exit_one() {
    let (_guard, dir) = failing_setup();
    let code = dispatch(
        Command::Login {
            server: Some("http://127.0.0.1:1".to_owned()),
            user: Some("alice".to_owned()),
        },
        ctx(dir.path()),
    )
    .await;
    assert_eq!(code, 1);
}

#[tokio::test]
async fn boards_list_routes_a_failed_credential_load_to_exit_one() {
    let (_guard, dir) = failing_setup();
    let code = dispatch(
        Command::Boards {
            cmd: BoardsCmd::List,
        },
        ctx(dir.path()),
    )
    .await;
    assert_eq!(code, 1);
}

#[tokio::test]
async fn boards_select_routes_a_failed_credential_load_to_exit_one() {
    let (_guard, dir) = failing_setup();
    let code = dispatch(
        Command::Boards {
            cmd: BoardsCmd::Select {
                board: 3,
                timeout_secs: 1,
                bind_local_board: false,
            },
        },
        ctx(dir.path()),
    )
    .await;
    assert_eq!(code, 1);
}

#[tokio::test]
async fn tasks_list_routes_a_failed_credential_load_to_exit_one() {
    let (_guard, dir) = failing_setup();
    let code = dispatch(
        Command::Tasks {
            cmd: TasksCmd::List { stack: None },
        },
        ctx(dir.path()),
    )
    .await;
    assert_eq!(code, 1);
}

#[tokio::test]
async fn tasks_add_routes_a_failed_credential_load_to_exit_one() {
    let (_guard, dir) = failing_setup();
    let code = dispatch(
        Command::Tasks {
            cmd: TasksCmd::Add {
                title: "x".to_owned(),
                stack: None,
            },
        },
        ctx(dir.path()),
    )
    .await;
    assert_eq!(code, 1);
}

#[tokio::test]
async fn tasks_done_routes_a_failed_credential_load_to_exit_one() {
    let (_guard, dir) = failing_setup();
    let code = dispatch(
        Command::Tasks {
            cmd: TasksCmd::Done {
                task: "some-task".to_owned(),
                undo: false,
            },
        },
        ctx(dir.path()),
    )
    .await;
    assert_eq!(code, 1);
}

#[tokio::test]
async fn tasks_move_routes_a_failed_credential_load_to_exit_one() {
    let (_guard, dir) = failing_setup();
    let code = dispatch(
        Command::Tasks {
            cmd: TasksCmd::Move {
                task: "some-task".to_owned(),
                stack: None,
                order: None,
            },
        },
        ctx(dir.path()),
    )
    .await;
    assert_eq!(code, 1);
}

#[tokio::test]
async fn sync_routes_a_failed_credential_load_to_exit_one() {
    let (_guard, dir) = failing_setup();
    let code = dispatch(
        Command::Sync {
            no_wait: false,
            timeout_secs: 1,
        },
        ctx(dir.path()),
    )
    .await;
    assert_eq!(code, 1);
}

/// Every core-booting arm also rejects an undecodable config before any
/// boot work (the `core_ctx` Err branch each arm shares).
/// WI-3 editing verbs: one routing test per new `Command` variant, in
/// the `failing_setup` shape (exit 1 at the credential load).
#[tokio::test]
async fn tasks_edit_routes_a_failed_credential_load_to_exit_one() {
    let (_guard, dir) = failing_setup();
    let code = dispatch(
        Command::Tasks {
            cmd: TasksCmd::Edit {
                task: "x".to_owned(),
                title: Some("t".to_owned()),
                description: None,
                due: None,
                archive: false,
                unarchive: false,
            },
        },
        ctx(dir.path()),
    )
    .await;
    assert_eq!(code, 1);
}

#[tokio::test]
async fn tasks_delete_routes_a_failed_credential_load_to_exit_one() {
    let (_guard, dir) = failing_setup();
    let code = dispatch(
        Command::Tasks {
            cmd: TasksCmd::Delete {
                task: "x".to_owned(),
            },
        },
        ctx(dir.path()),
    )
    .await;
    assert_eq!(code, 1);
}

#[tokio::test]
async fn boards_rename_routes_a_failed_credential_load_to_exit_one() {
    let (_guard, dir) = failing_setup();
    let code = dispatch(
        Command::Boards {
            cmd: BoardsCmd::Rename {
                title: "t".to_owned(),
            },
        },
        ctx(dir.path()),
    )
    .await;
    assert_eq!(code, 1);
}

#[tokio::test]
async fn stacks_rename_routes_a_failed_credential_load_to_exit_one() {
    let (_guard, dir) = failing_setup();
    let code = dispatch(
        Command::Stacks {
            cmd: StacksCmd::Rename {
                stack: "s".to_owned(),
                title: "t".to_owned(),
            },
        },
        ctx(dir.path()),
    )
    .await;
    assert_eq!(code, 1);
}

#[tokio::test]
async fn stacks_delete_routes_a_failed_credential_load_to_exit_one() {
    let (_guard, dir) = failing_setup();
    let code = dispatch(
        Command::Stacks {
            cmd: StacksCmd::Delete {
                stack: "s".to_owned(),
            },
        },
        ctx(dir.path()),
    )
    .await;
    assert_eq!(code, 1);
}

#[tokio::test]
async fn stacks_move_routes_a_failed_credential_load_to_exit_one() {
    let (_guard, dir) = failing_setup();
    let code = dispatch(
        Command::Stacks {
            cmd: StacksCmd::Move {
                stack: "s".to_owned(),
                order: 1,
            },
        },
        ctx(dir.path()),
    )
    .await;
    assert_eq!(code, 1);
}

#[tokio::test]
async fn a_core_command_with_an_undecodable_config_routes_to_exit_one() {
    let (_guard, dir) = garbage_setup();
    let code = dispatch(
        Command::Tasks {
            cmd: TasksCmd::List { stack: None },
        },
        ctx(dir.path()),
    )
    .await;
    assert_eq!(code, 1);
}
