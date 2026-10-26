// SPDX-License-Identifier: AGPL-3.0-only
//! Binary E2E for the daemon IPC frontend (WI-5): the *real* binary on
//! both sides — a real daemon process holding the instance lock, real
//! client processes routing over the socket. Process-level facts only:
//! exit codes across process boundaries, byte-identical stdout, the
//! socket artifact's lifecycle, signals. Verb semantics stay in the
//! handler suites and the IP-series.

use std::path::Path;
use std::time::{Duration, Instant};

mod common;

use common::{Child, binary_command, spawn_daemon, tempdir, wait_until, write_test_config};

use interprocess::local_socket::traits::tokio::Stream as _;

fn read_to_string(bytes: Vec<u8>) -> String {
    String::from_utf8(bytes).expect("utf8")
}

/// A standalone-install config (no server url → auto `standalone`) and
/// its db/socket paths.
struct Install {
    dir: tempfile::TempDir,
}

impl Install {
    fn new(name: &str) -> Self {
        let dir = tempdir();
        let config = dir.path().join(format!("{name}.toml"));
        write_test_config(&config, None, None).expect("write config");
        std::fs::rename(&config, dir.path().join("taskboard.toml")).expect("rename");
        Self { dir }
    }

    fn config(&self) -> String {
        self.dir
            .path()
            .join("taskboard.toml")
            .to_string_lossy()
            .into_owned()
    }

    fn socket(&self) -> std::path::PathBuf {
        self.dir.path().join("taskboard.sock")
    }

    /// Spawns `taskboard daemon` (output swallowed; a full pipe would
    /// deadlock a chatty daemon).
    fn spawn_daemon(&self) -> Child {
        self.spawn_daemon_inner(None)
    }

    /// Spawns the daemon with `TASKBOARD_APP_PASSWORD` set on the child
    /// (the env-store caveat's knob: the daemon's own environment
    /// governs its store loads — children snapshot the env at spawn).
    fn spawn_daemon_with_password(&self, password: &str) -> Child {
        self.spawn_daemon_inner(Some(password))
    }

    fn spawn_daemon_inner(&self, password: Option<&str>) -> Child {
        spawn_daemon(Path::new(&self.config()), password)
    }

    /// Runs a one-shot; returns (exit code, stdout, stderr).
    fn run(&self, args: &[&str]) -> (Option<i32>, String, String) {
        let output = binary_command()
            .args(["--config", self.config().as_str()])
            .args(args)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output()
            .expect("run one-shot");
        (
            output.status.code(),
            read_to_string(output.stdout),
            read_to_string(output.stderr),
        )
    }

    /// Waits until a daemon serves this install (`status --json` says
    /// `daemon: true`) and returns the daemon's reported pid.
    async fn wait_ready(&self) -> u32 {
        wait_until(
            || {
                self.run(&["--json", "status"])
                    .1
                    .contains("\"daemon\":true")
            },
            "daemon serves status",
        )
        .await;
        let (_, out, _) = self.run(&["--json", "status"]);
        let view: serde_json::Value =
            serde_json::from_str(out.trim()).expect("status json payload");
        u32::try_from(view["pid"].as_u64().expect("daemon pid")).expect("pid fits u32")
    }
}

/// The headline journey: daemon up → `status` live (pid/backend/phase)
/// → core verbs served **while the daemon holds the lock** (today's
/// `instance-locked` friction is gone) → `daemon stop` (daemon exits 0,
/// socket gone) → `status` flips to `daemon: false` → the same command
/// falls back in-process with **byte-identical** stdout, and works.
#[tokio::test]
#[allow(clippy::too_many_lines)] // one journey, sequential rows
async fn e1_daemon_serves_verbs_then_stop_then_fallback() {
    let install = Install::new("e1");
    let mut daemon = install.spawn_daemon();
    let daemon_pid = install.wait_ready().await;

    // Status: live daemon view; the reported pid is the child's.
    let (code, out, _) = install.run(&["--json", "status"]);
    assert_eq!(code, Some(0));
    let view: serde_json::Value = serde_json::from_str(out.trim()).expect("status json");
    assert_eq!(view["daemon"], serde_json::Value::Bool(true));
    assert_eq!(view["backend"], "standalone");
    assert_eq!(view["pid"], daemon_pid, "the daemon reports its own pid");
    assert_eq!(view["sync"]["phase"], "idle");

    // Core verbs while the daemon holds the lock: served, exit 0 — the
    // headline (this was `instance-locked`, exit 1, before WI-5).
    for command in [
        &["boards", "create", "Personal"][..],
        &["stacks", "add", "Todo"][..],
    ] {
        let (code, out, err) = install.run(command);
        assert_eq!(code, Some(0), "{command:?} over ipc: {out} {err}");
    }
    let (code, out, err) = install.run(&["tasks", "add", "--stack", "Todo", "first"]);
    assert_eq!(code, Some(0), "tasks add over ipc: {out} {err}");
    let (code, out, _) = install.run(&["--json", "tasks", "list"]);
    assert_eq!(code, Some(0));
    let listing: serde_json::Value = serde_json::from_str(out.trim()).expect("list json");
    assert_eq!(listing["stacks"][0]["tasks"][0]["title"], "first");

    // The silence contract: the same command's stdout bytes are
    // identical over IPC and in-process (auto-routing is invisible).
    let (_, served, _) = install.run(&["--json", "tasks", "list"]);

    // `daemon stop`: exit 0, the daemon child exits 0, socket removed.
    let (code, _, _) = install.run(&["daemon", "stop"]);
    assert_eq!(code, Some(0));
    let stop_deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if daemon.0.try_wait().expect("poll daemon").is_some() {
            break;
        }
        assert!(
            Instant::now() < stop_deadline,
            "daemon did not exit after stop"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(
        !install.socket().exists(),
        "graceful stop removes the socket artifact"
    );
    drop(daemon);

    // Status without a daemon: exit 0, daemon false.
    let (code, out, _) = install.run(&["--json", "status"]);
    assert_eq!(code, Some(0));
    let view: serde_json::Value = serde_json::from_str(out.trim()).expect("status json");
    assert_eq!(view["daemon"], serde_json::Value::Bool(false));

    // In-process fallback: byte-identical stdout, still works.
    let (code, fallback, _) = install.run(&["--json", "tasks", "list"]);
    assert_eq!(code, Some(0));
    assert_eq!(
        fallback, served,
        "fallback stdout must be byte-identical to the ipc answer"
    );
    let (code, out, err) = install.run(&["tasks", "add", "--stack", "Todo", "second"]);
    assert_eq!(code, Some(0), "fallback boot works: {out} {err}");
    let _ = out;
}

/// A no-daemon-from-the-start install falls back in-process everywhere.
#[tokio::test]
async fn e2_no_daemon_falls_back_from_the_start() {
    let install = Install::new("e2");
    let (code, _, _) = install.run(&["--json", "status"]);
    assert_eq!(code, Some(0));
    let (_, out, _) = install.run(&["--json", "status"]);
    let view: serde_json::Value = serde_json::from_str(out.trim()).expect("status json");
    assert_eq!(view["daemon"], serde_json::Value::Bool(false));
    let (code, _, err) = install.run(&["tasks", "list"]);
    assert_eq!(code, Some(0), "{err}");
}

/// `daemon stop` without a daemon: typed `no-daemon`, exit 1.
#[tokio::test]
async fn e3_stop_without_a_daemon_is_typed() {
    let install = Install::new("e3");
    let (code, out, err) = install.run(&["--json", "daemon", "stop"]);
    assert_eq!(code, Some(1), "{out} {err}");
    // The error envelope is the last line on stderr (logs precede it).
    let last = err
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("");
    let envelope: serde_json::Value = serde_json::from_str(last).expect("error envelope");
    assert_eq!(envelope["error"]["kind"], "no-daemon");
}

/// After a `kill -9` (stale socket artifact left behind) the next
/// daemon recovers and serves; the graceful `daemon stop` still works.
#[cfg(unix)]
#[tokio::test]
async fn e4_stale_socket_recovered_after_kill_nine() {
    let install = Install::new("e4");
    let mut daemon = install.spawn_daemon();
    install.wait_ready().await;
    // SIGKILL: no drain, no socket removal — the artifact goes stale.
    let pid = daemon.0.id();
    tokio::task::spawn_blocking(move || {
        std::process::Command::new("kill")
            .args(["-9", &pid.to_string()])
            .status()
            .expect("send SIGKILL")
    })
    .await
    .expect("kill task joins");
    wait_until(
        || daemon.0.try_wait().expect("poll daemon").is_some(),
        "daemon dies",
    )
    .await;
    drop(daemon);
    assert!(
        install.socket().exists(),
        "SIGKILL leaves the socket artifact behind (the stale case)"
    );

    let mut successor = install.spawn_daemon();
    install.wait_ready().await;
    let (code, _, _) = install.run(&["daemon", "stop"]);
    assert_eq!(code, Some(0));
    let stop_deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if successor.0.try_wait().expect("poll successor").is_some() {
            break;
        }
        assert!(Instant::now() < stop_deadline, "successor did not exit");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Concurrent CLI *processes* at binary level (IP8's in-process
/// counterpart): both connect, both are answered, both receipts are in
/// the state afterwards.
#[tokio::test]
async fn e5_concurrent_cli_processes_both_serve() {
    let install = Install::new("e5");
    let daemon = install.spawn_daemon();
    install.wait_ready().await;
    for command in [
        &["boards", "create", "Personal"][..],
        &["stacks", "add", "Todo"][..],
    ] {
        let (code, out, err) = install.run(command);
        assert_eq!(code, Some(0), "{command:?}: {out} {err}");
    }

    let first = binary_command()
        .args([
            "--config",
            &install.config(),
            "tasks",
            "add",
            "--stack",
            "Todo",
            "from-one",
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("spawn first client");
    let second = binary_command()
        .args([
            "--config",
            &install.config(),
            "tasks",
            "add",
            "--stack",
            "Todo",
            "from-two",
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("spawn second client");
    let one = first.wait_with_output().expect("first waits");
    let two = second.wait_with_output().expect("second waits");
    assert_eq!(one.status.code(), Some(0));
    assert_eq!(two.status.code(), Some(0));

    let (code, out, _) = install.run(&["--json", "tasks", "list"]);
    assert_eq!(code, Some(0));
    let listing: serde_json::Value = serde_json::from_str(out.trim()).expect("list json");
    let titles: Vec<&str> = listing["stacks"][0]["tasks"]
        .as_array()
        .expect("tasks")
        .iter()
        .map(|t| t["title"].as_str().expect("title"))
        .collect();
    assert_eq!(titles.len(), 2);
    assert!(titles.contains(&"from-one"));
    assert!(titles.contains(&"from-two"));

    let (code, _, _) = install.run(&["daemon", "stop"]);
    assert_eq!(code, Some(0));
    drop(daemon);
}

/// A daemon booted with a *relative* `--config` honors a client's
/// identical flag over IPC (the WI-5 review regression: identity is by
/// canonicalized path, not by the raw string — a relative boot spelling
/// used to refuse the daemon's own config, exit 2).
#[tokio::test]
async fn e6_a_relative_config_spelling_binds_the_relay() {
    let install = Install::new("e6");
    let dir = install.dir.path().to_path_buf();

    let daemon = Child(
        binary_command()
            .args(["--config", "taskboard.toml", "daemon"])
            .current_dir(&dir)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("spawn daemon (relative --config)"),
    );

    // Ready over the same relative spelling: `status` reaching the
    // daemon proves the client resolved the identical install.
    wait_until(
        || {
            let out = binary_command()
                .args(["--config", "taskboard.toml", "--json", "status"])
                .current_dir(&dir)
                .output()
                .expect("status runs");
            read_to_string(out.stdout).contains("\"daemon\":true")
        },
        "daemon serves status over the relative --config",
    )
    .await;

    // A core verb relayed over the socket under that spelling: the
    // daemon's own config must be stripped, not refused.
    let out = binary_command()
        .args(["--config", "taskboard.toml", "tasks", "list"])
        .current_dir(&dir)
        .output()
        .expect("tasks list runs");
    assert_eq!(out.status.code(), Some(0), "{}", read_to_string(out.stderr));

    // The remote stop under the same spelling.
    let out = binary_command()
        .args(["--config", "taskboard.toml", "daemon", "stop"])
        .current_dir(&dir)
        .output()
        .expect("daemon stop runs");
    assert_eq!(out.status.code(), Some(0), "{}", read_to_string(out.stderr));
    drop(daemon);
}

// ---------------------------------------------------------------------
// WI-H: the detach-connector verb against a real daemon process
// ---------------------------------------------------------------------

/// E7: the daemon side of the logout nudge, at the process tier. A real
/// nextcloud-backend daemon (env store + wiremock) boots and serves; the
/// test speaks one hand-rolled `DetachConnector` frame over the real
/// socket (the full binary `logout` → nudge path needs a deletable
/// store = keyring, and stays pinned at the IP tier); the real
/// `status --json` client process then reports
/// `connector_attached: false` and the real `sync` client process
/// refuses `sync-unavailable`. The full logout journey's tier split
/// matches this file's process-level-facts contract.
#[tokio::test]
#[allow(clippy::too_many_lines)] // one journey, sequential rows
async fn e7_detach_connector_flips_a_real_daemons_status() {
    let server = wiremock::MockServer::start().await;
    let dir = tempdir();
    let install_config = dir.path().join("taskboard.toml");
    common::write_test_config(&install_config, Some(server.uri().as_str()), Some("alice"))
        .expect("write config");

    // The daemon boots its connector from the env password (set per
    // child — `binary_command` strips every TASKBOARD_* inheritance).
    let daemon = Child(
        binary_command()
            .args([
                "--config",
                install_config.to_string_lossy().as_ref(),
                "daemon",
            ])
            .env("TASKBOARD_APP_PASSWORD", "pw")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("spawn daemon"),
    );
    wait_until(
        || {
            let output = binary_command()
                .args([
                    "--config",
                    install_config.to_string_lossy().as_ref(),
                    "--json",
                    "status",
                ])
                .output()
                .expect("status runs");
            read_to_string(output.stdout).contains("\"daemon\":true")
        },
        "daemon serves status",
    )
    .await;
    // Boardless: the boot cycle skips silently; the connector is attached.
    let db = dir.path().join("taskboard.db");

    // One hand-rolled detach frame over the daemon's real socket.
    let request = taskboard_app::ipc::protocol::Request::DetachConnector {
        v: taskboard_app::ipc::protocol::PROTOCOL_VERSION,
        server_url: server.uri(),
        username: "alice".to_owned(),
    };
    let bytes = serde_json::to_vec(&request).expect("serialize request");
    let name = taskboard_app::ipc::socket_name(&db).expect("socket name");
    let mut stream = interprocess::local_socket::tokio::Stream::connect(name)
        .await
        .expect("connect to the daemon socket");
    taskboard_app::ipc::codec::write_frame(
        &mut stream,
        &bytes,
        taskboard_app::ipc::codec::MAX_REQUEST_BYTES,
    )
    .await
    .expect("write frame");
    let answer = taskboard_app::ipc::codec::read_frame(
        &mut stream,
        taskboard_app::ipc::codec::MAX_RESPONSE_BYTES,
    )
    .await
    .expect("read frame");
    let response: taskboard_app::ipc::protocol::Response =
        serde_json::from_slice(&answer).expect("parse response");
    assert_eq!(
        response,
        taskboard_app::ipc::protocol::Response::DetachConnector {
            v: taskboard_app::ipc::protocol::PROTOCOL_VERSION,
            outcome: taskboard_app::ipc::protocol::DetachWireOutcome::Detached,
        }
    );
    drop(stream);

    // The real client process sees the detached connector...
    let output = binary_command()
        .args([
            "--config",
            install_config.to_string_lossy().as_ref(),
            "--json",
            "status",
        ])
        .output()
        .expect("status runs");
    assert_eq!(output.status.code(), Some(0));
    let view: serde_json::Value =
        serde_json::from_str(read_to_string(output.stdout).trim()).expect("status json");
    assert_eq!(
        view["connector_attached"], false,
        "the real daemon reports the detached connector: {view}"
    );
    assert_eq!(view["backend"], "nextcloud");

    // ...and the real sync client process refuses typed.
    let output = binary_command()
        .args([
            "--config",
            install_config.to_string_lossy().as_ref(),
            "--json",
            "sync",
        ])
        .output()
        .expect("sync runs");
    assert_eq!(output.status.code(), Some(1));
    assert!(read_to_string(output.stderr).contains("\"sync-unavailable\""));

    drop(daemon);
}

/// The daemon-attach nudge (ADR 0014 decision 7, the D11 contract
/// flip): a connect against a db a daemon serves succeeds and the
/// daemon attaches from persisted state — receipt
/// `daemon_connector: "attached"`, no warning line on stderr (the old
/// passive heads-up is gone), live `connector_attached: true` under the
/// boot-shaped `standalone` backend, exit 0. The env-store caveat is
/// the daemon's own environment: the daemon child inherits
/// `TASKBOARD_APP_PASSWORD` (set on its spawn), so its store load
/// succeeds.
#[tokio::test]
#[allow(clippy::too_many_lines)] // one journey, sequential rows
async fn e8_connect_while_a_daemon_serves_attaches_it() {
    let install = Install::new("e8");
    let daemon = install.spawn_daemon_with_password("pw");
    install.wait_ready().await;

    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(
            "/index.php/apps/deck/api/v1.0/boards",
        ))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_string(common::boards_json()))
        .mount(&server)
        .await;

    // Piped (non-tty) stdin feeds the empty app password, which the
    // mock accepts; the install is auto-backend so no guard fires.
    let (code, out, err) = install.run(&[
        "--json",
        "connect",
        "--server",
        &server.uri(),
        "--user",
        "alice",
    ]);
    assert_eq!(code, Some(0), "connect succeeded: {err}");
    let receipt: serde_json::Value = serde_json::from_str(out.trim()).expect("receipt json");
    assert_eq!(receipt["verified"], true);
    assert_eq!(receipt["backend_switched"], false);
    assert_eq!(
        receipt["daemon_connector"], "attached",
        "the daemon picked up the connection: {receipt}"
    );
    assert!(
        !err.lines().any(|line| line.starts_with("warning:")),
        "success is silent on the warning channel: {err}"
    );

    // The daemon reports the live connector under its boot-shaped
    // backend word.
    let (_, out, _) = install.run(&["--json", "status"]);
    let view: serde_json::Value = serde_json::from_str(out.trim()).expect("status json");
    assert_eq!(view["daemon"], true);
    assert_eq!(view["connector_attached"], true, "{view}");
    assert_eq!(view["backend"], "standalone");

    // Stop the daemon; the CLI is unaffected either way.
    let (stop_code, _, _) = install.run(&["daemon", "stop"]);
    assert_eq!(stop_code, Some(0));
    let _ = daemon; // killed on drop if stop raced
}

/// The env-store caveat live (D6's degradation arm): the daemon spawned
/// *without* `TASKBOARD_APP_PASSWORD` cannot load the secret, so the
/// attach answers `credential-unavailable` — the client maps it to
/// `unknown`, warns (structurally: a `warning:` line on stderr), and
/// the connect itself still exits 0; the daemon keeps serving
/// connector-less.
#[tokio::test]
async fn e9_connect_warns_when_the_daemon_cannot_load_the_secret() {
    let install = Install::new("e9");
    let daemon = install.spawn_daemon();
    install.wait_ready().await;

    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(
            "/index.php/apps/deck/api/v1.0/boards",
        ))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_string(common::boards_json()))
        .mount(&server)
        .await;

    let (code, out, err) = install.run(&[
        "--json",
        "connect",
        "--server",
        &server.uri(),
        "--user",
        "alice",
    ]);
    assert_eq!(code, Some(0), "connect succeeded: {err}");
    let receipt: serde_json::Value = serde_json::from_str(out.trim()).expect("receipt json");
    assert_eq!(
        receipt["daemon_connector"], "unknown",
        "the credential miss is the daemon-side bucket: {receipt}"
    );
    assert!(
        err.lines().any(|line| line.starts_with("warning:")),
        "the degraded arm warns: {err}"
    );

    // The daemon keeps serving, connector-less.
    let (_, out, _) = install.run(&["--json", "status"]);
    let view: serde_json::Value = serde_json::from_str(out.trim()).expect("status json");
    assert_eq!(view["daemon"], true);
    assert_eq!(view["connector_attached"], false, "{view}");

    let (stop_code, _, _) = install.run(&["daemon", "stop"]);
    assert_eq!(stop_code, Some(0));
    let _ = daemon;
}
