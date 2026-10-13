// SPDX-License-Identifier: AGPL-3.0-only
//! C-series: the `connect` ceremony tier (plan §7.1) — verify-then-
//! persist, the D2 backend-switch and D3 re-point guards in every
//! scripted mode, the env-store arm, comment-preserving config
//! overlay, prompt order, and the leak floors. All wiremock-hermetic:
//! scripted `FakePrompts`, fake/env credential stores, tempdirs — no
//! TTY, no network beyond the wiremock surface.

use std::path::Path;
use std::sync::Arc;

use taskboard_app::commands::connect::{self, ConnectArgs, PromptReader};
use taskboard_app::commands::{CommandError, output::ConnectReceipt};
use taskboard_app::config::AppConfig;
use taskboard_app::secrets::{CredentialError, CredentialStore, EnvCredentialStore};

mod common;

use common::{FakeCredentialStore, FakePrompts, tempdir};

const BOARDS_PATH: &str = "/index.php/apps/deck/api/v1.0/boards";

/// The default bundle: `connect`'s surface (confirm guards enabled).
fn args(server: Option<String>) -> ConnectArgs {
    ConnectArgs {
        server,
        user: Some("alice".to_owned()),
        ..ConnectArgs::default()
    }
}

/// Mounts the happy boards endpoint (the credential-verification call).
async fn mount_boards_ok() -> wiremock::MockServer {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(BOARDS_PATH))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_string(common::boards_json()))
        .mount(&server)
        .await;
    server
}

/// Seeds a config file with the given TOML body and returns its path.
fn seed_config(dir: &tempfile::TempDir, body: &str) -> std::path::PathBuf {
    let config_path = dir.path().join("taskboard.toml");
    std::fs::write(&config_path, body).expect("seed config");
    config_path
}

async fn run_connect(
    config_path: &Path,
    args: ConnectArgs,
    prompts: &mut dyn PromptReader,
    store: &Arc<dyn CredentialStore>,
) -> Result<ConnectReceipt, CommandError> {
    let raw = AppConfig::extract_raw(Some(config_path)).expect("raw config decodes");
    connect::run(config_path, raw, args, prompts, store).await
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

/// C13 (was L1): happy path — `boards()` 200 → verified receipt; the
/// config file is written at the resolved path containing server/user
/// but **not** the password; the store recorded the secret.
#[tokio::test]
async fn c13_connect_happy_path_verifies_then_persists() {
    let server = mount_boards_ok().await;
    let dir = tempdir();
    let config_path = dir.path().join("taskboard.toml");
    let store: Arc<dyn CredentialStore> = Arc::new(FakeCredentialStore::default());

    let receipt = run_connect(
        &config_path,
        args(Some(server.uri())),
        &mut FakePrompts::new(&["s3cret-c13-pw"]),
        &store,
    )
    .await
    .expect("connect succeeds");

    assert!(receipt.verified);
    assert_eq!(receipt.stored, "keyring");
    assert_eq!(receipt.server_url, server.uri());
    assert_eq!(receipt.username, "alice");
    assert!(!receipt.backend_switched);
    assert!(!receipt.repointed);

    // Config written, secret-free.
    let written = std::fs::read_to_string(&config_path).expect("config written");
    assert!(written.contains(&server.uri()), "server persisted");
    assert!(written.contains("username"), "username key persisted");
    assert!(written.contains("alice"), "username value persisted");
    assert!(!written.contains("s3cret-c13-pw"), "no secret in the file");

    // The store recorded the secret under (server, user).
    let stored = store.load(&server.uri(), "alice").expect("secret stored");
    assert_eq!(stored.expose(), "s3cret-c13-pw");
}

/// C14 (was L2): 401/403 → `InvalidCredentials`; nothing persisted (no
/// file write, no store call).
#[tokio::test]
async fn c14_connect_rejects_401_without_persisting() {
    let server = mount_auth_reject(401).await;
    let dir = tempdir();
    let config_path = dir.path().join("taskboard.toml");
    let store: Arc<dyn CredentialStore> = Arc::new(FakeCredentialStore::default());

    let err = run_connect(
        &config_path,
        args(Some(server.uri())),
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

/// C14 (403 arm): some servers answer 403 for bad tokens — same typed
/// failure, same absence of persistence.
#[tokio::test]
async fn c14_connect_rejects_403_without_persisting() {
    let server = mount_auth_reject(403).await;
    let dir = tempdir();
    let config_path = dir.path().join("taskboard.toml");
    let store: Arc<dyn CredentialStore> = Arc::new(FakeCredentialStore::default());

    let err = run_connect(
        &config_path,
        args(Some(server.uri())),
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

/// C14's ordering trap: a **forced** re-point with bad credentials
/// leaves the old identity fully intact — config bytes unchanged, old
/// secret still stored, new secret absent. The deletion strictly
/// follows the new verify (D7).
#[tokio::test]
async fn c14_forced_repoint_with_bad_credentials_leaves_the_old_identity_intact() {
    let server = mount_auth_reject(401).await;
    let dir = tempdir();
    let old_server = "https://old.example.com";
    let config_path = seed_config(
        &dir,
        &format!(
            "[nextcloud]\nserver_url = '{old_server}'\nusername = 'alice'\n\
             credential_store = 'keyring'\n"
        ),
    );
    let before = std::fs::read_to_string(&config_path).expect("seeded");
    let store: Arc<dyn CredentialStore> =
        Arc::new(FakeCredentialStore::with(old_server, "alice", "old-s3cret"));

    let err = run_connect(
        &config_path,
        ConnectArgs {
            server: Some(server.uri()),
            force: true,
            ..args(Some(server.uri()))
        },
        &mut FakePrompts::new(&["new-pw"]),
        &store,
    )
    .await
    .expect_err("401 rejects the forced re-point");
    assert_eq!(err.error_kind(), "invalid-credentials");

    assert_eq!(
        std::fs::read_to_string(&config_path).expect("config"),
        before,
        "config bytes unchanged"
    );
    let old = store.load(old_server, "alice").expect("old secret alive");
    assert_eq!(old.expose(), "old-s3cret");
    assert!(matches!(
        store.load(&server.uri(), "alice"),
        Err(CredentialError::NotFound)
    ));
}

/// C15 (was L3): an unreachable server → `Unreachable` (connection
/// refused is a deterministic transport failure).
#[tokio::test]
async fn c15_connect_unreachable_is_typed() {
    let dir = tempdir();
    let config_path = dir.path().join("taskboard.toml");
    let store: Arc<dyn CredentialStore> = Arc::new(FakeCredentialStore::default());

    let err = run_connect(
        &config_path,
        args(Some("http://127.0.0.1:9".to_owned())),
        &mut FakePrompts::new(&["pw"]),
        &store,
    )
    .await
    .expect_err("unreachable");
    assert_eq!(err.error_kind(), "unreachable");
    assert!(!config_path.exists());
}

/// C15 (was L8): an invalid server string fails at client construction
/// (typed Deck error) and nothing is persisted.
#[tokio::test]
async fn c15_connect_invalid_server_url_is_typed() {
    let dir = tempdir();
    let config_path = dir.path().join("taskboard.toml");
    let store: Arc<dyn CredentialStore> = Arc::new(FakeCredentialStore::default());

    let err = run_connect(
        &config_path,
        args(Some("not a url".to_owned())),
        &mut FakePrompts::new(&["pw"]),
        &store,
    )
    .await
    .expect_err("invalid url");
    assert_eq!(err.error_kind(), "deck");
    assert!(!config_path.exists());
}

/// C16 (was L4): env store → connect succeeds with `stored == "env"`.
#[tokio::test]
async fn c16_env_store_connect_succeeds_with_stored_env() {
    let server = mount_boards_ok().await;
    let dir = tempdir();
    let config_path = dir.path().join("taskboard.toml");
    let store: Arc<dyn CredentialStore> = Arc::new(EnvCredentialStore);

    let receipt = run_connect(
        &config_path,
        args(Some(server.uri())),
        &mut FakePrompts::new(&["pw"]),
        &store,
    )
    .await
    .expect("env-store connect succeeds");
    assert_eq!(receipt.stored, "env");
    assert!(receipt.verified);
    assert!(config_path.exists(), "config still written");
}

/// C16's D7 arm: a **forced** re-point under the env store succeeds —
/// the old identity's deletion tolerates `Unsupported` (the env store
/// has no secret to orphan).
#[tokio::test]
async fn c16_env_store_forced_repoint_succeeds() {
    let server = mount_boards_ok().await;
    let dir = tempdir();
    let old_server = "https://old.example.com";
    let config_path = seed_config(
        &dir,
        &format!(
            "[nextcloud]\nserver_url = '{old_server}'\nusername = 'alice'\n\
             credential_store = 'env'\n"
        ),
    );
    let store: Arc<dyn CredentialStore> = Arc::new(EnvCredentialStore);

    let receipt = run_connect(
        &config_path,
        ConnectArgs {
            server: Some(server.uri()),
            force: true,
            ..args(Some(server.uri()))
        },
        &mut FakePrompts::new(&["pw"]),
        &store,
    )
    .await
    .expect("env-store forced re-point succeeds");
    assert!(receipt.repointed);
    let written = std::fs::read_to_string(&config_path).expect("config written");
    assert!(written.contains(&server.uri()), "new server persisted");
}

/// C17 (was L5): read-modify-write preserves comments, formatting, and
/// unrelated overrides; the nextcloud keys are updated. And the auto
/// path writes **no** `backend` key at all (D6: a materialized word
/// would break the next `logout`'s strip).
#[tokio::test]
async fn c17_connect_overlays_preserving_comments_and_unknown_keys() {
    let server = mount_boards_ok().await;
    let dir = tempdir();
    let config_path = seed_config(
        &dir,
        "# my hand-tuned config\n\
         [sync]\npoll_interval_secs = 5 # keep this\n\
         [storage]\ndb_path = '/tmp/kiosk.db'\n",
    );
    let store: Arc<dyn CredentialStore> = Arc::new(FakeCredentialStore::default());

    run_connect(
        &config_path,
        args(Some(server.uri())),
        &mut FakePrompts::new(&["pw"]),
        &store,
    )
    .await
    .expect("connect succeeds");

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

    // The result still loads and validates — and the auto install
    // stayed auto: no backend key materialized.
    let loaded = AppConfig::load(Some(config_path.clone())).expect("overlay config is valid");
    assert_eq!(loaded.sync.poll_interval_secs, 5);
    assert_eq!(loaded.username(), "alice");
    assert!(
        loaded.sync.backend.is_none(),
        "no backend key written on the auto path"
    );
    let doc = std::fs::read_to_string(&config_path)
        .expect("read back")
        .parse::<toml_edit::DocumentMut>()
        .expect("parses");
    assert!(
        doc.get("sync")
            .and_then(|t| t.as_table())
            .is_none_or(|t| t.get("backend").is_none()),
        "the sync table carries no backend key"
    );
}

/// C17's switch arm: the D2 switch writes/updates the backend word
/// into `[sync]`, preserving the table's existing decor.
#[tokio::test]
async fn c17_switch_backend_writes_the_word_preserving_decor() {
    let server = mount_boards_ok().await;
    let dir = tempdir();
    let config_path = seed_config(
        &dir,
        "# hand-tuned\n\
         [sync]\npoll_interval_secs = 5 # keep this\nbackend = 'standalone'\n\
         [storage]\ndb_path = '/tmp/kiosk.db'\n",
    );
    let store: Arc<dyn CredentialStore> = Arc::new(FakeCredentialStore::default());

    let receipt = run_connect(
        &config_path,
        ConnectArgs {
            switch_backend: true,
            ..args(Some(server.uri()))
        },
        &mut FakePrompts::new(&["pw"]),
        &store,
    )
    .await
    .expect("switched connect succeeds");
    assert!(receipt.backend_switched);

    let written = std::fs::read_to_string(&config_path).expect("read back");
    assert!(written.contains("# hand-tuned"), "comment survives");
    assert!(
        written.contains("poll_interval_secs = 5 # keep this"),
        "override + inline comment survive"
    );
    assert!(written.contains("db_path = '/tmp/kiosk.db'"));
    let loaded = AppConfig::load(Some(config_path)).expect("overlay config is valid");
    assert!(matches!(
        loaded.sync.backend,
        Some(taskboard_app::config::SyncBackend::Nextcloud)
    ));
}

/// C17's D6 arm: even the `--switch-backend` flag on an *auto* install
/// (no explicit backend word) writes **no** backend key — the flag only
/// flips an explicit `standalone`; the auto install stays auto so the
/// next `logout` can strip the identity keys.
#[tokio::test]
async fn c17_switch_flag_on_an_auto_install_writes_no_backend_key() {
    let server = mount_boards_ok().await;
    let dir = tempdir();
    let config_path = seed_config(&dir, "# fresh\n");
    let store: Arc<dyn CredentialStore> = Arc::new(FakeCredentialStore::default());

    let receipt = run_connect(
        &config_path,
        ConnectArgs {
            switch_backend: true,
            ..args(Some(server.uri()))
        },
        &mut FakePrompts::new(&["pw"]),
        &store,
    )
    .await
    .expect("auto-path connect succeeds");
    assert!(!receipt.backend_switched);

    let doc = std::fs::read_to_string(&config_path)
        .expect("read back")
        .parse::<toml_edit::DocumentMut>()
        .expect("parses");
    assert!(
        doc.get("sync").is_none(),
        "no [sync] table materialized on the auto path"
    );
}

/// C18 (was L6): missing flags prompt in order — server URL, username,
/// then the secret; exactly those prompts are consumed.
#[tokio::test]
async fn c18_connect_prompts_in_order() {
    let server = mount_boards_ok().await;
    let dir = tempdir();
    let config_path = dir.path().join("taskboard.toml");
    let store: Arc<dyn CredentialStore> = Arc::new(FakeCredentialStore::default());
    let mut prompts = FakePrompts::new(&[server.uri().as_str(), "alice", "pw"]);

    let receipt = run_connect(&config_path, ConnectArgs::default(), &mut prompts, &store)
        .await
        .expect("prompted connect succeeds");
    assert_eq!(receipt.username, "alice");
    // Exactly the three scripted answers were consumed.
    assert!(
        prompts.read_line("probe: ").is_err(),
        "no further prompts after url/user/secret"
    );
}

/// C19 (was L7): the explicit `--config` path does not exist — other
/// commands answer `FileNotFound`, `connect` creates the file.
#[tokio::test]
async fn c19_connect_creates_an_explicit_missing_config() {
    let server = mount_boards_ok().await;
    let dir = tempdir();
    let config_path = dir.path().join("fresh").join("taskboard.toml");
    assert!(matches!(
        AppConfig::load(Some(config_path.clone())),
        Err(taskboard_app::config::ConfigError::FileNotFound(_))
    ));
    let store: Arc<dyn CredentialStore> = Arc::new(FakeCredentialStore::default());

    run_connect(
        &config_path,
        args(Some(server.uri())),
        &mut FakePrompts::new(&["pw"]),
        &store,
    )
    .await
    .expect("connect creates the config");
    AppConfig::load(Some(config_path)).expect("the created config loads");
}
