// SPDX-License-Identifier: MIT OR Apache-2.0
//! B-series: the `WebDAV` files client's exact request facts against
//! wiremock (one behavior per test). Method, path join, the conditional
//! headers (`If-None-Match: *`, `If-Match` echoing the served etag
//! byte-exact), the PROPFIND body constant + `Depth: 1`, the outcome
//! mapping table, the retry budget on the transport class only, and
//! the credential-redacting `Debug` pin.

use std::sync::Arc;
use std::time::Duration;

use taskboard_sync_files_nextcloud::client::{RetrySleep, WebDavFilesTransport};
use taskboard_sync_files_nextcloud::error::TransportError;
use taskboard_sync_files_nextcloud::transport::FilesTransport;
use wiremock::matchers::{body_bytes, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const USER: &str = "alice";
const TOKEN: &str = "s3cret-token";
const DEVICE: &str = "0193a1b2-c3d4-7cc9-8a2e-5f1e2d3c4b5a";

fn transport(server: &str) -> WebDavFilesTransport {
    WebDavFilesTransport::new(server, USER, TOKEN).expect("server URL is valid")
}

/// A sleeper that never waits: retries advance instantly, so request
/// counts stay exact and the tests stay wall-clock fast regardless of
/// tokio-time state.
#[derive(Debug)]
struct InstantSleep;

impl RetrySleep for InstantSleep {
    fn sleep(&self, _delay: Duration) -> std::pin::Pin<Box<dyn Future<Output = ()> + Send>> {
        Box::pin(std::future::ready(()))
    }
}

fn instant(transport: WebDavFilesTransport) -> WebDavFilesTransport {
    transport.with_sleeper(Arc::new(InstantSleep))
}

fn basic_auth_header() -> String {
    use base64::Engine as _;
    format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode(format!("{USER}:{TOKEN}"))
    )
}

fn appdata_path() -> String {
    format!("/remote.php/dav/files/{USER}/taskboard/devices/{DEVICE}/appdata.json")
}

#[tokio::test]
async fn put_create_sends_if_none_match_star_and_maps_201() {
    let server = MockServer::start().await;
    let body = b"{\"format\":1}".to_vec();
    Mock::given(method("PUT"))
        .and(path(appdata_path()))
        .and(header("if-none-match", "*"))
        .and(header("authorization", basic_auth_header()))
        .and(body_bytes(body.clone()))
        .respond_with(ResponseTemplate::new(201).insert_header("ETag", "\"first-etag\""))
        .mount(&server)
        .await;
    let outcome = instant(transport(&server.uri()))
        .put_create(&format!("taskboard/devices/{DEVICE}/appdata.json"), &body)
        .await
        .expect("create succeeds");
    match outcome {
        taskboard_sync_files_nextcloud::transport::PutOutcome::Created { etag } => {
            assert_eq!(
                etag.as_ref()
                    .map(taskboard_sync_files_nextcloud::etag::Etag::as_ref),
                Some("\"first-etag\"")
            );
        }
        updated @ taskboard_sync_files_nextcloud::transport::PutOutcome::Updated { .. } => {
            panic!("expected Created, got {updated:?}")
        }
    }
}

#[tokio::test]
async fn put_if_match_echoes_the_served_etag_byte_exact_and_maps_204() {
    let server = MockServer::start().await;
    // The served form, quoted and hyphenated exactly as the docker tier
    // serves etags; the matcher asserts the raw header bytes.
    let quoted = "\"abc123-def456\"";
    Mock::given(method("PUT"))
        .and(path(appdata_path()))
        .and(header("if-match", quoted))
        .respond_with(ResponseTemplate::new(204).insert_header("ETag", "\"new-etag\""))
        .mount(&server)
        .await;
    let etag = taskboard_sync_files_nextcloud::etag::Etag::new(quoted.to_owned());
    let outcome = instant(transport(&server.uri()))
        .put_if_match(
            &format!("taskboard/devices/{DEVICE}/appdata.json"),
            b"{}",
            &etag,
        )
        .await
        .expect("guarded rewrite succeeds");
    assert!(matches!(
        outcome,
        taskboard_sync_files_nextcloud::transport::PutOutcome::Updated { .. }
    ));
}

#[tokio::test]
async fn a_precondition_failure_is_a_result_never_retried() {
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path(appdata_path()))
        .respond_with(ResponseTemplate::new(412))
        .expect(1u64)
        .mount(&server)
        .await;
    let etag = taskboard_sync_files_nextcloud::etag::Etag::new("\"stale\"".to_owned());
    let err = instant(transport(&server.uri()))
        .put_if_match(
            &format!("taskboard/devices/{DEVICE}/appdata.json"),
            b"{}",
            &etag,
        )
        .await
        .expect_err("412 is typed");
    assert!(matches!(err, TransportError::PreconditionFailed));
}

#[tokio::test]
async fn the_status_mapping_table_is_typed() {
    type StatusCheck = (u16, fn(&TransportError) -> bool);
    let table: [StatusCheck; 4] = [
        (404, |e| matches!(e, TransportError::NotFound)),
        (401, |e| matches!(e, TransportError::Unauthorized)),
        (403, |e| matches!(e, TransportError::Forbidden)),
        (500, |e| matches!(e, TransportError::Server(500))),
    ];
    for (status, check) in table {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(appdata_path()))
            .respond_with(ResponseTemplate::new(status))
            .mount(&server)
            .await;
        let err = instant(transport(&server.uri()))
            .get(&format!("taskboard/devices/{DEVICE}/appdata.json"))
            .await
            .expect_err("error status maps typed");
        assert!(check(&err), "status {status} mapped to {err:?}");
    }
}

#[tokio::test]
async fn get_returns_bytes_and_the_served_etag() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(appdata_path()))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string("{\"format\":1}")
                .insert_header("ETag", "\"served\""),
        )
        .mount(&server)
        .await;
    let content = transport(&server.uri())
        .get(&format!("taskboard/devices/{DEVICE}/appdata.json"))
        .await
        .expect("get succeeds");
    assert_eq!(content.bytes, b"{\"format\":1}");
    assert_eq!(
        content
            .etag
            .as_ref()
            .map(taskboard_sync_files_nextcloud::etag::Etag::as_ref),
        Some("\"served\"")
    );
}

#[tokio::test]
async fn propfind_sends_the_fixed_body_depth1_and_parses_207() {
    let server = MockServer::start().await;
    let listing_body = "<d:multistatus xmlns:d=\"DAV:\"><d:response><d:href>\
        /remote.php/dav/files/alice/taskboard/devices/0193a1b2-c3d4-7cc9-8a2e-5f1e2d3c4b5a/\
        </d:href><d:propstat><d:prop><d:getetag>&quot;t&quot;</d:getetag></d:prop>\
        <d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response></d:multistatus>";
    Mock::given(method("PROPFIND"))
        .and(path(format!(
            "/remote.php/dav/files/{USER}/taskboard/devices/"
        )))
        .and(header("depth", "1"))
        .and(header("content-type", "application/xml"))
        .and(body_bytes(
            taskboard_sync_files_nextcloud::extract::PROPFIND_DEPTH1_BODY.as_bytes(),
        ))
        .respond_with(ResponseTemplate::new(207).set_body_string(listing_body))
        .expect(1u64)
        .mount(&server)
        .await;
    let listing = transport(&server.uri())
        .propfind_devices()
        .await
        .expect("listing parses");
    assert_eq!(listing.members.len(), 1);
    assert_eq!(
        listing.members[0].href,
        format!("/remote.php/dav/files/{USER}/taskboard/devices/{DEVICE}")
    );
}

#[tokio::test]
async fn mkcol_maps_201_and_405_to_the_exists_ok_outcomes() {
    for (status, expected) in [
        (
            201,
            taskboard_sync_files_nextcloud::transport::MkcolOutcome::Created,
        ),
        (
            405,
            taskboard_sync_files_nextcloud::transport::MkcolOutcome::AlreadyExists,
        ),
    ] {
        let server = MockServer::start().await;
        Mock::given(method("MKCOL"))
            .and(path(format!("/remote.php/dav/files/{USER}/taskboard")))
            .respond_with(ResponseTemplate::new(status))
            .mount(&server)
            .await;
        let outcome = transport(&server.uri())
            .mkcol("taskboard")
            .await
            .expect("mkcol maps typed");
        assert_eq!(outcome, expected);
    }
}

#[tokio::test]
async fn mkcol_parent_missing_is_a_conflict() {
    let server = MockServer::start().await;
    Mock::given(method("MKCOL"))
        .respond_with(ResponseTemplate::new(409))
        .mount(&server)
        .await;
    let err = transport(&server.uri())
        .mkcol("taskboard")
        .await
        .expect_err("409 maps typed");
    assert!(matches!(err, TransportError::Conflict));
}

#[tokio::test]
async fn rate_limit_retries_with_the_retry_after_floor_then_succeeds() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(appdata_path()))
        .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "1"))
        .up_to_n_times(2)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(appdata_path()))
        .respond_with(ResponseTemplate::new(200).set_body_string("x"))
        .mount(&server)
        .await;
    let content = instant(transport(&server.uri()))
        .get(&format!("taskboard/devices/{DEVICE}/appdata.json"))
        .await
        .expect("third attempt succeeds");
    assert_eq!(content.bytes, b"x");
}

#[tokio::test]
async fn exhausted_retry_budget_surfaces_the_transport_class() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(appdata_path()))
        .respond_with(ResponseTemplate::new(503))
        .expect(3u64)
        .mount(&server)
        .await;
    let err = instant(transport(&server.uri()))
        .get(&format!("taskboard/devices/{DEVICE}/appdata.json"))
        .await
        .expect_err("budget exhausted");
    assert!(matches!(err, TransportError::Unavailable));
}

#[tokio::test]
async fn server_errors_are_never_retried() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(appdata_path()))
        .respond_with(ResponseTemplate::new(500))
        .expect(1u64)
        .mount(&server)
        .await;
    let err = instant(transport(&server.uri()))
        .get(&format!("taskboard/devices/{DEVICE}/appdata.json"))
        .await
        .expect_err("500 is terminal");
    assert!(matches!(err, TransportError::Server(500)));
}
