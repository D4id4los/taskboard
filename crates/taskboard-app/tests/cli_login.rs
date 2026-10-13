// SPDX-License-Identifier: AGPL-3.0-only
//! L-series: the routing surface the deprecated `login` alias owns
//! (plan §7.2) — the typed standalone refusal at the dispatch tier
//! (the ceremony-body pins live at the `connect` seam, C-series) and
//! the write-path resolution table the alias drives. All
//! wiremock-hermetic.

use std::path::Path;
use std::sync::Arc;

use taskboard_app::secrets::{CredentialError, CredentialStore};

mod common;

use common::{FakeCredentialStore, tempdir};

const BOARDS_PATH: &str = "/index.php/apps/deck/api/v1.0/boards";

async fn mount_boards_ok() -> wiremock::MockServer {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(BOARDS_PATH))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_string(common::boards_json()))
        .mount(&server)
        .await;
    server
}

/// L9: explicit `[sync] backend = "standalone"` refuses the alias
/// *before any prompting or store touch* — with the typed
/// `backend-explicit-standalone` kind (D10's recorded kind change off
/// `sync-disabled`). Under auto the ceremony behaves exactly as before
/// (the C-series carries no backend key and stays green).
#[tokio::test]
async fn l9_explicit_standalone_refuses_login_before_prompting() {
    use taskboard_app::cli::Command;
    use taskboard_app::commands::{CommandCtx, CommandIo, dispatch};

    let dir = tempdir();
    let config_path = dir.path().join("taskboard.toml");
    std::fs::write(&config_path, "[sync]\nbackend = 'standalone'\n").expect("write toml");
    let store: Arc<dyn CredentialStore> = Arc::new(FakeCredentialStore::default());
    let mut err = Vec::new();

    // `server: None` would prompt on real stdin if the guard failed —
    // the prompt would hang the test; the timely exit-1 is the proof.
    let code = dispatch(
        Command::Login {
            server: None,
            user: None,
        },
        CommandCtx {
            config_path: Some(config_path.clone()),
            io: CommandIo {
                out: &mut Vec::new(),
                err: &mut err,
                json: true,
            },
        },
    )
    .await;
    assert_eq!(code, 1);
    // The typed refusal kind on the `--json` error envelope (the
    // deprecation notice precedes it on stderr; the last line is the
    // envelope).
    let last = std::str::from_utf8(&err)
        .expect("utf8 stderr")
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .expect("a non-empty stderr");
    let envelope: serde_json::Value = serde_json::from_str(last).expect("one JSON error envelope");
    assert_eq!(envelope["error"]["kind"], "backend-explicit-standalone");
    // Nothing was persisted through the store.
    assert!(matches!(
        store.load("https://unused.example.com", "alice"),
        Err(CredentialError::NotFound)
    ));
}

/// L10 (the flip): the alias with **no config anywhere** — no
/// `--config`, no `TASKBOARD_CONFIG`, no XDG file — persists to the
/// XDG candidate (`<XDG_CONFIG_HOME>/taskboard/taskboard.toml`), not
/// `./taskboard.toml`. Runs through `dispatch` because the flip lives
/// in the command's path resolution. The secret is read from stdin
/// (nextest: /dev/null → the empty line; the wiremock mock accepts any
/// credential), and the credential store is steered to `env` via the
/// config env overlay so no keyring is touched.
#[tokio::test]
async fn l10_login_with_no_config_anywhere_writes_the_xdg_candidate() {
    use taskboard_app::cli::Command;
    use taskboard_app::commands::{CommandCtx, CommandIo, dispatch};

    let server = mount_boards_ok().await;
    let xdg = tempdir();
    // Absolute path: `dirs` ignores a relative XDG_CONFIG_HOME (spec).
    let xdg_root = xdg.path().to_path_buf();

    let code = {
        let _guard = common::env_lock();
        let _xdg_var =
            common::EnvOverride::set("XDG_CONFIG_HOME", xdg_root.to_str().expect("utf8"));
        let _store_var = common::EnvOverride::set("TASKBOARD_NEXTCLOUD__CREDENTIAL_STORE", "env");
        common::remove_env("TASKBOARD_CONFIG");
        dispatch(
            Command::Login {
                server: Some(server.uri()),
                user: Some("alice".to_owned()),
            },
            CommandCtx {
                config_path: None,
                io: CommandIo {
                    out: &mut Vec::new(),
                    err: &mut Vec::new(),
                    json: true,
                },
            },
        )
        .await
    };
    assert_eq!(code, 0, "login succeeds on a fresh machine");

    let written_path = xdg_root.join("taskboard").join("taskboard.toml");
    let written = std::fs::read_to_string(&written_path).expect("config written to XDG");
    assert!(written.contains(&server.uri()), "server persisted");
    assert!(written.contains("alice"), "username persisted");
    let loaded = taskboard_app::config::AppConfig::load(Some(written_path))
        .expect("the created config loads");
    assert_eq!(loaded.username(), "alice");
    assert!(
        !Path::new("taskboard.toml").exists(),
        "no CWD config appeared (fresh-machine premise)"
    );
}

/// L11 (D1-a): an explicit `TASKBOARD_CONFIG` naming a *missing* file
/// is the operator's "the config is this file" declaration — the alias
/// writes there (symmetric with `--config`, C19), never the XDG
/// candidate.
#[tokio::test]
async fn l11_login_honors_a_missing_taskboard_config_target() {
    use taskboard_app::cli::Command;
    use taskboard_app::commands::{CommandCtx, CommandIo, dispatch};

    let server = mount_boards_ok().await;
    let env_dir = tempdir();
    let env_config = env_dir.path().join("env.toml");
    let xdg = tempdir();
    let xdg_root = xdg.path().to_path_buf();

    let code = {
        let _guard = common::env_lock();
        let _xdg_var =
            common::EnvOverride::set("XDG_CONFIG_HOME", xdg_root.to_str().expect("utf8"));
        let _store_var = common::EnvOverride::set("TASKBOARD_NEXTCLOUD__CREDENTIAL_STORE", "env");
        let _cfg_var =
            common::EnvOverride::set("TASKBOARD_CONFIG", env_config.to_str().expect("utf8"));
        dispatch(
            Command::Login {
                server: Some(server.uri()),
                user: Some("alice".to_owned()),
            },
            CommandCtx {
                config_path: None,
                io: CommandIo {
                    out: &mut Vec::new(),
                    err: &mut Vec::new(),
                    json: true,
                },
            },
        )
        .await
    };
    assert_eq!(code, 0, "login creates the declared env target");
    assert!(
        env_config.is_file(),
        "the TASKBOARD_CONFIG target was created"
    );
    assert!(
        !xdg_root.join("taskboard").join("taskboard.toml").exists(),
        "the XDG candidate stays untouched"
    );
}

/// L12 (row 3): an *existing* XDG config is the active file — the
/// alias overlays it in place (comments and unrelated overrides
/// survive), exactly as `--config` would; the first compositional pin
/// of the XDG-if-present read arm through the verb that owns it.
#[tokio::test]
async fn l12_login_overlays_an_existing_xdg_config() {
    use taskboard_app::cli::Command;
    use taskboard_app::commands::{CommandCtx, CommandIo, dispatch};

    let server = mount_boards_ok().await;
    let xdg = tempdir();
    let xdg_root = xdg.path().to_path_buf();
    let xdg_config = xdg_root.join("taskboard").join("taskboard.toml");
    std::fs::create_dir_all(xdg_config.parent().expect("parent")).expect("mkdir");
    std::fs::write(
        &xdg_config,
        "# my hand-tuned config\n\
         [sync]\npoll_interval_secs = 5 # keep this\n",
    )
    .expect("seed xdg config");

    let code = {
        let _guard = common::env_lock();
        let _xdg_var =
            common::EnvOverride::set("XDG_CONFIG_HOME", xdg_root.to_str().expect("utf8"));
        let _store_var = common::EnvOverride::set("TASKBOARD_NEXTCLOUD__CREDENTIAL_STORE", "env");
        common::remove_env("TASKBOARD_CONFIG");
        let code = dispatch(
            Command::Login {
                server: Some(server.uri()),
                user: Some("alice".to_owned()),
            },
            CommandCtx {
                config_path: None,
                io: CommandIo {
                    out: &mut Vec::new(),
                    err: &mut Vec::new(),
                    json: true,
                },
            },
        )
        .await;
        // Still under the env lock: the resolution arm under test.
        let loaded =
            taskboard_app::config::AppConfig::load(None).expect("the XDG config is the active one");
        assert_eq!(loaded.sync.poll_interval_secs, 5);
        assert_eq!(loaded.username(), "alice");
        code
    };
    assert_eq!(code, 0, "login overlays the existing XDG config");

    let written = std::fs::read_to_string(&xdg_config).expect("read back");
    assert!(
        written.contains("# my hand-tuned config"),
        "comment survives"
    );
    assert!(
        written.contains("poll_interval_secs = 5 # keep this"),
        "override + inline comment survive"
    );
    assert!(written.contains(&server.uri()), "server overlaid in place");
    assert!(
        !Path::new("taskboard.toml").exists(),
        "no CWD config appeared"
    );
}

/// L9b: `Command::Connect` (flags, no `--switch-backend`) through
/// `dispatch` under explicit standalone: exit 1, the same typed kind —
/// the verb's non-TTY refusal at the dispatch tier (the guard refuses,
/// never blocks on the confirm).
#[tokio::test]
async fn l9b_connect_under_explicit_standalone_refuses_at_the_dispatch_tier() {
    use taskboard_app::cli::Command;
    use taskboard_app::commands::{CommandCtx, CommandIo, dispatch};

    let dir = tempdir();
    let config_path = dir.path().join("taskboard.toml");
    std::fs::write(&config_path, "[sync]\nbackend = 'standalone'\n").expect("write toml");
    let store: Arc<dyn CredentialStore> = Arc::new(FakeCredentialStore::default());
    let mut err = Vec::new();

    // `server: None` would prompt on real stdin if the guard failed —
    // the timely exit-1 is the proof.
    let code = dispatch(
        Command::Connect {
            server: None,
            user: None,
            switch_backend: false,
            force: false,
        },
        CommandCtx {
            config_path: Some(config_path.clone()),
            io: CommandIo {
                out: &mut Vec::new(),
                err: &mut err,
                json: true,
            },
        },
    )
    .await;
    assert_eq!(code, 1);
    let last = std::str::from_utf8(&err)
        .expect("utf8 stderr")
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .expect("a non-empty stderr");
    let envelope: serde_json::Value = serde_json::from_str(last).expect("one JSON error envelope");
    assert_eq!(envelope["error"]["kind"], "backend-explicit-standalone");
    assert!(matches!(
        store.load("https://unused.example.com", "alice"),
        Err(CredentialError::NotFound)
    ));
}

/// L13: the deprecation notice, structurally — the alias happy path
/// writes non-empty stderr; the identical `Command::Connect` happy path
/// keeps stderr empty. No content assertions (the wording is UX).
#[tokio::test]
async fn l13_alias_happy_path_writes_stderr_connect_keeps_it_empty() {
    use taskboard_app::cli::Command;
    use taskboard_app::commands::{CommandCtx, CommandIo, dispatch};

    let server = mount_boards_ok().await;
    let alias_dir = tempdir();
    let connect_dir = tempdir();

    let _guard = common::env_lock();
    let _store_var = common::EnvOverride::set("TASKBOARD_NEXTCLOUD__CREDENTIAL_STORE", "env");
    common::remove_env("TASKBOARD_CONFIG");

    let mut alias_err = Vec::new();
    let alias_code = dispatch(
        Command::Login {
            server: Some(server.uri()),
            user: Some("alice".to_owned()),
        },
        CommandCtx {
            config_path: Some(alias_dir.path().join("taskboard.toml")),
            io: CommandIo {
                out: &mut Vec::new(),
                err: &mut alias_err,
                json: true,
            },
        },
    )
    .await;

    let mut connect_err = Vec::new();
    let connect_code = dispatch(
        Command::Connect {
            server: Some(server.uri()),
            user: Some("alice".to_owned()),
            switch_backend: false,
            force: false,
        },
        CommandCtx {
            config_path: Some(connect_dir.path().join("taskboard.toml")),
            io: CommandIo {
                out: &mut Vec::new(),
                err: &mut connect_err,
                json: true,
            },
        },
    )
    .await;

    assert_eq!(alias_code, 0);
    assert_eq!(connect_code, 0);
    assert!(!alias_err.is_empty(), "the alias writes a notice on stderr");
    assert!(connect_err.is_empty(), "connect keeps stderr clean");
}
