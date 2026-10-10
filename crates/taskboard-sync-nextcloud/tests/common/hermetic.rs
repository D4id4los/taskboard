// SPDX-License-Identifier: MIT OR Apache-2.0
//! Shared harness for the hermetic suites (A-series wiremock actor tests,
//! I-series end-to-end): fixture JSON builders, deterministic-time boot
//! helpers, and the conditional-read pull mount. The per-suite *content*
//! (board labels, seeds, config) stays in each suite — only the mechanics
//! are shared.

#![allow(dead_code)] // not every suite uses every helper

use std::time::Duration;

use taskboard_domain::SystemEvent;
use tokio::sync::broadcast;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// The one remote board the hermetic suites bind to.
pub(crate) const BOARD: u64 = 42;
/// The board's one remote stack.
pub(crate) const STACK: u64 = 7;
/// The fixtures' base timestamp (epoch seconds).
pub(crate) const T: i64 = 1_700_000_000;

pub(crate) fn uuid(raw: u128) -> uuid::Uuid {
    uuid::Uuid::from_u128(raw)
}

pub(crate) fn ts(secs: i64) -> chrono::DateTime<chrono::Utc> {
    use chrono::TimeZone as _;
    chrono::Utc.timestamp_opt(secs, 0).unwrap()
}

/// The boards *listing* payload (the actor's conditional first read).
pub(crate) fn boards_json(dead: bool, labels: &serde_json::Value) -> String {
    serde_json::json!([{
        "id": BOARD, "title": "board", "color": "00ff00",
        "lastModified": T, "deletedAt": if dead { T + 5 } else { 0 },
        "archived": false, "labels": labels
    }])
    .to_string()
}

/// The board *detail* payload (the actor's authoritative labels read).
pub(crate) fn board_json(dead: bool, labels: &serde_json::Value) -> String {
    serde_json::json!({
        "id": BOARD, "title": "board", "color": "00ff00",
        "lastModified": T, "deletedAt": if dead { T + 5 } else { 0 },
        "archived": false, "labels": labels
    })
    .to_string()
}

/// The active-stacks listing with the given cards nested in stack 7.
pub(crate) fn stacks_json(cards: &serde_json::Value) -> String {
    serde_json::json!([{
        "id": STACK, "title": "col", "boardId": BOARD, "order": 0,
        "lastModified": T, "deletedAt": 0, "archived": false,
        "cards": cards
    }])
    .to_string()
}

/// A bare card payload in stack 7.
pub(crate) fn card_json(id: u64, title: &str) -> serde_json::Value {
    serde_json::json!({
        "id": id, "title": title, "stackId": STACK, "type": "plain",
        "order": 0, "lastModified": T, "labels": [], "archived": false,
        "duedate": null, "done": null
    })
}

/// Pull endpoints that answer 304 whenever the caller re-emits the `ETag`
/// they issued, else fresh data. The board detail is unconditional (no
/// validators travel on it) and always answers fresh. `labels` is the
/// board's label set as both listings and the detail carry it.
pub(crate) async fn mount_conditional_pull(server: &MockServer, labels: &serde_json::Value) {
    let labels = labels.clone();
    let conditional = move |req: &wiremock::Request| {
        let warm = req
            .headers
            .get("If-None-Match")
            .is_some_and(|v| v.to_str().unwrap_or("").contains("gen-1"));
        if warm {
            return ResponseTemplate::new(304).insert_header("ETag", "\"gen-1\"");
        }
        let p = req.url.path().to_string();
        let body = if p.ends_with("/stacks") {
            stacks_json(&serde_json::json!([]))
        } else if p.ends_with("/stacks/archived") {
            "[]".to_string()
        } else if p.ends_with(&format!("/boards/{BOARD}")) {
            board_json(false, &labels)
        } else {
            boards_json(false, &labels)
        };
        ResponseTemplate::new(200)
            .insert_header("ETag", "\"gen-1\"")
            .set_body_string(body)
    };
    for p in [
        "/index.php/apps/deck/api/v1.0/boards".to_string(),
        format!("/index.php/apps/deck/api/v1.0/boards/{BOARD}"),
        format!("/index.php/apps/deck/api/v1.0/boards/{BOARD}/stacks"),
        format!("/index.php/apps/deck/api/v1.0/boards/{BOARD}/stacks/archived"),
    ] {
        Mock::given(method("GET"))
            .and(path(p))
            .respond_with(conditional.clone())
            .mount(server)
            .await;
    }
}

/// Starts a wiremock server under paused time: the server's startup polls
/// readiness with a 25 ms sleep, so time must advance while it boots.
pub(crate) async fn start_server() -> MockServer {
    let started = tokio::spawn(MockServer::start());
    let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
    loop {
        if started.is_finished() {
            return started.await.expect("wiremock server starts");
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "wiremock never became ready"
        );
        tokio::time::advance(Duration::from_millis(10)).await;
    }
}

/// Waits until two subscribers (engine and actor, or actor and test) are on
/// the system broadcast — a structural fact, not a timed one; no sleeps.
pub(crate) async fn wait_subscribed(system_tx: &broadcast::Sender<SystemEvent>) {
    for _ in 0..10_000 {
        if system_tx.receiver_count() >= 2 {
            return;
        }
        tokio::task::yield_now().await;
    }
    panic!("the expected subscribers never joined the system broadcast");
}
