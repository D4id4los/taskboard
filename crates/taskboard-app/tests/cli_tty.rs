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

use std::io::Write as _;
use std::time::{Duration, Instant};

mod common;

use common::{tempdir, write_test_config};

const BOARDS_PATH: &str = "/index.php/apps/deck/api/v1.0/boards";

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
