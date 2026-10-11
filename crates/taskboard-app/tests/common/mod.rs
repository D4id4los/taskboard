// SPDX-License-Identifier: AGPL-3.0-only
//! Fixtures shared by the app crate's integration suites: the fake
//! credential store, a minimal wiremock Deck harness (the sync crate's
//! harnesses are test-internal and not importable), and an env mutex.
//! Each test binary uses a different subset, hence the module-level
//! `dead_code` allowance.

#![allow(dead_code)]

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

use taskboard_app::secrets::{AppPassword, CredentialError, CredentialStore};

/// A per-test tempdir (fails the test, never silently reuses).
#[must_use]
pub fn tempdir() -> tempfile::TempDir {
    tempfile::tempdir().expect("tempdir")
}

/// A scripted [`taskboard_app::commands::login::PromptReader`] (fakes
/// over mocks): answers are consumed strictly in order, so a test
/// proves the prompt order by exhausting the queue.
#[derive(Debug, Default)]
pub struct FakePrompts {
    answers: Mutex<std::collections::VecDeque<String>>,
}

impl FakePrompts {
    #[must_use]
    pub fn new(answers: &[&str]) -> Self {
        Self {
            answers: Mutex::new(answers.iter().map(|s| (*s).to_owned()).collect()),
        }
    }
}

impl taskboard_app::commands::login::PromptReader for FakePrompts {
    fn read_line(&mut self, _prompt: &str) -> Result<String, std::io::Error> {
        self.answers
            .lock()
            .expect("fake prompts lock")
            .pop_front()
            .ok_or_else(|| std::io::Error::other("no scripted answer"))
    }

    fn read_secret(&mut self, prompt: &str) -> Result<String, std::io::Error> {
        self.read_line(prompt)
    }
}

/// Process env is a shared resource and nextest runs test binaries in
/// parallel processes: every env-mutating test takes this lock (per
/// process) and restores prior values on drop.
pub fn env_lock() -> EnvGuard {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let guard = LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    EnvGuard(guard)
}

/// Holds the env mutex for the guard's lifetime.
pub struct EnvGuard(std::sync::MutexGuard<'static, ()>);

/// Sets an env var (edition-2024 unsafe) for the duration of the lock.
pub fn set_env(key: &str, value: &str) {
    // SAFETY: callers hold the process-local env lock; test binaries are
    // single-threaded per env-mutating section.
    unsafe { std::env::set_var(key, value) };
}

/// Removes an env var (edition-2024 unsafe) for the duration of the lock.
pub fn remove_env(key: &str) {
    // SAFETY: as `set_env`.
    unsafe { std::env::remove_var(key) };
}

/// RAII env override: sets `key = value` now, restores the prior state
/// on drop (callers hold the env lock).
pub struct EnvOverride {
    key: &'static str,
    prior: Option<String>,
}

impl EnvOverride {
    #[must_use]
    pub fn set(key: &'static str, value: &str) -> Self {
        let prior = std::env::var(key).ok();
        set_env(key, value);
        Self { key, prior }
    }
}

impl Drop for EnvOverride {
    fn drop(&mut self) {
        match &self.prior {
            Some(v) => set_env(self.key, v),
            None => remove_env(self.key),
        }
    }
}

/// A `HashMap`-backed [`CredentialStore`]: the fake for stateful
/// credential behavior (fakes over mocks). Stores are keyed by
/// `(server, username)` — tests prove scoping by storing under one key
/// and loading under another.
#[derive(Debug, Default)]
pub struct FakeCredentialStore {
    entries: Mutex<HashMap<(String, String), String>>,
}

impl FakeCredentialStore {
    #[must_use]
    pub fn with(server: &str, username: &str, secret: &str) -> Self {
        let mut entries = HashMap::new();
        entries.insert((server.to_owned(), username.to_owned()), secret.to_owned());
        Self {
            entries: Mutex::new(entries),
        }
    }
}

impl CredentialStore for FakeCredentialStore {
    fn load(&self, server: &str, username: &str) -> Result<AppPassword, CredentialError> {
        self.entries
            .lock()
            .expect("fake store lock")
            .get(&(server.to_owned(), username.to_owned()))
            .map(|s| AppPassword::new(s.clone()))
            .ok_or(CredentialError::NotFound)
    }

    fn store(
        &self,
        server: &str,
        username: &str,
        secret: AppPassword,
    ) -> Result<(), CredentialError> {
        self.entries.lock().expect("fake store lock").insert(
            (server.to_owned(), username.to_owned()),
            secret.expose().to_owned(),
        );
        Ok(())
    }

    fn delete(&self, server: &str, username: &str) -> Result<(), CredentialError> {
        self.entries
            .lock()
            .expect("fake store lock")
            .remove(&(server.to_owned(), username.to_owned()))
            .map(|_| ())
            .ok_or(CredentialError::NotFound)
    }
}

/// A fully valid config pointing at `server_url`, for tests that tweak
/// one field.
pub fn valid_config(server_url: &str) -> taskboard_app::config::AppConfig {
    taskboard_app::config::AppConfig {
        nextcloud: taskboard_app::config::NextcloudSection {
            server_url: Some(server_url.to_owned()),
            username: Some("alice".to_owned()),
            credential_store: taskboard_app::config::CredentialStoreKind::Env,
        },
        ..taskboard_app::config::AppConfig::default()
    }
}

/// The remote ids the minimal Deck harness uses.
pub const BOARD: u64 = 3;
/// The active stack remote id.
pub const STACK: u64 = 8;
/// Fixture timestamp (seconds).
pub const T: i64 = 1_700_000_000;

/// Mirror of the sync crate's hermetic `boards_json` (test-internal
/// there): one live board with an explicit labels array.
pub fn boards_json() -> String {
    serde_json::json!([{
        "id": BOARD, "title": "board", "color": "00ff00",
        "lastModified": T, "deletedAt": 0,
        "archived": false, "labels": []
    }])
    .to_string()
}

/// The board detail endpoint's payload (authoritative labels read).
pub fn board_json() -> String {
    serde_json::json!({
        "id": BOARD, "title": "board", "color": "00ff00",
        "lastModified": T, "deletedAt": 0,
        "archived": false, "labels": []
    })
    .to_string()
}

/// The active stacks listing with the given cards in the one stack.
pub fn stacks_json(cards: &serde_json::Value) -> String {
    serde_json::json!([{
        "id": STACK, "title": "col", "boardId": BOARD, "order": 0,
        "lastModified": T, "deletedAt": 0, "archived": false,
        "cards": cards
    }])
    .to_string()
}

/// One remote card.
pub fn card_json(id: u64, title: &str) -> serde_json::Value {
    serde_json::json!({
        "id": id, "title": title, "stackId": STACK, "type": "plain",
        "order": 0, "lastModified": T, "labels": [], "archived": false,
        "duedate": null, "done": null
    })
}

/// Mounts the standard happy pull over the four endpoints the sync
/// actor reads, with `ETag`s so conditional follow-ups work.
pub async fn mount_standard_pull(server: &wiremock::MockServer, cards: serde_json::Value) {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, ResponseTemplate};
    for (p, body) in [
        (
            "/index.php/apps/deck/api/v1.0/boards".to_owned(),
            boards_json(),
        ),
        (
            format!("/index.php/apps/deck/api/v1.0/boards/{BOARD}"),
            board_json(),
        ),
        (
            format!("/index.php/apps/deck/api/v1.0/boards/{BOARD}/stacks"),
            stacks_json(&cards),
        ),
        (
            format!("/index.php/apps/deck/api/v1.0/boards/{BOARD}/stacks/archived"),
            "[]".to_owned(),
        ),
    ] {
        Mock::given(method("GET"))
            .and(path(p))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("ETag", "\"gen-1\"")
                    .set_body_string(body),
            )
            .mount(server)
            .await;
    }
}

/// The outer wall-clock deadline for one live-tier test.
pub const LIVE_DEADLINE: std::time::Duration = std::time::Duration::from_secs(300);

/// Tier 2 docker-tier credentials; `None` when any is unset.
pub struct LiveCfg {
    pub url: String,
    pub user: String,
    pub token: String,
}

#[must_use]
pub fn live_docker_config() -> Option<LiveCfg> {
    let (url, user, token) = (
        std::env::var("TASKBOARD_IT_DOCKER_URL").ok()?,
        std::env::var("TASKBOARD_IT_DOCKER_USER").ok()?,
        std::env::var("TASKBOARD_IT_DOCKER_TOKEN").ok()?,
    );
    if url.is_empty() || user.is_empty() || token.is_empty() {
        return None;
    }
    Some(LiveCfg { url, user, token })
}

/// Unique per-run prefix for test resources so parallel/never-deleting
/// runs cannot collide on the server.
#[must_use]
pub fn run_id() -> String {
    let id = uuid::Uuid::new_v4().simple().to_string();
    format!("taskboard-it-{}", &id[..8])
}

/// Runs `body` under a hard deadline so a hung server fails the test
/// instead of wedging CI.
pub async fn with_deadline<T>(
    body: impl std::future::Future<Output = T>,
) -> Result<T, tokio::time::error::Elapsed> {
    tokio::time::timeout(LIVE_DEADLINE, body).await
}

/// Retries `op` while Deck answers one of its sporadic write-path
/// `Server(500)`s (known upstream flakiness, per-run resources only).
pub async fn retry<T>(
    mut op: impl AsyncFnMut() -> Result<T, taskboard_sync_nextcloud::DeckError>,
) -> Result<T, taskboard_sync_nextcloud::DeckError> {
    let mut attempts = 0u32;
    loop {
        match op().await {
            Err(taskboard_sync_nextcloud::DeckError::Server(_)) if attempts < 4 => {
                attempts += 1;
                tokio::time::sleep(std::time::Duration::from_millis(200 * u64::from(attempts)))
                    .await;
            }
            other => return other,
        }
    }
}

/// Create a fresh run-scoped board with one stack on the live server.
pub async fn create_board_and_stack(
    client: &DeckClient,
    run: &str,
) -> (
    taskboard_sync_nextcloud::Board,
    taskboard_sync_nextcloud::Stack,
) {
    use taskboard_sync_nextcloud::DeckColor;
    let board = retry(|| async {
        client
            .create_board(run, &DeckColor::from_hex("00c2e0").expect("hex"))
            .await
    })
    .await
    .expect("board creation must succeed");
    let stack = retry(|| async {
        client
            .create_stack(board.id, &format!("{run}-col"), 0)
            .await
    })
    .await
    .expect("stack creation must succeed");
    (board, stack)
}

/// Seed the local database with a board binding for the remote board
/// through the storage crate's public API (tests that must not go
/// through the binding ceremony itself).
pub async fn seed_board_binding(db_path: &Path, local_id: u128, remote_id: u64, run: &str) {
    let repo = std::sync::Arc::new(
        taskboard_storage_sqlite::open(db_path)
            .await
            .expect("seed db"),
    );
    let (handle, join) = taskboard_storage_sqlite::spawn_storage_actor(repo);
    handle
        .apply(vec![taskboard_domain::PersistenceAction::UpsertBoard(
            taskboard_domain::Board {
                id: taskboard_domain::BoardId::from_uuid(uuid::Uuid::from_u128(local_id)),
                remote: Some(taskboard_domain::RemoteBoardId(remote_id)),
                title: run.to_owned(),
                color: taskboard_domain::Color::new("00c2e0"),
                archived: false,
                deleted: false,
                clocks: taskboard_domain::BoardClocks {
                    title: taskboard_domain::BoardClocks::epoch(),
                },
                remote_seen: None,
            },
        )])
        .await
        .expect("seed applies");
    drop(handle);
    join.await.expect("seed actor exits");
}

/// Env-store config pointed at the live server and the test db.
pub fn live_config(cfg: &LiveCfg, db_path: &Path) -> taskboard_app::config::AppConfig {
    use taskboard_app::config::{AppConfig, CredentialStoreKind};
    let mut config = AppConfig::default();
    config.nextcloud.server_url = Some(cfg.url.clone());
    config.nextcloud.username = Some(cfg.user.clone());
    config.nextcloud.credential_store = CredentialStoreKind::Env;
    config.storage.db_path = Some(db_path.to_owned());
    config
}

/// A `taskboard` binary command with per-child env hygiene: every
/// `TASKBOARD_*` variable of the *test* process is removed so children
/// cannot inherit foreign config; the caller re-sets exactly what the
/// child needs (`--config` names the config file; the password, when
/// used, travels via `TASKBOARD_APP_PASSWORD`).
#[must_use]
pub fn binary_command() -> std::process::Command {
    let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_taskboard"));
    for (key, _) in std::env::vars_os() {
        let is_taskboard = key.as_encoded_bytes().starts_with(b"TASKBOARD_");
        if is_taskboard {
            cmd.env_remove(&key);
        }
    }
    cmd
}

/// Writes a minimal env-store config file for binary-driving tests:
/// fast polling, bounded shutdown, per-test db. `server_url`/`user`
/// are omitted for `login` (which writes them).
pub fn write_test_config(
    path: &Path,
    server_url: Option<&str>,
    user: Option<&str>,
) -> std::io::Result<()> {
    let db = path.parent().unwrap_or(Path::new(".")).join("taskboard.db");
    let mut toml = String::from("[nextcloud]\ncredential_store = 'env'\n");
    if let Some(url) = server_url {
        toml.push_str("server_url = '");
        toml.push_str(url);
        toml.push_str("'\n");
    }
    if let Some(user) = user {
        toml.push_str("username = '");
        toml.push_str(user);
        toml.push_str("'\n");
    }
    let _ = write!(toml, "[storage]\ndb_path = '{}'\n", db.display());
    toml.push_str(
        "[sync]\npoll_interval_secs = 1\nbackoff_initial_secs = 1\nbackoff_max_secs = 5\n",
    );
    toml.push_str("[app]\nshutdown_timeout_secs = 5\n");
    std::fs::write(path, toml)
}

use taskboard_sync_nextcloud::DeckClient;
