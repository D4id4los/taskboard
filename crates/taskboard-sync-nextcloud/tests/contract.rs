// SPDX-License-Identifier: MIT OR Apache-2.0
//! Tier 1 contract tests: `DeckClient` against an in-process mock HTTP
//! server (`docs/testing_strategy.org` §8). Runs in the default suite; fully
//! hermetic.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use base64::Engine as _;
use taskboard_sync_nextcloud::{DeckClient, DeckColor, DeckError, StackFilter};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const BOARDS_PATH: &str = "/index.php/apps/deck/api/v1.0/boards";

fn client_for(server: &MockServer) -> DeckClient {
    DeckClient::new(&server.uri(), "it-user", "it-token").unwrap()
}

fn basic_auth_header() -> String {
    format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode("it-user:it-token")
    )
}

fn fixture(name: &str) -> String {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/deck")
        .join(name);
    std::fs::read_to_string(p).unwrap()
}

/// Responds with a scripted status sequence, then repeats the last status.
#[derive(Default)]
struct SequenceResponder {
    statuses: Vec<u16>,
    index: AtomicUsize,
}

impl SequenceResponder {
    fn new(statuses: Vec<u16>) -> Self {
        // Deliberately not `assert!(!statuses.is_empty())` / a
        // `first().is_some()` check: the pinned dev toolchain and CI's
        // newer stable deny those shapes in opposite directions
        // (clippy::assert_is_empty vs clippy::unnecessary_first_then_check).
        assert_ne!(statuses.len(), 0, "status sequence must not be empty");
        Self {
            statuses,
            index: AtomicUsize::new(0),
        }
    }
}

impl wiremock::Respond for SequenceResponder {
    fn respond(&self, _request: &wiremock::Request) -> ResponseTemplate {
        let i = self
            .index
            .fetch_add(1, Ordering::SeqCst)
            .min(self.statuses.len() - 1);
        let template = ResponseTemplate::new(self.statuses[i]);
        if self.statuses[i] == 200 {
            template.set_body_string(fixture("boards_list.json"))
        } else {
            template
        }
    }
}

/// Records the delays the client asks to sleep; returns immediately so the
/// timing assertions are exact and independent of real sockets.
#[derive(Clone, Debug, Default)]
struct RecordingSleeper(std::sync::Arc<std::sync::Mutex<Vec<Duration>>>);

impl taskboard_sync_nextcloud::RetrySleep for RecordingSleeper {
    fn sleep(
        &self,
        delay: Duration,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
        self.0.lock().unwrap().push(delay);
        Box::pin(std::future::ready(()))
    }
}

#[tokio::test]
async fn boards_request_carries_auth_and_ocs_headers_and_unwraps_envelope() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(BOARDS_PATH))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_string(fixture("boards_list.json")),
        )
        .mount(&server)
        .await;

    let client = client_for(&server);
    let boards = client.boards().await.unwrap();

    assert_eq!(boards.len(), 2);
    assert_eq!(boards[0].id, 3);
    assert_eq!(boards[0].title, "taskboard-sync");
    assert_eq!(boards[1].color.as_str(), "00c2e0");

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    let req = &requests[0];
    assert_eq!(req.method.as_str(), "GET");
    assert_eq!(req.url.path(), BOARDS_PATH);
    let headers = |name: &str| {
        req.headers
            .get(name)
            .expect("header must be present")
            .to_str()
            .unwrap()
            .to_owned()
    };
    assert_eq!(headers("Authorization"), basic_auth_header());
    assert_eq!(headers("OCS-APIRequest"), "true");
    assert_eq!(headers("Accept"), "application/json");
}

#[tokio::test]
async fn create_board_posts_json_and_decodes_board() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(BOARDS_PATH))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_string(fixture("board_create_response.json")),
        )
        .mount(&server)
        .await;

    let board = client_for(&server)
        .create_board("new board", &DeckColor::from_hex("ff0000").unwrap())
        .await
        .unwrap();
    assert_eq!(board.id, 42);

    let req = &server.received_requests().await.unwrap()[0];
    assert_eq!(req.method.as_str(), "POST");
    let body: serde_json::Value = serde_json::from_slice(&req.body).unwrap();
    assert_eq!(
        body,
        serde_json::json!({"title": "new board", "color": "ff0000"})
    );
}

#[tokio::test]
async fn delete_board_sends_delete_to_resource_path_and_returns_board() {
    let server = MockServer::start().await;
    Mock::given(method("DELETE"))
        .and(path(format!("{BOARDS_PATH}/42")))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "ocs": {
                "meta": {"statuscode": 200},
                "data": {"id": 42, "title": "b", "color": "00c2e0", "deletedAt": 1700}
            }
        })))
        .mount(&server)
        .await;

    let board = client_for(&server).delete_board(42).await.unwrap();
    server.verify().await;
    assert_eq!(board.id, 42);
    assert!(
        !board.is_live(),
        "soft-deleted board must be reported as such"
    );
}

#[tokio::test]
async fn unauthorized_maps_from_401_without_retry() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(BOARDS_PATH))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;

    let err = client_for(&server).boards().await.unwrap_err();
    server.verify().await;
    assert!(matches!(err, DeckError::Unauthorized));
}

#[tokio::test]
async fn forbidden_maps_from_403_without_retry() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(BOARDS_PATH))
        .respond_with(ResponseTemplate::new(403))
        .mount(&server)
        .await;

    let err = client_for(&server).boards().await.unwrap_err();
    server.verify().await;
    assert!(matches!(err, DeckError::Forbidden));
}

#[tokio::test]
async fn not_found_maps_from_404_without_retry() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(BOARDS_PATH))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    let err = client_for(&server).boards().await.unwrap_err();
    server.verify().await;
    assert!(matches!(err, DeckError::NotFound));
}

#[tokio::test]
async fn internal_server_error_is_not_retried() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(BOARDS_PATH))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;

    let err = client_for(&server).boards().await.unwrap_err();
    server.verify().await;
    assert!(matches!(err, DeckError::Server(500)));
}

#[tokio::test]
async fn service_unavailable_is_retried_until_success() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(BOARDS_PATH))
        .respond_with(SequenceResponder::new(vec![503, 503, 200]))
        .mount(&server)
        .await;

    let sleeper = RecordingSleeper::default();
    let boards = DeckClient::new(&server.uri(), "it-user", "it-token")
        .unwrap()
        .with_sleeper(std::sync::Arc::new(sleeper.clone()))
        .boards()
        .await
        .unwrap();
    assert_eq!(boards.len(), 2);

    // Exactly the policy's delay sequence before the third attempt.
    assert_eq!(
        sleeper.0.lock().unwrap().as_slice(),
        [Duration::from_millis(500), Duration::from_secs(1)]
    );
    assert_eq!(
        server.received_requests().await.unwrap().len(),
        3,
        "initial attempt plus two retries"
    );
}

#[tokio::test]
async fn rate_limiting_exhausts_policy_and_reports_rate_limited() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(BOARDS_PATH))
        .respond_with(ResponseTemplate::new(429))
        .expect(3) // initial attempt + 2 retries = policy budget
        .mount(&server)
        .await;

    let sleeper = RecordingSleeper::default();
    let err = DeckClient::new(&server.uri(), "it-user", "it-token")
        .unwrap()
        .with_sleeper(std::sync::Arc::new(sleeper.clone()))
        .boards()
        .await
        .unwrap_err();
    server.verify().await;
    assert!(matches!(err, DeckError::RateLimited));
    assert_eq!(
        sleeper.0.lock().unwrap().as_slice(),
        [Duration::from_millis(500), Duration::from_secs(1)]
    );
}

#[tokio::test]
async fn malformed_json_maps_to_envelope_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(BOARDS_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_string("{not json"))
        .mount(&server)
        .await;

    let err = client_for(&server).boards().await.unwrap_err();
    assert!(matches!(err, DeckError::Envelope(_)));
}

#[tokio::test]
async fn non_json_content_type_maps_to_envelope_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(BOARDS_PATH))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/html")
                .set_body_string("<html>maintenance</html>"),
        )
        .mount(&server)
        .await;

    let err = client_for(&server).boards().await.unwrap_err();
    assert!(matches!(err, DeckError::Envelope(_)));
}

#[tokio::test]
async fn transport_failure_is_retried_then_reported() {
    // Reserve a free port, then release it: connecting there is
    // deterministically refused (a dropped MockServer can keep serving).
    let port = {
        std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port()
    };
    let uri = format!("http://127.0.0.1:{port}");

    let sleeper = RecordingSleeper::default();
    let err = DeckClient::new(&uri, "u", "t")
        .unwrap()
        .with_sleeper(std::sync::Arc::new(sleeper.clone()))
        .boards()
        .await
        .unwrap_err();
    assert!(matches!(err, DeckError::Transport(_)));
    assert_eq!(
        sleeper.0.lock().unwrap().as_slice(),
        [Duration::from_millis(500), Duration::from_secs(1)]
    );
}

#[tokio::test]
async fn authorization_header_is_per_instance_not_global() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(BOARDS_PATH))
        .and(header("Authorization", basic_auth_header()))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("boards_list.json")))
        .mount(&server)
        .await;

    let boards = client_for(&server).boards().await;
    assert!(boards.is_ok(), "exact Basic-auth header must be accepted");
}

#[tokio::test]
async fn get_board_hits_resource_path_and_decodes_board() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!("{BOARDS_PATH}/42")))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(fixture("board_create_response.json")),
        )
        .mount(&server)
        .await;

    let board = client_for(&server).board(42).await.unwrap();
    server.verify().await;
    assert_eq!(board.id, 42);
    assert_eq!(board.title, "taskboard-it-ab12cd34");
}

#[tokio::test]
async fn stacks_listing_decodes_nested_collection() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!("{BOARDS_PATH}/42/stacks")))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("stacks_list.json")))
        .mount(&server)
        .await;

    let stacks = client_for(&server)
        .stacks(42, StackFilter::Active)
        .await
        .unwrap();
    server.verify().await;
    assert_eq!(stacks.len(), 2);
    assert_eq!(stacks[1].cards[0].title, "harvest fixtures");
}

#[tokio::test]
async fn archived_stacks_use_the_dedicated_route() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!("{BOARDS_PATH}/42/stacks/archived")))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("stacks_list.json")))
        .mount(&server)
        .await;

    let stacks = client_for(&server)
        .stacks(42, StackFilter::Archived)
        .await
        .unwrap();
    server.verify().await;
    assert_eq!(stacks.len(), 2);
}

#[tokio::test]
async fn stack_detail_decodes_nested_cards() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!("{BOARDS_PATH}/42/stacks/9")))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("stack_detail.json")))
        .mount(&server)
        .await;

    let stack = client_for(&server).stack(42, 9).await.unwrap();
    server.verify().await;
    assert_eq!(stack.cards.len(), 2);
    assert!(stack.cards.iter().any(|c| c.archived));
}

#[tokio::test]
async fn card_detail_decodes_iso8601_timestamps() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!("{BOARDS_PATH}/42/stacks/9/cards/5")))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("card_detail.json")))
        .mount(&server)
        .await;

    let card = client_for(&server).card(42, 9, 5).await.unwrap();
    server.verify().await;
    assert_eq!(card.id, 5);
    assert!(card.duedate.is_some());
}

#[tokio::test]
async fn labels_listing_decodes_collection() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!("{BOARDS_PATH}/42/labels")))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("labels_list.json")))
        .mount(&server)
        .await;

    let labels = client_for(&server).labels(42).await.unwrap();
    server.verify().await;
    assert_eq!(labels.len(), 2);
    // Lenient read path: server strings pass through verbatim.
    assert_eq!(labels[1].color.as_str(), "00C2E0");
}

#[tokio::test]
async fn attachments_listing_hits_nested_resource_path() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!(
            "{BOARDS_PATH}/42/stacks/9/cards/5/attachments"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("attachments_list.json")))
        .mount(&server)
        .await;

    let attachments = client_for(&server).attachments(42, 9, 5).await.unwrap();
    server.verify().await;
    assert_eq!(attachments.len(), 1);
    assert_eq!(attachments[0].kind, "deck_file");
}

#[tokio::test]
async fn bad_request_maps_from_400_without_retry() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(BOARDS_PATH))
        .respond_with(ResponseTemplate::new(400))
        .mount(&server)
        .await;

    let err = client_for(&server).boards().await.unwrap_err();
    server.verify().await;
    assert!(matches!(err, DeckError::BadRequest));
}

#[tokio::test]
async fn conflict_maps_from_409_without_retry() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(BOARDS_PATH))
        .respond_with(ResponseTemplate::new(409))
        .mount(&server)
        .await;

    let err = client_for(&server).boards().await.unwrap_err();
    server.verify().await;
    assert!(matches!(err, DeckError::Conflict));
}

#[tokio::test]
async fn precondition_failed_maps_from_412_without_retry() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(BOARDS_PATH))
        .respond_with(ResponseTemplate::new(412))
        .mount(&server)
        .await;

    let err = client_for(&server).boards().await.unwrap_err();
    server.verify().await;
    assert!(matches!(err, DeckError::PreconditionFailed));
}
