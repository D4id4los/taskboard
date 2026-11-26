// SPDX-License-Identifier: MIT OR Apache-2.0
//! Tier-1 wiremock pins for `write_move_marker` (D-BF11, #1366): the
//! move-marker sidecar contract asserted entirely on the recorded wire —
//! wiremock request expectations and captured bodies — with the written
//! document round-tripped through the public domain API
//! (`AppDataDocument` → `moved_to_files()`, `AppDataSections`). One
//! `#[tokio::test]` per scenario S1–S10 of the chunk plan; every test
//! builds its own `MockServer`, so the suite is hermetic by
//! construction.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use chrono::TimeZone as _;
use taskboard_domain::board_move::{MOVED_TO_FILES_KEY, MovedToFilesMarker};
use taskboard_domain::{
    ANCHOR_CARD_TITLE, APPDATA_FORMAT, AppDataDocument, AppDataSections, RemoteBoardId, TaskEntry,
};
use taskboard_sync_nextcloud::DeckClient;
use taskboard_sync_nextcloud::move_marker::{MoveMarkerError, write_move_marker};
use uuid::Uuid;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

const BOARDS_PATH: &str = "/index.php/apps/deck/api/v1.0/boards";
const DECK_BOARD: u64 = 42;
/// The reserved stored filename of the sidecar document — the wire
/// face of `anchor::ANCHOR_ATTACHMENT_FILENAME` (`pub(crate)`, so
/// re-declared here as the literal the wire carries).
const SIDECAR_FILENAME: &str = "taskboard-appdata.json";
/// The app-level attachment-update route (the `deck_file` app route).
fn update_route(card: u64, attachment: u64) -> String {
    format!("/index.php/apps/deck/cards/{card}/attachment/deck_file:{attachment}")
}

fn client_for(server: &MockServer) -> DeckClient {
    DeckClient::new(&server.uri(), "it-user", "it-token").unwrap()
}

fn fixture(name: &str) -> String {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/deck")
        .join(name);
    std::fs::read_to_string(p).unwrap()
}

/// The anchored marker under test (`testing_strategy` §6.1: the fixture
/// is a fixed instant because the oracle is exact-instant equality —
/// nothing here reads the wall clock).
fn marker() -> MovedToFilesMarker {
    MovedToFilesMarker {
        files_board: Uuid::from_u128(0x9F0B7A61_4C2E_4D58_8A31_C2D9E5F70123),
        committed_at: chrono::Utc.with_ymd_and_hms(2026, 11, 9, 12, 0, 0).unwrap(),
    }
}

fn card_json(id: u64, title: &str, order: i64) -> serde_json::Value {
    serde_json::json!({
        "id": id, "title": title, "stackId": 7, "type": "plain", "order": order,
        "attachments": []
    })
}

/// Serializes a document exactly as the writer round-trips it, for use
/// as the mock sidecar's stored bytes.
fn document_bytes(document: &AppDataDocument) -> Vec<u8> {
    serde_json::to_vec(document).expect("the document serializes")
}

/// The live sidecar attachment of an anchor card (the only match the
/// writer's `deleted_at == 0` + filename filter accepts).
fn sidecar_attachment_json(id: u64, deleted_at: i64) -> String {
    serde_json::json!([{
        "id": id,
        "cardId": 20,
        "type": "deck_file",
        "deletedAt": deleted_at,
        "extendedData": {
            "fileName": SIDECAR_FILENAME,
            "size": 141,
            "mimetype": "application/json",
            "mtime": 1_760_000_000
        }
    }])
    .to_string()
}

// ---- multipart body extraction (verbatim from suite/contract.rs) ----

/// One parsed part of a `multipart/form-data` request body.
#[derive(Debug)]
struct MultipartPart {
    filename: Option<String>,
    body: Vec<u8>,
}

/// First occurrence of `needle` in `haystack`.
fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// Splits a haystack once at the first occurrence of `needle`.
fn split_once<'a>(haystack: &'a [u8], needle: &[u8]) -> (&'a [u8], &'a [u8]) {
    match find(haystack, needle) {
        Some(i) => (&haystack[..i], &haystack[i + needle.len()..]),
        None => (haystack, &[]),
    }
}

/// Splits a haystack at every occurrence of `needle` (which is itself never
/// a part of the returned segments).
fn split_all<'a>(haystack: &'a [u8], needle: &[u8]) -> Vec<&'a [u8]> {
    let mut segments = Vec::new();
    let mut rest = haystack;
    while let Some(i) = find(rest, needle) {
        segments.push(&rest[..i]);
        rest = &rest[i + needle.len()..];
    }
    segments.push(rest);
    segments
}

/// Hand-rolled multipart parser for the wiremock-recorded request bodies:
/// reads the boundary off the request's `Content-Type`, splits the parts,
/// and parses each part's `Content-Disposition` filename. (Slimmed from
/// the `contract.rs` original: these pins only need the file part.)
fn parse_multipart(content_type: &str, body: &[u8]) -> Vec<MultipartPart> {
    let boundary = content_type
        .split("boundary=")
        .nth(1)
        .expect("multipart content type must carry the boundary")
        .trim_matches('"');
    let delimiter = format!("--{boundary}");
    let mut parts = Vec::new();
    for segment in split_all(body, delimiter.as_bytes()) {
        let Some(segment) = segment.strip_prefix(b"\r\n") else {
            continue;
        };
        let segment = segment.strip_suffix(b"\r\n").unwrap_or(segment);
        let (header_block, content) = split_once(segment, b"\r\n\r\n");
        let headers = String::from_utf8_lossy(header_block);
        let mut filename = None;
        for line in headers.split('\n') {
            let line = line.trim_end_matches('\r');
            if let Some(value) = line.strip_prefix("Content-Disposition:") {
                for param in value.split(';').skip(1) {
                    let param = param.trim();
                    if let Some(f) = param.strip_prefix("filename=") {
                        filename = Some(f.trim_matches('"').to_owned());
                    }
                }
            }
        }
        parts.push(MultipartPart {
            filename,
            body: content.to_vec(),
        });
    }
    parts
}

/// Extracts the sidecar file part of a recorded multipart request and
/// round-trips it through the public domain API — the written document.
fn written_document(req: &Request) -> AppDataDocument {
    let content_type = req
        .headers
        .get("content-type")
        .expect("multipart content type")
        .to_str()
        .expect("ascii content type")
        .to_owned();
    let parts = parse_multipart(&content_type, &req.body);
    let file = parts
        .iter()
        .find(|p| p.filename.as_deref() == Some(SIDECAR_FILENAME))
        .expect("the sidecar file part must be present");
    serde_json::from_slice(&file.body).expect("the file part decodes to a document")
}

async fn requests_of(server: &MockServer) -> Vec<Request> {
    server.received_requests().await.unwrap()
}

fn paths_of(requests: &[Request]) -> Vec<String> {
    requests
        .iter()
        .map(|req| req.url.path().to_owned())
        .collect()
}

fn stacks_path() -> String {
    format!("{BOARDS_PATH}/{DECK_BOARD}/stacks")
}

/// A minimal live sidecar document (no marker yet).
fn plain_document() -> AppDataDocument {
    AppDataDocument {
        format: APPDATA_FORMAT,
        board: RemoteBoardId::deck(DECK_BOARD),
        sections: AppDataSections::default(),
        unknown: BTreeMap::new(),
    }
}

/// A document with the marker merged in — the settled sidecar the
/// recheck oracle accepts.
fn set_marker(mut document: AppDataDocument, marker: &MovedToFilesMarker) -> AppDataDocument {
    document.unknown.insert(
        MOVED_TO_FILES_KEY.to_owned(),
        serde_json::to_value(marker).unwrap(),
    );
    document
}

// ---- S1: fresh board, no anchor card, non-empty first stack ----------

#[tokio::test]
async fn s1_fresh_board_creates_anchor_card_and_uploads_fresh_document() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(stacks_path()))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(fixture("move_marker_stacks_without_anchor.json")),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(format!("{BOARDS_PATH}/{DECK_BOARD}/stacks/7/cards")))
        .respond_with(ResponseTemplate::new(200).set_body_json(card_json(99, ANCHOR_CARD_TITLE, 4)))
        .expect(1)
        .mount(&server)
        .await;
    // The freshly created anchor card's attachment listing is read before
    // the upload decision (the fresh-document branch reads as no sidecar).
    Mock::given(method("GET"))
        .and(path(format!(
            "{BOARDS_PATH}/{DECK_BOARD}/stacks/7/cards/99/attachments"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_string("[]"))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(format!(
            "{BOARDS_PATH}/{DECK_BOARD}/stacks/7/cards/99/attachments"
        )))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(fixture("attachment_create_response.json")),
        )
        .expect(1)
        .mount(&server)
        .await;

    write_move_marker(&client_for(&server), DECK_BOARD, marker())
        .await
        .expect("the marker write succeeds");

    let requests = requests_of(&server).await;
    assert_eq!(
        requests.len(),
        4,
        "stacks + create_card + attachment listing + upload, no more"
    );
    assert_eq!(
        paths_of(&requests),
        [
            stacks_path(),
            format!("{BOARDS_PATH}/{DECK_BOARD}/stacks/7/cards"),
            format!("{BOARDS_PATH}/{DECK_BOARD}/stacks/7/cards/99/attachments"),
            format!("{BOARDS_PATH}/{DECK_BOARD}/stacks/7/cards/99/attachments"),
        ],
        "no download or update traffic on a fresh board"
    );

    // The anchor card is created in the min-(order, id) stack at
    // max(existing order) + 1.
    let create: serde_json::Value = serde_json::from_slice(&requests[1].body).unwrap();
    assert_eq!(create["title"], ANCHOR_CARD_TITLE);
    assert_eq!(create["order"], 4);

    // The uploaded document: fresh shape plus exactly the marker.
    let document = written_document(&requests[3]);
    assert_eq!(document.format, APPDATA_FORMAT);
    assert_eq!(document.board.as_deck(), Some(DECK_BOARD));
    assert_eq!(document.sections, AppDataSections::default());
    assert_eq!(document.moved_to_files(), Some(marker()));
}

// ---- S2: anchor card already exists → update, not upload -------------

#[tokio::test]
async fn s2_existing_anchor_updates_the_sidecar_instead_of_uploading() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(stacks_path()))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(fixture("move_marker_stacks_with_anchor.json")),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!(
            "{BOARDS_PATH}/{DECK_BOARD}/stacks/7/cards/20/attachments"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_string(sidecar_attachment_json(5, 0)))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!(
            "{BOARDS_PATH}/{DECK_BOARD}/stacks/7/cards/20/attachments/5"
        )))
        // The initial read and the post-write recheck; the recheck sees
        // the settled document (the marker present), so one attempt ends
        // the loop.
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(document_bytes(&set_marker(plain_document(), &marker()))),
        )
        .expect(2)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(update_route(20, 5)))
        .and(query_param("boardId", DECK_BOARD.to_string()))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(fixture("attachment_create_response.json")),
        )
        .expect(1)
        .mount(&server)
        .await;

    write_move_marker(&client_for(&server), DECK_BOARD, marker())
        .await
        .expect("the marker write succeeds");

    let requests = requests_of(&server).await;
    assert_eq!(requests.len(), 5);
    assert_eq!(
        paths_of(&requests),
        [
            stacks_path(),
            format!("{BOARDS_PATH}/{DECK_BOARD}/stacks/7/cards/20/attachments"),
            format!("{BOARDS_PATH}/{DECK_BOARD}/stacks/7/cards/20/attachments/5"),
            update_route(20, 5),
            format!("{BOARDS_PATH}/{DECK_BOARD}/stacks/7/cards/20/attachments/5"),
        ],
        "the existing sidecar is updated, never re-uploaded; one recheck read"
    );

    let document = written_document(&requests[3]);
    assert_eq!(document.moved_to_files(), Some(marker()));
    assert_eq!(document.sections, AppDataSections::default());
}

// ---- S3: idempotence on re-run ----------------------------------------

#[tokio::test]
async fn s3_rerun_writes_the_same_document_and_never_creates_a_card() {
    let server = MockServer::start().await;
    // The scripted server: the stored sidecar bytes mutate only through
    // recorded update requests, so run 2 downloads exactly run 1's write.
    let stored: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(document_bytes(&plain_document())));
    let stored_for_download = stored.clone();
    Mock::given(method("GET"))
        .and(path(stacks_path()))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(fixture("move_marker_stacks_with_anchor.json")),
        )
        .expect(2)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!(
            "{BOARDS_PATH}/{DECK_BOARD}/stacks/7/cards/20/attachments"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_string(sidecar_attachment_json(5, 0)))
        .expect(2)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!(
            "{BOARDS_PATH}/{DECK_BOARD}/stacks/7/cards/20/attachments/5"
        )))
        .respond_with(move |_: &Request| {
            let bytes = stored_for_download.lock().unwrap().clone();
            ResponseTemplate::new(200).set_body_bytes(bytes)
        })
        // Two runs × (initial read + recheck).
        .expect(4)
        .mount(&server)
        .await;
    let stored_for_update = stored.clone();
    Mock::given(method("POST"))
        .and(path(update_route(20, 5)))
        .and(query_param("boardId", DECK_BOARD.to_string()))
        .respond_with(move |req: &Request| {
            let content_type = req
                .headers
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .unwrap_or_default()
                .to_owned();
            let file = parse_multipart(&content_type, &req.body)
                .into_iter()
                .find(|p| p.filename.as_deref() == Some(SIDECAR_FILENAME));
            if let Some(file) = file {
                *stored_for_update.lock().unwrap() = file.body;
            }
            ResponseTemplate::new(200).set_body_string(fixture("attachment_create_response.json"))
        })
        // Two runs × one update per run.
        .expect(2)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(format!("{BOARDS_PATH}/{DECK_BOARD}/stacks/7/cards")))
        .respond_with(ResponseTemplate::new(200).set_body_json(card_json(99, "nope", 0)))
        .expect(0)
        .mount(&server)
        .await;

    let client = client_for(&server);
    write_move_marker(&client, DECK_BOARD, marker())
        .await
        .expect("run 1 succeeds");
    let after_run_1: AppDataDocument =
        serde_json::from_slice(&stored.lock().unwrap().clone()).unwrap();
    write_move_marker(&client, DECK_BOARD, marker())
        .await
        .expect("run 2 succeeds");
    let after_run_2: AppDataDocument =
        serde_json::from_slice(&stored.lock().unwrap().clone()).unwrap();

    assert_eq!(after_run_2, after_run_1, "run 2 rewrites the same document");
    assert_eq!(after_run_2.moved_to_files(), Some(marker()));
    assert_eq!(after_run_2.sections, AppDataSections::default());

    let paths = paths_of(&requests_of(&server).await);
    assert_eq!(
        paths.iter().filter(|p| **p == stacks_path()).count(),
        2,
        "two stacks reads, one per run"
    );
    assert_eq!(
        paths.iter().filter(|p| p.ends_with("/cards")).count(),
        0,
        "no create_card traffic in either run"
    );
    server.verify().await;
}

// ---- S4: merge must preserve the live lane's writes -------------------

#[tokio::test]
async fn s4_merge_preserves_live_sections_and_foreign_keys_verbatim() {
    let server = MockServer::start().await;
    // The scripted server: the stored sidecar bytes mutate only through
    // recorded update requests, so the post-write recheck observes the
    // merged document (live sections + foreign key + marker) and one
    // attempt settles the loop.
    let stored: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(
        fixture("move_marker_sidecar_live_document.json").into_bytes(),
    ));
    let stored_for_download = stored.clone();
    Mock::given(method("GET"))
        .and(path(stacks_path()))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(fixture("move_marker_stacks_with_anchor.json")),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!(
            "{BOARDS_PATH}/{DECK_BOARD}/stacks/7/cards/20/attachments"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_string(sidecar_attachment_json(5, 0)))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!(
            "{BOARDS_PATH}/{DECK_BOARD}/stacks/7/cards/20/attachments/5"
        )))
        .respond_with(move |_: &Request| {
            let bytes = stored_for_download.lock().unwrap().clone();
            ResponseTemplate::new(200).set_body_bytes(bytes)
        })
        .expect(2)
        .mount(&server)
        .await;
    let stored_for_update = stored.clone();
    Mock::given(method("POST"))
        .and(path(update_route(20, 5)))
        .and(query_param("boardId", DECK_BOARD.to_string()))
        .respond_with(move |req: &Request| {
            let content_type = req
                .headers
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .unwrap_or_default()
                .to_owned();
            let file = parse_multipart(&content_type, &req.body)
                .into_iter()
                .find(|p| p.filename.as_deref() == Some(SIDECAR_FILENAME));
            if let Some(file) = file {
                *stored_for_update.lock().unwrap() = file.body;
            }
            ResponseTemplate::new(200).set_body_string(fixture("attachment_create_response.json"))
        })
        .expect(1)
        .mount(&server)
        .await;

    write_move_marker(&client_for(&server), DECK_BOARD, marker())
        .await
        .expect("the marker write succeeds");

    let requests = requests_of(&server).await;
    assert_eq!(requests.len(), 5, "stacks + list + read + update + recheck");
    let document = written_document(&requests[3]);

    // The live lane's task entry survives verbatim.
    let expected_sections = AppDataSections {
        tasks: Some(BTreeMap::from([(
            "21".to_owned(),
            TaskEntry {
                fields: BTreeMap::new(),
                unmodeled: BTreeMap::from([("live_note".to_owned(), serde_json::json!("keep me"))]),
            },
        )])),
        ..AppDataSections::default()
    };
    assert_eq!(document.sections, expected_sections);
    // The foreign envelope key survives verbatim.
    assert_eq!(
        document.unknown.get("live_lane_key"),
        Some(&serde_json::json!({"kept": true})),
    );
    // Only the moved_to_files key was replaced.
    assert_eq!(document.moved_to_files(), Some(marker()));
}

// ---- S5/S6: already-deleted / forbidden board → typed no-op arms ------

#[tokio::test]
async fn s5_stacks_not_found_is_a_no_op() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(stacks_path()))
        .respond_with(ResponseTemplate::new(404))
        .expect(1)
        .mount(&server)
        .await;

    let result = write_move_marker(&client_for(&server), DECK_BOARD, marker()).await;

    assert!(matches!(result, Ok(())));
    assert_eq!(
        requests_of(&server).await.len(),
        1,
        "a deleted board gets exactly the stacks probe and nothing else"
    );
}

#[tokio::test]
async fn s6_stacks_forbidden_is_a_no_op() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(stacks_path()))
        .respond_with(ResponseTemplate::new(403))
        .expect(1)
        .mount(&server)
        .await;

    let result = write_move_marker(&client_for(&server), DECK_BOARD, marker()).await;

    assert!(matches!(result, Ok(())));
    assert_eq!(requests_of(&server).await.len(), 1);
}

// ---- S7: anchor-title dedup targets the min card id -------------------

#[tokio::test]
async fn s7_dedup_targets_the_lowest_anchor_card_id_only() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(stacks_path()))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(fixture("move_marker_stacks_dedup.json")),
        )
        .expect(1)
        .mount(&server)
        .await;
    // Card 20 (stack 8) wins over card 30 (stack 7): the lower id.
    Mock::given(method("GET"))
        .and(path(format!(
            "{BOARDS_PATH}/{DECK_BOARD}/stacks/8/cards/20/attachments"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_string("[]"))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(format!(
            "{BOARDS_PATH}/{DECK_BOARD}/stacks/8/cards/20/attachments"
        )))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(fixture("attachment_create_response.json")),
        )
        .expect(1)
        .mount(&server)
        .await;

    write_move_marker(&client_for(&server), DECK_BOARD, marker())
        .await
        .expect("the marker write succeeds");

    let requests = requests_of(&server).await;
    assert_eq!(requests.len(), 3);
    let paths = paths_of(&requests[1..]);
    assert!(
        paths.iter().all(|p| p.contains("/stacks/8/cards/20/")),
        "all sidecar traffic targets card 20: {paths:?}"
    );
    assert!(
        !paths.iter().any(|p| p.contains("/cards/30")),
        "card 30 is never contacted"
    );
    let document = written_document(&requests[2]);
    assert_eq!(document.moved_to_files(), Some(marker()));
}

// ---- S8: only a soft-deleted sidecar exists → fresh upload ------------

#[tokio::test]
async fn s8_soft_deleted_sidecar_is_filtered_and_a_fresh_document_is_uploaded() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(stacks_path()))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(fixture("move_marker_stacks_with_anchor.json")),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!(
            "{BOARDS_PATH}/{DECK_BOARD}/stacks/7/cards/20/attachments"
        )))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(sidecar_attachment_json(6, 1_762_000_000)),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!(
            "{BOARDS_PATH}/{DECK_BOARD}/stacks/7/cards/20/attachments/6"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(document_bytes(&plain_document())))
        .expect(0)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(format!(
            "{BOARDS_PATH}/{DECK_BOARD}/stacks/7/cards/20/attachments"
        )))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(fixture("attachment_create_response.json")),
        )
        .expect(1)
        .mount(&server)
        .await;

    write_move_marker(&client_for(&server), DECK_BOARD, marker())
        .await
        .expect("the marker write succeeds");

    let requests = requests_of(&server).await;
    assert_eq!(requests.len(), 3, "no download: the tombstone is skipped");
    let document = written_document(&requests[2]);
    assert_eq!(document.sections, AppDataSections::default());
    assert_eq!(document.moved_to_files(), Some(marker()));
}

// ---- S9: unreadable sidecar is never rewritten ------------------------

#[tokio::test]
async fn s9_unreadable_sidecar_fails_typedly_with_zero_write_requests() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(stacks_path()))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(fixture("move_marker_stacks_with_anchor.json")),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!(
            "{BOARDS_PATH}/{DECK_BOARD}/stacks/7/cards/20/attachments"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_string(sidecar_attachment_json(5, 0)))
        .expect(1)
        .mount(&server)
        .await;
    // Not decodable as a document: no `format` key at all.
    Mock::given(method("GET"))
        .and(path(format!(
            "{BOARDS_PATH}/{DECK_BOARD}/stacks/7/cards/20/attachments/5"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(br#"{"board": 42}"#.to_vec()))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(update_route(20, 5)))
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&server)
        .await;

    let result = write_move_marker(&client_for(&server), DECK_BOARD, marker()).await;

    assert!(
        matches!(result, Err(MoveMarkerError::UnreadableDocument)),
        "the unreadable sidecar is a typed refusal: {result:?}"
    );
    assert_eq!(
        requests_of(&server).await.len(),
        3,
        "stacks + listing + download only — the sidecar is never destroyed"
    );
    server.verify().await;
}

// ---- S10: stacks ok but empty → the typed no-op -----------------------

#[tokio::test]
async fn s10_empty_stack_list_is_a_typed_no_op() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(stacks_path()))
        .respond_with(ResponseTemplate::new(200).set_body_string("[]"))
        .expect(1)
        .mount(&server)
        .await;

    let result = write_move_marker(&client_for(&server), DECK_BOARD, marker()).await;

    assert!(matches!(result, Ok(())));
    assert_eq!(requests_of(&server).await.len(), 1);
}

// ---- #1385: the sidecar concurrency guard -----------------------------
//
// The attachment content route serves no `ETag`/`If-Match` validators,
// so a concurrent sidecar writer's PUT silently clobbers the marker
// write (last write wins). The guard is a bounded read→write→recheck
// loop: after each update the sidecar is re-downloaded and the write is
// settled iff the server document equals its own merge with the marker;
// on mismatch the fresh document becomes the next merge base, bounded
// at three attempts with a typed `Contended` exhaustion outcome.

/// A sidecar document with one foreign envelope key.
fn document_with_unknown(key: &str, value: serde_json::Value) -> AppDataDocument {
    let mut document = plain_document();
    document.unknown.insert(key.to_owned(), value);
    document
}

/// Scripted download responses: each call pops the next body, so the
/// interleaving is deterministic — no sleeps, no polling.
fn scripted_downloads(
    bodies: Vec<AppDataDocument>,
) -> impl Fn(&Request) -> ResponseTemplate + 'static {
    let bodies = Mutex::new(
        bodies
            .into_iter()
            .map(|d| document_bytes(&d))
            .collect::<std::collections::VecDeque<_>>(),
    );
    move |_: &Request| {
        let mut queue = bodies.lock().unwrap();
        let bytes = queue
            .pop_front()
            .expect("the download script is not exhausted");
        ResponseTemplate::new(200).set_body_bytes(bytes)
    }
}

/// P1 — the lost-update pin: a concurrent sidecar writer's PUT lands
/// between our update and our recheck (the recheck sees a marker-less
/// document with the peer's evolved keys), so the guard retries with the
/// peer's document as the merge base; the settled write carries the
/// marker AND both of the peer's keys.
#[tokio::test]
async fn p1_concurrent_sidecar_write_is_not_lost() {
    let server = MockServer::start().await;
    let peer_base = document_with_unknown("live_key_b", serde_json::json!(1));
    // The concurrent writer's output: our first PUT was clobbered —
    // the peer's keys evolved/new, and the marker is gone.
    let peer_clobber = AppDataDocument {
        unknown: BTreeMap::from([
            ("live_key_b".to_owned(), serde_json::json!(2)),
            ("live_key_c".to_owned(), serde_json::json!(true)),
        ]),
        ..plain_document()
    };
    let settled = set_marker(peer_clobber.clone(), &marker());

    Mock::given(method("GET"))
        .and(path(stacks_path()))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(fixture("move_marker_stacks_with_anchor.json")),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!(
            "{BOARDS_PATH}/{DECK_BOARD}/stacks/7/cards/20/attachments"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_string(sidecar_attachment_json(5, 0)))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!(
            "{BOARDS_PATH}/{DECK_BOARD}/stacks/7/cards/20/attachments/5"
        )))
        .respond_with(scripted_downloads(vec![peer_base, peer_clobber, settled]))
        .expect(3)
        .mount(&server)
        .await;
    let updates: Arc<Mutex<Vec<Vec<u8>>>> = Arc::new(Mutex::new(Vec::new()));
    let updates_for_mock = Arc::clone(&updates);
    Mock::given(method("POST"))
        .and(path(update_route(20, 5)))
        .and(query_param("boardId", DECK_BOARD.to_string()))
        .respond_with(move |req: &Request| {
            let content_type = req
                .headers
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .unwrap_or_default()
                .to_owned();
            let file = parse_multipart(&content_type, &req.body)
                .into_iter()
                .find(|p| p.filename.as_deref() == Some(SIDECAR_FILENAME));
            if let Some(file) = file {
                updates_for_mock.lock().unwrap().push(file.body);
            }
            ResponseTemplate::new(200).set_body_string(fixture("attachment_create_response.json"))
        })
        .expect(2)
        .mount(&server)
        .await;

    let result = write_move_marker(&client_for(&server), DECK_BOARD, marker()).await;

    assert!(
        matches!(result, Ok(())),
        "the guard retries through the clobber: {result:?}"
    );
    // The scripted wire: read → write → recheck → re-merge → write →
    // recheck, nothing else.
    assert_eq!(
        paths_of(&requests_of(&server).await),
        [
            stacks_path(),
            format!("{BOARDS_PATH}/{DECK_BOARD}/stacks/7/cards/20/attachments"),
            format!("{BOARDS_PATH}/{DECK_BOARD}/stacks/7/cards/20/attachments/5"),
            update_route(20, 5),
            format!("{BOARDS_PATH}/{DECK_BOARD}/stacks/7/cards/20/attachments/5"),
            update_route(20, 5),
            format!("{BOARDS_PATH}/{DECK_BOARD}/stacks/7/cards/20/attachments/5"),
        ],
    );
    let updates = updates.lock().unwrap();
    assert_eq!(updates.len(), 2, "exactly two updates, the retry included");
    // The final captured multipart body: the marker merged into the
    // peer's clobbering document — nothing lost in either direction.
    let settled_write: AppDataDocument = serde_json::from_slice(&updates[1]).unwrap();
    assert_eq!(settled_write.moved_to_files(), Some(marker()));
    assert_eq!(
        settled_write.unknown.get("live_key_b"),
        Some(&serde_json::json!(2))
    );
    assert_eq!(
        settled_write.unknown.get("live_key_c"),
        Some(&serde_json::json!(true))
    );
    // The first attempt's write was based on the pre-clobber document.
    let first_write: AppDataDocument = serde_json::from_slice(&updates[0]).unwrap();
    assert_eq!(
        first_write.unknown.get("live_key_b"),
        Some(&serde_json::json!(1))
    );
    assert!(!first_write.unknown.contains_key("live_key_c"));
}

/// P2 — contention exhaustion: every recheck observes a marker-less
/// document (the peer keeps winning), so the loop stops after exactly
/// three attempts — exactly 3 updates and exactly 4 downloads (the
/// initial read plus one recheck per attempt) — and reports the typed
/// exhaustion outcome.
#[tokio::test]
async fn p2_contention_exhaustion_is_a_typed_error() {
    let server = MockServer::start().await;
    // The peer's document never carries the marker: the recheck oracle
    // never settles.
    let peer_document = document_with_unknown("live_key_b", serde_json::json!(7));
    Mock::given(method("GET"))
        .and(path(stacks_path()))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(fixture("move_marker_stacks_with_anchor.json")),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!(
            "{BOARDS_PATH}/{DECK_BOARD}/stacks/7/cards/20/attachments"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_string(sidecar_attachment_json(5, 0)))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!(
            "{BOARDS_PATH}/{DECK_BOARD}/stacks/7/cards/20/attachments/5"
        )))
        .respond_with(scripted_downloads(vec![
            peer_document.clone(),
            peer_document.clone(),
            peer_document.clone(),
            peer_document,
        ]))
        .expect(4)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(update_route(20, 5)))
        .and(query_param("boardId", DECK_BOARD.to_string()))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(fixture("attachment_create_response.json")),
        )
        .expect(3)
        .mount(&server)
        .await;

    let result = write_move_marker(&client_for(&server), DECK_BOARD, marker()).await;

    assert!(
        matches!(result, Err(MoveMarkerError::Contended)),
        "exhaustion is the typed outcome: {result:?}"
    );
    let paths = paths_of(&requests_of(&server).await);
    assert_eq!(
        paths
            .iter()
            .filter(|p| p.ends_with("/attachments/5"))
            .count(),
        4,
        "initial read + one recheck per attempt, no more"
    );
    assert_eq!(
        paths
            .iter()
            .filter(|p| p.as_str() == update_route(20, 5))
            .count(),
        3,
        "bounded at three attempts"
    );
}

/// P3 — the stable-sidecar regression guard: when no concurrent writer
/// interferes, the recheck observes exactly what we wrote and the happy
/// path costs one extra GET — one initial read, one update, one recheck
/// — never a retry cycle.
#[tokio::test]
async fn p3_stable_sidecar_write_is_a_single_update() {
    let server = MockServer::start().await;
    // The scripted server: the stored bytes mutate only through the
    // recorded update, so the recheck observes the written document.
    let stored: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(document_bytes(&plain_document())));
    let stored_for_download = stored.clone();
    Mock::given(method("GET"))
        .and(path(stacks_path()))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(fixture("move_marker_stacks_with_anchor.json")),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!(
            "{BOARDS_PATH}/{DECK_BOARD}/stacks/7/cards/20/attachments"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_string(sidecar_attachment_json(5, 0)))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!(
            "{BOARDS_PATH}/{DECK_BOARD}/stacks/7/cards/20/attachments/5"
        )))
        .respond_with(move |_: &Request| {
            let bytes = stored_for_download.lock().unwrap().clone();
            ResponseTemplate::new(200).set_body_bytes(bytes)
        })
        .expect(2)
        .mount(&server)
        .await;
    let stored_for_update = stored.clone();
    Mock::given(method("POST"))
        .and(path(update_route(20, 5)))
        .and(query_param("boardId", DECK_BOARD.to_string()))
        .respond_with(move |req: &Request| {
            let content_type = req
                .headers
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .unwrap_or_default()
                .to_owned();
            let file = parse_multipart(&content_type, &req.body)
                .into_iter()
                .find(|p| p.filename.as_deref() == Some(SIDECAR_FILENAME));
            if let Some(file) = file {
                *stored_for_update.lock().unwrap() = file.body;
            }
            ResponseTemplate::new(200).set_body_string(fixture("attachment_create_response.json"))
        })
        .expect(1)
        .mount(&server)
        .await;

    let result = write_move_marker(&client_for(&server), DECK_BOARD, marker()).await;

    assert!(matches!(result, Ok(())));
    assert_eq!(
        paths_of(&requests_of(&server).await),
        [
            stacks_path(),
            format!("{BOARDS_PATH}/{DECK_BOARD}/stacks/7/cards/20/attachments"),
            format!("{BOARDS_PATH}/{DECK_BOARD}/stacks/7/cards/20/attachments/5"),
            update_route(20, 5),
            format!("{BOARDS_PATH}/{DECK_BOARD}/stacks/7/cards/20/attachments/5"),
        ],
        "one read, one update, one recheck — the happy path"
    );
    let settled: AppDataDocument = serde_json::from_slice(&stored.lock().unwrap().clone()).unwrap();
    assert_eq!(settled.moved_to_files(), Some(marker()));
}
