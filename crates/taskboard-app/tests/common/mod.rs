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
use std::time::{Duration, Instant};

use taskboard_app::secrets::{AppPassword, CredentialError, CredentialStore};

/// A per-test tempdir (fails the test, never silently reuses).
#[must_use]
pub fn tempdir() -> tempfile::TempDir {
    tempfile::tempdir().expect("tempdir")
}

/// A scripted [`taskboard_app::commands::connect::PromptReader`] (fakes
/// over mocks): answers are consumed strictly in order, so a test
/// proves the prompt order by exhausting the queue. The line/secret
/// queue and the confirm queue are independent, so "no confirm was
/// asked" is an exhaustion assertion.
#[derive(Debug, Default)]
pub struct FakePrompts {
    answers: Mutex<std::collections::VecDeque<String>>,
    confirms: Mutex<std::collections::VecDeque<bool>>,
}

impl FakePrompts {
    #[must_use]
    pub fn new(answers: &[&str]) -> Self {
        Self {
            answers: Mutex::new(answers.iter().map(|s| (*s).to_owned()).collect()),
            confirms: Mutex::new(std::collections::VecDeque::new()),
        }
    }

    /// Scripted y/N decisions, consumed in order by `read_confirm`.
    #[must_use]
    pub fn with_confirms(answers: &[&str], confirms: &[bool]) -> Self {
        Self {
            answers: Mutex::new(answers.iter().map(|s| (*s).to_owned()).collect()),
            confirms: Mutex::new(confirms.iter().copied().collect()),
        }
    }

    /// The confirmations left unconsumed (a non-empty queue after a run
    /// proves no confirm was asked).
    #[must_use]
    pub fn remaining_confirms(&self) -> Vec<bool> {
        self.confirms
            .lock()
            .expect("fake prompts lock")
            .iter()
            .copied()
            .collect()
    }
}

impl taskboard_app::commands::connect::PromptReader for FakePrompts {
    fn read_line(
        &mut self,
        _prompt: &str,
    ) -> impl std::future::Future<Output = Result<String, std::io::Error>> + Send {
        std::future::ready(
            self.answers
                .lock()
                .expect("fake prompts lock")
                .pop_front()
                .ok_or_else(|| std::io::Error::other("no scripted answer")),
        )
    }

    async fn read_secret(&mut self, prompt: &str) -> Result<String, std::io::Error> {
        self.read_line(prompt).await
    }

    fn read_confirm(
        &mut self,
        _prompt: &str,
    ) -> impl std::future::Future<Output = Result<bool, std::io::Error>> + Send {
        // An empty confirm queue models the non-TTY decline: the guard
        // refuses typed without consuming anything (visible through
        // `remaining_confirms`).
        std::future::ready(Ok(self
            .confirms
            .lock()
            .expect("fake prompts lock")
            .pop_front()
            .unwrap_or(false)))
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
        taskboard_storage_sqlite::open(db_path, taskboard_domain::ConnectorId::from("nextcloud"))
            .await
            .expect("seed db"),
    );
    let (handle, join) = taskboard_storage_sqlite::spawn_storage_actor(repo);
    handle
        .apply(vec![taskboard_domain::PersistenceAction::UpsertBoard(
            taskboard_domain::Board {
                id: taskboard_domain::BoardId::from_uuid(uuid::Uuid::from_u128(local_id)),
                remote: taskboard_domain::test_support::test_binding(
                    taskboard_domain::RemoteBoardId(remote_id),
                ),
                title: run.to_owned(),
                color: taskboard_domain::Color::new("00c2e0"),
                archived: false,
                deleted: false,
                clocks: taskboard_domain::BoardClocks {
                    title: taskboard_domain::BoardClocks::epoch(),
                    color: taskboard_domain::BoardClocks::epoch(),
                    archived: taskboard_domain::BoardClocks::epoch(),
                    deleted: taskboard_domain::BoardClocks::epoch(),
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

/// Polls `pred` with a real-time deadline (process synchronization —
/// the hermetic suites' tokio-clock rules do not reach subprocesses).
/// The wait yields to the runtime (`tokio::time::sleep`), keeping the
/// async test body free of blocking calls.
pub async fn wait_until(mut pred: impl FnMut() -> bool, what: &str) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !pred() {
        assert!(Instant::now() < deadline, "{what} did not happen in time");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Kills the child on drop so a failing assertion cannot leak a
/// lock-holding daemon into the next test.
pub struct Child(pub std::process::Child);

impl Drop for Child {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Spawns `taskboard daemon` against `config` (output swallowed; a full
/// pipe would deadlock a chatty daemon). The password variable is
/// always scrubbed from the child first: presence/absence is each
/// test's explicit choice, never the invoking shell's (an inherited
/// export would flip the credential-miss arm into a spurious attach).
pub fn spawn_daemon(config: &Path, password: Option<&str>) -> Child {
    let mut command = binary_command();
    command
        .args(["--config"])
        .arg(config.as_os_str())
        .arg("daemon")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .env_remove("TASKBOARD_APP_PASSWORD");
    if let Some(password) = password {
        command.env("TASKBOARD_APP_PASSWORD", password);
    }
    Child(command.spawn().expect("spawn daemon"))
}

use taskboard_sync_nextcloud::DeckClient;

/// Fixture handles of the v0.1.x-shaped database (stable ids for
/// assertions).
pub(crate) struct V01xIds {
    pub board: uuid::Uuid,
    pub stack: uuid::Uuid,
    pub task: uuid::Uuid,
    pub op: uuid::Uuid,
}

/// Builds a v0.1.x-shaped database (schema at migrations ≤ 0005): one
/// bound board (`remote_board_id`, no `connector` column), one bound
/// stack + one bound task carrying epoch-default clocks (0005's world:
/// remote wins every LWW comparison), one queued `UpdateTask` op, and a
/// non-idle `sync_status` singleton.
///
/// Method (the storage crate's own migration-fixture pattern): run the
/// shipped migrations, then strip 0006's effects (the column, the rebuilt
/// table, the bookkeeping row) — leaving exactly the pre-0006 shape.
#[allow(clippy::too_many_lines)] // one normative fixture; the seeded rows ARE the shape
pub(crate) async fn write_v01x_db(
    db_path: &Path,
    remote_board_id: u64,
    remote_stack_id: u64,
    remote_card_id: u64,
    task_title: &str,
) -> V01xIds {
    const EPOCH: &str = "0001-01-01T00:00:00+00:00";
    #[allow(clippy::redundant_clone)] // the vec![] bind lists read uniformly
    let raw = sqlx::SqlitePool::connect(&format!("sqlite://{}?mode=rwc", db_path.display()))
        .await
        .expect("raw connect");
    sqlx::migrate!("../taskboard-storage-sqlite/migrations")
        .run(&raw)
        .await
        .expect("migrate");
    for (sql, why) in [
        (
            "ALTER TABLE boards DROP COLUMN connector",
            "strip the tag column",
        ),
        ("DROP TABLE sync_status", "strip the rebuilt table"),
        (
            "CREATE TABLE sync_status (id INTEGER PRIMARY KEY CHECK (id = 1), \
             phase TEXT NOT NULL, last_error TEXT NULL, last_success TEXT NULL)",
            "restore the 0005 singleton",
        ),
        (
            "INSERT INTO sync_status (id, phase) VALUES (1, 'idle')",
            "the singleton seed",
        ),
        (
            "DELETE FROM _sqlx_migrations WHERE version = 6",
            "strip the bookkeeping",
        ),
    ] {
        sqlx::query(sql)
            .execute(&raw)
            .await
            .unwrap_or_else(|err| panic!("{why} must apply: {err}"));
    }

    let ids = V01xIds {
        board: uuid::Uuid::from_u128(0x00DA_0000_0000_0000_0000_0000_0000_0001),
        stack: uuid::Uuid::from_u128(0x00DA_0000_0000_0000_0000_0000_0000_0002),
        task: uuid::Uuid::from_u128(0x00DA_0000_0000_0000_0000_0000_0000_0003),
        op: uuid::Uuid::from_u128(0x00DA_0000_0000_0000_0000_0000_0000_0004),
    };
    let board_text = ids.board.to_string();
    let stack_text = ids.stack.to_string();
    let task_text = ids.task.to_string();
    for (sql, binds, why) in [
        (
            "INSERT INTO boards (id, remote_board_id, title, color, archived, deleted, \
             ck_title, ck_color, ck_archived, ck_deleted, remote_seen) \
             VALUES (?, ?, 'old title', '0000ff', 0, 0, ?, ?, ?, ?, NULL)",
            vec![
                board_text.clone(),
                remote_board_id.to_string(),
                EPOCH.into(),
                EPOCH.into(),
                EPOCH.into(),
                EPOCH.into(),
            ],
            "board row",
        ),
        (
            "INSERT INTO stacks (id, board, remote_board_id, remote_stack_id, title, sort_order, \
             archived, deleted, ck_title, ck_sort_order, ck_deleted, remote_seen) \
             VALUES (?, ?, ?, ?, 'old col', 0, 0, 0, ?, ?, ?, NULL)",
            vec![
                stack_text.clone(),
                board_text.clone(),
                remote_board_id.to_string(),
                remote_stack_id.to_string(),
                EPOCH.into(),
                EPOCH.into(),
                EPOCH.into(),
            ],
            "stack row",
        ),
        (
            "INSERT INTO tasks (id, stack, remote_board_id, remote_stack_id, remote_card_id, \
             title, description, duedate, done, sort_order, archived, deleted, \
             ck_title, ck_description, ck_duedate, ck_done, ck_position, ck_labels, \
             ck_archived, ck_deleted, remote_seen) \
             VALUES (?, ?, ?, ?, ?, ?, '', NULL, NULL, 0, 0, 0, ?, ?, ?, ?, ?, ?, ?, ?, NULL)",
            vec![
                task_text.clone(),
                stack_text.clone(),
                remote_board_id.to_string(),
                remote_stack_id.to_string(),
                remote_card_id.to_string(),
                task_title.to_owned(),
                EPOCH.into(),
                EPOCH.into(),
                EPOCH.into(),
                EPOCH.into(),
                EPOCH.into(),
                EPOCH.into(),
                EPOCH.into(),
                EPOCH.into(),
            ],
            "task row",
        ),
        (
            "INSERT INTO outbox (op_id, op_kind, task_id, queued_at) \
             VALUES (?, 'update_task', ?, '2026-01-01T00:00:00+00:00')",
            vec![ids.op.to_string(), task_text],
            "queued op",
        ),
        (
            "UPDATE sync_status SET phase = 'syncing'",
            vec![],
            "non-idle status",
        ),
    ] {
        let mut query = sqlx::query(sql);
        for bind in binds {
            query = query.bind(bind);
        }
        query
            .execute(&raw)
            .await
            .unwrap_or_else(|err| panic!("{why} must insert: {err}"));
    }
    raw.close().await;
    ids
}

// ---- the two-connector tier (chunk 5) ----------------------------------

/// One test-owned throwaway Nextcloud instance: brought up through the
/// harness script (the same compose file, Deck install, user, and
/// app-password flow as the shared instance — suffixed project, port,
/// and export names), stopped/started and torn down by the owning test.
/// Owning both instances is what makes `docker stop` mid-test safe: the
/// sibling tier tests keep the shared singleton untouched.
#[derive(Debug, Clone)]
pub struct OwnedInstance {
    pub url: String,
    pub user: String,
    pub token: String,
    /// The compose project (`taskboard-it<suffix>`) — the `down` target.
    pub suffix: String,
}

/// Brings up one instance via `scripts/nextcloud_it_setup.sh up`
/// (blocking; call from `spawn_blocking`). The export block on stdout is
/// parsed into the instance's credentials.
pub fn spawn_instance_sync(suffix: &str, port: u16) -> Result<OwnedInstance, String> {
    let script = format!(
        "{}/../../scripts/nextcloud_it_setup.sh",
        env!("CARGO_MANIFEST_DIR")
    );
    // A previous run's panic can have leaked the project (its cleanup
    // never ran); a stale stopped container poisons this run — always
    // start from a clean project.
    teardown_instance_sync(suffix);
    let output = std::process::Command::new(&script)
        .arg("up")
        .env("TASKBOARD_IT_SUFFIX", suffix)
        .env("TASKBOARD_IT_PORT", port.to_string())
        .env("EXPORT_PREFIX", "TASKBOARD_IT_OWNED")
        .output()
        .map_err(|err| format!("harness script failed to run: {err:?}"))?;
    if !output.status.success() {
        return Err(format!(
            "instance {suffix} did not come up: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let mut url = None;
    let mut user = None;
    let mut token = None;
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let Some(line) = line.strip_prefix("export ") else {
            continue;
        };
        if let Some(value) = line.strip_prefix("TASKBOARD_IT_OWNED_URL=") {
            url = Some(value.to_owned());
        }
        if let Some(value) = line.strip_prefix("TASKBOARD_IT_OWNED_USER=") {
            user = Some(value.to_owned());
        }
        if let Some(value) = line.strip_prefix("TASKBOARD_IT_OWNED_TOKEN=") {
            token = Some(value.to_owned());
        }
    }
    match (url, user, token) {
        (Some(url), Some(user), Some(token)) => Ok(OwnedInstance {
            url,
            user,
            token,
            suffix: suffix.to_owned(),
        }),
        other => Err(format!("harness export block incomplete: {other:?}")),
    }
}

/// Tears one instance down (volumes included; throwaway by contract).
/// Belt-and-braces: a broken project network can make `compose down`
/// fail partway and leave the container running (a zombie poisons the
/// next run), so the container is force-removed afterwards regardless.
pub fn teardown_instance_sync(suffix: &str) {
    let script = format!(
        "{}/../../scripts/nextcloud_it_setup.sh",
        env!("CARGO_MANIFEST_DIR")
    );
    let _ = std::process::Command::new(&script)
        .arg("down")
        .env("TASKBOARD_IT_SUFFIX", suffix)
        .output();
    let _ = std::process::Command::new("docker")
        .args(["rm", "-f", &format!("taskboard-it-nextcloud{suffix}")])
        .output();
}

/// A `docker` CLI invocation (`stop`/`start`); the two-connector test's
/// outage orchestration. Sync; call from `spawn_blocking`.
pub fn docker_cli(args: &[&str]) -> Result<(), String> {
    let output = std::process::Command::new("docker")
        .args(args)
        .output()
        .map_err(|err| format!("docker {args:?} failed to run: {err:?}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "docker {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

/// A test-owned keyed credential store: two `(server, username)` pairs,
/// one app password each — `EnvCredentialStore` is single-secret by
/// construction, and the two-connector world needs N identities.
#[derive(Debug, Default)]
pub struct KeyedFixedStore {
    pub passwords: std::sync::Mutex<std::collections::HashMap<(String, String), String>>,
}

impl KeyedFixedStore {
    pub fn with(server: &str, user: &str, token: &str) -> Self {
        let mut passwords = std::collections::HashMap::new();
        passwords.insert((server.to_owned(), user.to_owned()), token.to_owned());
        Self {
            passwords: std::sync::Mutex::new(passwords),
        }
    }

    pub fn add(&mut self, server: &str, user: &str, token: &str) {
        self.passwords
            .lock()
            .expect("store lock")
            .insert((server.to_owned(), user.to_owned()), token.to_owned());
    }
}

impl taskboard_app::secrets::CredentialStore for KeyedFixedStore {
    fn load(
        &self,
        server: &str,
        username: &str,
    ) -> Result<taskboard_app::secrets::AppPassword, taskboard_app::secrets::CredentialError> {
        self.passwords
            .lock()
            .expect("store lock")
            .get(&(server.to_owned(), username.to_owned()))
            .map(|token| taskboard_app::secrets::AppPassword::new(token.clone()))
            .ok_or(taskboard_app::secrets::CredentialError::NotFound)
    }

    fn store(
        &self,
        server: &str,
        username: &str,
        secret: taskboard_app::secrets::AppPassword,
    ) -> Result<(), taskboard_app::secrets::CredentialError> {
        self.passwords.lock().expect("store lock").insert(
            (server.to_owned(), username.to_owned()),
            secret.expose().to_owned(),
        );
        Ok(())
    }

    fn delete(
        &self,
        server: &str,
        username: &str,
    ) -> Result<(), taskboard_app::secrets::CredentialError> {
        self.passwords
            .lock()
            .expect("store lock")
            .remove(&(server.to_owned(), username.to_owned()))
            .map(|_| ())
            .ok_or(taskboard_app::secrets::CredentialError::NotFound)
    }
}

/// Seeds one board binding with an explicit connector tag through the
/// storage API (current schema, no rollback — the per-board *tagged*
/// fixture the adoption path cannot mint: `BindBoard` tags the engine's
/// default connector only).
pub async fn seed_tagged_board_binding(
    db_path: &Path,
    local_id: u128,
    remote_id: u64,
    connector: &str,
    run: &str,
) {
    let repo = std::sync::Arc::new(
        taskboard_storage_sqlite::open(db_path, taskboard_domain::ConnectorId::from(connector))
            .await
            .expect("seed db"),
    );
    let (handle, join) = taskboard_storage_sqlite::spawn_storage_actor(repo);
    handle
        .apply(vec![taskboard_domain::PersistenceAction::UpsertBoard(
            taskboard_domain::Board {
                id: taskboard_domain::BoardId::from_uuid(uuid::Uuid::from_u128(local_id)),
                remote: Some(taskboard_domain::RemoteBoardBinding {
                    connector: taskboard_domain::ConnectorId::from(connector),
                    board: taskboard_domain::RemoteBoardId(remote_id),
                }),
                title: run.to_owned(),
                color: taskboard_domain::Color::new("00c2e0"),
                archived: false,
                deleted: false,
                clocks: taskboard_domain::BoardClocks {
                    title: taskboard_domain::BoardClocks::epoch(),
                    color: taskboard_domain::BoardClocks::epoch(),
                    archived: taskboard_domain::BoardClocks::epoch(),
                    deleted: taskboard_domain::BoardClocks::epoch(),
                },
                remote_seen: None,
            },
        )])
        .await
        .expect("seed applies");
    drop(handle);
    join.await.expect("seed actor exits");
}
