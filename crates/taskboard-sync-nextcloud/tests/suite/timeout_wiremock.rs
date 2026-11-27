// SPDX-License-Identifier: MIT OR Apache-2.0

//! The transport-timeout facts (#1485): a hung Deck request — a stalled
//! response or an unroutable dial — surfaces as the existing
//! `DeckError::Transport` carrying a reqwest timeout, after the bounded
//! retry budget, never as a hang. The builders receive sub-second
//! bounds through the `with_timeouts` seam so the typed-timeout oracle
//! runs in wall-clock-fraction time; the production consts (10 s
//! connect / 30 s idle-read) stay the only values `new` feeds.
//! Wiremock delay is real-clock time — the tokio clock stays unpaused
//! around the requests.

use std::sync::Arc;
use std::time::Duration;

use taskboard_sync_nextcloud::{DeckClient, DeckError, RetrySleep};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const BOARDS_PATH: &str = "/index.php/apps/deck/api/v1.0/boards";

/// A sleeper that never waits: retries advance instantly, so what the
/// tests wait on is the timeout oracle, not the backoff curve.
#[derive(Debug)]
struct InstantSleep;

impl RetrySleep for InstantSleep {
    fn sleep(
        &self,
        _delay: Duration,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
        Box::pin(std::future::ready(()))
    }
}

#[tokio::test]
async fn stalled_response_surfaces_as_typed_transport_timeout() {
    let server = MockServer::start().await;
    // The response arrives long after the injected idle-read bound;
    // the request must return as a typed timeout instead of waiting.
    Mock::given(method("GET"))
        .and(path(BOARDS_PATH))
        .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(30)))
        .mount(&server)
        .await;
    let err = DeckClient::with_timeouts(
        &server.uri(),
        "it-user",
        "it-token",
        Duration::from_secs(5),
        Duration::from_millis(500),
    )
    .unwrap()
    .with_sleeper(Arc::new(InstantSleep))
    .boards()
    .await
    .unwrap_err();
    assert!(
        matches!(&err, DeckError::Transport(e) if e.is_timeout()),
        "expected a typed transport timeout, got {err:?}"
    );
}

#[tokio::test]
async fn unroutable_dial_surfaces_as_typed_connect_timeout() {
    // A non-routable address: no server answers, the dial hangs on the
    // OS stack until the injected connect bound fires (the address is
    // in an unrouted range, so nothing local can accept it).
    let err = DeckClient::with_timeouts(
        "http://10.255.255.1:9",
        "u",
        "t",
        Duration::from_millis(500),
        Duration::from_secs(5),
    )
    .unwrap()
    .with_sleeper(Arc::new(InstantSleep))
    .boards()
    .await
    .unwrap_err();
    assert!(
        matches!(&err, DeckError::Transport(e) if e.is_timeout()),
        "expected a typed connect timeout, got {err:?}"
    );
}
