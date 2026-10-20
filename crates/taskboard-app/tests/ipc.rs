// SPDX-License-Identifier: AGPL-3.0-only
//! IP-series: hermetic in-process daemon-IPC journeys (WI-5). One test
//! boots a standalone core through `bootstrap`, serves it with
//! `ipc::server::spawn_server`, and drives it with in-process
//! `ipc::client` connections. Structural asserts only — exit codes,
//! parsed JSON envelopes, and follow-up reads; rendered text is never
//! asserted (AGENTS §5). The binary-level journeys (real daemon
//! process, real locks, signals) are the `cli_ipc` suite's job.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;

mod common;

use common::tempdir;

use interprocess::local_socket::traits::tokio::Stream as _;
use taskboard_app::commands::BasicSession;
use taskboard_app::config::AppConfig;
use taskboard_app::ipc::{client, codec, protocol, server};
use taskboard_app::secrets::{AppPassword, CredentialStore, EnvCredentialStore};

/// A standalone-backend config over a per-test db (the default config
/// resolves `standalone` — no server URL — and the credential store is
/// never consulted).
fn standalone_config(db: &Path) -> AppConfig {
    AppConfig {
        storage: taskboard_app::config::StorageSection {
            db_path: Some(db.to_path_buf()),
        },
        ..AppConfig::default()
    }
}

/// Boots a standalone core and serves it; returns (handle, db path).
/// The store is shared between the bootstrap and the server session,
/// exactly like the daemon lifecycle wires it.
async fn served(dir: &Path, db_name: &str) -> (server::IpcServerHandle, PathBuf) {
    serve_with_config(dir, db_name, standalone_config(&dir.join(db_name))).await
}

/// Serves a caller-supplied config (the wiremock row boots a
/// connector-backed daemon through this).
async fn serve_with_config(
    dir: &Path,
    db_name: &str,
    config: AppConfig,
) -> (server::IpcServerHandle, PathBuf) {
    let db = dir.join(db_name);
    let store: Arc<dyn CredentialStore> = Arc::new(EnvCredentialStore);
    let app = Arc::new(
        taskboard_app::bootstrap::bootstrap(
            config,
            store.clone(),
            Arc::new(taskboard_app::notifications::LogSink),
        )
        .await
        .expect("core boots"),
    );
    let session = Arc::new(BasicSession {
        config: app.config.clone(),
        store,
        config_path: None,
    });
    let handle = server::spawn_server(app, session, &db)
        .await
        .expect("server binds");
    (handle, db)
}

/// Splits a shell-free argv (test titles carry no spaces here).
fn argv(words: &str) -> Vec<String> {
    words.split_whitespace().map(str::to_owned).collect()
}

/// One exec over a fresh connection (a connection serves one request).
async fn exec_once(db: &Path, words: &[String]) -> client::ExecOutcome {
    let conn = client::try_connect(db).await.expect("daemon answers");
    conn.exec(words).await.expect("exec")
}

/// Every journey runs under a hard deadline: a hung server fails the
/// test instead of wedging CI.
async fn bounded<T>(body: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(common::LIVE_DEADLINE, body)
        .await
        .expect("journey completes within the deadline")
}

/// The drain budget handed to every test's `join` — comfortably above
/// the hermetic server's millisecond drains, far under the deadline.
const DRAIN_BUDGET: std::time::Duration = std::time::Duration::from_secs(5);

/// Parses a newline-separated JSON stream (one value per line).
fn json_lines(bytes: &[u8]) -> Vec<serde_json::Value> {
    String::from_utf8_lossy(bytes)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("json line"))
        .collect()
}

/// IP1: the happy path — boards create → stacks add → tasks add →
/// `tasks list --json` over one server, then a durability re-read from
/// a second client.
#[tokio::test]
async fn ip1_core_verbs_serve_and_persist_over_ipc() {
    bounded(async {
        let dir = tempdir();
        let (handle, db) = served(dir.path(), "ip1.db").await;

        for command in ["boards create Personal", "stacks add Todo"] {
            let outcome = exec_once(&db, &argv(command)).await;
            assert_eq!(outcome.code, 0, "{command}: {}", outcome.err);
        }
        let outcome = exec_once(
            &db,
            &[
                "tasks".to_owned(),
                "add".to_owned(),
                "first task".to_owned(),
            ],
        )
        .await;
        assert_eq!(outcome.code, 0);

        // A second client reads the durable state.
        let outcome = exec_once(&db, &argv("tasks list --json")).await;
        assert_eq!(outcome.code, 0);
        let listing = &json_lines(outcome.out.as_bytes())[0];
        let titles: Vec<&str> = listing["stacks"][0]["tasks"]
            .as_array()
            .expect("tasks array")
            .iter()
            .map(|task| task["title"].as_str().expect("title"))
            .collect();
        assert_eq!(titles, ["first task"]);

        handle.join(DRAIN_BUDGET).await;
    })
    .await;
}

/// IP2: a typed handler failure crosses the wire as code 1 plus the
/// error envelope (parsed; the `kind` is asserted, never the text).
#[tokio::test]
async fn ip2_typed_failures_cross_the_wire_as_envelopes() {
    bounded(async {
        let dir = tempdir();
        let (handle, db) = served(dir.path(), "ip2.db").await;

        for command in [
            "boards create Personal",
            "stacks add Todo",
            "tasks add real",
        ] {
            let outcome = exec_once(&db, &argv(command)).await;
            assert_eq!(outcome.code, 0, "{command}");
        }
        let outcome = exec_once(&db, &argv("--json tasks done ghost")).await;
        assert_eq!(outcome.code, 1);
        let envelope = &json_lines(outcome.err.as_bytes())[0];
        assert_eq!(envelope["error"]["kind"], "unknown-task");

        handle.join(DRAIN_BUDGET).await;
    })
    .await;
}

/// IP3: the clap surface is served by the daemon — garbage verbs are
/// code 2 on `err`, `--help` is code 0 on `out`.
#[tokio::test]
async fn ip3_clap_surface_routes_help_and_usage_errors() {
    bounded(async {
        let dir = tempdir();
        let (handle, db) = served(dir.path(), "ip3.db").await;

        let outcome = exec_once(&db, &argv("frobnicate")).await;
        assert_eq!(outcome.code, 2);
        assert_ne!(outcome.err, String::new(), "usage error on err");
        assert_eq!(outcome.out, "");

        let outcome = exec_once(&db, &argv("--help")).await;
        assert_eq!(outcome.code, 0);
        assert_ne!(outcome.out, String::new(), "help on out");
        assert_eq!(outcome.err, "");

        handle.join(DRAIN_BUDGET).await;
    })
    .await;
}

/// IP4: session-class refusals are code 2 and leave the server healthy.
#[tokio::test]
async fn ip4_refusals_are_usage_class_and_leave_the_session_healthy() {
    bounded(async {
        let dir = tempdir();
        let (handle, db) = served(dir.path(), "ip4.db").await;

        for refused in [
            "daemon",
            "daemon stop",
            "connect",
            "connect --server http://x --user alice",
            "login --server http://x --user alice",
            "logout",
            "config print",
            "tasks list --config other.toml",
        ] {
            let outcome = exec_once(&db, &argv(refused)).await;
            assert_eq!(outcome.code, 2, "{refused} must be refused");
        }

        // The server serves on after the refusals.
        let outcome = exec_once(&db, &argv("boards create Personal")).await;
        assert_eq!(outcome.code, 0);

        handle.join(DRAIN_BUDGET).await;
    })
    .await;
}

/// IP5: a hand-built request with a stale version gets the typed
/// `VersionMismatch` response, carrying the server's version.
#[tokio::test]
async fn ip5_version_mismatch_is_typed_and_parseable() {
    bounded(async {
        let dir = tempdir();
        let (handle, db) = served(dir.path(), "ip5.db").await;

        let name = taskboard_app::ipc::socket_name(&db).expect("socket name");
        let mut stream = interprocess::local_socket::tokio::Stream::connect(name)
            .await
            .expect("connect");
        let stale = serde_json::to_vec(&protocol::Request::Exec {
            v: protocol::PROTOCOL_VERSION - 1,
            argv: vec!["tasks".to_owned(), "list".to_owned()],
        })
        .expect("serialize");
        codec::write_frame(&mut stream, &stale, codec::MAX_REQUEST_BYTES)
            .await
            .expect("write");
        let answer = codec::read_frame(&mut stream, codec::MAX_RESPONSE_BYTES)
            .await
            .expect("read");
        let response: protocol::Response = serde_json::from_slice(&answer).expect("parse");
        assert_eq!(
            response,
            protocol::Response::VersionMismatch {
                server_v: protocol::PROTOCOL_VERSION
            }
        );
        drop(stream);

        handle.join(DRAIN_BUDGET).await;
    })
    .await;
}

/// IP6: the `Status` request answers live off the engine without
/// triggering a cycle — the phase stays `idle` through repeated reads.
#[tokio::test]
async fn ip6_status_answers_live_without_triggering_a_cycle() {
    bounded(async {
        let dir = tempdir();
        let (handle, db) = served(dir.path(), "ip6.db").await;

        for _ in 0..3 {
            let conn = client::try_connect(&db).await.expect("daemon answers");
            let view = conn.status().await.expect("status");
            assert!(view.daemon);
            assert_eq!(view.backend.as_deref(), Some("standalone"));
            let sync = view.sync.as_ref().expect("sync view");
            assert_eq!(sync.phase, "idle", "status must never trigger a cycle");
        }

        handle.join(DRAIN_BUDGET).await;
    })
    .await;
}

/// IP7: the `Shutdown` request acks, the drain completes, the join
/// finishes, and the unix socket artifact is removed. (Unix-only: the
/// socket *artifact* is a unix concept — Windows named pipes have no
/// filesystem sidecar to assert on.)
#[cfg(unix)]
#[tokio::test]
async fn ip7_shutdown_acks_drains_and_removes_the_socket() {
    bounded(async {
        let dir = tempdir();
        let (handle, db) = served(dir.path(), "ip7.db").await;
        let sidecar = taskboard_app::ipc::socket_sidecar(&db);

        let conn = client::try_connect(&db).await.expect("daemon answers");
        conn.shutdown().await.expect("shutdown ack");

        tokio::time::timeout(common::LIVE_DEADLINE, handle.stopped())
            .await
            .expect("stopped resolves");
        handle.join(DRAIN_BUDGET).await;
        assert!(
            !sidecar.exists(),
            "graceful shutdown removes the socket artifact"
        );
    })
    .await;
}

/// IP8: concurrent connections are accepted and both answered (the
/// dispatcher serializes execution); the state is consistent after.
#[tokio::test]
async fn ip8_concurrent_connections_serialize_and_both_answer() {
    bounded(async {
        let dir = tempdir();
        let (handle, db) = served(dir.path(), "ip8.db").await;

        for command in ["boards create Personal", "stacks add Todo"] {
            exec_once(&db, &argv(command)).await;
        }

        let c1 = client::try_connect(&db).await.expect("daemon answers");
        let c2 = client::try_connect(&db).await.expect("daemon answers");
        let one = argv("tasks add from-one");
        let two = argv("tasks add from-two");
        let (r1, r2) = tokio::join!(c1.exec(&one), c2.exec(&two));
        assert_eq!(r1.expect("exec one").code, 0);
        assert_eq!(r2.expect("exec two").code, 0);

        let outcome = exec_once(&db, &argv("tasks list --json")).await;
        let listing = &json_lines(outcome.out.as_bytes())[0];
        let titles: Vec<&str> = listing["stacks"][0]["tasks"]
            .as_array()
            .expect("tasks array")
            .iter()
            .map(|task| task["title"].as_str().expect("title"))
            .collect();
        assert_eq!(titles, ["from-one", "from-two"]);

        handle.join(DRAIN_BUDGET).await;
    })
    .await;
}

/// IP9: a stale socket artifact (a crashed daemon's file) is recovered
/// at bind time — the junk file is replaced by a serving listener.
/// (Unix-only: the stale *file* is the unix sidecar; Windows namespaced
/// names have no artifact that can go stale.)
#[cfg(unix)]
#[tokio::test]
async fn ip9_stale_socket_artifacts_are_recovered_at_bind() {
    bounded(async {
        let dir = tempdir();
        let db = dir.path().join("ip9.db");
        let sidecar = taskboard_app::ipc::socket_sidecar(&db);
        std::fs::write(&sidecar, b"junk from a crashed daemon").expect("junk file");

        let (handle, _) = served(dir.path(), "ip9.db").await;

        let outcome = exec_once(&db, &argv("boards create Personal")).await;
        assert_eq!(outcome.code, 0);

        handle.join(DRAIN_BUDGET).await;
        assert!(!sidecar.exists());
    })
    .await;
}

/// IP10: `try_connect` on a database nobody serves answers `None`
/// promptly (no daemon — the caller falls through to in-process).
#[tokio::test]
async fn ip10_no_daemon_answers_none() {
    bounded(async {
        let dir = tempdir();
        let db = dir.path().join("ip10.db");
        assert!(client::try_connect(&db).await.is_none());
    })
    .await;
}

/// IP11 (the wiremock row): a *connector*-backend daemon — wiremock
/// answering 401 on everything — boots, the boot cycle fails auth in
/// the background, and the socket still serves: `status` reports
/// `nextcloud` with the auth failure, and `tasks add` rides IPC
/// regardless of sync health.
#[tokio::test]
async fn ip11_connector_daemon_serves_even_when_sync_fails() {
    bounded(async {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .respond_with(wiremock::ResponseTemplate::new(401))
            .mount(&server)
            .await;

        let dir = tempdir();
        let db = dir.path().join("ip11.db");
        // A bound board: without one the cycle skips silently (NoBoard)
        // and never touches the server — the 401 needs a live pull.
        common::seed_board_binding(&db, 1, common::BOARD, "ip11-run").await;
        let mut config = common::valid_config(&server.uri());
        config.storage.db_path = Some(db.clone());
        let password = common::EnvOverride::set("TASKBOARD_APP_PASSWORD", "pw");
        let (handle, db) = serve_with_config(dir.path(), "ip11.db", config).await;

        // The boot cycle fails auth in the background; poll (with a
        // deadline) until it settles — status itself never triggers it.
        let view;
        let settle = std::time::Instant::now() + std::time::Duration::from_secs(30);
        loop {
            let current = client::try_connect(&db)
                .await
                .expect("daemon answers")
                .status()
                .await
                .expect("status");
            let failed = current
                .sync
                .as_ref()
                .is_some_and(|sync| sync.error_kind.as_deref() == Some("auth"));
            if failed {
                view = Some(current);
                break;
            }
            assert!(
                std::time::Instant::now() < settle,
                "the boot cycle never failed auth"
            );
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        let view = view.expect("settled view");
        assert_eq!(view.backend.as_deref(), Some("nextcloud"));
        assert_eq!(view.sync.as_ref().expect("sync view").phase, "failed");

        // IPC does not depend on sync health: mutations still serve.
        let outcome = exec_once(&db, &argv("stacks add Todo")).await;
        assert_eq!(outcome.code, 0, "{}", outcome.err);

        handle.join(DRAIN_BUDGET).await;
        drop(password);
    })
    .await;
}

/// IP12 (the WI-5 review regression): a connected-but-silent peer must
/// not hold the daemon's shutdown hostage — `join` completes within its
/// budget and the wedged connection is dropped, so the daemon's own
/// shutdown (engine flush, actor joins) always proceeds.
#[tokio::test]
async fn ip12_a_silent_connection_cannot_wedge_the_drain() {
    bounded(async {
        let dir = tempdir();
        let (handle, db) = served(dir.path(), "ip12.db").await;

        let name = taskboard_app::ipc::socket_name(&db).expect("socket name");
        let _silent = interprocess::local_socket::tokio::Stream::connect(name)
            .await
            .expect("connect");
        // No frame follows: the connection sits silent in its read, the
        // exact state that used to stall the drain forever.

        // A budget far under the journey deadline: the join must make
        // it anyway.
        tokio::time::timeout(
            common::LIVE_DEADLINE,
            handle.join(std::time::Duration::from_millis(300)),
        )
        .await
        .expect("join completes despite a silent connection");

        // The server is gone, but a fresh boot on the same db still
        // binds (the artifact was removed even on the aborted path).
        let (reborn, db) = served(dir.path(), "ip12.db").await;
        let outcome = exec_once(&db, &argv("boards create Personal")).await;
        assert_eq!(outcome.code, 0);
        reborn.join(DRAIN_BUDGET).await;
    })
    .await;
}

// ---------------------------------------------------------------------
// WI-H: the detach-connector verb
// ---------------------------------------------------------------------

/// The detach journeys' server: a *nextcloud*-backend daemon booted with
/// the shared fake store (boardless — cycles skip silently, no wiremock
/// needed) plus its server identity for the wire.
async fn served_nextcloud(
    dir: &Path,
    db_name: &str,
    server_url: &str,
) -> (
    server::IpcServerHandle,
    PathBuf,
    Arc<common::FakeCredentialStore>,
) {
    let mut config = standalone_config(&dir.join(db_name));
    config.nextcloud.server_url = Some(server_url.to_owned());
    config.nextcloud.username = Some("alice".to_owned());
    config.nextcloud.credential_store = taskboard_app::config::CredentialStoreKind::Keyring;
    let db = dir.join(db_name);
    let store: Arc<common::FakeCredentialStore> =
        Arc::new(common::FakeCredentialStore::with(server_url, "alice", "pw"));
    let app = Arc::new(
        taskboard_app::bootstrap::bootstrap(
            config,
            store.clone(),
            Arc::new(taskboard_app::notifications::LogSink),
        )
        .await
        .expect("core boots"),
    );
    let session = Arc::new(BasicSession {
        config: app.config.clone(),
        store: store.clone(),
        config_path: None,
    });
    let handle = server::spawn_server(app, session, &db)
        .await
        .expect("server binds");
    (handle, db, store)
}

/// One detach request over a fresh connection.
async fn detach_once(db: &Path, server_url: &str, username: &str) -> protocol::DetachWireOutcome {
    let conn = client::try_connect(db).await.expect("daemon answers");
    conn.detach(server_url, username).await.expect("detach")
}

/// IP13: the logout nudge's happy path — `detach` answers `detached`,
/// `status` flips `connector_attached` to `false` (backend stays
/// `nextcloud`), a relayed `--json sync` refuses `sync-unavailable`
/// (code 1), and local mutations still serve (code 0). A second detach
/// is idempotent: `no-connector`.
#[tokio::test]
async fn ip13_detach_drops_the_connector_and_the_daemon_still_serves() {
    bounded(async {
        let dir = tempdir();
        let (handle, db, _store) =
            served_nextcloud(dir.path(), "ip13.db", "https://cloud.example.com").await;

        // Before: attached.
        let view = client::try_connect(&db)
            .await
            .expect("daemon answers")
            .status()
            .await
            .expect("status");
        assert_eq!(view.connector_attached, Some(true));

        assert_eq!(
            detach_once(&db, "https://cloud.example.com", "alice").await,
            protocol::DetachWireOutcome::Detached
        );

        let view = client::try_connect(&db)
            .await
            .expect("daemon answers")
            .status()
            .await
            .expect("status");
        assert_eq!(view.backend.as_deref(), Some("nextcloud"));
        assert_eq!(view.connector_attached, Some(false));

        // The relayed sync verb refuses typed (code 1, envelope kind).
        let outcome = exec_once(&db, &argv("--json sync")).await;
        assert_eq!(outcome.code, 1);
        let envelope = &json_lines(outcome.err.as_bytes())[0];
        assert_eq!(envelope["error"]["kind"], "sync-unavailable");

        // Local editing still serves connector-less.
        let outcome = exec_once(&db, &argv("boards create Personal")).await;
        assert_eq!(outcome.code, 0, "{}", outcome.err);

        // Idempotent.
        assert_eq!(
            detach_once(&db, "https://cloud.example.com", "alice").await,
            protocol::DetachWireOutcome::NoConnector
        );

        handle.join(DRAIN_BUDGET).await;
    })
    .await;
}

/// IP14: a detach request carrying another identity is refused with
/// `identity-mismatch` — the daemon's connector (and its booted
/// credential) stays attached, evidence untouched.
#[tokio::test]
async fn ip14_a_foreign_identity_never_detaches() {
    bounded(async {
        let dir = tempdir();
        let (handle, db, store) =
            served_nextcloud(dir.path(), "ip14.db", "https://cloud.example.com").await;

        assert_eq!(
            detach_once(&db, "https://other.example.com", "alice").await,
            protocol::DetachWireOutcome::IdentityMismatch
        );
        assert_eq!(
            detach_once(&db, "https://cloud.example.com", "mallory").await,
            protocol::DetachWireOutcome::IdentityMismatch
        );

        let view = client::try_connect(&db)
            .await
            .expect("daemon answers")
            .status()
            .await
            .expect("status");
        assert_eq!(view.connector_attached, Some(true));
        assert!(
            store.load("https://cloud.example.com", "alice").is_ok(),
            "the booted credential is untouched by a mismatched request"
        );

        handle.join(DRAIN_BUDGET).await;
    })
    .await;
}

/// IP15: a standalone daemon has no connector — the detach answers
/// `no-connector` regardless of the identity.
#[tokio::test]
async fn ip15_a_standalone_daemon_answers_no_connector() {
    bounded(async {
        let dir = tempdir();
        let (handle, db) = served(dir.path(), "ip15.db").await;

        assert_eq!(
            detach_once(&db, "https://cloud.example.com", "alice").await,
            protocol::DetachWireOutcome::NoConnector
        );

        handle.join(DRAIN_BUDGET).await;
    })
    .await;
}

// ---------------------------------------------------------------------
// P5: the daemon-side attach verb (attach from persisted state)
// ---------------------------------------------------------------------

use common::FakeCredentialStore;

/// The config file the attach tests re-read: boot standalone over the
/// test db; the tests rewrite the identity keys into it to simulate
/// what a `connect` ceremony just persisted.
fn config_file_body(db: &Path, server_url: Option<&str>, username: Option<&str>) -> String {
    let mut toml = format!("[storage]\ndb_path = '{}'\n", db.display());
    if let Some(server) = server_url {
        let _ = writeln!(toml, "\n[nextcloud]\nserver_url = '{server}'");
        if let Some(user) = username {
            let _ = writeln!(toml, "username = '{user}'");
        }
    }
    toml
}

/// Serves a daemon whose `BasicSession.config_path` is a real tempdir
/// file (the attach verb re-reads it — `None` would resolve against the
/// CWD/XDG and break hermeticity) and whose store is the shared fake.
async fn served_with_config_file(
    dir: &Path,
    db_name: &str,
) -> (
    server::IpcServerHandle,
    PathBuf,
    PathBuf,
    Arc<FakeCredentialStore>,
) {
    let db = dir.join(db_name);
    let config_path = dir.join(format!("{db_name}.toml"));
    // Boot standalone (no identity in the file — a standalone boot never
    // consults the store); the tests write the identity keys in to
    // simulate what a `connect` ceremony just persisted.
    std::fs::write(&config_path, config_file_body(&db, None, None)).expect("write config file");
    let config = AppConfig::load(Some(config_path.clone())).expect("config parses");
    // The secret a `connect` ceremony would have stored: the boot needs
    // it (the config names an identity), the attach re-reads it live.
    let store: Arc<FakeCredentialStore> = Arc::new(FakeCredentialStore::with(
        "https://cloud.example.com",
        "alice",
        "pw",
    ));
    let app = Arc::new(
        taskboard_app::bootstrap::bootstrap(
            config,
            store.clone(),
            Arc::new(taskboard_app::notifications::LogSink),
        )
        .await
        .expect("core boots"),
    );
    let session = Arc::new(BasicSession {
        config: app.config.clone(),
        store: store.clone(),
        config_path: Some(config_path.clone()),
    });
    let handle = server::spawn_server(app, session, &db)
        .await
        .expect("server binds");
    (handle, db, config_path, store)
}

/// One attach request over a fresh connection.
async fn attach_once(db: &Path, server_url: &str, username: &str) -> protocol::AttachWireOutcome {
    let conn = client::try_connect(db).await.expect("daemon answers");
    conn.attach(server_url, username).await.expect("attach")
}

/// The live status view over a fresh connection.
async fn status_once(db: &Path) -> taskboard_app::commands::status::StatusView {
    client::try_connect(db)
        .await
        .expect("daemon answers")
        .status()
        .await
        .expect("status")
}

/// IP16: the headline — a standalone-booted daemon attaches from
/// persisted state: `attached`, `status` reports the live flag under the
/// boot-shaped `standalone` backend (D7), a relayed `sync` serves the
/// attached connector (code 0 — not `sync-unavailable`), local edits
/// still serve, and the terminal-bound ceremony argv still refuses code
/// 2 over the Exec lane.
#[tokio::test]
async fn ip16_attach_from_persisted_state_attaches_and_serves() {
    bounded(async {
        let dir = tempdir();
        let (handle, db, config_path, store) = served_with_config_file(dir.path(), "ip16.db").await;

        // Before: standalone boot, connector-less, live flag present.
        let view = status_once(&db).await;
        assert_eq!(view.backend.as_deref(), Some("standalone"));
        assert_eq!(view.connector_attached, Some(false));

        // What a `connect` ceremony just persisted: identity into the
        // file, secret into the store.
        std::fs::write(
            &config_path,
            config_file_body(&db, Some("https://cloud.example.com"), Some("alice")),
        )
        .expect("persist identity");
        store
            .store(
                "https://cloud.example.com",
                "alice",
                AppPassword::new("pw".to_owned()),
            )
            .expect("store secret");

        assert_eq!(
            attach_once(&db, "https://cloud.example.com", "alice").await,
            protocol::AttachWireOutcome::Attached
        );

        // The live flag under the boot-shaped backend word.
        let view = status_once(&db).await;
        assert_eq!(view.backend.as_deref(), Some("standalone"));
        assert_eq!(view.connector_attached, Some(true));

        // The attached connector serves cycles: the verb gets past the
        // connector gate (the boot-frozen backend word would refuse
        // `sync-disabled`) and runs the real cycle — any *cycle*
        // outcome is fine (boardless skip, its own failure, or a
        // timeout under loaded runners); the refusal classes are the
        // pinned absence.
        let outcome = exec_once(&db, &argv("--json sync")).await;
        if outcome.code != 0 {
            let envelope = &json_lines(outcome.err.as_bytes())[0];
            let kind = envelope["error"]["kind"].as_str().expect("error kind");
            assert_ne!(
                kind, "sync-disabled",
                "the live connector must outrank the boot-frozen gate"
            );
            assert_ne!(
                kind, "sync-unavailable",
                "the attached connector must serve the cycle"
            );
        }

        // Local editing still serves.
        let outcome = exec_once(&db, &argv("boards create Personal")).await;
        assert_eq!(outcome.code, 0, "{}", outcome.err);

        // The ceremony verbs stay refused over the Exec lane.
        let outcome = exec_once(&db, &argv("connect")).await;
        assert_eq!(outcome.code, 2, "ceremony argv stays refused");

        // `boards use` is refused routing-level too (D6-6): the daemon
        // holds no interactive session to select a board for.
        let outcome = exec_once(&db, &argv("boards use")).await;
        assert_eq!(outcome.code, 2, "boards use stays refused over the wire");

        handle.join(DRAIN_BUDGET).await;
    })
    .await;
}

/// IP17: the swap-first rule — attaching again over a live connector
/// detaches and re-attaches (a rotated store entry is what the rebuilt
/// client loads), so secret rotation reaches the running daemon.
#[tokio::test]
async fn ip17_attach_swaps_a_live_connector() {
    bounded(async {
        let dir = tempdir();
        let (handle, db, config_path, store) = served_with_config_file(dir.path(), "ip17.db").await;
        std::fs::write(
            &config_path,
            config_file_body(&db, Some("https://cloud.example.com"), Some("alice")),
        )
        .expect("persist identity");
        store
            .store(
                "https://cloud.example.com",
                "alice",
                AppPassword::new("pw-one".to_owned()),
            )
            .expect("store secret");

        assert_eq!(
            attach_once(&db, "https://cloud.example.com", "alice").await,
            protocol::AttachWireOutcome::Attached
        );

        // Rotate the secret; the swap-first attach rebuilds the client
        // off the store's current entry.
        store
            .store(
                "https://cloud.example.com",
                "alice",
                AppPassword::new("pw-two".to_owned()),
            )
            .expect("rotate secret");
        assert_eq!(
            attach_once(&db, "https://cloud.example.com", "alice").await,
            protocol::AttachWireOutcome::Attached
        );
        let view = status_once(&db).await;
        assert_eq!(view.connector_attached, Some(true), "still attached");

        handle.join(DRAIN_BUDGET).await;
    })
    .await;
}

/// IP18: the typed refusals leave the daemon untouched — a foreign
/// identity answers `identity-mismatch`, a missing identity key
/// `not-configured`, a missing store entry `credential-unavailable`;
/// each leaves `connector_attached` unchanged.
#[tokio::test]
async fn ip18_attach_refusals_are_typed_and_leave_the_daemon_untouched() {
    bounded(async {
        let dir = tempdir();
        let (handle, db, config_path, store) = served_with_config_file(dir.path(), "ip18.db").await;
        std::fs::write(
            &config_path,
            config_file_body(&db, Some("https://cloud.example.com"), Some("alice")),
        )
        .expect("persist identity");
        store
            .store(
                "https://cloud.example.com",
                "alice",
                AppPassword::new("pw".to_owned()),
            )
            .expect("store secret");

        // Foreign identity: refused, nothing attached.
        assert_eq!(
            attach_once(&db, "https://other.example.com", "alice").await,
            protocol::AttachWireOutcome::IdentityMismatch
        );
        let before = status_once(&db).await.connector_attached;

        // Identity stripped from the file: the merged unconfigured
        // bucket.
        std::fs::write(&config_path, config_file_body(&db, None, None)).expect("strip identity");
        assert_eq!(
            attach_once(&db, "https://cloud.example.com", "alice").await,
            protocol::AttachWireOutcome::NotConfigured
        );

        // Identity restored, store entry removed: the credential miss.
        std::fs::write(
            &config_path,
            config_file_body(&db, Some("https://cloud.example.com"), Some("alice")),
        )
        .expect("persist identity");
        store
            .delete("https://cloud.example.com", "alice")
            .expect("remove secret");
        assert_eq!(
            attach_once(&db, "https://cloud.example.com", "alice").await,
            protocol::AttachWireOutcome::CredentialUnavailable
        );

        let after = status_once(&db).await.connector_attached;
        assert_eq!(before, after, "the refusals changed nothing");
        assert_eq!(after, Some(false));

        handle.join(DRAIN_BUDGET).await;
    })
    .await;
}

/// Boots a standalone core the way the daemon lifecycle does: this
/// scope keeps the daemon-side `Arc<App>` and a clone goes into
/// `spawn_server` — the exact two-ref composition of `main.rs`.
async fn served_with_core(
    dir: &Path,
    db_name: &str,
) -> (
    server::IpcServerHandle,
    Arc<taskboard_app::bootstrap::App>,
    PathBuf,
) {
    let db = dir.join(db_name);
    let store: Arc<dyn CredentialStore> = Arc::new(EnvCredentialStore);
    let app = Arc::new(
        taskboard_app::bootstrap::bootstrap(
            standalone_config(&db),
            store.clone(),
            Arc::new(taskboard_app::notifications::LogSink),
        )
        .await
        .expect("core boots"),
    );
    let session = Arc::new(BasicSession {
        config: app.config.clone(),
        store,
        config_path: None,
    });
    let handle = server::spawn_server(app.clone(), session, &db)
        .await
        .expect("server binds");
    (handle, app, db)
}

/// IP19 — the F9-01 regression: a stray second strong ref to the core
/// held across the server drain must turn the daemon-shutdown tail into
/// a typed failure, not a panic (the old `expect` died here).
#[tokio::test]
async fn ip19_a_stray_second_ref_across_join_is_a_typed_shutdown_failure() {
    bounded(async {
        let dir = tempdir();
        let (handle, app, _db) = served_with_core(dir.path(), "ip19.db").await;
        // The "future bug" stray: a clone a mis-planted feature would
        // leave behind, alive across the entire shutdown tail.
        let stray = app.clone();
        handle.join(DRAIN_BUDGET).await;
        let Err(shared) = taskboard_app::bootstrap::shutdown_from_arc(app).await else {
            panic!("a shared core must fail typed, not shut down");
        };
        // Advisory count, never exact: a floor only (AGENTS §5).
        assert!(shared.strong_refs >= 2);
        drop(stray);
    })
    .await;
}

/// IP20 — the happy-path seam: with no stray ref, the daemon's own
/// dispatcher ref is fully released by a real `join`, so the tail takes
/// exclusive ownership and the graceful sequence completes normally.
#[tokio::test]
async fn ip20_the_shutdown_tail_takes_exclusive_ownership_after_the_drain() {
    bounded(async {
        let dir = tempdir();
        let (handle, app, _db) = served_with_core(dir.path(), "ip20.db").await;
        handle.join(DRAIN_BUDGET).await;
        let outcome = taskboard_app::bootstrap::shutdown_from_arc(app).await;
        // Standalone idle boot: no sync actor to abort.
        assert_eq!(
            outcome,
            Ok(taskboard_app::bootstrap::ShutdownOutcome {
                actor_aborted: false
            })
        );
    })
    .await;
}
