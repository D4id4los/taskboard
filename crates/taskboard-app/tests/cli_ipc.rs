// SPDX-License-Identifier: AGPL-3.0-only
//! Binary E2E for the daemon IPC frontend (WI-5): the *real* binary on
//! both sides — a real daemon process holding the instance lock, real
//! client processes routing over the socket. Process-level facts only:
//! exit codes across process boundaries, byte-identical stdout, the
//! socket artifact's lifecycle, signals. Verb semantics stay in the
//! handler suites and the IP-series.

use std::time::{Duration, Instant};

mod common;

use common::{binary_command, tempdir, write_test_config};

/// Polls `pred` with a real-time deadline (process synchronization —
/// the hermetic suites' tokio-clock rules do not reach subprocesses).
async fn wait_until(mut pred: impl FnMut() -> bool, what: &str) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !pred() {
        assert!(Instant::now() < deadline, "{what} did not happen in time");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Kills the child on drop so a failing assertion cannot leak a
/// lock-holding daemon into the next test.
struct Child(std::process::Child);

impl Drop for Child {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

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
        let child = binary_command()
            .args(["--config", &self.config(), "daemon"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("spawn daemon");
        Child(child)
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
    let (code, out, err) = install.run(&["tasks", "add", "first"]);
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
    let (code, out, err) = install.run(&["tasks", "add", "second"]);
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
        .args(["--config", &install.config(), "tasks", "add", "from-one"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("spawn first client");
    let second = binary_command()
        .args(["--config", &install.config(), "tasks", "add", "from-two"])
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
