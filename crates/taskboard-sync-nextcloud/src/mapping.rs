// SPDX-License-Identifier: MIT OR Apache-2.0
//! Pure DTO ↔ domain mapping plus the push classification table (phase 4
//! plan §5.3/§5.6).
//!
//! Read direction: Deck wire structs → the domain's `Remote*` observation
//! views (epoch-second timestamps, `deleted_at: 0` ⇒ live). Write
//! direction: the planner's domain-side shapes → the client's write-param
//! structs. [`classify_push`] is the normative `DeckError → PushResult`
//! table; transport-classified errors return `None` (the cycle aborts, no
//! outcome is emitted).

use std::collections::BTreeSet;

use chrono::{DateTime, Utc};

use taskboard_domain::{
    CardShape, NewCardShape, PushResult, RemoteBoard, RemoteBoardId, RemoteCardRef, RemoteEcho,
    RemoteLabel, RemoteLabelId, RemoteLabelRef, RemoteStack, RemoteStackId, RemoteStackRef,
    RemoteTask, SyncErrorKind,
};

use crate::client::{LabelChanges, NewCard, StackChanges};
use crate::color::DeckColor;
use crate::error::DeckError;
use crate::model::{Board, Card, Label, Stack};

/// Decodes an epoch-seconds wire timestamp. `0` (or missing) is the
/// documented sentinel of old servers and decodes to the minimum
/// `DateTime` — losing every LWW comparison, as the domain view's contract
/// requires.
fn epoch_secs(raw: i64) -> DateTime<Utc> {
    if raw <= 0 {
        return DateTime::<Utc>::MIN_UTC;
    }
    DateTime::from_timestamp(raw, 0).unwrap_or(DateTime::<Utc>::MIN_UTC)
}

/// Soft-delete stamp: `0` (or missing) means live.
fn deleted_at_secs(raw: i64) -> Option<DateTime<Utc>> {
    (raw > 0).then_some(epoch_secs(raw))
}

/// Maps a wire board onto the domain view (color passes through as the raw
/// wire string — the domain view is color-string based by design).
#[must_use]
pub fn map_board(board: &Board) -> RemoteBoard {
    RemoteBoard {
        id: RemoteBoardId(board.id),
        title: board.title.clone(),
        color: board.color.as_str().to_owned(),
        archived: board.archived,
        deleted_at: deleted_at_secs(board.deleted_at),
        last_modified: epoch_secs(board.last_modified),
    }
}

/// Maps a wire stack (from either listing) onto the domain view.
#[must_use]
pub fn map_stack(stack: &Stack, board: RemoteBoardId) -> RemoteStack {
    RemoteStack {
        id: RemoteStackRef {
            board,
            stack: RemoteStackId(stack.id),
        },
        title: stack.title.clone(),
        order: stack.order,
        archived: stack.archived,
        deleted_at: deleted_at_secs(stack.deleted_at),
        last_modified: epoch_secs(stack.last_modified),
    }
}

/// Maps a wire card onto the domain task view. Labels decode id-or-object
/// on the wire and map to the bare remote label ids.
#[must_use]
pub fn map_card(card: &Card, board: RemoteBoardId) -> RemoteTask {
    RemoteTask {
        id: RemoteCardRef {
            board,
            stack: RemoteStackId(card.stack_id),
            card: taskboard_domain::RemoteCardId(card.id),
        },
        title: card.title.clone(),
        description: card.description.clone(),
        duedate: card.duedate,
        done: card.done,
        stack: RemoteStackId(card.stack_id),
        order: card.order,
        labels: card
            .labels
            .iter()
            .map(|l| RemoteLabelId(l.id))
            .collect::<BTreeSet<_>>(),
        archived: card.archived,
        last_modified: epoch_secs(card.last_modified),
    }
}

/// Maps a wire label onto the domain view.
#[must_use]
pub fn map_label(label: &Label, board: RemoteBoardId) -> RemoteLabel {
    RemoteLabel {
        id: RemoteLabelRef {
            board,
            label: RemoteLabelId(label.id),
        },
        title: label.title.clone(),
        color: label.color.as_str().to_owned(),
        deleted_at: None, // labels carry no deletedAt on the wire
        last_modified: epoch_secs(label.last_modified),
    }
}

/// Echo of a successful write, mapped into the domain's remote-echo form.
#[must_use]
pub fn map_echo(card: &Card, board: RemoteBoardId) -> RemoteEcho {
    RemoteEcho::Task(map_card(card, board))
}

/// Echo of a successful stack write.
#[must_use]
pub fn map_stack_echo(stack: &Stack, board: RemoteBoardId) -> RemoteEcho {
    RemoteEcho::Stack(map_stack(stack, board))
}

/// Echo of a successful label write.
#[must_use]
pub fn map_label_echo(label: &Label, board: RemoteBoardId) -> RemoteEcho {
    RemoteEcho::Label(map_label(label, board))
}

/// Context for the write classification: the entity's existence/binding
/// state decides whether a 404/403 means "gone" or "not pushable".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PushCtx {
    /// The op targets an entity that is bound (the server knew it).
    pub existing: bool,
    /// The op is a delete.
    pub delete: bool,
}

/// The normative `DeckError → PushResult` classification for writes
/// (phase 4 decision 8).
///
/// `Ok(None)` means transport-classified (`Transport`, exhausted
/// `RateLimited`/`Unavailable`): the caller aborts the cycle and emits no
/// outcome for this or any later group. `Envelope` on a write is a *2xx
/// status whose body failed to decode* — the write landed, so it maps to
/// `Applied { echo: None }` exactly like the endpoints that return no
/// usable body at all.
#[must_use]
pub fn classify_push(err: &DeckError, ctx: PushCtx) -> Option<PushResult> {
    let rejected = |kind| Some(PushResult::Rejected { kind });
    match err {
        DeckError::NotFound => {
            if ctx.existing {
                Some(PushResult::RemoteMissing)
            } else {
                rejected(SyncErrorKind::LocalData)
            }
        }
        // Forbidden on a delete is treated as gone (tier-2-verified);
        // everywhere else an ACL problem must never fabricate a delete.
        DeckError::Forbidden if ctx.delete => Some(PushResult::RemoteMissing),
        DeckError::Forbidden => rejected(SyncErrorKind::Forbidden),
        DeckError::Unauthorized => rejected(SyncErrorKind::Auth),
        DeckError::BadRequest => rejected(SyncErrorKind::BadRequest),
        DeckError::Conflict | DeckError::PreconditionFailed | DeckError::Server(_) => {
            rejected(SyncErrorKind::Server)
        }
        // 2xx body that failed to decode: the write succeeded.
        DeckError::Envelope(_) => Some(PushResult::Applied { echo: None }),
        DeckError::RateLimited | DeckError::Unavailable | DeckError::Transport(_) => None,
        // The client is constructed once at boot; an invalid base URL can
        // never surface mid-cycle. Classified conservatively as a server
        // rejection (retryable, stays queued).
        DeckError::InvalidBaseUrl => rejected(SyncErrorKind::Server),
    }
}

/// Classifies a read failure into the deck-agnostic cycle-failure kind
/// (phase 4 plan §5.4). Transport-class errors (including exhausted
/// retries) map to `Network`; a read `Envelope` failure means the server
/// answered 2xx with undecodable data — a server problem, not a network
/// one.
#[must_use]
pub fn classify_read(err: &DeckError) -> SyncErrorKind {
    match err {
        DeckError::Transport(_) | DeckError::RateLimited | DeckError::Unavailable => {
            SyncErrorKind::Network
        }
        DeckError::Unauthorized => SyncErrorKind::Auth,
        DeckError::Forbidden => SyncErrorKind::Forbidden,
        DeckError::BadRequest | DeckError::Envelope(_) | DeckError::NotFound => {
            SyncErrorKind::Server
        }
        DeckError::Conflict
        | DeckError::PreconditionFailed
        | DeckError::Server(_)
        | DeckError::InvalidBaseUrl => SyncErrorKind::Server,
    }
}

/// Builds the create-card payload from the planner's shape.
#[must_use]
pub fn new_card_from(shape: &NewCardShape) -> NewCard {
    NewCard {
        title: shape.title.clone(),
        order: Some(shape.order),
        description: Some(shape.description.clone()),
        duedate: shape.duedate,
        kind: None,
    }
}

/// Builds the full-send stack changeset from the current local stack.
#[must_use]
pub fn stack_changes_from(stack: &taskboard_domain::Stack) -> StackChanges {
    StackChanges {
        title: stack.title.clone(),
        order: stack.order,
    }
}

/// Builds the full-send label changeset from the current local label.
/// `None` when the local (lenient) color is not a valid six-hex `DeckColor`
/// — the caller classifies that as a `BadRequest` rejection.
#[must_use]
pub fn label_changes_from(label: &taskboard_domain::Label) -> Option<LabelChanges> {
    Some(LabelChanges {
        title: label.title.clone(),
        color: DeckColor::from_hex(label.color.as_str()).ok()?,
    })
}

/// Builds the round-trip PUT body: the *fetched* wire card with the local
/// fields applied. `resolved_labels: None` keeps the fetched label list
/// (the caller defers when only unresolvable labels would have changed).
#[must_use]
pub fn apply_card_shape(
    fetched: &Card,
    shape: &CardShape,
    resolved_labels: Option<&[u64]>,
) -> Card {
    let mut card = fetched.clone();
    card.title.clone_from(&shape.title);
    card.description.clone_from(&shape.description);
    card.duedate = shape.duedate;
    card.done = shape.done;
    card.order = shape.order;
    card.archived = shape.archived;
    if let Some(labels) = resolved_labels {
        card.labels = labels
            .iter()
            .copied()
            .map(crate::model::CardLabel::from)
            .collect();
    }
    card
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use proptest::prelude::*;

    fn ts(secs: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(secs, 0).unwrap()
    }

    fn wire_card() -> Card {
        serde_json::from_str(
            r#"{
                "id": 5, "title": "card", "stackId": 9, "type": "plain",
                "order": 3, "lastModified": 1700000000,
                "labels": [7, {"id": 11, "title": "x", "color": "ff0000"}],
                "archived": false, "duedate": "2020-01-20T09:52:43+00:00",
                "done": null
            }"#,
        )
        .unwrap()
    }

    #[test]
    fn board_mapping_preserves_fields_and_decodes_deleted_at() {
        let board: Board = serde_json::from_str(
            r#"{"id": 42, "title": "b", "color": "00c2e0", "deletedAt": 0,
                "lastModified": 1700000000}"#,
        )
        .unwrap();
        let view = map_board(&board);
        assert_eq!(view.id, RemoteBoardId(42));
        assert_eq!(view.title, "b");
        assert_eq!(view.color, "00c2e0");
        assert_eq!(view.deleted_at, None, "deletedAt 0 means live");
        assert_eq!(view.last_modified, ts(1_700_000_000));

        let deleted: Board =
            serde_json::from_str(r#"{"id": 1, "title": "b", "color": "ffffff", "deletedAt": 42}"#)
                .unwrap();
        assert_eq!(map_board(&deleted).deleted_at, Some(ts(42)));
    }

    #[test]
    fn epoch_zero_sentinel_maps_to_the_minimum_datetime() {
        let board: Board =
            serde_json::from_str(r#"{"id": 1, "title": "b", "color": "ffffff"}"#).unwrap();
        assert_eq!(
            map_board(&board).last_modified,
            DateTime::<Utc>::MIN_UTC,
            "old-server epoch 0 loses every LWW comparison"
        );
    }

    #[test]
    fn card_mapping_carries_labels_ids_stack_and_archived() {
        let view = map_card(&wire_card(), RemoteBoardId(42));
        assert_eq!(
            view.id,
            RemoteCardRef {
                board: RemoteBoardId(42),
                stack: RemoteStackId(9),
                card: taskboard_domain::RemoteCardId(5),
            }
        );
        assert_eq!(view.stack, RemoteStackId(9));
        assert_eq!(view.order, 3);
        assert_eq!(view.labels.len(), 2, "id-or-object both decode to ids");
        assert!(view.labels.contains(&RemoteLabelId(7)));
        assert!(view.labels.contains(&RemoteLabelId(11)));
        assert_eq!(view.duedate, Some("2020-01-20T09:52:43Z".parse().unwrap()));
        assert_eq!(view.done, None);
    }

    #[test]
    fn archived_listing_cards_map_with_the_flag() {
        // NC35-style bare JSON (no OCS envelope) archived listing.
        let stacks: Vec<Stack> = crate::client::decode_envelope(
            br#"[{"id": 4, "title": "old", "boardId": 42, "order": 9,
                  "cards": [{"id": 6, "title": "archived card", "stackId": 4,
                             "archived": true, "lastModified": 100}]}]"#,
        )
        .unwrap();
        let card = &stacks[0].cards[0];
        let view = map_card(card, RemoteBoardId(42));
        assert!(view.archived);
        assert_eq!(view.id.stack, RemoteStackId(4));
    }

    #[test]
    fn label_mapping_passes_color_through() {
        let label: Label = serde_json::from_str(
            r#"{"id": 2, "title": "urgent", "color": "00C2E0", "boardId": 42}"#,
        )
        .unwrap();
        let view = map_label(&label, RemoteBoardId(42));
        assert_eq!(view.id.label, RemoteLabelId(2));
        assert_eq!(view.color, "00C2E0", "lenient read: verbatim");
        assert_eq!(view.deleted_at, None);
    }

    #[tokio::test]
    async fn classify_push_maps_every_table_row() {
        let existing = PushCtx {
            existing: true,
            delete: false,
        };
        let unbound = PushCtx {
            existing: false,
            delete: false,
        };
        let del = PushCtx {
            existing: true,
            delete: true,
        };

        // Transport class: abort (None), never an outcome.
        for err in [
            DeckError::Transport(reqwest::get("http://127.0.0.1:1/").await.unwrap_err()),
            DeckError::RateLimited,
            DeckError::Unavailable,
        ] {
            assert!(classify_push(&err, existing).is_none(), "{err:?}");
        }
        // 2xx decode failure on a write: the write landed.
        let envelope = DeckError::Envelope(serde_json::from_str::<Card>("{bad").unwrap_err());
        assert!(matches!(
            classify_push(&envelope, existing),
            Some(PushResult::Applied { echo: None })
        ));
        // 404 on an existing entity: gone; on a create: not pushable.
        assert!(matches!(
            classify_push(&DeckError::NotFound, existing),
            Some(PushResult::RemoteMissing)
        ));
        assert!(matches!(
            classify_push(&DeckError::NotFound, unbound),
            Some(PushResult::Rejected {
                kind: SyncErrorKind::LocalData
            })
        ));
        // 403 on a delete: gone (tier-2-verified); elsewhere: rejected.
        assert!(matches!(
            classify_push(&DeckError::Forbidden, del),
            Some(PushResult::RemoteMissing)
        ));
        assert!(matches!(
            classify_push(&DeckError::Forbidden, existing),
            Some(PushResult::Rejected {
                kind: SyncErrorKind::Forbidden
            })
        ));
        assert!(matches!(
            classify_push(&DeckError::Unauthorized, existing),
            Some(PushResult::Rejected {
                kind: SyncErrorKind::Auth
            })
        ));
        assert!(matches!(
            classify_push(&DeckError::BadRequest, existing),
            Some(PushResult::Rejected {
                kind: SyncErrorKind::BadRequest
            })
        ));
        for err in [DeckError::Conflict, DeckError::Server(500)] {
            assert!(matches!(
                classify_push(&err, existing),
                Some(PushResult::Rejected {
                    kind: SyncErrorKind::Server
                })
            ));
        }
    }

    #[tokio::test]
    async fn classify_read_maps_transport_and_server_classes() {
        assert_eq!(
            classify_read(&DeckError::Transport(
                reqwest::get("http://127.0.0.1:1/").await.unwrap_err()
            )),
            SyncErrorKind::Network
        );
        assert_eq!(
            classify_read(&DeckError::RateLimited),
            SyncErrorKind::Network
        );
        assert_eq!(classify_read(&DeckError::Unauthorized), SyncErrorKind::Auth);
        assert_eq!(
            classify_read(&DeckError::Forbidden),
            SyncErrorKind::Forbidden
        );
        let envelope = DeckError::Envelope(serde_json::from_str::<Card>("{bad").unwrap_err());
        assert_eq!(
            classify_read(&envelope),
            SyncErrorKind::Server,
            "a 2xx read whose body fails to decode is a server problem"
        );
    }

    #[test]
    fn apply_card_shape_overrides_local_fields_and_keeps_wire_scaffolding() {
        let fetched = wire_card();
        let shape = CardShape {
            title: "renamed".into(),
            description: "desc".into(),
            duedate: None,
            done: Some(ts(1_700_000_000)),
            order: 7,
            archived: true,
            labels: BTreeSet::new(),
        };
        let card = apply_card_shape(&fetched, &shape, Some(&[7, 11]));
        assert_eq!(card.title, "renamed");
        assert_eq!(card.description, "desc");
        assert_eq!(card.duedate, None);
        assert_eq!(card.done, Some(ts(1_700_000_000)));
        assert_eq!(card.order, 7);
        assert!(card.archived);
        assert_eq!(
            card.labels,
            vec![
                crate::model::CardLabel::from(7),
                crate::model::CardLabel::from(11)
            ]
        );
        assert_eq!(card.id, 5, "wire identity untouched");
        assert_eq!(card.stack_id, 9, "location comes from the fetched card");
        // None keeps the fetched label list (deferral path).
        let untouched = apply_card_shape(&fetched, &shape, None);
        assert_eq!(untouched.labels, fetched.labels);
    }

    #[test]
    fn label_changes_require_a_valid_deck_color() {
        let mut label = taskboard_domain::Label {
            id: taskboard_domain::LabelId::from(uuid::Uuid::from_u128(1)),
            remote: None,
            board: taskboard_domain::BoardId::from(uuid::Uuid::from_u128(2)),
            title: "urgent".into(),
            color: taskboard_domain::Color::new("00ff00"),
            deleted: false,
            clocks: taskboard_domain::LabelClocks {
                title: ts(0),
                color: ts(0),
                deleted: ts(0),
            },
            remote_seen: None,
        };
        let changes = label_changes_from(&label).expect("valid color");
        assert_eq!(changes.title, "urgent");
        assert_eq!(changes.color.as_str(), "00ff00");

        label.color = taskboard_domain::Color::new("not-a-color");
        assert!(
            label_changes_from(&label).is_none(),
            "a lenient local color that Deck would reject defers to BadRequest"
        );
    }

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(256))]

        #[test]
        fn card_mapping_preserves_generated_fields(
            id in 1_u64..1_000_000,
            stack in 1_u64..1_000_000,
            board_num in 1_u64..1_000_000,
            title in "[a-zA-Z0-9 ]{0,40}",
            description in "[a-zA-Z0-9 ]{0,80}",
            order in -100_i64..100,
            last_modified in 31_536_000_i64..4_102_444_800,
            archived in proptest::bool::ANY,
            has_due in proptest::bool::ANY,
        ) {
            let duedate = has_due.then_some(ts(1_700_000_000));
            let raw = format!(
                r#"{{"id": {id}, "title": "{title}", "stackId": {stack},
                    "description": "{description}", "order": {order},
                    "lastModified": {last_modified}, "archived": {archived}}}"#
            );
            let mut card: Card = serde_json::from_str(&raw).unwrap();
            card.duedate = duedate;
            let view = map_card(&card, RemoteBoardId(board_num));
            prop_assert_eq!(view.title, title);
            prop_assert_eq!(view.description, description);
            prop_assert_eq!(view.order, order);
            prop_assert_eq!(view.archived, archived);
            prop_assert_eq!(view.duedate, duedate);
            prop_assert_eq!(view.last_modified, ts(last_modified));
            prop_assert_eq!(view.id.card, taskboard_domain::RemoteCardId(id));
            prop_assert_eq!(view.id.stack, RemoteStackId(stack));
            prop_assert_eq!(view.id.board, RemoteBoardId(board_num));
        }

        #[test]
        fn stack_mapping_preserves_generated_fields(
            id in 1_u64..1_000_000,
            board_num in 1_u64..1_000_000,
            title in "[a-zA-Z0-9 ]{0,40}",
            order in -100_i64..100,
            deleted in 0_i64..1_700_000_000,
        ) {
            let raw = format!(
                r#"{{"id": {id}, "title": "{title}", "boardId": {board_num},
                    "order": {order}, "deletedAt": {deleted}}}"#
            );
            let stack: Stack = serde_json::from_str(&raw).unwrap();
            let view = map_stack(&stack, RemoteBoardId(board_num));
            prop_assert_eq!(view.title, title);
            prop_assert_eq!(view.order, order);
            prop_assert_eq!(view.deleted_at, deleted_at_secs(deleted));
            prop_assert_eq!(view.id.stack, RemoteStackId(id));
            prop_assert_eq!(view.id.board, RemoteBoardId(board_num));
        }
    }
}
