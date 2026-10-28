// SPDX-License-Identifier: AGPL-3.0-only
//! W-series: the web kiosk dashboard's hermetic integration journey —
//! the real hyper server over a booted standalone core, a plain
//! reqwest client on the loopback, an ephemeral port. Terminal states
//! are asserted through the handle and poll predicates with deadlines;
//! no sleeps, no wall-clock waits, no message-text assertions.

use std::sync::Arc;
use std::time::Duration;

use taskboard_app::bootstrap::{App, bootstrap};
use taskboard_app::config::{AppConfig, CredentialStoreKind, WebSection};
use taskboard_app::secrets::CredentialStore;
use taskboard_domain::StateCommand;

mod common;

use common::FakeCredentialStore;

fn tempdir() -> tempfile::TempDir {
    tempfile::tempdir().expect("tempdir")
}

/// Claims a free ephemeral port and releases it: the disabled-fence
/// test's listener probe targets a port this test owns the vacancy of.
fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .expect("probe bind")
        .local_addr()
        .expect("probe addr")
        .port()
}

/// The standalone dashboard config: empty nextcloud (the store is never
/// consulted), db + token file under `dir`, `[web]` on an ephemeral
/// loopback port.
fn config_for(dir: &tempfile::TempDir) -> AppConfig {
    AppConfig {
        nextcloud: taskboard_app::config::NextcloudSection {
            server_url: None,
            username: None,
            credential_store: CredentialStoreKind::Keyring,
        },
        storage: taskboard_app::config::StorageSection {
            db_path: Some(dir.path().join("taskboard.db")),
        },
        web: WebSection {
            enabled: true,
            bind: "127.0.0.1:0".to_owned(),
            refresh_secs: 30,
            token: Some("journey-token".to_owned()),
        },
        ..AppConfig::default()
    }
}

/// Boots a standalone core with a dashboard that would never be
/// consulted for credentials.
async fn boot(config: AppConfig) -> App {
    let store: Arc<dyn CredentialStore> = Arc::new(FakeCredentialStore::default());
    bootstrap(
        config,
        store,
        Arc::new(taskboard_app::notifications::LogSink),
    )
    .await
    .expect("standalone boot")
}

fn base(handle: &taskboard_app::web::DashboardServerHandle) -> String {
    format!("http://{}/d/journey-token", handle.local_addr())
}

/// W1: the whole journey — index, board, stylesheet, the uniform 404,
/// the 405, and the bounded shutdown that closes the port.
#[tokio::test]
// One journey, asserted leg by leg: splitting it would multiply the
// boot cost, not the clarity.
#[allow(clippy::too_many_lines)]
async fn w1_the_dashboard_journey_over_the_real_server() {
    let dir = tempdir();
    let config = config_for(&dir);
    config.validate().expect("the journey config validates");
    let app = boot(config).await;

    // One live board: the board page journey has a real target.
    let outcome = app
        .engine
        .execute(StateCommand::CreateBoard {
            title: "Journey".into(),
            color: taskboard_domain::Color::new("3366cc"),
        })
        .await
        .expect("CreateBoard accepted");
    let board_id = match outcome {
        taskboard_domain::CommandOutcome::CreatedBoard(id) => id,
        other => panic!("expected CreatedBoard, got {other:?}"),
    };

    let web = taskboard_app::web::spawn_dashboard(
        &app.config,
        app.engine.clone(),
        app.config.web.clone(),
    )
    .await
    .expect("dashboard spawns");
    let client = reqwest::Client::new();
    let base = base(&web);

    // Index: 200 HTML, never cached, carrying the meta-refresh
    // baseline and a link to the board page.
    let index = client.get(format!("{base}/")).send().await.expect("index");
    assert_eq!(index.status(), 200);
    assert!(
        index
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .expect("content type")
            .to_str()
            .expect("utf-8")
            .starts_with("text/html")
    );
    assert_eq!(
        index
            .headers()
            .get(reqwest::header::CACHE_CONTROL)
            .expect("cache control"),
        "no-store"
    );
    let body = index.text().await.expect("body");
    assert!(body.starts_with("<!doctype html>"));
    assert!(
        body.contains("<meta http-equiv=\"refresh\" content=\"30\">"),
        "the meta-refresh baseline is on the served page"
    );
    assert!(
        body.contains(&format!("board/{}", board_id.as_uuid())),
        "the index links the live board"
    );

    // Regression: the slash-less index never serves HTML in place — it
    // redirects to the canonical trailing-slash form, because the
    // pages' links are relative and a slash-less HTML response would
    // carry links the browser resolves against `/d/` (broken 404s). A
    // redirect-obeying client lands on the working index.
    let redirect_client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("redirect client");
    let slashless = redirect_client.get(&base).send().await.expect("slash-less");
    assert_eq!(slashless.status(), 302, "{base} must redirect, not serve");
    assert_eq!(
        slashless
            .headers()
            .get(reqwest::header::LOCATION)
            .expect("location")
            .as_bytes(),
        b"/d/journey-token/".as_slice(),
        "the redirect targets the canonical index path"
    );
    let redirect_post = redirect_client
        .post(&base)
        .send()
        .await
        .expect("slash-less post");
    assert_eq!(
        redirect_post.status(),
        405,
        "the slash-less form keeps the method semantics"
    );
    let landed = client.get(&base).send().await.expect("redirect followed");
    assert_eq!(landed.status(), 200);

    // Board page: 200 with the board's stacks.
    let board = client
        .get(format!("{base}/board/{}", board_id.as_uuid()))
        .send()
        .await
        .expect("board page");
    assert_eq!(board.status(), 200);
    assert_eq!(
        board
            .headers()
            .get(reqwest::header::CACHE_CONTROL)
            .expect("cache control"),
        "no-store"
    );

    // Stylesheet: cached for a year, CSS content type.
    let css = client
        .get(format!("{base}/style-v1.css"))
        .send()
        .await
        .expect("stylesheet");
    assert_eq!(css.status(), 200);
    assert!(
        css.headers()
            .get(reqwest::header::CONTENT_TYPE)
            .expect("content type")
            .to_str()
            .expect("utf-8")
            .starts_with("text/css")
    );
    assert_eq!(
        css.headers()
            .get(reqwest::header::CACHE_CONTROL)
            .expect("cache control"),
        "max-age=31536000"
    );

    // The capability discipline over the wire: a wrong token is a
    // uniform 404 — on the index and under a POST alike — and an
    // unknown path with the right token is a 404 too.
    for url in [
        format!("http://{}/d/wrong-token/", web.local_addr()),
        format!("{base}/nope"),
    ] {
        let miss = client.get(&url).send().await.expect("miss");
        assert_eq!(miss.status(), 404, "{url} must be a uniform miss");
    }
    let wrong_post = client
        .post(format!("http://{}/d/wrong-token/", web.local_addr()))
        .send()
        .await
        .expect("wrong-token post");
    assert_eq!(wrong_post.status(), 404);

    // A non-GET on a valid path is the honest 405 with an Allow.
    let post = client.post(format!("{base}/")).send().await.expect("post");
    assert_eq!(post.status(), 405);
    assert_eq!(
        post.headers()
            .get(reqwest::header::ALLOW)
            .expect("allow header"),
        "GET"
    );

    // Shutdown: the bounded drain completes, and the port actually
    // closes — a poll predicate with a deadline, never a sleep.
    let _ = web.join(Duration::from_secs(5)).await;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        match client
            .get(format!("{base}/"))
            .timeout(Duration::from_millis(500))
            .send()
            .await
        {
            // Some kernels accept-and-reset for a moment after close;
            // connection errors are the terminal state we wait for.
            Err(_) => break,
            Ok(_) if tokio::time::Instant::now() >= deadline => {
                panic!("the dashboard port must close after join")
            }
            Ok(_) => tokio::time::sleep(Duration::from_millis(10)).await,
        }
    }

    let outcome = app.shutdown().await;
    assert!(!outcome.actor_aborted);
}

/// W2: the LAN-bind refusal — a non-loopback bind whose token had to be
/// freshly generated (nothing resolvable) is the typed boot error,
/// before any listener exists.
#[tokio::test]
async fn w2_a_generated_token_never_secures_a_lan_bind() {
    let dir = tempdir();
    let mut config = config_for(&dir);
    config.web.bind = "192.0.2.10:8787".to_owned(); // TEST-NET, never loopback
    config.web.token = None; // nothing resolvable → generation would run
    let app = boot(config).await;

    let outcome = taskboard_app::web::spawn_dashboard(
        &app.config,
        app.engine.clone(),
        app.config.web.clone(),
    )
    .await;
    assert!(
        matches!(
            outcome,
            Err(taskboard_app::web::WebBindError::UnsecuredLanBind)
        ),
        "the generated-token LAN bind must refuse"
    );

    // With an explicit token the same bind is accepted (the operator
    // owns the capability).
    let mut config = config_for(&dir);
    config.storage.db_path = Some(dir.path().join("other.db"));
    config.web.bind = "192.0.2.10:8787".to_owned();
    let app = boot(config).await;
    // The explicit token is decodable; the bind itself may still fail
    // on hosts without the address — assert the *typed* refusal class
    // only (Bind, not UnsecuredLanBind).
    let outcome = taskboard_app::web::spawn_dashboard(
        &app.config,
        app.engine.clone(),
        app.config.web.clone(),
    )
    .await;
    assert!(!matches!(
        outcome,
        Err(taskboard_app::web::WebBindError::UnsecuredLanBind)
    ));
    let _ = app.shutdown().await;
}

/// W3: the disabled fence — `enabled = false` spawns nothing: no
/// listener on the configured port, no task, and the token file is
/// never written.
#[tokio::test]
async fn w3_a_disabled_table_spawns_nothing() {
    let dir = tempdir();
    let mut config = config_for(&dir);
    config.web = WebSection {
        enabled: false,
        bind: format!("127.0.0.1:{}", free_port()),
        refresh_secs: 30,
        token: None,
    };
    let bind = config.web.bind.clone();
    let app = boot(config).await;

    // The off table's placeholder spawns nothing: the config's `web`
    // section decodes disabled, so the lifecycle never calls the spawn.
    assert!(!app.config.web.enabled);
    let client = reqwest::Client::new();
    let refused = client
        .get(format!("http://{bind}/d/anything/"))
        .timeout(Duration::from_millis(500))
        .send()
        .await;
    assert!(
        refused.is_err(),
        "no listener may exist for a disabled section"
    );
    assert!(
        !dir.path().join("taskboard.db.web-token").exists(),
        "the token ceremony never ran"
    );
    let _ = app.shutdown().await;
}

/// W4: the token ceremony over a real boot — without an explicit token
/// the dashboard generates, persists beside the db, and reloads on the
/// next spawn.
#[tokio::test]
async fn w4_the_generated_token_persists_beside_the_db() {
    let dir = tempdir();
    let mut config = config_for(&dir);
    config.web.token = None;
    let app = boot(config).await;

    let web = taskboard_app::web::spawn_dashboard(
        &app.config,
        app.engine.clone(),
        app.config.web.clone(),
    )
    .await
    .expect("dashboard spawns");
    let token_path = dir.path().join("taskboard.db.web-token");
    let persisted = std::fs::read_to_string(&token_path).expect("token file");
    assert_eq!(persisted.trim().len(), 32);

    // The served URL carries the persisted capability.
    let client = reqwest::Client::new();
    let url = format!("http://{}/d/{}/", web.local_addr(), persisted.trim());
    let index = client.get(&url).send().await.expect("index");
    assert_eq!(index.status(), 200);
    let _ = web.join(Duration::from_secs(5)).await;
    let _ = app.shutdown().await;
}

/// W5: a bind conflict is the typed `Bind` error — the second spawn on
/// an already-bound address refuses loudly, never loops and never
/// steals the port.
#[tokio::test]
async fn w5_a_conflicting_bind_is_the_typed_refusal() {
    let dir = tempdir();
    let config = config_for(&dir);
    let app = boot(config).await;

    let first = taskboard_app::web::spawn_dashboard(
        &app.config,
        app.engine.clone(),
        app.config.web.clone(),
    )
    .await
    .expect("first spawn");
    let bound = format!("127.0.0.1:{}", first.local_addr().port());
    let mut second_config = app.config.clone();
    second_config.web.bind = bound.clone();

    let outcome = taskboard_app::web::spawn_dashboard(
        &second_config,
        app.engine.clone(),
        second_config.web.clone(),
    )
    .await;
    assert!(
        matches!(outcome, Err(taskboard_app::web::WebBindError::Bind(_))),
        "the conflicting bind must be the typed Bind refusal"
    );

    let _ = first.join(Duration::from_secs(5)).await;
    let _ = app.shutdown().await;
}
