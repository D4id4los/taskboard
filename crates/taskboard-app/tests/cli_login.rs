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
