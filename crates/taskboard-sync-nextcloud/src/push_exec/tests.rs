// SPDX-License-Identifier: MIT OR Apache-2.0
//! The push executor behavior suite, moved wholesale from the former
//! single-file `push_exec.rs`.

use std::collections::BTreeMap;

use taskboard_domain::{
    CardShape, DeckFeatures, EntityTables, LocalOp, MaterializedOp, PendingOp, PushGroup,
    PushResult, RemoteBoardId, RemoteCardRef, RemoteEcho, RemoteIndex, RemoteLabelId,
    RemoteStackId,
};

use uuid::Uuid;
use wiremock::matchers::{body_partial_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::client::DeckClient;

use super::*;

/// The executor's flag fixture: the capability-true arm — every existing
/// wire assertion (valued `startdate` carried through) was written
/// against a startdate-capable server and keeps its semantics.
const FEATURES: DeckFeatures = DeckFeatures::NATIVE;

fn uid(raw: u128) -> Uuid {
    Uuid::from_u128(raw)
}

fn task_id(raw: u128) -> taskboard_domain::TaskId {
    taskboard_domain::TaskId::from(uid(raw))
}

fn stack_id(raw: u128) -> taskboard_domain::StackId {
    taskboard_domain::StackId::from(uid(raw))
}

fn label_id(raw: u128) -> taskboard_domain::LabelId {
    taskboard_domain::LabelId::from(uid(raw))
}

fn op(raw: u128, kind: LocalOp) -> PendingOp {
    PendingOp {
        op_id: taskboard_domain::OpId(uid(raw)),
        op: kind,
        queued_at: chrono::Utc::now(),
    }
}

fn group(
    primary_raw: u128,
    materialized: MaterializedOp,
    target: taskboard_domain::PushTarget,
    subsumed: &[u128],
) -> PushGroup {
    PushGroup {
        target,
        materialized,
        primary: taskboard_domain::OpId(uid(primary_raw)),
        subsumed: subsumed
            .iter()
            .map(|raw| taskboard_domain::OpId(uid(*raw)))
            .collect(),
    }
}

fn empty_tables() -> EntityTables<'static> {
    EntityTables {
        tasks: Box::leak(Box::new(BTreeMap::new())),
        stacks: Box::leak(Box::new(BTreeMap::new())),
        labels: Box::leak(Box::new(BTreeMap::new())),
        boards: Box::leak(Box::new(BTreeMap::new())),
    }
}

fn card_json(id: u64, stack_id: u64, order: i64) -> String {
    serde_json::json!({
        "id": id, "title": "old", "stackId": stack_id, "type": "plain",
        "order": order, "labels": [], "archived": false,
        "duedate": null, "done": null
    })
    .to_string()
}

fn stack_json(id: u64, title: &str, order: i64) -> String {
    serde_json::json!({"id": id, "title": title, "order": order, "archived": false}).to_string()
}

fn board_json(id: u64, title: &str) -> String {
    serde_json::json!({
        "id": id, "title": title, "color": "00c2e0",
        "lastModified": 1_700_000_000, "deletedAt": 0,
        "archived": false, "labels": []
    })
    .to_string()
}

fn label_json(id: u64, title: &str, color: &str) -> String {
    serde_json::json!({"id": id, "title": title, "color": color}).to_string()
}

fn stack_row(raw: u128, title: &str, order: i64) -> taskboard_domain::Stack {
    let now = chrono::Utc::now();
    taskboard_domain::Stack {
        id: stack_id(raw),
        remote: None,
        board: taskboard_domain::BoardId::from(uid(1)),
        title: title.into(),
        order,
        archived: false,
        deleted: false,
        clocks: taskboard_domain::StackClocks {
            title: now,
            order: now,
            deleted: now,
        },
        remote_seen: None,
    }
}

fn label_row(raw: u128, title: &str, color: &str) -> taskboard_domain::Label {
    let now = chrono::Utc::now();
    taskboard_domain::Label {
        id: label_id(raw),
        remote: None,
        board: taskboard_domain::BoardId::from(uid(1)),
        title: title.into(),
        color: taskboard_domain::Color::new(color),
        deleted: false,
        clocks: taskboard_domain::LabelClocks {
            title: now,
            color: now,
            deleted: now,
        },
        remote_seen: None,
    }
}

fn tables_with(
    stacks: &[taskboard_domain::Stack],
    labels: &[taskboard_domain::Label],
) -> EntityTables<'static> {
    EntityTables {
        tasks: Box::leak(Box::new(BTreeMap::new())),
        stacks: Box::leak(Box::new(stacks.iter().map(|s| (s.id, s.clone())).collect())),
        labels: Box::leak(Box::new(labels.iter().map(|l| (l.id, l.clone())).collect())),
        boards: Box::leak(Box::new(BTreeMap::new())),
    }
}

fn board_id(raw: u128) -> taskboard_domain::BoardId {
    taskboard_domain::BoardId::from(uid(raw))
}

fn board_row(raw: u128, title: &str) -> taskboard_domain::Board {
    let now = chrono::Utc::now();
    taskboard_domain::Board {
        id: taskboard_domain::BoardId::from(uid(raw)),
        remote: Some(taskboard_domain::RemoteBoardBinding {
            connector: taskboard_domain::ConnectorId::from("nextcloud"),
            board: RemoteBoardId(42),
        }),
        title: title.into(),
        color: taskboard_domain::Color::new("00c2e0"),
        archived: false,
        deleted: false,
        clocks: taskboard_domain::BoardClocks {
            title: now,
            color: now,
            archived: now,
            deleted: now,
        },
        remote_seen: None,
    }
}

fn tables_with_boards(boards: &[taskboard_domain::Board]) -> EntityTables<'static> {
    EntityTables {
        tasks: Box::leak(Box::new(BTreeMap::new())),
        stacks: Box::leak(Box::new(BTreeMap::new())),
        labels: Box::leak(Box::new(BTreeMap::new())),
        boards: Box::leak(Box::new(boards.iter().map(|b| (b.id, b.clone())).collect())),
    }
}

/// A refused local port: deterministically transport-class. The
/// listener is dropped before use — a bound-but-unaccepted socket
/// would complete the TCP handshake and hang the client instead of
/// refusing it. The refusal is verified, not assumed: in the
/// drop-to-connect window a concurrently starting server can win the
/// same ephemeral port, so any probe outcome other than
/// `ConnectionRefused` triggers bounded regeneration with a fresh
/// port.
fn refused_client() -> DeckClient {
    const MAX_ATTEMPTS: u32 = 10;
    for attempt in 1..=MAX_ATTEMPTS {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        match std::net::TcpStream::connect_timeout(
            &std::net::SocketAddr::from(([127, 0, 0, 1], port)),
            std::time::Duration::from_millis(100),
        ) {
            Err(err) if err.kind() == std::io::ErrorKind::ConnectionRefused => {
                if attempt > 1 {
                    let regenerations = attempt - 1;
                    eprintln!("refused_client: regenerated port {regenerations} time(s)");
                }
                return DeckClient::new(&format!("http://127.0.0.1:{port}"), "u", "t").unwrap();
            }
            other => {
                let why = match &other {
                    Ok(_) => "connect succeeded (port won by another process)".to_string(),
                    Err(err) => format!("probe error: {err}"),
                };
                eprintln!("refused_client: port {port} not refusing ({why}), regenerating");
            }
        }
    }
    panic!("refused_client: no refusing port found after {MAX_ATTEMPTS} attempts");
}

fn overlay_with(
    board: u64,
    stacks: &[(u128, u64)],
    cards: &[(u128, u64, u64)],
    labels: &[(u128, u64)],
) -> BindingOverlay {
    let board = RemoteBoardId(board);
    let index = RemoteIndex::from_bindings(
        cards.iter().map(|(local, stack, card)| {
            (
                RemoteCardRef {
                    board,
                    stack: RemoteStackId(*stack),
                    card: taskboard_domain::RemoteCardId(*card),
                },
                taskboard_domain::TaskId::from(uid(*local)),
            )
        }),
        stacks.iter().map(|(local, stack)| {
            (
                taskboard_domain::RemoteStackRef {
                    board,
                    stack: RemoteStackId(*stack),
                },
                stack_id(*local),
            )
        }),
        labels.iter().map(|(local, label)| {
            (
                taskboard_domain::RemoteLabelRef {
                    board,
                    label: RemoteLabelId(*label),
                },
                label_id(*local),
            )
        }),
    );
    BindingOverlay::from_index(board, &index)
}

/// The overlay is scoped to the named board: refs naming a *different*
/// remote board are invisible (their local ids resolve to nothing), so
/// `card_of` can never stamp this cycle's board id onto them at N ≥ 2
/// boards on one connector.
#[test]
fn from_index_scopes_the_overlay_to_the_named_board() {
    let index = RemoteIndex::from_bindings(
        [
            (
                RemoteCardRef {
                    board: RemoteBoardId(1),
                    stack: RemoteStackId(10),
                    card: taskboard_domain::RemoteCardId(100),
                },
                task_id(1),
            ),
            (
                RemoteCardRef {
                    board: RemoteBoardId(2),
                    stack: RemoteStackId(20),
                    card: taskboard_domain::RemoteCardId(200),
                },
                task_id(2),
            ),
        ]
        .into_iter(),
        [
            (
                taskboard_domain::RemoteStackRef {
                    board: RemoteBoardId(1),
                    stack: RemoteStackId(10),
                },
                stack_id(3),
            ),
            (
                taskboard_domain::RemoteStackRef {
                    board: RemoteBoardId(2),
                    stack: RemoteStackId(20),
                },
                stack_id(4),
            ),
        ]
        .into_iter(),
        std::iter::empty(),
    );

    let overlay = BindingOverlay::from_index(RemoteBoardId(1), &index);
    assert_eq!(
        overlay.card_of(task_id(1)),
        Some(RemoteCardRef {
            board: RemoteBoardId(1),
            stack: RemoteStackId(10),
            card: taskboard_domain::RemoteCardId(100),
        }),
        "the named board's own ref resolves fully",
    );
    assert_eq!(
        overlay.card_of(task_id(2)),
        None,
        "another board's ref is invisible to this overlay"
    );
    assert_eq!(
        overlay.stack_by_local.get(&stack_id(3)),
        Some(&RemoteStackId(10)),
    );
    assert_eq!(
        overlay.stack_by_local.get(&stack_id(4)),
        None,
        "another board's stack binding is not in this overlay"
    );
}

#[tokio::test]
async fn create_binds_the_echo_and_rides_label_assignments() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(
            "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_string(card_json(55, 7, 0)))
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path(
            "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55/assignLabel",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_string(card_json(55, 7, 0)))
        .mount(&server)
        .await;

    let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
    let mut exec = PushExecutor::new(
        &client,
        overlay_with(42, &[(5, 7)], &[], &[(6, 3)]),
        FEATURES,
    );
    let outbox = vec![
        op(100, LocalOp::CreateTask(task_id(10))),
        op(101, LocalOp::AssignLabel(task_id(10), label_id(6))),
    ];
    let shape = taskboard_domain::NewCardShape {
        title: "created".into(),
        order: 0,
        description: String::new(),
        duedate: None,
        scheduled: None,
        done: None,
        archived: false,
        labels: std::collections::BTreeSet::new(),
    };
    let outcome = exec
        .execute(
            &group(
                100,
                MaterializedOp::CreateTask {
                    task: task_id(10),
                    stack: stack_id(5),
                    new_card: shape,
                },
                taskboard_domain::PushTarget::Task(task_id(10)),
                &[101],
            ),
            &outbox,
            empty_tables(),
        )
        .await;

    let GroupOutcome::Outcomes(outcomes) = outcome else {
        panic!("expected outcomes");
    };
    assert_eq!(outcomes.len(), 2);
    assert_eq!(outcomes[0].op, taskboard_domain::OpId(uid(100)));
    assert!(matches!(
        &outcomes[0].result,
        PushResult::Applied { echo: Some(RemoteEcho::Task(view)) }
            if view.id.card == taskboard_domain::RemoteCardId(55)
    ));
    assert_eq!(outcomes[1].op, taskboard_domain::OpId(uid(101)));
    assert!(matches!(
        outcomes[1].result,
        PushResult::Applied { echo: None }
    ));
    assert_eq!(
        server.received_requests().await.unwrap().len(),
        2,
        "one create POST plus one assignLabel PUT"
    );
    // The echo extended the overlay: the created card is now resolvable.
    assert!(exec.overlay().card_of(task_id(10)).is_some());
}

#[tokio::test]
async fn create_with_an_unresolved_stack_rejects_local_data_without_sending() {
    let server = MockServer::start().await;
    let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
    let mut exec = PushExecutor::new(&client, overlay_with(42, &[], &[], &[]), FEATURES);
    let shape = taskboard_domain::NewCardShape {
        title: "created".into(),
        order: 0,
        description: String::new(),
        duedate: None,
        scheduled: None,
        done: None,
        archived: false,
        labels: std::collections::BTreeSet::new(),
    };
    let outcome = exec
        .execute(
            &group(
                100,
                MaterializedOp::CreateTask {
                    task: task_id(10),
                    stack: stack_id(5),
                    new_card: shape,
                },
                taskboard_domain::PushTarget::Task(task_id(10)),
                &[],
            ),
            &[],
            empty_tables(),
        )
        .await;

    let GroupOutcome::Outcomes(outcomes) = outcome else {
        panic!("expected outcomes");
    };
    assert!(matches!(
        outcomes[0].result,
        PushResult::Rejected {
            kind: taskboard_domain::SyncErrorKind::LocalData
        }
    ));
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn delete_of_an_unbound_entity_completes_synthetic_without_sending() {
    let server = MockServer::start().await;
    let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
    let mut exec = PushExecutor::new(&client, overlay_with(42, &[], &[], &[]), FEATURES);
    let outbox = vec![
        op(100, LocalOp::CreateTask(task_id(10))),
        op(101, LocalOp::DeleteTask(task_id(10))),
    ];
    let outcome = exec
        .execute(
            &group(
                101,
                MaterializedOp::DeleteTask { task: task_id(10) },
                taskboard_domain::PushTarget::Task(task_id(10)),
                &[100],
            ),
            &outbox,
            empty_tables(),
        )
        .await;

    let GroupOutcome::Outcomes(outcomes) = outcome else {
        panic!("expected outcomes");
    };
    assert_eq!(outcomes.len(), 2, "both ops complete synthetically");
    assert!(
        outcomes
            .iter()
            .all(|o| matches!(o.result, PushResult::Applied { echo: None }))
    );
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn update_preflights_the_card_then_puts_the_full_body() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(
            "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_string(card_json(55, 7, 3)))
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path(
            "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_string(card_json(55, 7, 9)))
        .mount(&server)
        .await;

    let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
    let mut exec = PushExecutor::new(
        &client,
        overlay_with(42, &[], &[(10, 7, 55)], &[]),
        FEATURES,
    );
    let shape = CardShape {
        title: "edited".into(),
        description: String::new(),
        duedate: None,
        scheduled: None,
        done: None,
        order: 9,
        archived: false,
        labels: std::collections::BTreeSet::new(),
    };
    let outcome = exec
        .execute(
            &group(
                100,
                MaterializedOp::UpdateTask {
                    task: task_id(10),
                    card: shape,
                },
                taskboard_domain::PushTarget::Task(task_id(10)),
                &[],
            ),
            &[],
            empty_tables(),
        )
        .await;

    let GroupOutcome::Outcomes(outcomes) = outcome else {
        panic!("expected outcomes");
    };
    assert!(matches!(
        &outcomes[0].result,
        PushResult::Applied { echo: Some(RemoteEcho::Task(view)) } if view.order == 9
    ));
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 2, "GET first, then the PUT");
    assert_eq!(requests[0].method.as_str(), "GET");
    assert_eq!(requests[1].method.as_str(), "PUT");
}

#[tokio::test]
async fn label_assignment_on_an_update_rides_the_sub_endpoint() {
    // Tier-2 verified (Deck 1.19): the card-update PUT rejects a
    // non-empty `labels` field with 400 — the PUT carries an empty
    // list and the assignment rides `assignLabel`.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(
            "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_string(card_json(55, 7, 3)))
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path(
            "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_string(card_json(55, 7, 3)))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path(
            "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55/assignLabel",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_string(card_json(55, 7, 3)))
        .expect(1)
        .mount(&server)
        .await;

    let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
    let mut exec = PushExecutor::new(
        &client,
        overlay_with(42, &[], &[(10, 7, 55)], &[(6, 3)]),
        FEATURES,
    );
    let shape = CardShape {
        title: "edited".into(),
        description: String::new(),
        duedate: None,
        scheduled: None,
        done: None,
        order: 3,
        archived: false,
        labels: std::collections::BTreeSet::from([label_id(6)]),
    };
    let outcome = exec
        .execute(
            &group(
                100,
                MaterializedOp::UpdateTask {
                    task: task_id(10),
                    card: shape,
                },
                taskboard_domain::PushTarget::Task(task_id(10)),
                &[],
            ),
            &[],
            empty_tables(),
        )
        .await;
    server.verify().await;

    let GroupOutcome::Outcomes(outcomes) = outcome else {
        panic!("expected outcomes");
    };
    assert!(matches!(outcomes[0].result, PushResult::Applied { .. }));
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 3, "GET, PUT (fields), PUT (assignLabel)");
    // The field PUT carries no label ids: the wire has no fallback.
    let put_body: serde_json::Value = serde_json::from_slice(&requests[1].body).unwrap();
    assert_eq!(put_body["labels"], serde_json::json!([]));
}

#[tokio::test]
async fn label_removal_on_an_update_rides_the_sub_endpoint() {
    // Mirror of the assign test: a fetched card carrying label 3 and
    // a desired set without it — the removal rides `removeLabel`,
    // never the field PUT (the update path cannot touch labels).
    let server = MockServer::start().await;
    let labelled_card = serde_json::json!({
        "id": 55, "title": "old", "stackId": 7, "type": "plain",
        "order": 3, "labels": [3], "archived": false,
        "duedate": null, "done": null
    })
    .to_string();
    Mock::given(method("GET"))
        .and(path(
            "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_string(labelled_card.clone()))
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path(
            "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_string(labelled_card.clone()))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path(
            "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55/removeLabel",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_string(labelled_card))
        .expect(1)
        .mount(&server)
        .await;

    let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
    let mut exec = PushExecutor::new(
        &client,
        overlay_with(42, &[], &[(10, 7, 55)], &[(6, 3)]),
        FEATURES,
    );
    let shape = CardShape {
        title: "edited".into(),
        description: String::new(),
        duedate: None,
        scheduled: None,
        done: None,
        order: 3,
        archived: false,
        labels: std::collections::BTreeSet::new(),
    };
    let outcome = exec
        .execute(
            &group(
                100,
                MaterializedOp::UpdateTask {
                    task: task_id(10),
                    card: shape,
                },
                taskboard_domain::PushTarget::Task(task_id(10)),
                &[],
            ),
            &[],
            empty_tables(),
        )
        .await;
    server.verify().await;

    let GroupOutcome::Outcomes(outcomes) = outcome else {
        panic!("expected outcomes");
    };
    assert!(matches!(outcomes[0].result, PushResult::Applied { .. }));
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 3, "GET, PUT (fields), PUT (removeLabel)");
    assert_eq!(
        requests[2].url.path(),
        "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55/removeLabel"
    );
}

#[tokio::test]
async fn preflight_404_on_an_update_maps_to_remote_missing() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(
            "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55",
        ))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
    let mut exec = PushExecutor::new(
        &client,
        overlay_with(42, &[], &[(10, 7, 55)], &[]),
        FEATURES,
    );
    let shape = CardShape {
        title: "edited".into(),
        description: String::new(),
        duedate: None,
        scheduled: None,
        done: None,
        order: 0,
        archived: false,
        labels: std::collections::BTreeSet::new(),
    };
    let outcome = exec
        .execute(
            &group(
                100,
                MaterializedOp::UpdateTask {
                    task: task_id(10),
                    card: shape,
                },
                taskboard_domain::PushTarget::Task(task_id(10)),
                &[],
            ),
            &[],
            empty_tables(),
        )
        .await;

    let GroupOutcome::Outcomes(outcomes) = outcome else {
        panic!("expected outcomes");
    };
    assert!(matches!(outcomes[0].result, PushResult::RemoteMissing));
}

#[tokio::test]
async fn update_defers_when_only_unresolvable_labels_differ() {
    // The desired card equals the fetched one in every field; the only
    // delta is label 9, which has no remote binding. The whole group
    // defers: empty outcomes, and the only wire traffic is the
    // pre-flight GET — no mutating request may go out.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(
            "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_string(card_json(55, 7, 3)))
        .mount(&server)
        .await;

    let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
    let mut exec = PushExecutor::new(
        &client,
        overlay_with(42, &[], &[(10, 7, 55)], &[]),
        FEATURES,
    );
    let shape = CardShape {
        title: "old".into(),
        description: String::new(),
        duedate: None,
        scheduled: None,
        done: None,
        order: 3,
        archived: false,
        labels: std::collections::BTreeSet::from([label_id(9)]),
    };
    let outcome = exec
        .execute(
            &group(
                100,
                MaterializedOp::UpdateTask {
                    task: task_id(10),
                    card: shape,
                },
                taskboard_domain::PushTarget::Task(task_id(10)),
                &[],
            ),
            &[],
            empty_tables(),
        )
        .await;

    let GroupOutcome::Outcomes(outcomes) = outcome else {
        panic!("expected outcomes");
    };
    assert!(outcomes.is_empty(), "the deferred group completes nothing");
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1, "the pre-flight GET alone");
    assert_eq!(requests[0].method.as_str(), "GET");
}

#[tokio::test]
async fn update_completes_subsumed_label_ops_by_resolvability() {
    // One outcome per op, no duplicates (#310): the subsumed label
    // ops are decided by resolvability alone — label 6 is bound to
    // remote 3 (applied by the delta's single `assignLabel(3)`),
    // label 9 has no binding and must surface its `Rejected
    // { LocalData }` instead of being shadowed by a synthetic
    // `Applied`.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(
            "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_string(card_json(55, 7, 3)))
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path(
            "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_string(card_json(55, 7, 3)))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path(
            "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55/assignLabel",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_string(card_json(55, 7, 3)))
        .expect(1)
        .mount(&server)
        .await;

    let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
    let mut exec = PushExecutor::new(
        &client,
        overlay_with(42, &[], &[(10, 7, 55)], &[(6, 3)]),
        FEATURES,
    );
    let shape = CardShape {
        title: "edited".into(),
        description: String::new(),
        duedate: None,
        scheduled: None,
        done: None,
        order: 3,
        archived: false,
        labels: std::collections::BTreeSet::from([label_id(6), label_id(9)]),
    };
    let outbox = [
        op(101, LocalOp::AssignLabel(task_id(10), label_id(6))),
        op(102, LocalOp::AssignLabel(task_id(10), label_id(9))),
    ];
    let outcome = exec
        .execute(
            &group(
                100,
                MaterializedOp::UpdateTask {
                    task: task_id(10),
                    card: shape,
                },
                taskboard_domain::PushTarget::Task(task_id(10)),
                &[101, 102],
            ),
            &outbox,
            empty_tables(),
        )
        .await;
    server.verify().await;

    let GroupOutcome::Outcomes(outcomes) = outcome else {
        panic!("expected outcomes");
    };
    assert_eq!(outcomes.len(), 3, "one outcome per op, no duplicates");
    assert_eq!(outcomes[0].op, taskboard_domain::OpId(uid(100)));
    assert!(matches!(
        outcomes[0].result,
        PushResult::Applied { echo: Some(_) }
    ));
    assert_eq!(outcomes[1].op, taskboard_domain::OpId(uid(101)));
    assert!(matches!(
        outcomes[1].result,
        PushResult::Applied { echo: None }
    ));
    assert_eq!(outcomes[2].op, taskboard_domain::OpId(uid(102)));
    assert!(
        matches!(
            outcomes[2].result,
            PushResult::Rejected {
                kind: taskboard_domain::SyncErrorKind::LocalData
            }
        ),
        "the unresolvable label op must not be shadowed"
    );
}

/// A fetched card variant carrying label ids (bare ints on the wire).
fn labelled_card_json(id: u64, stack_id: u64, order: i64, labels: &[u64]) -> String {
    serde_json::json!({
        "id": id, "title": "old", "stackId": stack_id, "type": "plain",
        "order": order, "labels": labels, "archived": false,
        "duedate": null, "done": null
    })
    .to_string()
}

/// T1: a 403 on one `assignLabel` sub-call completes each subsumed op
/// by its own class (issue #314, F1): the primary stays `Applied`
/// (the fields PUT landed), the label op that succeeded is `Applied`,
/// the failing label's op carries the classified `Rejected
/// { Forbidden }`, and updates/moves keep their synthetic `Applied`.
#[tokio::test]
async fn update_assign_rejection_completes_ops_per_class() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(
            "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_string(card_json(55, 7, 3)))
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path(
            "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_string(card_json(55, 7, 3)))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path(
            "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55/assignLabel",
        ))
        .and(body_partial_json(serde_json::json!({"labelId": 2})))
        .respond_with(ResponseTemplate::new(200).set_body_string(card_json(55, 7, 3)))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path(
            "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55/assignLabel",
        ))
        .and(body_partial_json(serde_json::json!({"labelId": 3})))
        .respond_with(ResponseTemplate::new(403))
        .expect(1)
        .mount(&server)
        .await;

    let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
    let mut exec = PushExecutor::new(
        &client,
        overlay_with(42, &[], &[(10, 7, 55)], &[(6, 2), (7, 3)]),
        FEATURES,
    );
    let mut shape = edited_shape();
    shape.labels = std::collections::BTreeSet::from([label_id(6), label_id(7)]);
    let outbox = [
        op(101, LocalOp::AssignLabel(task_id(10), label_id(6))),
        op(102, LocalOp::AssignLabel(task_id(10), label_id(7))),
        op(103, LocalOp::UpdateTask(task_id(10))),
        op(104, LocalOp::MoveTask(task_id(10))),
    ];
    let outcome = exec
        .execute(
            &group(
                100,
                MaterializedOp::UpdateTask {
                    task: task_id(10),
                    card: shape,
                },
                taskboard_domain::PushTarget::Task(task_id(10)),
                &[101, 102, 103, 104],
            ),
            &outbox,
            empty_tables(),
        )
        .await;
    server.verify().await;

    let GroupOutcome::Outcomes(outcomes) = outcome else {
        panic!("expected outcomes, got {outcome:?}");
    };
    assert_eq!(outcomes.len(), 5, "one outcome per op, no omissions");
    assert_eq!(outcomes[0].op, taskboard_domain::OpId(uid(100)));
    assert!(
        matches!(outcomes[0].result, PushResult::Applied { echo: Some(_) }),
        "the fields PUT landed, so the primary is Applied with its echo"
    );
    assert_eq!(outcomes[1].op, taskboard_domain::OpId(uid(101)));
    assert!(matches!(
        outcomes[1].result,
        PushResult::Applied { echo: None }
    ));
    assert_eq!(outcomes[2].op, taskboard_domain::OpId(uid(102)));
    assert!(
        matches!(
            outcomes[2].result,
            PushResult::Rejected {
                kind: taskboard_domain::SyncErrorKind::Forbidden
            }
        ),
        "the failing label op carries its own classification, not Applied: {:?}",
        outcomes[2].result
    );
    assert!(matches!(
        outcomes[3].result,
        PushResult::Applied { echo: None }
    ));
    assert!(matches!(
        outcomes[4].result,
        PushResult::Applied { echo: None }
    ));
}

/// T2: a 404 on an `assignLabel` sub-call classifies `RemoteMissing`
/// for *the label op alone* (issue #314, F4) — the primary keeps its
/// `Applied { echo }` so the task is never tombstone-fuelled by a
/// label-level failure.
#[tokio::test]
async fn update_assign_404_routes_remote_missing_to_the_label_op_not_the_primary() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(
            "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_string(card_json(55, 7, 3)))
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path(
            "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_string(card_json(55, 7, 3)))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path(
            "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55/assignLabel",
        ))
        .respond_with(ResponseTemplate::new(404))
        .expect(1)
        .mount(&server)
        .await;

    let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
    let mut exec = PushExecutor::new(
        &client,
        overlay_with(42, &[], &[(10, 7, 55)], &[(6, 2)]),
        FEATURES,
    );
    let mut shape = edited_shape();
    shape.labels = std::collections::BTreeSet::from([label_id(6)]);
    let outbox = [op(101, LocalOp::AssignLabel(task_id(10), label_id(6)))];
    let outcome = exec
        .execute(
            &group(
                100,
                MaterializedOp::UpdateTask {
                    task: task_id(10),
                    card: shape,
                },
                taskboard_domain::PushTarget::Task(task_id(10)),
                &[101],
            ),
            &outbox,
            empty_tables(),
        )
        .await;
    server.verify().await;

    let GroupOutcome::Outcomes(outcomes) = outcome else {
        panic!("expected outcomes, got {outcome:?}");
    };
    assert_eq!(outcomes.len(), 2);
    assert!(
        matches!(outcomes[0].result, PushResult::Applied { echo: Some(_) }),
        "the primary must never inherit a label-level RemoteMissing"
    );
    assert!(matches!(outcomes[1].result, PushResult::RemoteMissing));
}

/// T3: the same per-op classes on the `removeLabel` arm (issue #314,
/// F5) — a 403 on one removal leaves that label's op `Rejected
/// { Forbidden }` while the removed-earlier one is `Applied`.
#[tokio::test]
async fn update_remove_rejection_completes_ops_per_class() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(
            "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55",
        ))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(labelled_card_json(55, 7, 3, &[3, 4])),
        )
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path(
            "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55",
        ))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(labelled_card_json(55, 7, 3, &[3, 4])),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path(
            "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55/removeLabel",
        ))
        .and(body_partial_json(serde_json::json!({"labelId": 3})))
        .respond_with(ResponseTemplate::new(200).set_body_string(card_json(55, 7, 3)))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path(
            "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55/removeLabel",
        ))
        .and(body_partial_json(serde_json::json!({"labelId": 4})))
        .respond_with(ResponseTemplate::new(403))
        .expect(1)
        .mount(&server)
        .await;

    let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
    let mut exec = PushExecutor::new(
        &client,
        overlay_with(42, &[], &[(10, 7, 55)], &[(11, 3), (12, 4)]),
        FEATURES,
    );
    let mut shape = edited_shape();
    shape.labels = std::collections::BTreeSet::new();
    let outbox = [
        op(101, LocalOp::UnassignLabel(task_id(10), label_id(11))),
        op(102, LocalOp::UnassignLabel(task_id(10), label_id(12))),
        op(103, LocalOp::UpdateTask(task_id(10))),
    ];
    let outcome = exec
        .execute(
            &group(
                100,
                MaterializedOp::UpdateTask {
                    task: task_id(10),
                    card: shape,
                },
                taskboard_domain::PushTarget::Task(task_id(10)),
                &[101, 102, 103],
            ),
            &outbox,
            empty_tables(),
        )
        .await;
    server.verify().await;

    let GroupOutcome::Outcomes(outcomes) = outcome else {
        panic!("expected outcomes, got {outcome:?}");
    };
    assert_eq!(outcomes.len(), 4);
    assert!(matches!(
        outcomes[0].result,
        PushResult::Applied { echo: Some(_) }
    ));
    assert!(matches!(
        outcomes[1].result,
        PushResult::Applied { echo: None }
    ));
    assert!(
        matches!(
            outcomes[2].result,
            PushResult::Rejected {
                kind: taskboard_domain::SyncErrorKind::Forbidden
            }
        ),
        "the failing removal's op carries the classification: {:?}",
        outcomes[2].result
    );
    assert!(matches!(
        outcomes[3].result,
        PushResult::Applied { echo: None }
    ));
}

/// T4: an unresolvable label's `Rejected { LocalData }` survives a
/// classified rejection on the same delta (issue #314, F2 — the #310
/// semantics must hold on the failure path too).
#[tokio::test]
async fn update_rejection_does_not_shadow_unresolvable_label_ops() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(
            "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_string(card_json(55, 7, 3)))
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path(
            "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_string(card_json(55, 7, 3)))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path(
            "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55/assignLabel",
        ))
        .respond_with(ResponseTemplate::new(403))
        .expect(1)
        .mount(&server)
        .await;

    let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
    let mut exec = PushExecutor::new(
        &client,
        overlay_with(42, &[], &[(10, 7, 55)], &[(6, 3)]),
        FEATURES,
    );
    let mut shape = edited_shape();
    shape.labels = std::collections::BTreeSet::from([label_id(6), label_id(9)]);
    let outbox = [op(101, LocalOp::AssignLabel(task_id(10), label_id(9)))];
    let outcome = exec
        .execute(
            &group(
                100,
                MaterializedOp::UpdateTask {
                    task: task_id(10),
                    card: shape,
                },
                taskboard_domain::PushTarget::Task(task_id(10)),
                &[101],
            ),
            &outbox,
            empty_tables(),
        )
        .await;
    server.verify().await;

    let GroupOutcome::Outcomes(outcomes) = outcome else {
        panic!("expected outcomes, got {outcome:?}");
    };
    assert_eq!(outcomes.len(), 2);
    assert!(matches!(
        outcomes[0].result,
        PushResult::Applied { echo: Some(_) }
    ));
    assert!(
        matches!(
            outcomes[1].result,
            PushResult::Rejected {
                kind: taskboard_domain::SyncErrorKind::LocalData
            }
        ),
        "the unresolvable label op keeps its LocalData rejection: {:?}",
        outcomes[1].result
    );
}

/// T5: label ops the failing delta never reached (a later assign, and
/// any remove) are *omitted* from the report — they stay queued and
/// retry next cycle (issue #314, decision D1a).
#[tokio::test]
async fn update_rejection_leaves_unreached_label_ops_queued() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(
            "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55",
        ))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(labelled_card_json(55, 7, 3, &[4])),
        )
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path(
            "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55",
        ))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(labelled_card_json(55, 7, 3, &[4])),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path(
            "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55/assignLabel",
        ))
        .and(body_partial_json(serde_json::json!({"labelId": 2})))
        .respond_with(ResponseTemplate::new(200).set_body_string(card_json(55, 7, 3)))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path(
            "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55/assignLabel",
        ))
        .and(body_partial_json(serde_json::json!({"labelId": 3})))
        .respond_with(ResponseTemplate::new(403))
        .expect(1)
        .mount(&server)
        .await;

    let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
    let mut exec = PushExecutor::new(
        &client,
        overlay_with(42, &[], &[(10, 7, 55)], &[(6, 2), (7, 3), (8, 5), (9, 4)]),
        FEATURES,
    );
    let mut shape = edited_shape();
    shape.labels = std::collections::BTreeSet::from([label_id(6), label_id(7), label_id(8)]);
    let outbox = [
        op(101, LocalOp::AssignLabel(task_id(10), label_id(6))),
        op(102, LocalOp::AssignLabel(task_id(10), label_id(7))),
        op(103, LocalOp::AssignLabel(task_id(10), label_id(8))),
        op(104, LocalOp::UnassignLabel(task_id(10), label_id(9))),
    ];
    let outcome = exec
        .execute(
            &group(
                100,
                MaterializedOp::UpdateTask {
                    task: task_id(10),
                    card: shape,
                },
                taskboard_domain::PushTarget::Task(task_id(10)),
                &[101, 102, 103, 104],
            ),
            &outbox,
            empty_tables(),
        )
        .await;
    server.verify().await;

    let GroupOutcome::Outcomes(outcomes) = outcome else {
        panic!("expected outcomes, got {outcome:?}");
    };
    // Primary + the two label ops the delta actually reached; ops 103
    // (assign, never attempted) and 104 (remove phase never ran) stay
    // queued — absent from the report.
    assert_eq!(outcomes.len(), 3, "{outcomes:?}");
    assert!(
        !outcomes
            .iter()
            .any(|o| o.op == taskboard_domain::OpId(uid(103)))
    );
    assert!(
        !outcomes
            .iter()
            .any(|o| o.op == taskboard_domain::OpId(uid(104)))
    );
    assert!(matches!(
        outcomes[0].result,
        PushResult::Applied { echo: Some(_) }
    ));
    assert!(matches!(
        outcomes[1].result,
        PushResult::Applied { echo: None }
    ));
    assert!(matches!(
        outcomes[2].result,
        PushResult::Rejected {
            kind: taskboard_domain::SyncErrorKind::Forbidden
        }
    ));
}

/// T6: a transport-class failure on a label sub-call aborts the cycle
/// with no landed evidence — the PUT is an idempotent full-send, so
/// re-sending the whole group is safe (the preserved arm, D1).
#[tokio::test]
async fn label_delta_transport_failure_aborts_the_cycle() {
    // Raw TCP server: the first two requests (the pre-flight GET, then
    // the fields PUT) get real HTTP 200 card responses; every later
    // connection is accepted and dropped, so the `assignLabel` fails
    // transport-class.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        if let Ok((stream, _)) = listener.accept() {
            serve_one_card(stream);
        }
        if let Ok((stream, _)) = listener.accept() {
            serve_one_card(stream);
        }
        for _ in 0..8 {
            if listener.accept().is_err() {
                break;
            }
        }
    });

    let client = DeckClient::new(&format!("http://{addr}"), "u", "t").unwrap();
    let mut exec = PushExecutor::new(
        &client,
        overlay_with(42, &[], &[(10, 7, 55)], &[(6, 3)]),
        FEATURES,
    );
    let mut shape = edited_shape();
    shape.labels = std::collections::BTreeSet::from([label_id(6)]);
    let outbox = [op(101, LocalOp::AssignLabel(task_id(10), label_id(6)))];
    let outcome = exec
        .execute(
            &group(
                100,
                MaterializedOp::UpdateTask {
                    task: task_id(10),
                    card: shape,
                },
                taskboard_domain::PushTarget::Task(task_id(10)),
                &[101],
            ),
            &outbox,
            empty_tables(),
        )
        .await;

    assert!(
        matches!(outcome, GroupOutcome::Aborted { ref landed } if landed.is_empty()),
        "a transport-class label failure aborts with nothing landed: {outcome:?}"
    );
}

#[tokio::test]
async fn move_same_stack_different_order_sends_the_reorder() {
    // Card sits at stack 7 / order 3; the local intent is the *same*
    // stack at order 9. The fast path must not fire: exactly one
    // reorder carrying order 9 and the destination stack.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(
            "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_string(card_json(55, 7, 3)))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path(
            "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55/reorder",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_string("[]"))
        .expect(1)
        .mount(&server)
        .await;

    let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
    let mut exec = PushExecutor::new(
        &client,
        overlay_with(42, &[(5, 7)], &[(10, 7, 55)], &[]),
        FEATURES,
    );
    let outcome = exec
        .execute(
            &group(
                100,
                MaterializedOp::MoveTask {
                    task: task_id(10),
                    to: (stack_id(5), 9),
                },
                taskboard_domain::PushTarget::Task(task_id(10)),
                &[],
            ),
            &[],
            empty_tables(),
        )
        .await;
    server.verify().await;

    let GroupOutcome::Outcomes(outcomes) = outcome else {
        panic!("expected outcomes");
    };
    assert_eq!(outcomes.len(), 1);
    assert_eq!(outcomes[0].op, taskboard_domain::OpId(uid(100)));
    assert!(matches!(
        outcomes[0].result,
        PushResult::Applied { echo: None }
    ));
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 2, "the pre-flight GET, then one reorder");
    assert_eq!(requests[1].method.as_str(), "PUT");
    let body: serde_json::Value = serde_json::from_slice(&requests[1].body).unwrap();
    assert_eq!(
        body,
        serde_json::json!({"order": 9, "stackId": 7}),
        "the reorder carries order 9 and the destination stack"
    );
}

#[tokio::test]
async fn move_sends_reorder_only_when_the_position_differs() {
    // Card sits at stack 7 / order 3; the local intent is stack 8 / order 1.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(
            "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_string(card_json(55, 7, 3)))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path(
            "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55/reorder",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_string("[]"))
        .expect(1)
        .mount(&server)
        .await;

    let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
    let mut exec = PushExecutor::new(
        &client,
        overlay_with(42, &[(5, 7), (6, 8)], &[(10, 7, 55)], &[]),
        FEATURES,
    );
    let outcome = exec
        .execute(
            &group(
                100,
                MaterializedOp::MoveTask {
                    task: task_id(10),
                    to: (stack_id(6), 1),
                },
                taskboard_domain::PushTarget::Task(task_id(10)),
                &[],
            ),
            &[],
            empty_tables(),
        )
        .await;
    server.verify().await;
    let GroupOutcome::Outcomes(outcomes) = outcome else {
        panic!("expected outcomes");
    };
    assert!(matches!(
        outcomes[0].result,
        PushResult::Applied { echo: None }
    ));
    let body: serde_json::Value =
        serde_json::from_slice(&server.received_requests().await.unwrap()[1].body).unwrap();
    assert_eq!(
        body,
        serde_json::json!({"order": 1, "stackId": 8}),
        "destination stack travels in the body (tier-2-verified shape)"
    );

    // Same position already: no reorder goes out at all.
    let server2 = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(
            "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_string(card_json(55, 7, 3)))
        .mount(&server2)
        .await;
    let client2 = DeckClient::new(&server2.uri(), "u", "t").unwrap();
    let mut exec2 = PushExecutor::new(
        &client2,
        overlay_with(42, &[(5, 7)], &[(10, 7, 55)], &[]),
        FEATURES,
    );
    let outcome = exec2
        .execute(
            &group(
                100,
                MaterializedOp::MoveTask {
                    task: task_id(10),
                    to: (stack_id(5), 3),
                },
                taskboard_domain::PushTarget::Task(task_id(10)),
                &[],
            ),
            &[],
            empty_tables(),
        )
        .await;
    let GroupOutcome::Outcomes(outcomes) = outcome else {
        panic!("expected outcomes");
    };
    assert!(matches!(
        outcomes[0].result,
        PushResult::Applied { echo: None }
    ));
    assert!(
        server2.received_requests().await.unwrap().len() == 1,
        "only the pre-flight GET, no reorder"
    );
}

// ---- stale-binding pre-write-fetch 404s (#258) ---------------------

/// The stacks listing carrying card 55 under stack 8 (the card "moved"
/// there behind our back; the overlay still says stack 7).
fn moved_listing_json() -> String {
    serde_json::json!([{
        "id": 8, "title": "moved-here", "boardId": 42, "order": 1,
        "lastModified": 1_700_000_000, "deletedAt": 0, "archived": false,
        "cards": [{ "id": 55, "title": "old", "stackId": 8, "type": "plain",
                    "order": 3, "labels": [], "archived": false,
                    "duedate": null, "done": null }]
    }])
    .to_string()
}

fn edited_shape() -> CardShape {
    CardShape {
        title: "edited".into(),
        description: String::new(),
        duedate: None,
        scheduled: None,
        done: None,
        order: 9,
        archived: false,
        labels: std::collections::BTreeSet::new(),
    }
}

/// E1: a pre-write fetch 404 on a card another client *moved*
/// re-resolves the card from the fresh listing, rebinds the overlay,
/// and pushes at the fresh location — the update lands instead of
/// misclassifying as `RemoteMissing` (which tombstoned the task and
/// silently cancelled the queued edit; issue #258).
#[tokio::test]
async fn update_preflight_404_reresolves_the_card_from_a_fresh_listing() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(
            "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55",
        ))
        .respond_with(ResponseTemplate::new(404))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/index.php/apps/deck/api/v1.0/boards/42/stacks"))
        .respond_with(ResponseTemplate::new(200).set_body_string(moved_listing_json()))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(
            "/index.php/apps/deck/api/v1.0/boards/42/stacks/8/cards/55",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_string(card_json(55, 8, 3)))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path(
            "/index.php/apps/deck/api/v1.0/boards/42/stacks/8/cards/55",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_string(card_json(55, 8, 9)))
        .expect(1)
        .mount(&server)
        .await;

    let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
    let mut exec = PushExecutor::new(
        &client,
        overlay_with(42, &[], &[(10, 7, 55)], &[]),
        FEATURES,
    );
    let outcome = exec
        .execute(
            &group(
                100,
                MaterializedOp::UpdateTask {
                    task: task_id(10),
                    card: edited_shape(),
                },
                taskboard_domain::PushTarget::Task(task_id(10)),
                &[],
            ),
            &[],
            empty_tables(),
        )
        .await;
    server.verify().await;

    let GroupOutcome::Outcomes(outcomes) = outcome else {
        panic!("expected outcomes, got {outcome:?}");
    };
    assert!(matches!(
        &outcomes[0].result,
        PushResult::Applied { echo: Some(RemoteEcho::Task(view)) }
            if view.id.stack == RemoteStackId(8) && view.id.card == taskboard_domain::RemoteCardId(55)
    ));
    assert_eq!(
        exec.overlay().card_of(task_id(10)),
        Some(RemoteCardRef {
            board: RemoteBoardId(42),
            stack: RemoteStackId(8),
            card: taskboard_domain::RemoteCardId(55),
        }),
        "the overlay is rebound to the fresh location"
    );
    let requests = server.received_requests().await.unwrap();
    assert_eq!(
        requests.len(),
        4,
        "stale GET (404), fresh listing, fresh GET, PUT at the fresh stack"
    );
}

/// E2: the same recovery on the move path — the reorder's *path* is
/// the card's fetched (fresh) location while its *body* carries the
/// distinct destination (the wire contract); the stale stack appears
/// in no request after the initial 404.
#[tokio::test]
async fn move_preflight_404_reresolves_then_reorders_from_the_fresh_location() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(
            "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55",
        ))
        .respond_with(ResponseTemplate::new(404))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/index.php/apps/deck/api/v1.0/boards/42/stacks"))
        .respond_with(ResponseTemplate::new(200).set_body_string(moved_listing_json()))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(
            "/index.php/apps/deck/api/v1.0/boards/42/stacks/8/cards/55",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_string(card_json(55, 8, 3)))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path(
            "/index.php/apps/deck/api/v1.0/boards/42/stacks/8/cards/55/reorder",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_string("[]"))
        .expect(1)
        .mount(&server)
        .await;

    let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
    let mut exec = PushExecutor::new(
        &client,
        overlay_with(42, &[(6, 8), (7, 9)], &[(10, 7, 55)], &[]),
        FEATURES,
    );
    let outcome = exec
        .execute(
            &group(
                100,
                MaterializedOp::MoveTask {
                    task: task_id(10),
                    to: (stack_id(7), 5),
                },
                taskboard_domain::PushTarget::Task(task_id(10)),
                &[],
            ),
            &[],
            empty_tables(),
        )
        .await;
    server.verify().await;

    let GroupOutcome::Outcomes(outcomes) = outcome else {
        panic!("expected outcomes, got {outcome:?}");
    };
    assert!(matches!(
        outcomes[0].result,
        PushResult::Applied { echo: None }
    ));
    let requests = server.received_requests().await.unwrap();
    assert_eq!(
        requests.len(),
        4,
        "stale GET (404), fresh listing, fresh GET, reorder from the fresh stack"
    );
    let body: serde_json::Value = serde_json::from_slice(&requests[3].body).unwrap();
    assert_eq!(
        body,
        serde_json::json!({"order": 5, "stackId": 9}),
        "the body carries the destination (stack 9, order 5); the \
         request path stayed at the fetched stack 8"
    );
}

/// E3: a 404 the fresh listings *confirm* (the card is in neither)
/// still classifies `RemoteMissing` — now truthful absence, and the
/// genuine-deletion terminal is unchanged.
#[tokio::test]
async fn preflight_404_with_the_card_absent_from_both_listings_classifies_remote_missing() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(
            "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55",
        ))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    for listing in ["stacks", "stacks/archived"] {
        Mock::given(method("GET"))
            .and(path(format!(
                "/index.php/apps/deck/api/v1.0/boards/42/{listing}"
            )))
            .respond_with(ResponseTemplate::new(200).set_body_string("[]"))
            .mount(&server)
            .await;
    }

    let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
    let mut exec = PushExecutor::new(
        &client,
        overlay_with(42, &[], &[(10, 7, 55)], &[]),
        FEATURES,
    );
    let outcome = exec
        .execute(
            &group(
                100,
                MaterializedOp::UpdateTask {
                    task: task_id(10),
                    card: edited_shape(),
                },
                taskboard_domain::PushTarget::Task(task_id(10)),
                &[],
            ),
            &[],
            empty_tables(),
        )
        .await;
    server.verify().await;

    let GroupOutcome::Outcomes(outcomes) = outcome else {
        panic!("expected outcomes, got {outcome:?}");
    };
    assert!(matches!(outcomes[0].result, PushResult::RemoteMissing));
}

/// E4: a lagging listing that claims the card while the fresh detail
/// 404s defers to the detail — `RemoteMissing`.
#[tokio::test]
async fn preflight_404_with_a_lying_listing_maps_the_refetch_404_to_remote_missing() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(
            "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55",
        ))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/index.php/apps/deck/api/v1.0/boards/42/stacks"))
        .respond_with(ResponseTemplate::new(200).set_body_string(moved_listing_json()))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(
            "/index.php/apps/deck/api/v1.0/boards/42/stacks/8/cards/55",
        ))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
    let mut exec = PushExecutor::new(
        &client,
        overlay_with(42, &[], &[(10, 7, 55)], &[]),
        FEATURES,
    );
    let outcome = exec
        .execute(
            &group(
                100,
                MaterializedOp::UpdateTask {
                    task: task_id(10),
                    card: edited_shape(),
                },
                taskboard_domain::PushTarget::Task(task_id(10)),
                &[],
            ),
            &[],
            empty_tables(),
        )
        .await;
    server.verify().await;

    let GroupOutcome::Outcomes(outcomes) = outcome else {
        panic!("expected outcomes, got {outcome:?}");
    };
    assert!(matches!(outcomes[0].result, PushResult::RemoteMissing));
}

/// Sleeps zero: the E5 retry exhaustion must not depend on real time.
#[derive(Debug)]
struct InstantSleeper;

impl crate::client::RetrySleep for InstantSleeper {
    fn sleep(
        &self,
        _delay: std::time::Duration,
    ) -> std::pin::Pin<Box<dyn Future<Output = ()> + Send>> {
        Box::pin(std::future::ready(()))
    }
}

/// E5: a listing that fails transport-class aborts the cycle — the
/// re-resolve must not turn an unknown listing outcome into a
/// classification (today it never asks, which this test also pins as
/// a change).
#[tokio::test]
async fn preflight_404_with_a_transport_failing_listing_aborts_the_cycle() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(
            "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55",
        ))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/index.php/apps/deck/api/v1.0/boards/42/stacks"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;

    let client = DeckClient::new(&server.uri(), "u", "t")
        .unwrap()
        .with_sleeper(std::sync::Arc::new(InstantSleeper));
    let mut exec = PushExecutor::new(
        &client,
        overlay_with(42, &[], &[(10, 7, 55)], &[]),
        FEATURES,
    );
    let outcome = exec
        .execute(
            &group(
                100,
                MaterializedOp::UpdateTask {
                    task: task_id(10),
                    card: edited_shape(),
                },
                taskboard_domain::PushTarget::Task(task_id(10)),
                &[],
            ),
            &[],
            empty_tables(),
        )
        .await;
    server.verify().await;

    assert_eq!(
        outcome,
        GroupOutcome::Aborted { landed: Vec::new() },
        "a transport-class listing failure aborts with no outcomes"
    );
}

/// E6: a forbidden listing classifies `Rejected { Forbidden }` —
/// retryable, never a tombstone.
#[tokio::test]
async fn preflight_404_with_a_forbidden_listing_rejects_forbidden() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(
            "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55",
        ))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/index.php/apps/deck/api/v1.0/boards/42/stacks"))
        .respond_with(ResponseTemplate::new(403))
        .mount(&server)
        .await;

    let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
    let mut exec = PushExecutor::new(
        &client,
        overlay_with(42, &[], &[(10, 7, 55)], &[]),
        FEATURES,
    );
    let outcome = exec
        .execute(
            &group(
                100,
                MaterializedOp::UpdateTask {
                    task: task_id(10),
                    card: edited_shape(),
                },
                taskboard_domain::PushTarget::Task(task_id(10)),
                &[],
            ),
            &[],
            empty_tables(),
        )
        .await;
    server.verify().await;

    let GroupOutcome::Outcomes(outcomes) = outcome else {
        panic!("expected outcomes, got {outcome:?}");
    };
    assert!(matches!(
        outcomes[0].result,
        PushResult::Rejected {
            kind: taskboard_domain::SyncErrorKind::Forbidden
        }
    ));
}

#[tokio::test]
async fn transport_failure_aborts_without_outcomes() {
    // Reserve a port and release it: deterministically refused.
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let client = DeckClient::new(&format!("http://127.0.0.1:{port}"), "u", "t").unwrap();
    let mut exec = PushExecutor::new(
        &client,
        overlay_with(42, &[], &[(10, 7, 55)], &[]),
        FEATURES,
    );
    let shape = CardShape {
        title: "edited".into(),
        description: String::new(),
        duedate: None,
        scheduled: None,
        done: None,
        order: 0,
        archived: false,
        labels: std::collections::BTreeSet::new(),
    };
    let outcome = exec
        .execute(
            &group(
                100,
                MaterializedOp::UpdateTask {
                    task: task_id(10),
                    card: shape,
                },
                taskboard_domain::PushTarget::Task(task_id(10)),
                &[],
            ),
            &[],
            empty_tables(),
        )
        .await;
    assert!(
        matches!(outcome, GroupOutcome::Aborted { landed } if landed.is_empty()),
        "a transport failure on the group's own write lands nothing"
    );
}

/// F1 regression: a create that landed but whose post-create label ride
/// then hit a transport failure must carry the create's outcome in the
/// abort — otherwise the engine re-POSTs the card next cycle (no
/// idempotency keys).
#[tokio::test]
async fn transport_failure_after_a_landed_create_keeps_its_evidence() {
    // A raw TCP server: the first request (the create POST) gets a real
    // HTTP 200 response; every later connection is accepted and dropped,
    // which reqwest reports as a transport error (the client exhausts
    // its three attempts against it).
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        if let Ok((stream, _)) = listener.accept() {
            serve_one_create(stream);
        }
        // Subsequent connections: accept and immediately drop.
        for _ in 0..8 {
            if listener.accept().is_err() {
                break;
            }
        }
    });

    let client = DeckClient::new(&format!("http://{addr}"), "u", "t").unwrap();
    let mut exec = PushExecutor::new(
        &client,
        overlay_with(42, &[(5, 7)], &[], &[(6, 3)]),
        FEATURES,
    );
    let outbox = vec![
        op(100, LocalOp::CreateTask(task_id(10))),
        op(101, LocalOp::AssignLabel(task_id(10), label_id(6))),
    ];
    let shape = taskboard_domain::NewCardShape {
        title: "created".into(),
        order: 0,
        description: String::new(),
        duedate: None,
        scheduled: None,
        done: None,
        archived: false,
        labels: std::collections::BTreeSet::new(),
    };
    let outcome = exec
        .execute(
            &group(
                100,
                MaterializedOp::CreateTask {
                    task: task_id(10),
                    stack: stack_id(5),
                    new_card: shape,
                },
                taskboard_domain::PushTarget::Task(task_id(10)),
                &[101],
            ),
            &outbox,
            empty_tables(),
        )
        .await;

    let GroupOutcome::Aborted { landed } = outcome else {
        panic!("expected a transport abort");
    };
    assert_eq!(landed.len(), 1, "the create's evidence survives the abort");
    assert!(matches!(
        &landed[0].result,
        PushResult::Applied { echo: Some(RemoteEcho::Task(view)) }
            if view.id.card == taskboard_domain::RemoteCardId(55)
    ));
}

/// F2 regression: a failed materialized write completes only the
/// primary — the subsumed ops stay queued with their sibling.
#[tokio::test]
async fn a_rejected_write_leaves_the_subsumed_ops_queued() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(
            "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_string(card_json(55, 7, 0)))
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path(
            "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55",
        ))
        .respond_with(ResponseTemplate::new(400))
        .mount(&server)
        .await;

    let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
    let mut exec = PushExecutor::new(
        &client,
        overlay_with(42, &[], &[(10, 7, 55)], &[]),
        FEATURES,
    );
    let shape = CardShape {
        title: "edited".into(),
        description: String::new(),
        duedate: None,
        scheduled: None,
        done: None,
        order: 0,
        archived: false,
        labels: std::collections::BTreeSet::new(),
    };
    let outcome = exec
        .execute(
            &group(
                100,
                MaterializedOp::UpdateTask {
                    task: task_id(10),
                    card: shape,
                },
                taskboard_domain::PushTarget::Task(task_id(10)),
                &[101, 102],
            ),
            &[],
            empty_tables(),
        )
        .await;

    let GroupOutcome::Outcomes(outcomes) = outcome else {
        panic!("expected outcomes");
    };
    assert_eq!(
        outcomes.len(),
        1,
        "only the primary gets an outcome; the subsumed ops stay queued"
    );
    assert_eq!(outcomes[0].op, taskboard_domain::OpId(uid(100)));
    assert!(matches!(
        outcomes[0].result,
        PushResult::Rejected {
            kind: taskboard_domain::SyncErrorKind::BadRequest
        }
    ));
}

// ---- stack push paths --------------------------------------------

#[tokio::test]
async fn create_stack_posts_and_applies_the_echo() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/index.php/apps/deck/api/v1.0/boards/42/stacks"))
        .respond_with(ResponseTemplate::new(200).set_body_string(stack_json(9, "Todo", 2)))
        .expect(1)
        .mount(&server)
        .await;

    let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
    let mut exec = PushExecutor::new(&client, overlay_with(42, &[], &[], &[]), FEATURES);
    let outcome = exec
        .execute(
            &group(
                100,
                MaterializedOp::CreateStack { stack: stack_id(5) },
                taskboard_domain::PushTarget::Stack(stack_id(5)),
                &[],
            ),
            &[],
            tables_with(&[stack_row(5, "Todo", 2)], &[]),
        )
        .await;
    server.verify().await;

    let GroupOutcome::Outcomes(outcomes) = outcome else {
        panic!("expected outcomes");
    };
    assert!(matches!(
        &outcomes[0].result,
        PushResult::Applied { echo: Some(RemoteEcho::Stack(view)) }
            if view.id.stack == RemoteStackId(9)
                && view.title == "Todo"
                && view.order == 2
    ));
    let body: serde_json::Value =
        serde_json::from_slice(&server.received_requests().await.unwrap()[0].body).unwrap();
    assert_eq!(body, serde_json::json!({"title": "Todo", "order": 2}));
}

#[tokio::test]
async fn create_stack_with_a_missing_row_completes_synthetic_without_sending() {
    let server = MockServer::start().await;
    let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
    let mut exec = PushExecutor::new(&client, overlay_with(42, &[], &[], &[]), FEATURES);
    let outcome = exec
        .execute(
            &group(
                100,
                MaterializedOp::CreateStack { stack: stack_id(5) },
                taskboard_domain::PushTarget::Stack(stack_id(5)),
                &[101],
            ),
            &[],
            empty_tables(),
        )
        .await;

    let GroupOutcome::Outcomes(outcomes) = outcome else {
        panic!("expected outcomes");
    };
    assert_eq!(outcomes.len(), 2, "primary and subsumed op complete");
    assert!(
        outcomes
            .iter()
            .all(|o| matches!(o.result, PushResult::Applied { echo: None }))
    );
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn create_stack_400_is_rejected_bad_request() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/index.php/apps/deck/api/v1.0/boards/42/stacks"))
        .respond_with(ResponseTemplate::new(400))
        .mount(&server)
        .await;

    let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
    let mut exec = PushExecutor::new(&client, overlay_with(42, &[], &[], &[]), FEATURES);
    let outcome = exec
        .execute(
            &group(
                100,
                MaterializedOp::CreateStack { stack: stack_id(5) },
                taskboard_domain::PushTarget::Stack(stack_id(5)),
                &[],
            ),
            &[],
            tables_with(&[stack_row(5, "Todo", 0)], &[]),
        )
        .await;

    let GroupOutcome::Outcomes(outcomes) = outcome else {
        panic!("expected outcomes");
    };
    assert!(matches!(
        outcomes[0].result,
        PushResult::Rejected {
            kind: taskboard_domain::SyncErrorKind::BadRequest
        }
    ));
}

#[tokio::test]
async fn create_stack_transport_failure_aborts() {
    let client = refused_client();
    let mut exec = PushExecutor::new(&client, overlay_with(42, &[], &[], &[]), FEATURES);
    let outcome = exec
        .execute(
            &group(
                100,
                MaterializedOp::CreateStack { stack: stack_id(5) },
                taskboard_domain::PushTarget::Stack(stack_id(5)),
                &[],
            ),
            &[],
            tables_with(&[stack_row(5, "Todo", 0)], &[]),
        )
        .await;
    assert!(matches!(
        outcome,
        GroupOutcome::Aborted { landed } if landed.is_empty()
    ));
}

#[tokio::test]
async fn rename_stack_puts_the_changeset_and_applies_the_echo() {
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path("/index.php/apps/deck/api/v1.0/boards/42/stacks/7"))
        .respond_with(ResponseTemplate::new(200).set_body_string(stack_json(7, "Doing", 0)))
        .expect(1)
        .mount(&server)
        .await;

    let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
    let mut exec = PushExecutor::new(&client, overlay_with(42, &[(5, 7)], &[], &[]), FEATURES);
    let outcome = exec
        .execute(
            &group(
                100,
                MaterializedOp::RenameStack { stack: stack_id(5) },
                taskboard_domain::PushTarget::Stack(stack_id(5)),
                &[],
            ),
            &[],
            tables_with(&[stack_row(5, "Doing", 0)], &[]),
        )
        .await;
    server.verify().await;

    let GroupOutcome::Outcomes(outcomes) = outcome else {
        panic!("expected outcomes");
    };
    assert!(matches!(
        &outcomes[0].result,
        PushResult::Applied { echo: Some(RemoteEcho::Stack(view)) } if view.title == "Doing"
    ));
    let body: serde_json::Value =
        serde_json::from_slice(&server.received_requests().await.unwrap()[0].body).unwrap();
    assert_eq!(body["title"], "Doing");
}

#[tokio::test]
async fn move_stack_puts_the_full_row_and_applies_the_echo() {
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path("/index.php/apps/deck/api/v1.0/boards/42/stacks/7"))
        .respond_with(ResponseTemplate::new(200).set_body_string(stack_json(7, "Doing", 3)))
        .expect(1)
        .mount(&server)
        .await;

    let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
    let mut exec = PushExecutor::new(&client, overlay_with(42, &[(5, 7)], &[], &[]), FEATURES);
    let outcome = exec
        .execute(
            &group(
                100,
                MaterializedOp::MoveStack { stack: stack_id(5) },
                taskboard_domain::PushTarget::Stack(stack_id(5)),
                &[],
            ),
            &[],
            tables_with(&[stack_row(5, "Doing", 3)], &[]),
        )
        .await;
    server.verify().await;

    let GroupOutcome::Outcomes(outcomes) = outcome else {
        panic!("expected outcomes");
    };
    assert!(matches!(
        &outcomes[0].result,
        PushResult::Applied { echo: Some(RemoteEcho::Stack(view)) } if view.order == 3
    ));
    // Full-send: the PUT body carries the whole row (title + order).
    let body: serde_json::Value =
        serde_json::from_slice(&server.received_requests().await.unwrap()[0].body).unwrap();
    assert_eq!(body["title"], "Doing");
    assert_eq!(body["order"], 3);
}

#[tokio::test]
async fn move_of_an_unbound_stack_rejects_local_data_without_sending() {
    let server = MockServer::start().await;
    let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
    let mut exec = PushExecutor::new(&client, overlay_with(42, &[], &[], &[]), FEATURES);
    let outcome = exec
        .execute(
            &group(
                100,
                MaterializedOp::MoveStack { stack: stack_id(5) },
                taskboard_domain::PushTarget::Stack(stack_id(5)),
                &[],
            ),
            &[],
            tables_with(&[stack_row(5, "Doing", 0)], &[]),
        )
        .await;

    let GroupOutcome::Outcomes(outcomes) = outcome else {
        panic!("expected outcomes");
    };
    assert!(matches!(
        outcomes[0].result,
        PushResult::Rejected {
            kind: taskboard_domain::SyncErrorKind::LocalData
        }
    ));
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn move_stack_404_maps_to_remote_missing() {
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path("/index.php/apps/deck/api/v1.0/boards/42/stacks/7"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
    let mut exec = PushExecutor::new(&client, overlay_with(42, &[(5, 7)], &[], &[]), FEATURES);
    let outcome = exec
        .execute(
            &group(
                100,
                MaterializedOp::MoveStack { stack: stack_id(5) },
                taskboard_domain::PushTarget::Stack(stack_id(5)),
                &[],
            ),
            &[],
            tables_with(&[stack_row(5, "Doing", 0)], &[]),
        )
        .await;

    let GroupOutcome::Outcomes(outcomes) = outcome else {
        panic!("expected outcomes");
    };
    assert!(matches!(outcomes[0].result, PushResult::RemoteMissing));
}

#[tokio::test]
async fn move_stack_transport_failure_aborts_the_cycle() {
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path("/index.php/apps/deck/api/v1.0/boards/42/stacks/7"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;

    let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
    let mut exec = PushExecutor::new(&client, overlay_with(42, &[(5, 7)], &[], &[]), FEATURES);
    let outcome = exec
        .execute(
            &group(
                100,
                MaterializedOp::MoveStack { stack: stack_id(5) },
                taskboard_domain::PushTarget::Stack(stack_id(5)),
                &[],
            ),
            &[],
            tables_with(&[stack_row(5, "Doing", 0)], &[]),
        )
        .await;
    assert!(matches!(outcome, GroupOutcome::Aborted { landed } if landed.is_empty()));
}

#[tokio::test]
async fn rename_of_an_unbound_stack_rejects_local_data_without_sending() {
    let server = MockServer::start().await;
    let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
    let mut exec = PushExecutor::new(&client, overlay_with(42, &[], &[], &[]), FEATURES);
    let outcome = exec
        .execute(
            &group(
                100,
                MaterializedOp::RenameStack { stack: stack_id(5) },
                taskboard_domain::PushTarget::Stack(stack_id(5)),
                &[],
            ),
            &[],
            tables_with(&[stack_row(5, "Doing", 0)], &[]),
        )
        .await;

    let GroupOutcome::Outcomes(outcomes) = outcome else {
        panic!("expected outcomes");
    };
    assert!(matches!(
        outcomes[0].result,
        PushResult::Rejected {
            kind: taskboard_domain::SyncErrorKind::LocalData
        }
    ));
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn rename_board_puts_the_full_changeset_and_applies_the_echo() {
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path("/index.php/apps/deck/api/v1.0/boards/42"))
        .respond_with(ResponseTemplate::new(200).set_body_string(board_json(42, "renamed board")))
        .expect(1)
        .mount(&server)
        .await;

    let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
    let mut exec = PushExecutor::new(&client, overlay_with(42, &[], &[], &[]), FEATURES);
    let outcome = exec
        .execute(
            &group(
                100,
                MaterializedOp::UpdateBoard { board: board_id(1) },
                taskboard_domain::PushTarget::Board(board_id(1)),
                &[],
            ),
            &[],
            tables_with_boards(&[board_row(1, "renamed board")]),
        )
        .await;
    server.verify().await;

    let GroupOutcome::Outcomes(outcomes) = outcome else {
        panic!("expected outcomes");
    };
    assert!(matches!(
        &outcomes[0].result,
        PushResult::Applied { echo: Some(taskboard_domain::RemoteEcho::Board(view)) }
            if view.title == "renamed board"
    ));
    // Full-send: title + color + archived in the body.
    let body: serde_json::Value =
        serde_json::from_slice(&server.received_requests().await.unwrap()[0].body).unwrap();
    assert_eq!(body["title"], "renamed board");
    assert_eq!(body["color"], "00c2e0");
}

#[tokio::test]
async fn rename_board_falls_back_on_an_invalid_stored_color() {
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path("/index.php/apps/deck/api/v1.0/boards/42"))
        .respond_with(ResponseTemplate::new(200).set_body_string(board_json(42, "x")))
        .mount(&server)
        .await;

    let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
    let mut exec = PushExecutor::new(&client, overlay_with(42, &[], &[], &[]), FEATURES);
    let mut row = board_row(1, "x");
    row.color = taskboard_domain::Color::new("not-hex");
    let outcome = exec
        .execute(
            &group(
                100,
                MaterializedOp::UpdateBoard { board: board_id(1) },
                taskboard_domain::PushTarget::Board(board_id(1)),
                &[],
            ),
            &[],
            tables_with_boards(&[row]),
        )
        .await;
    server.verify().await;

    assert!(matches!(
        outcome,
        GroupOutcome::Outcomes(ref o) if matches!(o[0].result, PushResult::Applied { .. })
    ));
    let body: serde_json::Value =
        serde_json::from_slice(&server.received_requests().await.unwrap()[0].body).unwrap();
    assert_eq!(body["color"], "0082c9", "the fallback color is sent");
}

#[tokio::test]
async fn rename_board_404_maps_to_remote_missing() {
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path("/index.php/apps/deck/api/v1.0/boards/42"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
    let mut exec = PushExecutor::new(&client, overlay_with(42, &[], &[], &[]), FEATURES);
    let outcome = exec
        .execute(
            &group(
                100,
                MaterializedOp::UpdateBoard { board: board_id(1) },
                taskboard_domain::PushTarget::Board(board_id(1)),
                &[],
            ),
            &[],
            tables_with_boards(&[board_row(1, "x")]),
        )
        .await;
    assert!(matches!(
        outcome,
        GroupOutcome::Outcomes(ref o) if matches!(o[0].result, PushResult::RemoteMissing)
    ));
}

#[tokio::test]
async fn rename_board_transport_failure_aborts_the_cycle() {
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path("/index.php/apps/deck/api/v1.0/boards/42"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;

    let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
    let mut exec = PushExecutor::new(&client, overlay_with(42, &[], &[], &[]), FEATURES);
    let outcome = exec
        .execute(
            &group(
                100,
                MaterializedOp::UpdateBoard { board: board_id(1) },
                taskboard_domain::PushTarget::Board(board_id(1)),
                &[],
            ),
            &[],
            tables_with_boards(&[board_row(1, "x")]),
        )
        .await;
    assert!(matches!(outcome, GroupOutcome::Aborted { landed } if landed.is_empty()));
}

#[tokio::test]
async fn delete_stack_unbound_completes_synthetic_without_sending() {
    let server = MockServer::start().await;
    let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
    let mut exec = PushExecutor::new(&client, overlay_with(42, &[], &[], &[]), FEATURES);
    let outcome = exec
        .execute(
            &group(
                100,
                MaterializedOp::DeleteStack { stack: stack_id(5) },
                taskboard_domain::PushTarget::Stack(stack_id(5)),
                &[],
            ),
            &[],
            empty_tables(),
        )
        .await;

    let GroupOutcome::Outcomes(outcomes) = outcome else {
        panic!("expected outcomes");
    };
    assert!(matches!(
        outcomes[0].result,
        PushResult::Applied { echo: None }
    ));
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn delete_stack_sends_the_delete_and_applies_the_echo() {
    let server = MockServer::start().await;
    Mock::given(method("DELETE"))
        .and(path("/index.php/apps/deck/api/v1.0/boards/42/stacks/7"))
        .respond_with(ResponseTemplate::new(200).set_body_string(stack_json(7, "Doing", 0)))
        .expect(1)
        .mount(&server)
        .await;

    let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
    let mut exec = PushExecutor::new(&client, overlay_with(42, &[(5, 7)], &[], &[]), FEATURES);
    let outcome = exec
        .execute(
            &group(
                100,
                MaterializedOp::DeleteStack { stack: stack_id(5) },
                taskboard_domain::PushTarget::Stack(stack_id(5)),
                &[],
            ),
            &[],
            empty_tables(),
        )
        .await;
    server.verify().await;

    let GroupOutcome::Outcomes(outcomes) = outcome else {
        panic!("expected outcomes");
    };
    assert!(matches!(
        &outcomes[0].result,
        PushResult::Applied { echo: Some(RemoteEcho::Stack(view)) }
            if view.id.stack == RemoteStackId(7)
    ));
}

#[tokio::test]
async fn delete_stack_404_maps_to_remote_missing() {
    let server = MockServer::start().await;
    Mock::given(method("DELETE"))
        .and(path("/index.php/apps/deck/api/v1.0/boards/42/stacks/7"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
    let mut exec = PushExecutor::new(&client, overlay_with(42, &[(5, 7)], &[], &[]), FEATURES);
    let outcome = exec
        .execute(
            &group(
                100,
                MaterializedOp::DeleteStack { stack: stack_id(5) },
                taskboard_domain::PushTarget::Stack(stack_id(5)),
                &[],
            ),
            &[],
            empty_tables(),
        )
        .await;

    let GroupOutcome::Outcomes(outcomes) = outcome else {
        panic!("expected outcomes");
    };
    assert!(matches!(outcomes[0].result, PushResult::RemoteMissing));
}

// ---- board delete push path ----------------------------------------

#[tokio::test]
async fn delete_board_sends_the_delete_and_applies_the_board_echo() {
    let server = MockServer::start().await;
    Mock::given(method("DELETE"))
        .and(path("/index.php/apps/deck/api/v1.0/boards/42"))
        .respond_with(ResponseTemplate::new(200).set_body_string(board_json(42, "doomed")))
        .expect(1)
        .mount(&server)
        .await;

    let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
    let mut exec = PushExecutor::new(&client, overlay_with(42, &[], &[], &[]), FEATURES);
    let outcome = exec
        .execute(
            &group(
                100,
                MaterializedOp::DeleteBoard { board: board_id(1) },
                taskboard_domain::PushTarget::Board(board_id(1)),
                &[],
            ),
            &[],
            tables_with_boards(&[board_row(1, "doomed")]),
        )
        .await;
    server.verify().await;

    let GroupOutcome::Outcomes(outcomes) = outcome else {
        panic!("expected outcomes");
    };
    assert!(matches!(
        &outcomes[0].result,
        PushResult::Applied { echo: Some(RemoteEcho::Board(view)) }
            if view.id == RemoteBoardId(42)
    ));
}

#[tokio::test]
async fn delete_board_404_maps_to_remote_missing() {
    let server = MockServer::start().await;
    Mock::given(method("DELETE"))
        .and(path("/index.php/apps/deck/api/v1.0/boards/42"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
    let mut exec = PushExecutor::new(&client, overlay_with(42, &[], &[], &[]), FEATURES);
    let outcome = exec
        .execute(
            &group(
                100,
                MaterializedOp::DeleteBoard { board: board_id(1) },
                taskboard_domain::PushTarget::Board(board_id(1)),
                &[],
            ),
            &[],
            tables_with_boards(&[board_row(1, "doomed")]),
        )
        .await;

    let GroupOutcome::Outcomes(outcomes) = outcome else {
        panic!("expected outcomes");
    };
    assert!(matches!(outcomes[0].result, PushResult::RemoteMissing));
}

// ---- label push paths --------------------------------------------

#[tokio::test]
async fn create_label_posts_and_applies_the_echo() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/index.php/apps/deck/api/v1.0/boards/42/labels"))
        .respond_with(ResponseTemplate::new(200).set_body_string(label_json(4, "bug", "ff0000")))
        .expect(1)
        .mount(&server)
        .await;

    let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
    let mut exec = PushExecutor::new(&client, overlay_with(42, &[], &[], &[]), FEATURES);
    let outcome = exec
        .execute(
            &group(
                100,
                MaterializedOp::CreateLabel { label: label_id(6) },
                taskboard_domain::PushTarget::Label(label_id(6)),
                &[],
            ),
            &[],
            tables_with(&[], &[label_row(6, "bug", "FF0000")]),
        )
        .await;
    server.verify().await;

    let GroupOutcome::Outcomes(outcomes) = outcome else {
        panic!("expected outcomes");
    };
    assert!(matches!(
        &outcomes[0].result,
        PushResult::Applied { echo: Some(RemoteEcho::Label(view)) }
            if view.id.label == RemoteLabelId(4) && view.title == "bug"
    ));
    let body: serde_json::Value =
        serde_json::from_slice(&server.received_requests().await.unwrap()[0].body).unwrap();
    assert_eq!(
        body,
        serde_json::json!({"title": "bug", "color": "ff0000"}),
        "the local lenient color is normalized for the wire"
    );
}

#[tokio::test]
async fn create_label_with_an_invalid_local_color_rejects_bad_request_without_sending() {
    let server = MockServer::start().await;
    let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
    let mut exec = PushExecutor::new(&client, overlay_with(42, &[], &[], &[]), FEATURES);
    let outcome = exec
        .execute(
            &group(
                100,
                MaterializedOp::CreateLabel { label: label_id(6) },
                taskboard_domain::PushTarget::Label(label_id(6)),
                &[],
            ),
            &[],
            tables_with(&[], &[label_row(6, "bug", "not-a-color")]),
        )
        .await;

    let GroupOutcome::Outcomes(outcomes) = outcome else {
        panic!("expected outcomes");
    };
    assert!(matches!(
        outcomes[0].result,
        PushResult::Rejected {
            kind: taskboard_domain::SyncErrorKind::BadRequest
        }
    ));
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn create_label_with_a_missing_row_completes_synthetic_without_sending() {
    let server = MockServer::start().await;
    let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
    let mut exec = PushExecutor::new(&client, overlay_with(42, &[], &[], &[]), FEATURES);
    let outcome = exec
        .execute(
            &group(
                100,
                MaterializedOp::CreateLabel { label: label_id(6) },
                taskboard_domain::PushTarget::Label(label_id(6)),
                &[],
            ),
            &[],
            empty_tables(),
        )
        .await;

    let GroupOutcome::Outcomes(outcomes) = outcome else {
        panic!("expected outcomes");
    };
    assert!(matches!(
        outcomes[0].result,
        PushResult::Applied { echo: None }
    ));
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn create_label_transport_failure_aborts() {
    let client = refused_client();
    let mut exec = PushExecutor::new(&client, overlay_with(42, &[], &[], &[]), FEATURES);
    let outcome = exec
        .execute(
            &group(
                100,
                MaterializedOp::CreateLabel { label: label_id(6) },
                taskboard_domain::PushTarget::Label(label_id(6)),
                &[],
            ),
            &[],
            tables_with(&[], &[label_row(6, "bug", "ff0000")]),
        )
        .await;
    assert!(matches!(
        outcome,
        GroupOutcome::Aborted { landed } if landed.is_empty()
    ));
}

#[tokio::test]
async fn update_label_puts_the_changeset_and_applies_the_echo() {
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path("/index.php/apps/deck/api/v1.0/boards/42/labels/3"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(label_json(3, "critical", "0000ff")),
        )
        .expect(1)
        .mount(&server)
        .await;

    let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
    let mut exec = PushExecutor::new(&client, overlay_with(42, &[], &[], &[(6, 3)]), FEATURES);
    let outcome = exec
        .execute(
            &group(
                100,
                MaterializedOp::UpdateLabel { label: label_id(6) },
                taskboard_domain::PushTarget::Label(label_id(6)),
                &[],
            ),
            &[],
            tables_with(&[], &[label_row(6, "critical", "0000ff")]),
        )
        .await;
    server.verify().await;

    let GroupOutcome::Outcomes(outcomes) = outcome else {
        panic!("expected outcomes");
    };
    assert!(matches!(
        &outcomes[0].result,
        PushResult::Applied { echo: Some(RemoteEcho::Label(view)) } if view.title == "critical"
    ));
    let body: serde_json::Value =
        serde_json::from_slice(&server.received_requests().await.unwrap()[0].body).unwrap();
    assert_eq!(body["title"], "critical");
}

#[tokio::test]
async fn update_of_an_unbound_label_rejects_local_data_without_sending() {
    let server = MockServer::start().await;
    let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
    let mut exec = PushExecutor::new(&client, overlay_with(42, &[], &[], &[]), FEATURES);
    let outcome = exec
        .execute(
            &group(
                100,
                MaterializedOp::UpdateLabel { label: label_id(6) },
                taskboard_domain::PushTarget::Label(label_id(6)),
                &[],
            ),
            &[],
            tables_with(&[], &[label_row(6, "critical", "0000ff")]),
        )
        .await;

    let GroupOutcome::Outcomes(outcomes) = outcome else {
        panic!("expected outcomes");
    };
    assert!(matches!(
        outcomes[0].result,
        PushResult::Rejected {
            kind: taskboard_domain::SyncErrorKind::LocalData
        }
    ));
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn update_label_with_an_unencodable_color_rejects_bad_request_without_sending() {
    let server = MockServer::start().await;
    let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
    let mut exec = PushExecutor::new(&client, overlay_with(42, &[], &[], &[(6, 3)]), FEATURES);
    let outcome = exec
        .execute(
            &group(
                100,
                MaterializedOp::UpdateLabel { label: label_id(6) },
                taskboard_domain::PushTarget::Label(label_id(6)),
                &[],
            ),
            &[],
            tables_with(&[], &[label_row(6, "critical", "#0000ff")]),
        )
        .await;

    let GroupOutcome::Outcomes(outcomes) = outcome else {
        panic!("expected outcomes");
    };
    assert!(matches!(
        outcomes[0].result,
        PushResult::Rejected {
            kind: taskboard_domain::SyncErrorKind::BadRequest
        }
    ));
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn delete_label_unbound_completes_synthetic_without_sending() {
    let server = MockServer::start().await;
    let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
    let mut exec = PushExecutor::new(&client, overlay_with(42, &[], &[], &[]), FEATURES);
    let outcome = exec
        .execute(
            &group(
                100,
                MaterializedOp::DeleteLabel { label: label_id(6) },
                taskboard_domain::PushTarget::Label(label_id(6)),
                &[],
            ),
            &[],
            empty_tables(),
        )
        .await;

    let GroupOutcome::Outcomes(outcomes) = outcome else {
        panic!("expected outcomes");
    };
    assert!(matches!(
        outcomes[0].result,
        PushResult::Applied { echo: None }
    ));
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn delete_label_sends_the_delete_and_applies_the_echo() {
    let server = MockServer::start().await;
    Mock::given(method("DELETE"))
        .and(path("/index.php/apps/deck/api/v1.0/boards/42/labels/3"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(label_json(3, "critical", "0000ff")),
        )
        .expect(1)
        .mount(&server)
        .await;

    let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
    let mut exec = PushExecutor::new(&client, overlay_with(42, &[], &[], &[(6, 3)]), FEATURES);
    let outcome = exec
        .execute(
            &group(
                100,
                MaterializedOp::DeleteLabel { label: label_id(6) },
                taskboard_domain::PushTarget::Label(label_id(6)),
                &[],
            ),
            &[],
            empty_tables(),
        )
        .await;
    server.verify().await;

    let GroupOutcome::Outcomes(outcomes) = outcome else {
        panic!("expected outcomes");
    };
    assert!(matches!(
        &outcomes[0].result,
        PushResult::Applied { echo: Some(RemoteEcho::Label(view)) }
            if view.id.label == RemoteLabelId(3)
    ));
}

/// Writes one minimal HTTP/1.1 response carrying the card JSON, then
/// closes the connection (`Connection: close` forces the next request
/// onto a fresh connection, which the test server then drops).
fn serve_one_create(mut stream: std::net::TcpStream) {
    use std::io::{Read, Write};
    let mut buf = [0_u8; 4096];
    let mut request = Vec::new();
    // Read until the end of the headers, then the content-length body.
    loop {
        let Ok(n) = stream.read(&mut buf) else { return };
        if n == 0 {
            return;
        }
        request.extend_from_slice(&buf[..n]);
        let Some(header_end) = request.windows(4).position(|w| w == b"\r\n\r\n") else {
            continue;
        };
        let headers = String::from_utf8_lossy(&request[..header_end]).to_lowercase();
        let length: usize = headers
            .lines()
            .find_map(|l| l.strip_prefix("content-length:"))
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(0);
        if request.len() >= header_end + 4 + length {
            break;
        }
    }
    let body = card_json(55, 7, 0);
    let _ = write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    let _ = stream.flush();
}

/// Writes one minimal HTTP/1.1 response carrying the card JSON, then
/// closes the connection (`Connection: close` forces the next request
/// onto a fresh connection) — the two-response variant of
/// `serve_one_create`, for tests that drive one GET and one PUT before
/// the failure under test.
fn serve_one_card(mut stream: std::net::TcpStream) {
    use std::io::{Read, Write};
    let mut buf = [0_u8; 4096];
    let mut request = Vec::new();
    loop {
        let Ok(n) = stream.read(&mut buf) else { return };
        if n == 0 {
            return;
        }
        request.extend_from_slice(&buf[..n]);
        let Some(header_end) = request.windows(4).position(|w| w == b"\r\n\r\n") else {
            continue;
        };
        let headers = String::from_utf8_lossy(&request[..header_end]).to_lowercase();
        let length: usize = headers
            .lines()
            .find_map(|l| l.strip_prefix("content-length:"))
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(0);
        if request.len() >= header_end + 4 + length {
            break;
        }
    }
    let body = card_json(55, 7, 3);
    let _ = write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    let _ = stream.flush();
}

/// An active-listing variant carrying one card under a different stack
/// than the binding (the #258 moved-card shape).
fn relocated_listing(stack: u64, card_stack: u64) -> String {
    serde_json::json!([{
        "id": stack, "title": "moved-here", "boardId": 42, "order": 1,
        "lastModified": 1_700_000_000, "deletedAt": 0, "archived": false,
        "cards": [{ "id": 55, "title": "old", "stackId": card_stack,
                    "type": "plain", "order": 3, "labels": [],
                    "archived": false, "duedate": null, "done": null }]
    }])
    .to_string()
}

fn delete_group() -> (PushGroup, Vec<PendingOp>) {
    (
        group(
            100,
            MaterializedOp::DeleteTask { task: task_id(10) },
            taskboard_domain::PushTarget::Task(task_id(10)),
            &[],
        ),
        Vec::new(),
    )
}

/// D1t: a delete-path 404 re-resolves the card against the fresh
/// listings once, rebinds, and deletes at the fresh stack — exactly one
/// listing pair and one retried delete, no retry storm (issue #319).
#[tokio::test]
async fn delete_404_re_resolves_and_deletes_at_the_fresh_binding() {
    let server = MockServer::start().await;
    Mock::given(method("DELETE"))
        .and(path(
            "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55",
        ))
        .respond_with(ResponseTemplate::new(404))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/index.php/apps/deck/api/v1.0/boards/42/stacks"))
        .respond_with(ResponseTemplate::new(200).set_body_string(relocated_listing(8, 8)))
        .expect(1)
        .mount(&server)
        .await;
    // Only consulted when the active listing misses.
    Mock::given(method("GET"))
        .and(path(
            "/index.php/apps/deck/api/v1.0/boards/42/stacks/archived",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_string("[]"))
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path(
            "/index.php/apps/deck/api/v1.0/boards/42/stacks/8/cards/55",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_string(card_json(55, 8, 3)))
        .expect(1)
        .mount(&server)
        .await;

    let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
    let mut exec = PushExecutor::new(
        &client,
        overlay_with(42, &[], &[(10, 7, 55)], &[]),
        FEATURES,
    );
    let (grp, outbox) = delete_group();
    let outcome = exec.execute(&grp, &outbox, empty_tables()).await;
    server.verify().await;

    let GroupOutcome::Outcomes(outcomes) = outcome else {
        panic!("expected outcomes, got {outcome:?}");
    };
    assert_eq!(outcomes.len(), 1);
    assert!(
        matches!(
            &outcomes[0].result,
            PushResult::Applied {
                echo: Some(RemoteEcho::Task(view))
            } if view.id.stack == taskboard_domain::RemoteStackId(8)
                && view.id.card == taskboard_domain::RemoteCardId(55)
        ),
        "the delete lands at the fresh stack-8 binding: {:?}",
        outcomes[0].result
    );
}

/// D2t: a delete-path 404 whose re-resolve finds the card in neither
/// listing stays `RemoteMissing` — the card is genuinely gone from
/// every stack, so the tombstone converges (issue #319, D2).
#[tokio::test]
async fn delete_404_with_the_card_absent_from_both_listings_stays_remote_missing() {
    let server = MockServer::start().await;
    Mock::given(method("DELETE"))
        .and(path(
            "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55",
        ))
        .respond_with(ResponseTemplate::new(404))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/index.php/apps/deck/api/v1.0/boards/42/stacks"))
        .respond_with(ResponseTemplate::new(200).set_body_string("[]"))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(
            "/index.php/apps/deck/api/v1.0/boards/42/stacks/archived",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_string("[]"))
        .mount(&server)
        .await;

    let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
    let mut exec = PushExecutor::new(
        &client,
        overlay_with(42, &[], &[(10, 7, 55)], &[]),
        FEATURES,
    );
    let (grp, outbox) = delete_group();
    let outcome = exec.execute(&grp, &outbox, empty_tables()).await;
    server.verify().await;

    let GroupOutcome::Outcomes(outcomes) = outcome else {
        panic!("expected outcomes, got {outcome:?}");
    };
    assert_eq!(outcomes.len(), 1, "primary-only, subsumed stay queued");
    assert!(matches!(outcomes[0].result, PushResult::RemoteMissing));
}

/// D3t: a card located in the listing but 404 again on the retried
/// delete converges `RemoteMissing` — the write path outranks a lagging
/// listing (issue #319, D7).
#[tokio::test]
async fn delete_404_located_but_gone_on_the_wire_converges_remote_missing() {
    let server = MockServer::start().await;
    Mock::given(method("DELETE"))
        .and(path(
            "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55",
        ))
        .respond_with(ResponseTemplate::new(404))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/index.php/apps/deck/api/v1.0/boards/42/stacks"))
        .respond_with(ResponseTemplate::new(200).set_body_string(relocated_listing(8, 8)))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(
            "/index.php/apps/deck/api/v1.0/boards/42/stacks/archived",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_string("[]"))
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path(
            "/index.php/apps/deck/api/v1.0/boards/42/stacks/8/cards/55",
        ))
        .respond_with(ResponseTemplate::new(404))
        .expect(1)
        .mount(&server)
        .await;

    let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
    let mut exec = PushExecutor::new(
        &client,
        overlay_with(42, &[], &[(10, 7, 55)], &[]),
        FEATURES,
    );
    let (grp, outbox) = delete_group();
    let outcome = exec.execute(&grp, &outbox, empty_tables()).await;
    server.verify().await;

    let GroupOutcome::Outcomes(outcomes) = outcome else {
        panic!("expected outcomes, got {outcome:?}");
    };
    assert_eq!(outcomes.len(), 1, "primary-only, subsumed stay queued");
    assert!(matches!(outcomes[0].result, PushResult::RemoteMissing));
}

/// D4t: the re-resolve consults the *archived* listing too — a card
/// moved to an archived stack is found there and deleted (same presence
/// truth as the pull).
#[tokio::test]
async fn delete_re_resolve_consults_the_archived_listing() {
    let server = MockServer::start().await;
    Mock::given(method("DELETE"))
        .and(path(
            "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55",
        ))
        .respond_with(ResponseTemplate::new(404))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/index.php/apps/deck/api/v1.0/boards/42/stacks"))
        .respond_with(ResponseTemplate::new(200).set_body_string("[]"))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(
            "/index.php/apps/deck/api/v1.0/boards/42/stacks/archived",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_string(relocated_listing(9, 9)))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path(
            "/index.php/apps/deck/api/v1.0/boards/42/stacks/9/cards/55",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_string(card_json(55, 9, 3)))
        .expect(1)
        .mount(&server)
        .await;

    let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
    let mut exec = PushExecutor::new(
        &client,
        overlay_with(42, &[], &[(10, 7, 55)], &[]),
        FEATURES,
    );
    let (grp, outbox) = delete_group();
    let outcome = exec.execute(&grp, &outbox, empty_tables()).await;
    server.verify().await;

    let GroupOutcome::Outcomes(outcomes) = outcome else {
        panic!("expected outcomes, got {outcome:?}");
    };
    assert!(
        matches!(
            &outcomes[0].result,
            PushResult::Applied {
                echo: Some(RemoteEcho::Task(view))
            } if view.id.stack == taskboard_domain::RemoteStackId(9)
        ),
        "the archived listing located the card: {:?}",
        outcomes[0].result
    );
}

/// D5t: a forbidden listing during a delete re-resolve classifies
/// `RemoteMissing` — the delete ctx treats forbidden as gone (the
/// deliberate divergence from the update path's `Rejected
/// { Forbidden }`).
#[tokio::test]
async fn delete_re_resolve_with_a_forbidden_listing_converges_remote_missing() {
    let server = MockServer::start().await;
    Mock::given(method("DELETE"))
        .and(path(
            "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55",
        ))
        .respond_with(ResponseTemplate::new(404))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/index.php/apps/deck/api/v1.0/boards/42/stacks"))
        .respond_with(ResponseTemplate::new(403))
        .mount(&server)
        .await;

    let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
    let mut exec = PushExecutor::new(
        &client,
        overlay_with(42, &[], &[(10, 7, 55)], &[]),
        FEATURES,
    );
    let (grp, outbox) = delete_group();
    let outcome = exec.execute(&grp, &outbox, empty_tables()).await;
    server.verify().await;

    let GroupOutcome::Outcomes(outcomes) = outcome else {
        panic!("expected outcomes, got {outcome:?}");
    };
    assert_eq!(outcomes.len(), 1, "primary-only, subsumed stay queued");
    assert!(matches!(outcomes[0].result, PushResult::RemoteMissing));
}

/// F6-24: a 503 on the *post-create* archive sub-call is transport-class
/// — the group aborts carrying the create as landed evidence (the
/// re-POST hazard the `landed` field exists to prevent). `is_transport →
/// false` would swallow the failure into a plain `Outcomes`.
#[tokio::test]
async fn post_create_archive_transport_failure_aborts_with_the_create_as_landed() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(
            "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_string(card_json(55, 7, 0)))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path(
            "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55/archive",
        ))
        .respond_with(ResponseTemplate::new(503))
        .expect(3)
        .mount(&server)
        .await;

    let client = DeckClient::new(&server.uri(), "u", "t")
        .unwrap()
        .with_sleeper(std::sync::Arc::new(InstantSleeper));
    let mut exec = PushExecutor::new(&client, overlay_with(42, &[(5, 7)], &[], &[]), FEATURES);
    let shape = taskboard_domain::NewCardShape {
        title: "created".into(),
        order: 0,
        description: String::new(),
        duedate: None,
        scheduled: None,
        done: None,
        archived: true,
        labels: std::collections::BTreeSet::new(),
    };
    let outcome = exec
        .execute(
            &group(
                100,
                MaterializedOp::CreateTask {
                    task: task_id(10),
                    stack: stack_id(5),
                    new_card: shape,
                },
                taskboard_domain::PushTarget::Task(task_id(10)),
                &[],
            ),
            &[op(100, LocalOp::CreateTask(task_id(10)))],
            empty_tables(),
        )
        .await;

    let GroupOutcome::Aborted { landed } = outcome else {
        panic!("a transport failure on the post-create archive aborts the group");
    };
    assert_eq!(landed.len(), 1, "the create's evidence rides the abort");
    assert_eq!(landed[0].op, taskboard_domain::OpId(uid(100)));
    assert!(
        matches!(
            &landed[0].result,
            PushResult::Applied { echo: Some(RemoteEcho::Task(view)) }
                if view.id.card == taskboard_domain::RemoteCardId(55)
        ),
        "the landed create carries its echo so the next cycle never re-POSTs"
    );
    let () = server.verify().await;
}

/// F6-24: a *classified* rejection (403) on the post-create archive
/// sub-call is tolerated — the flag repairs on a later cycle, so the
/// group completes with exactly the create's `Applied` outcome and no
/// `Rejected` entry (the sub-call has no op of its own). `is_transport →
/// true` would turn the 403 into an abort.
#[tokio::test]
async fn post_create_archive_rejection_is_tolerated_without_aborting() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(
            "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_string(card_json(55, 7, 0)))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path(
            "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55/archive",
        ))
        .respond_with(ResponseTemplate::new(403))
        .expect(1)
        .mount(&server)
        .await;

    let client = DeckClient::new(&server.uri(), "u", "t")
        .unwrap()
        .with_sleeper(std::sync::Arc::new(InstantSleeper));
    let mut exec = PushExecutor::new(&client, overlay_with(42, &[(5, 7)], &[], &[]), FEATURES);
    let shape = taskboard_domain::NewCardShape {
        title: "created".into(),
        order: 0,
        description: String::new(),
        duedate: None,
        scheduled: None,
        done: None,
        archived: true,
        labels: std::collections::BTreeSet::new(),
    };
    let outcome = exec
        .execute(
            &group(
                100,
                MaterializedOp::CreateTask {
                    task: task_id(10),
                    stack: stack_id(5),
                    new_card: shape,
                },
                taskboard_domain::PushTarget::Task(task_id(10)),
                &[],
            ),
            &[op(100, LocalOp::CreateTask(task_id(10)))],
            empty_tables(),
        )
        .await;

    let GroupOutcome::Outcomes(outcomes) = outcome else {
        panic!("a classified archive rejection must not abort the group");
    };
    assert_eq!(outcomes.len(), 1, "the sub-call adds no outcome of its own");
    assert_eq!(outcomes[0].op, taskboard_domain::OpId(uid(100)));
    assert!(matches!(
        &outcomes[0].result,
        PushResult::Applied { echo: Some(RemoteEcho::Task(view)) }
            if view.id.card == taskboard_domain::RemoteCardId(55)
    ));
    let () = server.verify().await;
}
