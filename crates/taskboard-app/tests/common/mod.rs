// SPDX-License-Identifier: AGPL-3.0-only
//! Fixtures shared by the app crate's integration suites: the fake
//! credential store, a minimal wiremock Deck harness (the sync crate's
//! harnesses are test-internal and not importable), and an env mutex.
//! Each test binary uses a different subset, hence the module-level
//! `dead_code` allowance.

#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use taskboard_app::secrets::{AppPassword, CredentialError, CredentialStore};

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
