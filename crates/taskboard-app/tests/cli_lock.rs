// SPDX-License-Identifier: AGPL-3.0-only
//! K3: the cross-process, hermetic instance-lock test — the *real*
//! binary holds the lock (`daemon` against an unreachable server: it
//! boots fine offline), a second invocation fails with the typed
//! `instance-locked` error, and the lock is released when the holder
//! dies.

use std::time::{Duration, Instant};

mod common;

use common::{binary_command, tempdir, write_test_config};

/// Polls `pred` with a real-time deadline (process synchronization —
/// the hermetic suites' tokio-clock rules do not reach subprocesses).
fn wait_until(pred: impl Fn() -> bool, what: &str) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !pred() {
        assert!(Instant::now() < deadline, "{what} did not happen in time");
        std::thread::sleep(Duration::from_millis(50));
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

#[tokio::test]
async fn k3_daemon_holds_the_lock_across_processes() {
    let dir = tempdir();
    let config = dir.path().join("taskboard.toml");
    // Unreachable server: the daemon boots fine offline (the boot cycle
    // fails, local data serves) and then waits for its signal.
    write_test_config(&config, Some("http://127.0.0.1:9"), Some("alice")).expect("write config");
    let lock_sidecar = dir.path().join("taskboard.lock");

    let daemon = binary_command()
        .args(["--config", config.to_str().unwrap(), "daemon"])
        .env("TASKBOARD_APP_PASSWORD", "pw")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("spawn daemon");
    let mut daemon = Child(daemon);
    wait_until(|| lock_sidecar.is_file(), "daemon acquired the lock");

    // Second process on the same database: exit 1, typed stderr JSON.
    let locked = binary_command()
        .args([
            "--config",
            config.to_str().unwrap(),
            "--json",
            "tasks",
            "list",
        ])
        .env("TASKBOARD_APP_PASSWORD", "pw")
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn tasks list");
    let output = locked.wait_with_output().expect("wait");
    assert_eq!(output.status.code(), Some(1), "locked invocation exits 1");
    let stderr = read_to_string(output.stderr);
    // The error envelope is the last line on stderr (logs precede it).
    let last = stderr
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("");
    let err: serde_json::Value =
        serde_json::from_str(last).expect("stderr ends with one JSON error object");
    assert_eq!(err["error"]["kind"], "instance-locked");

    // The holder dies (SIGTERM → graceful drain → OS releases the
    // lock), and the next invocation succeeds.
    let pid = daemon.0.id();
    let _ = std::process::Command::new("kill")
        .arg(pid.to_string())
        .status()
        .expect("send SIGTERM");
    let exit_deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if daemon.0.try_wait().expect("poll daemon").is_some() {
            break;
        }
        assert!(
            Instant::now() < exit_deadline,
            "daemon did not exit after SIGTERM"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    drop(daemon);

    let free = binary_command()
        .args([
            "--config",
            config.to_str().unwrap(),
            "--json",
            "tasks",
            "list",
        ])
        .env("TASKBOARD_APP_PASSWORD", "pw")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn tasks list after release");
    let output = free.wait_with_output().expect("wait");
    assert_eq!(
        output.status.code(),
        Some(0),
        "offline tasks list succeeds after the holder died"
    );
    // The offline task list is empty but structurally valid JSON.
    let stdout = read_to_string(output.stdout);
    let payload: serde_json::Value = serde_json::from_str(stdout.trim()).expect("json payload");
    assert_eq!(payload["stacks"], serde_json::Value::Array(vec![]));
}
