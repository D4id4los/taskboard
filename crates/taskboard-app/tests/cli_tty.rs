// SPDX-License-Identifier: AGPL-3.0-only
//! Interactive-tty login test: drives the *real* binary under a
//! pseudo-terminal (via `script`), with no flags — all three values
//! prompted. This is the regression harness for the interactive path
//! the seam fakes cannot cover: prompt labels reach the terminal, the
//! masked `*` feedback read completes on a real tty (it once waited
//! forever there), and the ceremony persists.
//!
//! Structural assertions only (exit codes, written files); prompt
//! wording is UI text and never asserted (AGENTS §5). Skipped when
//! `script` (util-linux) is unavailable.

use std::fmt::Write as _;
use std::io::Write as _;
use std::time::Instant;

mod common;

use std::time::Duration;

use common::{binary_command, tempdir, write_test_config};

const BOARDS_PATH: &str = "/index.php/apps/deck/api/v1.0/boards";

/// The session prompt (kept in sync with `repl::PROMPT`; wording is
/// never asserted — only its *arrival* gates the scripted input).
const SESSION_PROMPT: &str = "taskboard> ";

/// `script` wraps the binary in a pty; `timeout` guards the hang
/// regression (a wedged prompt must fail the test, not stall it).
fn run_login_under_pty(config: &std::path::Path, url: &str, input: &str) -> std::process::Output {
    let binary = env!("CARGO_BIN_EXE_taskboard");
    let inner = format!(
        "{} --config {} login --server {} --user alice",
        binary,
        config.display(),
        url
    );
    let mut child = std::process::Command::new("timeout")
        .arg("30")
        .arg("script")
        .arg("-qec")
        .arg(&inner)
        .arg("/dev/null")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn script(1) pty wrapper");
    child
        .stdin
        .take()
        .expect("stdin piped")
        .write_all(input.as_bytes())
        .expect("write scripted input");
    child.wait_with_output().expect("pty run completes")
}

#[test]
fn tty_login_prompts_all_three_values_and_persists() {
    if std::process::Command::new("script")
        .arg("--help")
        .output()
        .is_err()
    {
        // Skip, never fail: util-linux `script` unavailable here.
        return;
    }

    // The pty run below is blocking; mount the wiremock surface via a
    // tiny current-thread runtime and keep the server alive for the
    // whole test.
    let server = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("runtime")
        .block_on(async {
            let server = wiremock::MockServer::start().await;
            wiremock::Mock::given(wiremock::matchers::method("GET"))
                .and(wiremock::matchers::path(BOARDS_PATH))
                .respond_with(
                    wiremock::ResponseTemplate::new(200).set_body_string(common::boards_json()),
                )
                .mount(&server)
                .await;
            server
        });

    let dir = tempdir();
    let config = dir.path().join("taskboard.toml");
    // login is pre-core: the config only needs to decode (env store so
    // no keyring backend is touched in CI).
    write_test_config(&config, None, None).expect("write config");

    // Scripted interactive input: server URL, username, password —
    // exactly what a human types at the three prompts.
    let started = Instant::now();
    let output = run_login_under_pty(
        &config,
        &server.uri(),
        &format!("{}\nalice\ns3cret-tty-pw\n", server.uri()),
    );

    // The hang guard: `timeout 30` exits 124 when the prompt wedges.
    assert_ne!(
        output.status.code(),
        Some(124),
        "the interactive login hung (prompt never completed)"
    );
    assert_eq!(
        output.status.code(),
        Some(0),
        "interactive login failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        started.elapsed() < Duration::from_secs(30),
        "interactive login must finish promptly"
    );

    // Persisted non-secrets, secret-free by construction.
    let written = std::fs::read_to_string(&config).expect("config written");
    assert!(written.contains(&server.uri()));
    assert!(written.contains("alice"));
    assert!(!written.contains("s3cret-tty-pw"));
}

/// `script` wraps the bare binary in a pty: stdin is a TTY, so the
/// no-argument invocation opens the interactive REPL (ADR 0011). The
/// scripted lines are one session: create the board, add a stack and a
/// task, leave via `exit`.
#[test]
fn tty_bare_invocation_opens_a_repl_session_and_persists() {
    if std::process::Command::new("script")
        .arg("--help")
        .output()
        .is_err()
    {
        // Skip, never fail: util-linux `script` unavailable here.
        return;
    }

    let dir = tempdir();
    let config = dir.path().join("taskboard.toml");
    // Standalone shape: no server, no user, no credentials — the
    // session must boot and work with none of them.
    let db = dir.path().join("taskboard.db");
    let mut toml = String::from("[nextcloud]\ncredential_store = 'env'\n");
    let _ = write!(toml, "[storage]\ndb_path = '{}'\n", db.display());
    toml.push_str("[app]\nshutdown_timeout_secs = 5\n");
    std::fs::write(&config, toml).expect("write config");

    let binary = env!("CARGO_BIN_EXE_taskboard");
    let inner = format!("{} --config {}", binary, config.display());
    let mut child = std::process::Command::new("timeout")
        .arg("60")
        .arg("script")
        .arg("-qec")
        .arg(&inner)
        .arg("/dev/null")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn script(1) pty wrapper");
    // Feed the script only after the editor's first prompt shows up in
    // the captured output. `script` forwards piped stdin to the pty
    // immediately, and bytes parked in the pty input queue while
    // rustyline switches the terminal to raw mode are flushed by the
    // termios change — the whole script was silently lost (the CI
    // flake this replaces). Once the prompt is echoed, raw mode is
    // active and nothing is pending: writes land in the editor. CR is
    // the Enter key in raw mode (LF is not a key at all with ICRNL
    // off); in canonical mode ICRNL maps CR to NL, so CR works on both
    // sides of the switch.
    let mut stdin = child.stdin.take().expect("stdin piped");
    let mut out_pipe = child.stdout.take().expect("stdout piped");
    let mut seen = Vec::new();
    let prompt_seen = loop {
        let mut chunk = [0u8; 256];
        match std::io::Read::read(&mut out_pipe, &mut chunk) {
            Ok(0) | Err(_) => break false,
            Ok(n) => {
                seen.extend_from_slice(&chunk[..n]);
                if seen
                    .windows(SESSION_PROMPT.len())
                    .any(|w| w == SESSION_PROMPT.as_bytes())
                {
                    break true;
                }
            }
        }
    };
    assert!(
        prompt_seen,
        "the session never reached its first prompt: {}",
        String::from_utf8_lossy(&seen)
    );
    stdin
        .write_all(
            b"boards create \"Personal\"\rstacks add \"Todo\"\rtasks add \"first task\"\rexit\r",
        )
        .expect("write scripted session");
    drop(stdin);
    let mut tail = Vec::new();
    std::io::Read::read_to_end(&mut out_pipe, &mut tail).expect("drain session output");
    seen.extend_from_slice(&tail);
    drop(out_pipe);
    let output = child.wait_with_output().expect("pty run completes");

    // The hang guard: `timeout 60` exits 124 when the session wedges.
    assert_ne!(
        output.status.code(),
        Some(124),
        "the REPL session hung (exit never completed)"
    );
    assert_eq!(
        output.status.code(),
        Some(0),
        "REPL session failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(db.exists(), "the session's core created its database");

    // Durability beyond the session: a one-shot (no TTY) hydrates the
    // session's data and exits 0 — the session released the lock.
    let mut follow_up = binary_command();
    follow_up.args([
        "--config",
        config.to_str().expect("utf8 path"),
        "tasks",
        "list",
        "--json",
    ]);
    let outcome = follow_up.output().expect("one-shot runs");
    assert_eq!(
        outcome.status.code(),
        Some(0),
        "one-shot after the session: {}",
        String::from_utf8_lossy(&outcome.stderr)
    );
    let list: serde_json::Value =
        serde_json::from_slice(&outcome.stdout).expect("one list payload");
    assert_eq!(list["stacks"][0]["title"], "Todo");
    assert_eq!(list["stacks"][0]["tasks"][0]["title"], "first task");
}

/// The instance-lock mutex runs both ways (ADR 0011): with the daemon
/// holding the database, the pty REPL boot fails typed and exits 1;
/// after the daemon stops, the database is servable again.
#[test]
fn tty_repl_refuses_while_the_daemon_holds_the_database() {
    if std::process::Command::new("script")
        .arg("--help")
        .output()
        .is_err()
    {
        return;
    }

    let dir = tempdir();
    let config = dir.path().join("taskboard.toml");
    // The daemon needs a connector (the standalone gate) but never a
    // reachable server: an unreachable URL idles it, cycles failing.
    write_test_config(&config, Some("http://127.0.0.1:1"), Some("alice")).expect("write config");
    let lock_sidecar = dir.path().join("taskboard.lock");

    let mut daemon = binary_command();
    daemon
        .args(["--config", config.to_str().expect("utf8 path"), "daemon"])
        .env("TASKBOARD_APP_PASSWORD", "tty-lock-secret")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::from(
            std::fs::File::create(dir.path().join("daemon.err")).expect("daemon err file"),
        ));
    let mut daemon = daemon.spawn().expect("spawn the daemon");
    // If the daemon exits early, surface its stderr in the panic.
    let early = daemon.try_wait().expect("probe daemon");
    assert!(early.is_none(), "daemon exited at boot; see daemon.err");

    // Deterministic-enough cross-process wait: the daemon holds the
    // lock the moment its sidecar exists. A deadline keeps a wedged
    // daemon a test failure, not a stall.
    let deadline = Instant::now() + Duration::from_secs(30);
    while !lock_sidecar.exists() {
        assert!(
            Instant::now() < deadline,
            "the daemon never acquired its instance lock"
        );
        std::thread::sleep(Duration::from_millis(50));
    }

    let binary = env!("CARGO_BIN_EXE_taskboard");
    let inner = format!("{} --config {}", binary, config.display());
    let mut child = std::process::Command::new("timeout")
        .arg("30")
        .arg("script")
        .arg("-qec")
        .arg(&inner)
        .arg("/dev/null")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn script(1) pty wrapper");
    // The session refuses at boot and never reads a line; a dead pty
    // stdin is not a test failure by itself.
    let _ = child
        .stdin
        .take()
        .expect("stdin piped")
        .write_all(b"exit\n");
    let output = child.wait_with_output().expect("pty run completes");
    assert_ne!(
        output.status.code(),
        Some(124),
        "the REPL boot hung instead of failing on the held lock"
    );
    assert_eq!(
        output.status.code(),
        Some(1),
        "the REPL must refuse while the daemon holds the lock"
    );

    // Release: the daemon dies, its lock goes with it, and a one-shot
    // core boots on the same database again.
    daemon.kill().expect("kill the daemon");
    daemon.wait().expect("reap the daemon");
    let mut follow_up = binary_command();
    follow_up
        .args([
            "--config",
            config.to_str().expect("utf8 path"),
            "tasks",
            "list",
            "--json",
        ])
        .env("TASKBOARD_APP_PASSWORD", "tty-lock-secret");
    let outcome = follow_up.output().expect("one-shot runs");
    assert_eq!(
        outcome.status.code(),
        Some(0),
        "one-shot after the daemon stopped: {}",
        String::from_utf8_lossy(&outcome.stderr)
    );
}

/// Regression (review 2026-10-11): `taskboard daemon` under a pty must
/// run the daemon, not the TTY-sensitive no-argument entry — the
/// explicit subcommand wins over the REPL in every environment. A
/// standalone config makes the daemon's own gate the observable
/// outcome: typed `sync-disabled`, exit 1.
#[test]
fn tty_explicit_daemon_stays_the_daemon_and_refuses_standalone() {
    if std::process::Command::new("script")
        .arg("--help")
        .output()
        .is_err()
    {
        return;
    }

    let dir = tempdir();
    let config = dir.path().join("taskboard.toml");
    let db = dir.path().join("taskboard.db");
    let mut toml = String::from("[sync]\nbackend = 'standalone'\n");
    let _ = write!(toml, "[storage]\ndb_path = '{}'\n", db.display());
    std::fs::write(&config, toml).expect("write config");

    let binary = env!("CARGO_BIN_EXE_taskboard");
    let inner = format!("{} --config {} daemon --json", binary, config.display());
    let mut child = std::process::Command::new("timeout")
        .arg("30")
        .arg("script")
        .arg("-qec")
        .arg(&inner)
        .arg("/dev/null")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn script(1) pty wrapper");
    // The daemon refuses before reading anything; a dead pty stdin is
    // not a test failure by itself.
    drop(child.stdin.take());
    let output = child.wait_with_output().expect("pty run completes");
    assert_ne!(
        output.status.code(),
        Some(124),
        "the daemon hung instead of refusing the standalone config"
    );
    assert_eq!(
        output.status.code(),
        Some(1),
        "the explicit daemon must refuse standalone (exit 1), stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    // `script(1)` serves the child one pty for both streams: the JSON
    // envelope can land on either captured buffer, so scan both.
    let mut combined = output.stdout.clone();
    combined.extend_from_slice(&output.stderr);
    let errors: Vec<serde_json::Value> = String::from_utf8_lossy(&combined)
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();
    assert_eq!(errors.len(), 1, "exactly one JSON error envelope");
    assert_eq!(errors[0]["error"]["kind"], "sync-disabled");
}
