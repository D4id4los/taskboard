// SPDX-License-Identifier: MIT OR Apache-2.0
//! Tier 1 contract tests: `DeckClient` against an in-process mock HTTP
//! server (`docs/testing_strategy.org` §8). Runs in the default suite; fully
//! hermetic.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use base64::Engine as _;
use taskboard_sync_nextcloud::Validators;
use taskboard_sync_nextcloud::model::Card;
use taskboard_sync_nextcloud::{
    BoardChanges, CloneOptions, DeckClient, DeckColor, DeckError, LabelChanges, NewCard,
    StackChanges, StackFilter,
};
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

fn board_json(id: u64, title: &str) -> String {
    serde_json::json!({
        "id": id, "title": title, "color": "00c2e0", "deletedAt": 0
    })
    .to_string()
}

fn stack_json(id: u64, title: &str) -> String {
    serde_json::json!({
        "id": id, "title": title, "boardId": 42, "order": 0, "deletedAt": 0, "cards": []
    })
    .to_string()
}

fn card_json(id: u64, title: &str) -> String {
    serde_json::json!({
        "id": id, "title": title, "stackId": 9, "type": "plain", "order": 0,
        "labels": [], "assignedUsers": [], "archived": false,
        "duedate": null, "done": null, "attachments": [],
        "attachmentCount": 0, "commentsUnread": 0, "overdue": 0
    })
    .to_string()
}

fn label_json(id: u64, title: &str) -> String {
    serde_json::json!({"id": id, "title": title, "color": "00c2e0", "boardId": 42}).to_string()
}

async fn last_body(server: &MockServer) -> serde_json::Value {
    let requests = server.received_requests().await.unwrap();
    serde_json::from_slice(&requests.last().unwrap().body).unwrap()
}

#[tokio::test]
async fn update_board_puts_sparse_changeset_verbatim() {
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path(format!("{BOARDS_PATH}/42")))
        .respond_with(ResponseTemplate::new(200).set_body_string(board_json(42, "renamed")))
        .mount(&server)
        .await;

    let board = client_for(&server)
        .update_board(
            42,
            &BoardChanges {
                title: "renamed".into(),
                color: DeckColor::from_hex("ff0000").unwrap(),
                archived: true,
            },
        )
        .await
        .unwrap();
    server.verify().await;
    assert_eq!(board.title, "renamed");

    // Exactly the changeset fields: no silent defaults can creep in.
    assert_eq!(
        last_body(&server).await,
        serde_json::json!({"title": "renamed", "color": "ff0000", "archived": true})
    );
}

#[tokio::test]
async fn restore_board_puts_undo_delete_route() {
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path(format!("{BOARDS_PATH}/42/undoDelete")))
        .respond_with(ResponseTemplate::new(200).set_body_string(board_json(42, "b")))
        .mount(&server)
        .await;

    let board = client_for(&server).restore_board(42).await.unwrap();
    server.verify().await;
    assert_eq!(board.id, 42);
}

#[tokio::test]
async fn clone_board_posts_options_with_default_flags_false() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(format!("{BOARDS_PATH}/42/clone")))
        .respond_with(ResponseTemplate::new(200).set_body_string(board_json(43, "copy")))
        .mount(&server)
        .await;

    let board = client_for(&server)
        .clone_board(42, &CloneOptions::default())
        .await
        .unwrap();
    server.verify().await;
    assert_eq!(board.id, 43);

    // Default copies nothing; unset title is omitted entirely.
    assert_eq!(
        last_body(&server).await,
        serde_json::json!({
            "copyDescription": false, "copyLabels": false, "copyAssignedUsers": false
        })
    );
}

#[tokio::test]
async fn create_stack_posts_title_and_order() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(format!("{BOARDS_PATH}/42/stacks")))
        .respond_with(ResponseTemplate::new(200).set_body_string(stack_json(8, "To do")))
        .mount(&server)
        .await;

    let stack = client_for(&server)
        .create_stack(42, "To do", 0)
        .await
        .unwrap();
    server.verify().await;
    assert_eq!(stack.id, 8);
    assert_eq!(
        last_body(&server).await,
        serde_json::json!({"title": "To do", "order": 0})
    );
}

#[tokio::test]
async fn update_stack_puts_sparse_changeset_verbatim() {
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path(format!("{BOARDS_PATH}/42/stacks/8")))
        .respond_with(ResponseTemplate::new(200).set_body_string(stack_json(8, "Done")))
        .mount(&server)
        .await;

    let stack = client_for(&server)
        .update_stack(
            42,
            8,
            &StackChanges {
                title: "Done".into(),
                order: 2,
            },
        )
        .await
        .unwrap();
    server.verify().await;
    assert_eq!(stack.title, "Done");
    assert_eq!(
        last_body(&server).await,
        serde_json::json!({"title": "Done", "order": 2})
    );
}

#[tokio::test]
async fn delete_stack_sends_delete_and_returns_stack() {
    let server = MockServer::start().await;
    Mock::given(method("DELETE"))
        .and(path(format!("{BOARDS_PATH}/42/stacks/8")))
        .respond_with(ResponseTemplate::new(200).set_body_string(stack_json(8, "To do")))
        .mount(&server)
        .await;

    let stack = client_for(&server).delete_stack(42, 8).await.unwrap();
    server.verify().await;
    assert_eq!(stack.id, 8);
}

#[tokio::test]
async fn create_card_posts_exact_payload_including_iso_duedate() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(format!("{BOARDS_PATH}/42/stacks/9/cards")))
        .respond_with(ResponseTemplate::new(200).set_body_string(card_json(5, "new card")))
        .mount(&server)
        .await;

    let card = client_for(&server)
        .create_card(
            42,
            9,
            &NewCard {
                title: "new card".into(),
                order: Some(3),
                description: None,
                duedate: Some("2020-01-20T09:52:43Z".parse().unwrap()),
                kind: None,
            },
        )
        .await
        .unwrap();
    server.verify().await;
    assert_eq!(card.id, 5);

    // Omitted fields are absent (server defaults apply); the duedate uses
    // Deck's +00:00 ISO shape, not chrono's Z suffix.
    assert_eq!(
        last_body(&server).await,
        serde_json::json!({
            "title": "new card", "order": 3,
            "duedate": "2020-01-20T09:52:43+00:00"
        })
    );
}

#[tokio::test]
async fn update_card_puts_full_round_trip_body() {
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path(format!("{BOARDS_PATH}/42/stacks/9/cards/5")))
        .respond_with(ResponseTemplate::new(200).set_body_string(card_json(5, "edited")))
        .mount(&server)
        .await;

    // Read-then-mutate: start from a fetched wire card and change fields.
    let mut card: Card = serde_json::from_str(&card_json(5, "original")).unwrap();
    card.title = "edited".into();
    card.duedate = Some("2026-12-31T23:59:59Z".parse().unwrap());
    let updated = client_for(&server).update_card(42, &card).await.unwrap();
    server.verify().await;
    assert_eq!(updated.title, "edited");

    let body = last_body(&server).await;
    // Full round-trip: structural fields the caller never touched are all
    // present, so the server cannot reset them.
    assert_eq!(body["id"], 5);
    assert_eq!(body["stackId"], 9);
    assert_eq!(body["type"], "plain");
    assert_eq!(body["title"], "edited");
    assert_eq!(body["duedate"], "2026-12-31T23:59:59+00:00");
}

#[tokio::test]
async fn delete_card_sends_delete_and_returns_card() {
    let server = MockServer::start().await;
    Mock::given(method("DELETE"))
        .and(path(format!("{BOARDS_PATH}/42/stacks/9/cards/5")))
        .respond_with(ResponseTemplate::new(200).set_body_string(card_json(5, "gone")))
        .mount(&server)
        .await;

    let card = client_for(&server).delete_card(42, 9, 5).await.unwrap();
    server.verify().await;
    assert_eq!(card.id, 5);
}

#[tokio::test]
async fn archive_and_unarchive_hit_dedicated_routes_with_empty_body() {
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path(format!("{BOARDS_PATH}/42/stacks/9/cards/5/archive")))
        .respond_with(ResponseTemplate::new(200).set_body_string(card_json(5, "c")))
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path(format!("{BOARDS_PATH}/42/stacks/9/cards/5/unarchive")))
        .respond_with(ResponseTemplate::new(200).set_body_string(card_json(5, "c")))
        .mount(&server)
        .await;

    let client = client_for(&server);
    client.archive_card(42, 9, 5).await.unwrap();
    client.unarchive_card(42, 9, 5).await.unwrap();
    server.verify().await;

    let requests = server.received_requests().await.unwrap();
    assert!(requests.iter().all(|r| r.body.is_empty()));
}

#[tokio::test]
async fn reorder_card_puts_order_and_target_stack_in_body_and_ignores_reply() {
    // Observed Deck versions answer with a card object, an array of cards,
    // or an empty body; none of that may fail the call.
    for reply in [
        card_json(5, "c"),
        format!("[{}]", card_json(5, "c")),
        String::new(),
    ] {
        let server = MockServer::start().await;
        Mock::given(method("PUT"))
            .and(path(format!("{BOARDS_PATH}/42/stacks/9/cards/5/reorder")))
            .respond_with(ResponseTemplate::new(200).set_body_string(reply.clone()))
            .mount(&server)
            .await;

        client_for(&server)
            .reorder_card(42, 9, 5, 0, 8)
            .await
            .unwrap_or_else(|e| panic!("reorder must tolerate reply {reply:?}: {e}"));
        server.verify().await;
        assert_eq!(
            last_body(&server).await,
            serde_json::json!({"order": 0, "stackId": 8})
        );
    }
}

#[tokio::test]
async fn assign_and_remove_label_post_label_id_body() {
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path(format!(
            "{BOARDS_PATH}/42/stacks/9/cards/5/assignLabel"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_string(card_json(5, "c")))
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path(format!(
            "{BOARDS_PATH}/42/stacks/9/cards/5/removeLabel"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_string(card_json(5, "c")))
        .mount(&server)
        .await;

    let client = client_for(&server);
    client.assign_label(42, 9, 5, 7).await.unwrap();
    client.remove_label(42, 9, 5, 7).await.unwrap();
    server.verify().await;
    assert_eq!(last_body(&server).await, serde_json::json!({"labelId": 7}));
}

#[tokio::test]
async fn create_label_posts_title_and_color() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(format!("{BOARDS_PATH}/42/labels")))
        .respond_with(ResponseTemplate::new(200).set_body_string(label_json(2, "urgent")))
        .mount(&server)
        .await;

    let label = client_for(&server)
        .create_label(42, "urgent", &DeckColor::from_hex("FF0000").unwrap())
        .await
        .unwrap();
    server.verify().await;
    assert_eq!(label.id, 2);
    assert_eq!(
        last_body(&server).await,
        serde_json::json!({"title": "urgent", "color": "ff0000"})
    );
}

#[tokio::test]
async fn update_label_puts_sparse_changeset_verbatim() {
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path(format!("{BOARDS_PATH}/42/labels/2")))
        .respond_with(ResponseTemplate::new(200).set_body_string(label_json(2, "later")))
        .mount(&server)
        .await;

    let label = client_for(&server)
        .update_label(
            42,
            2,
            &LabelChanges {
                title: "later".into(),
                color: DeckColor::from_hex("00ff00").unwrap(),
            },
        )
        .await
        .unwrap();
    server.verify().await;
    assert_eq!(label.title, "later");
    assert_eq!(
        last_body(&server).await,
        serde_json::json!({"title": "later", "color": "00ff00"})
    );
}

#[tokio::test]
async fn delete_label_sends_delete_to_label_path() {
    let server = MockServer::start().await;
    Mock::given(method("DELETE"))
        .and(path(format!("{BOARDS_PATH}/42/labels/2")))
        .respond_with(ResponseTemplate::new(200).set_body_string(label_json(2, "urgent")))
        .mount(&server)
        .await;

    let label = client_for(&server).delete_label(42, 2).await.unwrap();
    server.verify().await;
    assert_eq!(label.id, 2);
}

#[tokio::test]
async fn conditional_fetch_sends_validators_and_returns_fresh_data() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(BOARDS_PATH))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("ETag", "\"etag-2\"")
                .insert_header("Last-Modified", "Thu, 08 Oct 2026 12:00:00 GMT")
                .set_body_string(fixture("boards_list.json")),
        )
        .mount(&server)
        .await;

    let fetched = client_for(&server)
        .fetch_boards(&Validators {
            etag: Some("\"etag-1\"".into()),
            last_modified: Some("Wed, 08 Oct 2026 00:00:00 GMT".into()),
        })
        .await
        .unwrap();

    // Validators are re-emitted verbatim as condition headers.
    let req = &server.received_requests().await.unwrap()[0];
    let hdr = |name: &str| {
        req.headers
            .get(name)
            .expect("conditional header must be present")
            .to_str()
            .unwrap()
            .to_owned()
    };
    assert_eq!(hdr("If-None-Match"), "\"etag-1\"");
    assert_eq!(hdr("If-Modified-Since"), "Wed, 08 Oct 2026 00:00:00 GMT");

    let boards = fetched.data.expect("fresh data on 200");
    assert_eq!(boards.len(), 2);
    // The next poll's validators come from the response headers, verbatim.
    assert_eq!(
        fetched.validators,
        Validators {
            etag: Some("\"etag-2\"".into()),
            last_modified: Some("Thu, 08 Oct 2026 12:00:00 GMT".into()),
        }
    );
}

#[tokio::test]
async fn not_modified_304_with_empty_body_is_data_not_error() {
    // Guards the fallthrough bug class: 304 carries no body and must be
    // mapped before any envelope decoding.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(BOARDS_PATH))
        .and(header("If-None-Match", "\"etag-1\""))
        .respond_with(ResponseTemplate::new(304).insert_header("ETag", "\"etag-1\""))
        .mount(&server)
        .await;

    let fetched = client_for(&server)
        .fetch_boards(&Validators {
            etag: Some("\"etag-1\"".into()),
            last_modified: None,
        })
        .await
        .unwrap();
    server.verify().await;

    assert!(fetched.data.is_none(), "304 must surface as cached data");
    assert_eq!(fetched.validators.etag.as_deref(), Some("\"etag-1\""));
}

#[tokio::test]
async fn conditional_cycle_fetch_not_modified_mutate_refetch() {
    // First poll: 200 with validators; second: 304; after a mutation: 200
    // again with *new* validators and fresh data.
    let server = MockServer::start().await;
    let etag = std::sync::Arc::new(AtomicUsize::new(0));
    let etag_for_responder = etag.clone();
    Mock::given(method("GET"))
        .and(path(BOARDS_PATH))
        .respond_with(move |_req: &wiremock::Request| {
            let generation = etag_for_responder.load(Ordering::SeqCst);
            ResponseTemplate::new(if generation == 1 { 304 } else { 200 })
                .insert_header("ETag", format!("\"gen-{generation}\""))
                .set_body_string(fixture("boards_list.json"))
        })
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(BOARDS_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_string(board_json(99, "mutated")))
        .mount(&server)
        .await;

    let client = client_for(&server);
    let first = client.fetch_boards(&Validators::default()).await.unwrap();
    assert_eq!(first.validators.etag.as_deref(), Some("\"gen-0\""));

    etag.store(1, Ordering::SeqCst);
    let cached = client.fetch_boards(&first.validators).await.unwrap();
    assert!(cached.data.is_none(), "unchanged generation must yield 304");

    etag.store(2, Ordering::SeqCst);
    client
        .create_board("mutated", &DeckColor::from_hex("ff0000").unwrap())
        .await
        .unwrap();
    let fresh = client.fetch_boards(&cached.validators).await.unwrap();
    let boards = fresh.data.expect("mutation must produce fresh data");
    assert_eq!(boards.len(), 2);
    assert_eq!(fresh.validators.etag.as_deref(), Some("\"gen-2\""));
}

#[tokio::test]
async fn conditional_stack_fetch_hits_archived_route_with_validators() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!("{BOARDS_PATH}/42/stacks/archived")))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("ETag", "\"stacks-2\"")
                .set_body_string(fixture("stacks_list.json")),
        )
        .mount(&server)
        .await;

    let fetched = client_for(&server)
        .fetch_stacks(
            42,
            StackFilter::Archived,
            &Validators {
                etag: Some("\"stacks-1\"".into()),
                last_modified: None,
            },
        )
        .await
        .unwrap();

    // Path carries the filter's route; the stale validator was re-emitted;
    // the fresh one comes back for the next poll.
    let req = &server.received_requests().await.unwrap()[0];
    let if_none_match = req.headers.get("If-None-Match").unwrap().to_str().unwrap();
    assert_eq!(if_none_match, "\"stacks-1\"");
    assert_eq!(fetched.data.expect("fresh data on 200").len(), 2);
    assert_eq!(fetched.validators.etag.as_deref(), Some("\"stacks-2\""));
}

#[tokio::test]
async fn conditional_card_fetch_returns_304_as_data() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!("{BOARDS_PATH}/42/stacks/9/cards/5")))
        .respond_with(ResponseTemplate::new(304).insert_header("ETag", "\"card-1\""))
        .mount(&server)
        .await;

    let fetched = client_for(&server)
        .fetch_card(
            42,
            9,
            5,
            &Validators {
                etag: Some("\"card-1\"".into()),
                last_modified: None,
            },
        )
        .await
        .unwrap();
    server.verify().await;

    assert!(fetched.data.is_none(), "304 must surface as cached data");
    assert_eq!(fetched.validators.etag.as_deref(), Some("\"card-1\""));
}
