// SPDX-License-Identifier: AGPL-3.0-only

//! The #1554 real-home guard, end to end: with the sandbox sentinel
//! planted, a bare binary invocation that would resolve the *sandbox*
//! home's XDG config (and its db under the sandbox data dir) refuses
//! typed — nothing opens a store — while an explicit isolated config
//! still runs normally. The sandboxed home lives in a tempdir; the
//! operator's real home is never read or written.

mod common;

use std::process::Stdio;

use common::{binary_command, tempdir};

/// Arms the sentinel already planted by [`binary_command`]'s scrub and
/// points the child's HOME/XDG dirs at `home` (a tempdir, never the
/// real home).
fn sandbox_home_env(
    mut cmd: std::process::Command,
    home: &std::path::Path,
) -> std::process::Command {
    cmd.env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("XDG_DATA_HOME", home.join(".local/share"));
    cmd
}

/// Writes `<home>/.config/taskboard/taskboard.toml` naming a db under
/// the *sandbox* home's data dir — exactly the shape a bare invocation
/// picks up via the XDG-candidate fallback.
fn seed_sandbox_xdg_config(home: &std::path::Path) -> std::path::PathBuf {
    let config_dir = home.join(".config/taskboard");
    std::fs::create_dir_all(&config_dir).expect("sandbox config dir created");
    let config = config_dir.join("taskboard.toml");
    std::fs::write(
        &config,
        format!(
            "[storage]\ndb_path = '{}'\n",
            home.join(".local/share/taskboard/taskboard.db").display()
        ),
    )
    .expect("sandbox xdg config written");
    config
}

/// The refusal: sentinel armed + no `--config`/`TASKBOARD_CONFIG` —
/// resolution falls through to the sandbox home's XDG candidate and
/// its db-under-the-sandbox-home-data-dir — and the binary refuses
/// typed (`sandboxed-real-home-path`), exits non-zero, and creates no
/// database. The sandbox home is a tempdir; the real home is never
/// touched.
#[test]
fn bare_sandboxed_invocation_refuses_a_real_home_fallthrough() {
    let home = tempdir();
    seed_sandbox_xdg_config(home.path());
    let db = home.path().join(".local/share/taskboard/taskboard.db");

    let output = sandbox_home_env(binary_command(), home.path())
        .arg("--json")
        .arg("status")
        .stdin(Stdio::null())
        .output()
        .expect("the binary runs");

    assert_ne!(
        output.status.code(),
        Some(0),
        "the bare sandboxed invocation must not succeed"
    );
    assert!(
        !db.exists(),
        "no store was opened or created at the resolved db path"
    );
    // Typed outcome, not message text: the `--json` error envelope's
    // stable `ConfigError::kind` discriminant (the envelope is the
    // stderr's final line; tracing log lines precede it).
    let stderr = String::from_utf8_lossy(&output.stderr);
    let envelope_line = stderr
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .expect("a typed envelope line on stderr");
    let envelope: serde_json::Value =
        serde_json::from_str(envelope_line).expect("the typed json error envelope parses");
    assert_eq!(
        envelope["error"]["kind"], "sandboxed-real-home-path",
        "the refusal is the typed #1554 variant"
    );
}

/// The positive case: the sentinel armed but an explicit isolated
/// config (outside any home) — the verb runs normally. The guard
/// obstructs only real-home resolution, never a properly sandboxed
/// invocation.
#[test]
fn sentinel_armed_with_explicit_config_runs_normally() {
    let dir = tempdir();
    let config = dir.path().join("taskboard.toml");
    std::fs::write(
        &config,
        format!(
            "[nextcloud]\ncredential_store = 'env'\n[storage]\ndb_path = '{}'\n\
             [app]\nshutdown_timeout_secs = 1\n",
            dir.path().join("taskboard.db").display()
        ),
    )
    .expect("isolated config written");

    let output = binary_command()
        .arg("--config")
        .arg(&config)
        .arg("--json")
        .arg("status")
        .stdin(Stdio::null())
        .output()
        .expect("the binary runs");

    assert_eq!(
        output.status.code(),
        Some(0),
        "an explicit isolated config runs under the sentinel: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
