// SPDX-License-Identifier: MIT OR Apache-2.0

//! The transport-timeout facts (#1485): a hung `WebDAV` request — a
//! stalled conditional-PUT response or an unroutable dial — surfaces as
//! the existing `TransportError::Transport` carrying a reqwest timeout,
//! after the bounded retry budget, never as a hang. The builder
//! receives sub-second bounds through the `with_test_timeouts` seam so the
//! typed-timeout oracle runs in wall-clock-fraction time; the
//! production consts (10 s connect / 30 s idle-read) stay the only
//! values `new` feeds. Wiremock delay is real-clock time — the tokio
//! clock stays unpaused around the requests.

use std::sync::Arc;
use std::time::Duration;

use taskboard_sync_files_nextcloud::client::{RetrySleep, WebDavFilesTransport};
use taskboard_sync_files_nextcloud::error::TransportError;
use taskboard_sync_files_nextcloud::transport::FilesTransport;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const USER: &str = "alice";

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
async fn stalled_conditional_put_surfaces_as_typed_transport_timeout() {
    let server = MockServer::start().await;
    // The conditional-PUT response arrives long after the injected
    // idle-read bound; the verb must return as a typed timeout instead
    // of waiting. A timed-out attempt is not known to have landed
    // server-side — retrying the same `If-None-Match: *` create stays
    // safe (the server answers 412 when it did).
    Mock::given(method("PUT"))
        .and(path(format!(
            "/remote.php/dav/files/{USER}/taskboard/appdata.json"
        )))
        .respond_with(ResponseTemplate::new(201).set_delay(Duration::from_secs(30)))
        .mount(&server)
        .await;
    let err = WebDavFilesTransport::with_test_timeouts(
        &server.uri(),
        USER,
        "s3cret-token",
        Some(Duration::from_secs(5)),
        Some(Duration::from_millis(500)),
    )
    .unwrap()
    .with_sleeper(Arc::new(InstantSleep))
    .put_create("taskboard/appdata.json", b"{\"format\":1}")
    .await
    .unwrap_err();
    assert!(
        matches!(&err, TransportError::Transport(e) if e.is_timeout()),
        "expected a typed transport timeout, got {err:?}"
    );
}

#[tokio::test]
async fn unroutable_dial_surfaces_as_typed_connect_timeout() {
    // A non-routable address: no server answers, the dial hangs on the
    // OS stack until the injected connect bound fires (the address is
    // in an unrouted range, so nothing local can accept it).
    let err = WebDavFilesTransport::with_test_timeouts(
        "http://10.255.255.1:9",
        USER,
        "s3cret-token",
        Some(Duration::from_millis(500)),
        Some(Duration::from_secs(5)),
    )
    .unwrap()
    .with_sleeper(Arc::new(InstantSleep))
    .put_create("taskboard/appdata.json", b"{\"format\":1}")
    .await
    .unwrap_err();
    assert!(
        matches!(&err, TransportError::Transport(e) if e.is_timeout()),
        "expected a typed connect timeout, got {err:?}"
    );
}
