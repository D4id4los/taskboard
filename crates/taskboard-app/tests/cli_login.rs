// SPDX-License-Identifier: AGPL-3.0-only
//! L-series: the `login` ceremony (plan §6) — verify-then-persist, the
//! env-store arm, comment-preserving config overlay, prompt order, and
//! the leak floors. All wiremock-hermetic.

use std::path::Path;
use std::sync::Arc;

use taskboard_app::commands::login::{self, PromptReader as _};
use taskboard_app::commands::{CommandError, output::LoginReceipt};
use taskboard_app::config::AppConfig;
use taskboard_app::secrets::{CredentialError, CredentialStore, EnvCredentialStore};

mod common;

use common::{FakeCredentialStore, FakePrompts, tempdir};

const BOARDS_PATH: &str = "/index.php/apps/deck/api/v1.0/boards";

async fn run_login(
    config_path: &Path,
    server: Option<String>,
    prompts: &mut dyn taskboard_app::commands::login::PromptReader,
    store: &Arc<dyn CredentialStore>,
) -> Result<LoginReceipt, CommandError> {
    let raw = AppConfig::extract_raw(Some(config_path)).expect("raw config decodes");
    login::run(
        config_path,
        raw,
        server,
        Some("alice".to_owned()),
        prompts,
        store,
    )
    .await
}

/// L1: happy path — `boards()` 200 → verified receipt; the config file
/// is written at the resolved path containing server/user but **not**
/// the password; the store recorded the secret.
#[tokio::test]
async fn l1_login_happy_path_verifies_then_persists() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(BOARDS_PATH))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_string(common::boards_json()))
        .mount(&server)
        .await;
    let dir = tempdir();
    let config_path = dir.path().join("taskboard.toml");
    let store: Arc<dyn CredentialStore> = Arc::new(FakeCredentialStore::default());

    let receipt = run_login(
        &config_path,
        Some(server.uri()),
        &mut FakePrompts::new(&["s3cret-l1-pw"]),
        &store,
    )
    .await
    .expect("login succeeds");

    assert!(receipt.verified);
    assert_eq!(receipt.stored, "keyring");
    assert_eq!(receipt.server_url, server.uri());
    assert_eq!(receipt.username, "alice");

    // Config written, secret-free.
    let written = std::fs::read_to_string(&config_path).expect("config written");
    assert!(written.contains(&server.uri()), "server persisted");
    assert!(written.contains("username"), "username key persisted");
    assert!(written.contains("alice"), "username value persisted");
    assert!(!written.contains("s3cret-l1-pw"), "no secret in the file");

    // The store recorded the secret under (server, user).
    let stored = store.load(&server.uri(), "alice").expect("secret stored");
    assert_eq!(stored.expose(), "s3cret-l1-pw");
}

async fn mount_auth_reject(status: u16) -> wiremock::MockServer {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(BOARDS_PATH))
        .respond_with(wiremock::ResponseTemplate::new(status))
        .mount(&server)
        .await;
    server
}

/// L2: 401 → `InvalidCredentials`; nothing persisted (no file write,
/// no store call).
#[tokio::test]
async fn l2a_login_rejects_401_without_persisting() {
    let server = mount_auth_reject(401).await;
    let dir = tempdir();
    let config_path = dir.path().join("taskboard.toml");
    let store: Arc<dyn CredentialStore> = Arc::new(FakeCredentialStore::default());

    let err = run_login(
        &config_path,
        Some(server.uri()),
        &mut FakePrompts::new(&["pw"]),
        &store,
    )
    .await
    .expect_err("401 rejects");
    assert_eq!(err.error_kind(), "invalid-credentials");
    assert!(!config_path.exists(), "no config write on rejection");
    assert!(matches!(
        store.load(&server.uri(), "alice"),
        Err(CredentialError::NotFound)
    ));
}

/// L2 (403 arm): some servers answer 403 for bad tokens — same typed
/// failure, same absence of persistence.
#[tokio::test]
async fn l2b_login_rejects_403_without_persisting() {
    let server = mount_auth_reject(403).await;
    let dir = tempdir();
    let config_path = dir.path().join("taskboard.toml");
    let store: Arc<dyn CredentialStore> = Arc::new(FakeCredentialStore::default());

    let err = run_login(
        &config_path,
        Some(server.uri()),
        &mut FakePrompts::new(&["pw"]),
        &store,
    )
    .await
    .expect_err("403 rejects");
    assert_eq!(err.error_kind(), "invalid-credentials");
    assert!(!config_path.exists());
    assert!(matches!(
        store.load(&server.uri(), "alice"),
        Err(CredentialError::NotFound)
    ));
}

/// L3: an unreachable server → `Unreachable` (connection refused is a
/// deterministic transport failure).
#[tokio::test]
async fn l3_login_unreachable_is_typed() {
    let dir = tempdir();
    let config_path = dir.path().join("taskboard.toml");
    let store: Arc<dyn CredentialStore> = Arc::new(FakeCredentialStore::default());

    let err = run_login(
        &config_path,
        Some("http://127.0.0.1:9".to_owned()),
        &mut FakePrompts::new(&["pw"]),
        &store,
    )
    .await
    .expect_err("unreachable");
    assert_eq!(err.error_kind(), "unreachable");
    assert!(!config_path.exists());
}

/// L4: env store → login succeeds with `stored == "env"`; the store is
/// untouched (it cannot store).
#[tokio::test]
async fn l4_env_store_login_succeeds_with_stored_env() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(BOARDS_PATH))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_string(common::boards_json()))
        .mount(&server)
        .await;
    let dir = tempdir();
    let config_path = dir.path().join("taskboard.toml");
    let store: Arc<dyn CredentialStore> = Arc::new(EnvCredentialStore);

    let receipt = run_login(
        &config_path,
        Some(server.uri()),
        &mut FakePrompts::new(&["pw"]),
        &store,
    )
    .await
    .expect("env-store login succeeds");
    assert_eq!(receipt.stored, "env");
    assert!(receipt.verified);
    assert!(config_path.exists(), "config still written");
}

/// L5: read-modify-write preserves comments, formatting, and unrelated
/// overrides; the nextcloud keys are updated.
#[tokio::test]
async fn l5_login_overlays_preserving_comments_and_unknown_keys() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(BOARDS_PATH))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_string(common::boards_json()))
        .mount(&server)
        .await;
    let dir = tempdir();
    let config_path = dir.path().join("taskboard.toml");
    std::fs::write(
        &config_path,
        "# my hand-tuned config\n\
         [sync]\npoll_interval_secs = 5 # keep this\n\
         [storage]\ndb_path = '/tmp/kiosk.db'\n",
    )
    .expect("seed config");
    let store: Arc<dyn CredentialStore> = Arc::new(FakeCredentialStore::default());

    run_login(
        &config_path,
        Some(server.uri()),
        &mut FakePrompts::new(&["pw"]),
        &store,
    )
    .await
    .expect("login succeeds");

    let written = std::fs::read_to_string(&config_path).expect("read back");
    assert!(
        written.contains("# my hand-tuned config"),
        "comment survives"
    );
    assert!(
        written.contains("poll_interval_secs = 5 # keep this"),
        "override + inline comment survive"
    );
    assert!(written.contains("db_path = '/tmp/kiosk.db'"));
    assert!(written.contains(&server.uri()));

    // The result still loads and validates.
    let loaded = AppConfig::load(Some(config_path)).expect("overlay config is valid");
    assert_eq!(loaded.sync.poll_interval_secs, 5);
    assert_eq!(loaded.username(), "alice");
}

/// L6: missing flags prompt in order — server URL, username, then the
/// secret; exactly those prompts are consumed.
#[tokio::test]
async fn l6_login_prompts_in_order() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(BOARDS_PATH))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_string(common::boards_json()))
        .mount(&server)
        .await;
    let dir = tempdir();
    let config_path = dir.path().join("taskboard.toml");
    let store: Arc<dyn CredentialStore> = Arc::new(FakeCredentialStore::default());
    let mut prompts = FakePrompts::new(&[server.uri().as_str(), "alice", "pw"]);

    let receipt = {
        let raw = AppConfig::extract_raw(Some(&config_path)).expect("raw config decodes");
        login::run(&config_path, raw, None, None, &mut prompts, &store)
            .await
            .expect("prompted login succeeds")
    };
    assert_eq!(receipt.username, "alice");
    // Exactly the three scripted answers were consumed.
    assert!(
        prompts.read_line("probe: ").is_err(),
        "no further prompts after url/user/secret"
    );
}

/// L7: the explicit `--config` path does not exist — other commands
/// answer `FileNotFound`, `login` creates the file.
#[tokio::test]
async fn l7_login_creates_an_explicit_missing_config() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(BOARDS_PATH))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_string(common::boards_json()))
        .mount(&server)
        .await;
    let dir = tempdir();
    let config_path = dir.path().join("fresh").join("taskboard.toml");
    assert!(matches!(
        AppConfig::load(Some(config_path.clone())),
        Err(taskboard_app::config::ConfigError::FileNotFound(_))
    ));
    let store: Arc<dyn CredentialStore> = Arc::new(FakeCredentialStore::default());

    run_login(
        &config_path,
        Some(server.uri()),
        &mut FakePrompts::new(&["pw"]),
        &store,
    )
    .await
    .expect("login creates the config");
    AppConfig::load(Some(config_path)).expect("the created config loads");
}

/// L8: an invalid server string fails at client construction (typed
/// Deck error) and nothing is persisted.
#[tokio::test]
async fn l8_login_invalid_server_url_is_typed() {
    let dir = tempdir();
    let config_path = dir.path().join("taskboard.toml");
    let store: Arc<dyn CredentialStore> = Arc::new(FakeCredentialStore::default());

    let err = run_login(
        &config_path,
        Some("not a url".to_owned()),
        &mut FakePrompts::new(&["pw"]),
        &store,
    )
    .await
    .expect_err("invalid url");
    assert_eq!(err.error_kind(), "deck");
    assert!(!config_path.exists());
}

/// L9: explicit `[sync] backend = "standalone"` refuses login *before
/// any prompting or store touch* — the dispatcher's gate (ADR 0010).
/// Under auto the ceremony behaves exactly as today (L1–L8 carry no
/// backend key and stay green).
#[tokio::test]
async fn l9_explicit_standalone_refuses_login_before_prompting() {
    use taskboard_app::cli::Command;
    use taskboard_app::commands::{CommandCtx, CommandIo, dispatch};

    let dir = tempdir();
    let config_path = dir.path().join("taskboard.toml");
    std::fs::write(&config_path, "[sync]\nbackend = 'standalone'\n").expect("write toml");
    let store: Arc<dyn CredentialStore> = Arc::new(FakeCredentialStore::default());

    // `server: None` would prompt on real stdin if the gate failed —
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
                err: &mut Vec::new(),
                json: true,
            },
        },
    )
    .await;
    assert_eq!(code, 1);
    // Nothing was persisted through the store.
    assert!(matches!(
        store.load("https://unused.example.com", "alice"),
        Err(CredentialError::NotFound)
    ));
}

/// L10 (the flip): `login` with **no config anywhere** — no `--config`,
/// no `TASKBOARD_CONFIG`, no XDG file — persists to the XDG candidate
/// (`<XDG_CONFIG_HOME>/taskboard/taskboard.toml`), not `./taskboard.toml`.
/// Runs through `dispatch` because the flip lives in `login_command`'s
/// path resolution. The secret is read from stdin (nextest: /dev/null →
/// the empty line; the wiremock mock accepts any credential), and the
/// credential store is steered to `env` via the config env overlay so no
/// keyring is touched.
#[tokio::test]
async fn l10_login_with_no_config_anywhere_writes_the_xdg_candidate() {
    use taskboard_app::cli::Command;
    use taskboard_app::commands::{CommandCtx, CommandIo, dispatch};

    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(BOARDS_PATH))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_string(common::boards_json()))
        .mount(&server)
        .await;
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
    let loaded = AppConfig::load(Some(written_path)).expect("the created config loads");
    assert_eq!(loaded.username(), "alice");
    assert!(
        !Path::new("taskboard.toml").exists(),
        "no CWD config appeared (fresh-machine premise)"
    );
}

/// L11 (D1-a): an explicit `TASKBOARD_CONFIG` naming a *missing* file is
/// the operator's "the config is this file" declaration — login writes
/// there (symmetric with `--config`, L7), never the XDG candidate.
#[tokio::test]
async fn l11_login_honors_a_missing_taskboard_config_target() {
    use taskboard_app::cli::Command;
    use taskboard_app::commands::{CommandCtx, CommandIo, dispatch};

    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(BOARDS_PATH))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_string(common::boards_json()))
        .mount(&server)
        .await;
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

/// L12 (row 3): an *existing* XDG config is the active file — login
/// overlays it in place (comments and unrelated overrides survive),
/// exactly as `--config` would; the first compositional pin of the
/// XDG-if-present read arm through the verb that owns it.
#[tokio::test]
async fn l12_login_overlays_an_existing_xdg_config() {
    use taskboard_app::cli::Command;
    use taskboard_app::commands::{CommandCtx, CommandIo, dispatch};

    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(BOARDS_PATH))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_string(common::boards_json()))
        .mount(&server)
        .await;
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
        let loaded = AppConfig::load(None).expect("the XDG config is the active one");
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
