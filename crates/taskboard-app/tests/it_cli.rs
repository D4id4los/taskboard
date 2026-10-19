// SPDX-License-Identifier: AGPL-3.0-only
//! Tier 2 J-series: app-level E2E journeys that drive the **real
//! binary** against the dockerized Nextcloud (`docs/testing_strategy.org`
//! §8, plan §3.11). These prove only what only the binary can prove:
//! arg parsing, stdin prompting, config discovery, env resolution,
//! process exit codes, and the multi-process login → select → add →
//! sync choreography. Handler logic is covered hermetically (L/V/K/O).
//!
//! Secretless by construction — the setup script mints the app password
//! into `TASKBOARD_IT_DOCKER_TOKEN`; skip, never fail, without the tier
//! env. Children get their env via `binary_command()` (all parent
//! `TASKBOARD_*` stripped) plus explicit `--config` and password.
//!
//! Run locally:
//!
//! ```sh
//! eval "$(scripts/nextcloud_it_setup.sh up)" &&
//!   cargo nextest run -p taskboard-app --run-ignored only \
//!     -E 'test(it_nextcloud_docker)'
//! ```

use std::io::Write as _;
use std::path::Path;
use std::process::{Output, Stdio};
use std::time::Duration;

use taskboard_sync_nextcloud::{DeckClient, NewCard, StackFilter};

mod common;

use common::{LiveCfg, binary_command, tempdir, write_test_config};

/// Runs the real binary once and waits for it.
fn run_cli(config: &Path, args: &[&str], stdin: Option<&str>, password: Option<&str>) -> Output {
    let mut cmd = binary_command();
    cmd.arg("--config").arg(config).args(args);
    if let Some(password) = password {
        cmd.env("TASKBOARD_APP_PASSWORD", password);
    }
    cmd.stdin(if stdin.is_some() {
        Stdio::piped()
    } else {
        Stdio::null()
    })
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());
    let mut child = cmd.spawn().expect("spawn taskboard binary");
    if let Some(input) = stdin {
        child
            .stdin
            .take()
            .expect("stdin piped")
            .write_all(input.as_bytes())
            .expect("write stdin");
    }
    child.wait_with_output().expect("binary runs to completion")
}

/// The last non-empty stderr line: under `--json` that is the typed
/// error envelope (logs precede it).
fn last_stderr_line(output: &Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr);
    stderr
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .to_owned()
}

fn stdout_json(output: &Output) -> serde_json::Value {
    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(stdout.trim())
        .unwrap_or_else(|err| panic!("stdout is one JSON payload ({err}): {stdout}"))
}

fn expect_exit(output: &Output, code: i32, what: &str) {
    assert_eq!(
        output.status.code(),
        Some(code),
        "{what}: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
}

async fn teardown_board(client: &DeckClient, board: u64) {
    let _ = common::retry(|| async { client.delete_board(board).await }).await;
}

#[tokio::test]
#[ignore = "requires the dockerized Nextcloud tier (scripts/nextcloud_it_setup.sh up)"]
async fn it_nextcloud_docker_cli_login_select_add_verify() {
    let Some(cfg) = common::live_docker_config() else {
        return; // skip, never fail: tier not configured
    };
    common::with_deadline(j1_login_select_add_verify(&cfg))
        .await
        .expect("journey must finish within the live deadline");
}

/// J1 — the M1 journey in miniature, all through the real binary:
/// login (stdin-piped password) → boards list/select → tasks add →
/// sync → the card exists on the server; then done → sync → the done
/// state survives the round-trip.
#[allow(clippy::too_many_lines)] // the journey reads as one narrative
async fn j1_login_select_add_verify(cfg: &LiveCfg) {
    let client = DeckClient::new(&cfg.url, &cfg.user, &cfg.token).expect("base URL");
    let run = common::run_id();
    let dir = tempdir();
    let config = dir.path().join("taskboard.toml");
    write_test_config(&config, None, None).expect("write config");
    let (board, _stack) = common::create_board_and_stack(&client, &run).await;

    // login: password piped on stdin, flags for server/user.
    let output = run_cli(
        &config,
        ["login", "--server", &cfg.url, "--user", &cfg.user].as_slice(),
        Some(&cfg.token),
        Some(&cfg.token),
    );
    expect_exit(&output, 0, "login");
    assert!(config.is_file(), "login wrote the config");

    // boards list: the created board is discoverable (remote id).
    let output = run_cli(
        &config,
        ["boards", "list", "--json"].as_slice(),
        None,
        Some(&cfg.token),
    );
    expect_exit(&output, 0, "boards list");
    let listing = stdout_json(&output);
    let entry = listing
        .as_array()
        .expect("boards list is an array")
        .iter()
        .find(|b| b["id"] == board.id)
        .unwrap_or_else(|| panic!("board {} in listing: {listing}", board.id));
    assert_eq!(entry["title"], run);

    // boards select: the one-time binding ceremony.
    let board_id = board.id.to_string();
    let output = run_cli(
        &config,
        ["boards", "select", &board_id, "--json"].as_slice(),
        None,
        Some(&cfg.token),
    );
    expect_exit(&output, 0, "boards select");
    let receipt = stdout_json(&output);
    assert_eq!(receipt["board"]["id"], board.id);

    // tasks add: receipt chains the local id for scripting.
    let title = format!("{run}-task");
    let stack_title = format!("{run}-col");
    let output = run_cli(
        &config,
        ["tasks", "add", &title, "--stack", &stack_title, "--json"].as_slice(),
        None,
        Some(&cfg.token),
    );
    expect_exit(&output, 0, "tasks add");
    let receipt = stdout_json(&output);
    assert_eq!(receipt["title"], title);
    assert_eq!(receipt["stack_title"], stack_title);
    assert_eq!(receipt["done"], false);
    let task_id = receipt["id"]
        .as_str()
        .expect("receipt carries the id")
        .to_owned();

    // sync: push proof via the server's truth.
    let output = run_cli(
        &config,
        ["sync", "--json"].as_slice(),
        None,
        Some(&cfg.token),
    );
    expect_exit(&output, 0, "sync");
    let status = stdout_json(&output);
    assert_eq!(status["pending_ops"], 0, "outbox drained: {status}");
    let stacks = common::retry(|| async { client.stacks(board.id, StackFilter::Active).await })
        .await
        .expect("server listing");
    assert!(
        stacks
            .iter()
            .flat_map(|s| &s.cards)
            .any(|c| c.title == title),
        "the pushed card must exist on the server"
    );

    // tasks list: local view + healthy sync footer.
    let output = run_cli(
        &config,
        ["tasks", "list", "--json"].as_slice(),
        None,
        Some(&cfg.token),
    );
    expect_exit(&output, 0, "tasks list");
    let list = stdout_json(&output);
    let row = list["stacks"]
        .as_array()
        .expect("stack groups")
        .iter()
        .flat_map(|s| s["tasks"].as_array().expect("tasks").iter())
        .find(|t| t["id"] == task_id.as_str())
        .unwrap_or_else(|| panic!("task listed: {list}"));
    assert_eq!(row["done"], false);

    // done → sync → the completion survives the round-trip.
    let output = run_cli(
        &config,
        ["tasks", "done", &task_id, "--json"].as_slice(),
        None,
        Some(&cfg.token),
    );
    expect_exit(&output, 0, "tasks done");
    let output = run_cli(
        &config,
        ["sync", "--json"].as_slice(),
        None,
        Some(&cfg.token),
    );
    expect_exit(&output, 0, "sync after done");
    let status = stdout_json(&output);
    assert_eq!(status["pending_ops"], 0, "the done op was pushed: {status}");
    // Poll: Deck's listing cache lags its writes (tier-2 known-server
    // matrix), so one read is not proof of absence.
    let poll_deadline = std::time::Instant::now() + Duration::from_secs(60);
    loop {
        let stacks = common::retry(|| async { client.stacks(board.id, StackFilter::Active).await })
            .await
            .expect("server listing");
        let done_on_server = stacks
            .iter()
            .flat_map(|s| &s.cards)
            .find(|c| c.title == title)
            .expect("card on the server")
            .done
            .is_some();
        if done_on_server {
            break;
        }
        assert!(
            std::time::Instant::now() < poll_deadline,
            "the completion did not reach the server in time"
        );
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    let output = run_cli(
        &config,
        ["tasks", "list", "--json"].as_slice(),
        None,
        Some(&cfg.token),
    );
    expect_exit(&output, 0, "final tasks list");
    let list = stdout_json(&output);
    let row = list["stacks"]
        .as_array()
        .expect("stack groups")
        .iter()
        .flat_map(|s| s["tasks"].as_array().expect("tasks").iter())
        .find(|t| t["id"] == task_id.as_str())
        .expect("task still listed");
    assert_eq!(row["done"], true, "done state after the round-trip");

    teardown_board(&client, board.id).await;
}

#[tokio::test]
#[ignore = "requires the dockerized Nextcloud tier (scripts/nextcloud_it_setup.sh up)"]
async fn it_nextcloud_docker_cli_remote_create_pull_list() {
    let Some(cfg) = common::live_docker_config() else {
        return;
    };
    common::with_deadline(j2_remote_create_pull_list(&cfg))
        .await
        .expect("journey must finish within the live deadline");
}

/// J2 — the pull direction: a card created in the web-UI equivalent
/// (raw client) appears in `tasks list` after `sync`; `boards list` is
/// unaffected.
async fn j2_remote_create_pull_list(cfg: &LiveCfg) {
    let client = DeckClient::new(&cfg.url, &cfg.user, &cfg.token).expect("base URL");
    let run = common::run_id();
    let dir = tempdir();
    let config = dir.path().join("taskboard.toml");
    write_test_config(&config, Some(&cfg.url), Some(&cfg.user)).expect("write config");
    let (board, stack) = common::create_board_and_stack(&client, &run).await;

    let board_id = board.id.to_string();
    let output = run_cli(
        &config,
        ["boards", "select", &board_id, "--json"].as_slice(),
        None,
        Some(&cfg.token),
    );
    expect_exit(&output, 0, "boards select");

    let title = format!("{run}-remote");
    let card = common::retry(|| async {
        client
            .create_card(
                board.id,
                stack.id,
                &NewCard {
                    title: title.clone(),
                    order: Some(0),
                    description: None,
                    duedate: None,
                    kind: None,
                },
            )
            .await
    })
    .await
    .expect("server-side card creation must succeed");
    let _ = card;

    let output = run_cli(
        &config,
        ["sync", "--json"].as_slice(),
        None,
        Some(&cfg.token),
    );
    expect_exit(&output, 0, "sync pulls the remote card");

    let output = run_cli(
        &config,
        ["tasks", "list", "--json"].as_slice(),
        None,
        Some(&cfg.token),
    );
    expect_exit(&output, 0, "tasks list");
    let list = stdout_json(&output);
    let row = list["stacks"]
        .as_array()
        .expect("stack groups")
        .iter()
        .flat_map(|s| s["tasks"].as_array().expect("tasks").iter())
        .find(|t| t["title"] == title)
        .unwrap_or_else(|| panic!("the pulled card is listed: {list}"));
    assert_eq!(row["stack_title"], format!("{run}-col"));
    assert_eq!(row["done"], false);

    let output = run_cli(
        &config,
        ["boards", "list", "--json"].as_slice(),
        None,
        Some(&cfg.token),
    );
    expect_exit(&output, 0, "boards list unaffected");
    let listing = stdout_json(&output);
    assert!(
        listing
            .as_array()
            .expect("boards list is an array")
            .iter()
            .any(|b| b["id"] == board.id)
    );

    teardown_board(&client, board.id).await;
}

#[tokio::test]
#[ignore = "requires the dockerized Nextcloud tier (scripts/nextcloud_it_setup.sh up)"]
async fn it_nextcloud_docker_cli_login_rejects_bad_token() {
    let Some(cfg) = common::live_docker_config() else {
        return;
    };
    common::with_deadline(async { j3_login_rejects_bad_token(&cfg) })
        .await
        .expect("journey must finish within the live deadline");
}

/// J3 — bad credentials: exit 1, typed stderr envelope, and *nothing*
/// in the tempdir (no config, no lock sidecar — nothing booted).
fn j3_login_rejects_bad_token(cfg: &LiveCfg) {
    let dir = tempdir();
    let config = dir.path().join("taskboard.toml");
    write_test_config(&config, None, None).expect("write config");
    std::fs::remove_file(&config).expect("login must create it itself");

    let output = run_cli(
        &config,
        ["login", "--server", &cfg.url, "--user", &cfg.user, "--json"].as_slice(),
        Some("definitely-not-the-password"),
        None,
    );
    expect_exit(&output, 1, "login with a garbage token");
    let err: serde_json::Value =
        serde_json::from_str(&last_stderr_line(&output)).expect("stderr error envelope");
    assert_eq!(err["error"]["kind"], "invalid-credentials");
    assert!(!config.exists(), "no config written on rejection");
    assert!(
        !dir.path().join("taskboard.lock").exists(),
        "no instance lock sidecar: nothing booted"
    );
}

#[tokio::test]
#[ignore = "requires the dockerized Nextcloud tier (scripts/nextcloud_it_setup.sh up)"]
async fn it_nextcloud_docker_cli_connect_journey() {
    let Some(cfg) = common::live_docker_config() else {
        return; // skip, never fail: tier not configured
    };
    j4_connect_pipe_journey(&cfg);
}

/// J4 — the connect journeys through the real binary with the password
/// piped on stdin and the env store (mirrors J1's login leg, connect
/// chunk plan §7.5): fresh install → connect → re-point with `--force`
/// → the standalone flip with `--switch-backend`.
#[allow(clippy::too_many_lines)] // the journey reads as one narrative
fn j4_connect_pipe_journey(cfg: &LiveCfg) {
    let dir = tempdir();

    // Leg 1 — fresh config: connect with the token piped.
    let config = dir.path().join("taskboard.toml");
    write_test_config(&config, None, None).expect("write config");
    let output = run_cli(
        &config,
        [
            "connect", "--server", &cfg.url, "--user", &cfg.user, "--json",
        ]
        .as_slice(),
        Some(&cfg.token),
        Some(&cfg.token),
    );
    expect_exit(&output, 0, "connect (fresh install)");
    let receipt = stdout_json(&output);
    assert_eq!(receipt["verified"], true);
    assert_eq!(receipt["backend_switched"], false);
    assert_eq!(receipt["repointed"], false);
    let written = std::fs::read_to_string(&config).expect("config written");
    assert!(written.contains(&cfg.url), "server persisted");
    assert!(written.contains(&cfg.user), "username persisted");
    assert!(!written.contains(&cfg.token), "no secret in the file");

    // Leg 2 — re-point: the config carries a stale identity (it needs
    // no server truth — only the *new* identity verifies); `--force`
    // replaces it without a confirm.
    let stale_server = "https://stale.example.com";
    write_test_config(&config, Some(stale_server), Some(cfg.user.as_str()))
        .expect("reseed stale identity");
    let output = run_cli(
        &config,
        [
            "connect", "--server", &cfg.url, "--user", &cfg.user, "--force", "--json",
        ]
        .as_slice(),
        Some(&cfg.token),
        Some(&cfg.token),
    );
    expect_exit(&output, 0, "connect --force (re-point)");
    let receipt = stdout_json(&output);
    assert_eq!(receipt["repointed"], true);
    let written = std::fs::read_to_string(&config).expect("config written");
    assert!(
        written.contains(&cfg.url) && !written.contains(stale_server),
        "config names the new identity"
    );

    // Leg 3 — the standalone flip: fresh config seeded with an explicit
    // standalone backend; after the flip, the install syncs.
    let flip_config = dir.path().join("flip.toml");
    write_test_config(&flip_config, None, None).expect("write config");
    let seeded = std::fs::read_to_string(&flip_config).expect("read");
    std::fs::write(
        &flip_config,
        seeded.replacen("[sync]", "[sync]\nbackend = 'standalone'", 1),
    )
    .expect("seed standalone backend");
    let output = run_cli(
        &flip_config,
        [
            "connect",
            "--server",
            &cfg.url,
            "--user",
            &cfg.user,
            "--switch-backend",
            "--json",
        ]
        .as_slice(),
        Some(&cfg.token),
        Some(&cfg.token),
    );
    expect_exit(&output, 0, "connect --switch-backend (flip)");
    let receipt = stdout_json(&output);
    assert_eq!(receipt["backend_switched"], true);
    let written = std::fs::read_to_string(&flip_config).expect("config written");
    let doc = written.parse::<toml_edit::DocumentMut>().expect("parses");
    assert_eq!(
        doc["sync"]["backend"].as_str(),
        Some("nextcloud"),
        "the flipped word persisted: {written}"
    );
    let output = run_cli(
        &flip_config,
        ["boards", "list", "--json"].as_slice(),
        None,
        Some(&cfg.token),
    );
    expect_exit(&output, 0, "boards list after the flip");
}

/// J5 — the `gui` lifecycle verb (ADR 0009 amendment): the binary
/// recognises it as a first-class subcommand and reaches the config
/// load **without a TTY** (plain pipes — no pty, no display, no
/// docker): an undecodable config fails typed `decode`, exit 1, the
/// same envelope contract the one-shot verbs use under `--json`. The
/// success path is a window (a foreign, display-gated contract) and is
/// deliberately not pinned here.
#[test]
fn j5_gui_verb_reaches_config_load_without_a_tty() {
    let dir = tempdir();
    let config = dir.path().join("taskboard.toml");
    std::fs::write(&config, "= not toml =").expect("write garbage config");

    let output = run_cli(&config, ["--json", "gui"].as_slice(), None, None);
    expect_exit(&output, 1, "gui with an undecodable config exits 1");
    let envelope: serde_json::Value = serde_json::from_str(&last_stderr_line(&output))
        .expect("stderr ends with one JSON error object");
    assert_eq!(envelope["error"]["kind"], "decode");
}
