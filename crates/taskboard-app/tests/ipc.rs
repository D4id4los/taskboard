// SPDX-License-Identifier: AGPL-3.0-only
//! IP-series: hermetic in-process daemon-IPC journeys (WI-5). One test
//! boots a standalone core through `bootstrap`, serves it with
//! `ipc::server::spawn_server`, and drives it with in-process
//! `ipc::client` connections. Structural asserts only — exit codes,
//! parsed JSON envelopes, and follow-up reads; rendered text is never
//! asserted (AGENTS §5). The binary-level journeys (real daemon
//! process, real locks, signals) are the `cli_ipc` suite's job.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use interprocess::local_socket::traits::tokio::Stream as _;
use taskboard_app::commands::BasicSession;
use taskboard_app::config::AppConfig;
use taskboard_app::ipc::{client, codec, protocol, server};
use taskboard_app::secrets::{CredentialStore, EnvCredentialStore};

mod common;

use common::tempdir;

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
        taskboard_app::bootstrap::bootstrap(config, store.clone())
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

        handle.join().await;
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

        handle.join().await;
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

        handle.join().await;
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
            "login",
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

        handle.join().await;
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

        handle.join().await;
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

        handle.join().await;
    })
    .await;
}

/// IP7: the `Shutdown` request acks, the drain completes, the join
/// finishes, and the unix socket artifact is removed.
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
        handle.join().await;
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

        handle.join().await;
    })
    .await;
}

/// IP9: a stale socket artifact (a crashed daemon's file) is recovered
/// at bind time — the junk file is replaced by a serving listener.
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

        handle.join().await;
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
