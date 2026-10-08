// SPDX-License-Identifier: MIT OR Apache-2.0
//! Deck wire model: DTOs for every resource the client reads or writes.
//!
//! One reviewable file (split into a `model/` directory when it outgrows
//! ~500 lines). The adapter owns its wire format; `taskboard-domain` stays
//! Deck-agnostic. Epoch-second timestamps stay wire-faithful `i64`; the
//! ISO-8601 `duedate`/`done` fields decode into `chrono` UTC datetimes.
//! Unknown fields are ignored so a newer server cannot break decoding.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::color::DeckColor;

/// A Deck board as returned by the boards endpoints.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Board {
    pub id: u64,
    pub title: String,
    /// Lenient on reads (any server string passes through); strict
    /// six-hex-digit construction via [`DeckColor::from_hex`] for writes.
    pub color: DeckColor,
    /// Unix timestamp of a soft delete; `0` (or missing on older servers)
    /// means the board is live. Deck's DELETE does not remove the board
    /// from listings, it only stamps this field.
    #[serde(default)]
    pub deleted_at: i64,
    /// Unix timestamp of the last modification.
    #[serde(default)]
    pub last_modified: i64,
    #[serde(default)]
    pub archived: bool,
    #[serde(default)]
    pub shared: i64,
    #[serde(default)]
    pub owner: Option<Participant>,
    #[serde(default, deserialize_with = "null_to_empty_vec")]
    pub users: Vec<Participant>,
    #[serde(default, deserialize_with = "null_to_empty_vec")]
    pub labels: Vec<Label>,
    #[serde(default, deserialize_with = "null_to_empty_vec")]
    pub acl: Vec<Acl>,
    #[serde(default)]
    pub permissions: Option<BoardPermissions>,
}

impl Board {
    /// True when the board has not been soft-deleted.
    #[must_use]
    pub fn is_live(&self) -> bool {
        self.deleted_at == 0
    }
}

/// Which stacks a listing returns; the `stacks/archived` route modelled as
/// a type instead of a bool parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StackFilter {
    /// `GET /boards/{id}/stacks`
    Active,
    /// `GET /boards/{id}/stacks/archived`
    Archived,
}

impl StackFilter {
    /// Resource path relative to the board, as used by [`crate::client`].
    #[must_use]
    pub fn path_segment(self) -> &'static str {
        match self {
            Self::Active => "stacks",
            Self::Archived => "stacks/archived",
        }
    }
}

/// A Deck stack (column) with its nested cards.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stack {
    pub id: u64,
    pub title: String,
    #[serde(default)]
    pub board_id: u64,
    #[serde(default)]
    pub deleted_at: i64,
    #[serde(default)]
    pub order: i64,
    #[serde(default, deserialize_with = "null_to_empty_vec")]
    pub cards: Vec<Card>,
}

/// A Deck card. Server-managed fields (`overdue`, `attachment_count`,
/// `comments_unread`) are read-only; labels/assignments/archived state
/// change via dedicated sub-endpoints, not card PUT.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Card {
    pub id: u64,
    pub title: String,
    #[serde(default)]
    pub stack_id: u64,
    #[serde(default)]
    pub description: String,
    /// Deck card type: `plain` or `rich` (wire field `type`).
    #[serde(rename = "type", default)]
    pub kind: String,
    /// Display order within the stack.
    #[serde(default)]
    pub order: i64,
    /// Unix timestamps (epoch seconds).
    #[serde(default)]
    pub created_at: i64,
    #[serde(default)]
    pub last_modified: i64,
    /// ISO-8601 deadline (`2020-01-20T09:52:43+00:00`) or `null`.
    #[serde(default, alias = "dueDate", serialize_with = "ser_opt_datetime")]
    pub duedate: Option<DateTime<Utc>>,
    /// ISO-8601 completion timestamp or `null`.
    #[serde(default, serialize_with = "ser_opt_datetime")]
    pub done: Option<DateTime<Utc>>,
    #[serde(default)]
    pub archived: bool,
    #[serde(default)]
    pub owner: Option<Participant>,
    #[serde(default)]
    pub last_editor: Option<String>,
    #[serde(default, deserialize_with = "null_to_empty_vec")]
    pub labels: Vec<u64>,
    #[serde(default, deserialize_with = "null_to_empty_vec")]
    pub assigned_users: Vec<Participant>,
    #[serde(default, deserialize_with = "null_to_empty_vec")]
    pub attachments: Vec<Attachment>,
    #[serde(default)]
    pub attachment_count: i64,
    #[serde(default)]
    pub comments_unread: i64,
    /// Server-managed overdue flag (`0` = not overdue).
    #[serde(default)]
    pub overdue: i64,
}

/// A colored tag attachable to cards.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Label {
    pub id: u64,
    pub title: String,
    /// Lenient on reads (any server string passes through); strict
    /// six-hex-digit construction via [`DeckColor::from_hex`] for writes.
    pub color: DeckColor,
    #[serde(default)]
    pub board_id: u64,
}

/// An access-control entry on a board (wire field `type` is the participant
/// kind: user/group/circle). The bool quartet mirrors the server payload
/// one-to-one; restructuring a wire DTO would hurt more than the lint helps.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Acl {
    pub id: u64,
    #[serde(rename = "type", default)]
    pub kind: i64,
    #[serde(default)]
    pub board_id: u64,
    #[serde(default)]
    pub participant: Option<Participant>,
    #[serde(default)]
    pub owner: bool,
    #[serde(default)]
    pub permission_edit: bool,
    #[serde(default)]
    pub permission_share: bool,
    #[serde(default)]
    pub permission_manage: bool,
}

/// A Nextcloud user (or group/circle) reference; fields may be absent
/// depending on the endpoint that embedded it.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Participant {
    #[serde(default)]
    pub primary_key: String,
    #[serde(default)]
    pub uid: String,
    #[serde(default)]
    pub displayname: String,
}

/// The permission set the requesting account holds on a board.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BoardPermissions {
    #[serde(rename = "PERMISSION_EDIT", default)]
    pub edit: bool,
    #[serde(rename = "PERMISSION_SHARE", default)]
    pub share: bool,
    #[serde(rename = "PERMISSION_MANAGE", default)]
    pub manage: bool,
}

/// Attachment *metadata* (uploads and content download are out of scope).
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Attachment {
    pub id: u64,
    #[serde(default)]
    pub card_id: u64,
    /// e.g. `deck_file` or `deck_comment` (wire field `type`).
    #[serde(rename = "type", default)]
    pub kind: String,
    #[serde(default)]
    pub created_at: i64,
    #[serde(default)]
    pub created_by: String,
    #[serde(default)]
    pub deleted_at: i64,
    #[serde(default)]
    pub data: Option<ExtendedData>,
    #[serde(default)]
    pub extended_data: Option<ExtendedData>,
}

/// Embedded file information of an attachment; every field optional because
/// the two payloads (`data`, `extendedData`) carry different subsets.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtendedData {
    #[serde(default)]
    pub file_name: String,
    #[serde(default)]
    pub size: i64,
    #[serde(default, alias = "mime")]
    pub mimetype: String,
    #[serde(default)]
    pub mtime: i64,
}

/// Deck serializes some empty collections as `null` (e.g. a stack's
/// `cards` once its only card is moved away — seen on the dockerized
/// tier); decode those as empty vectors instead of failing the payload.
fn null_to_empty_vec<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Ok(Option::<Vec<T>>::deserialize(deserializer)?.unwrap_or_default())
}

/// Serializes an optional UTC datetime as Deck's ISO-8601 shape
/// (`2020-01-20T09:52:43+00:00`), not chrono's serde default (`...Z`).
#[allow(clippy::ref_option)] // serde's serialize_with signature is fixed
pub(crate) fn ser_opt_datetime<S: serde::Serializer>(
    value: &Option<DateTime<Utc>>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    match value {
        Some(dt) => {
            serializer.serialize_str(&dt.to_rfc3339_opts(chrono::SecondsFormat::Secs, false))
        }
        None => serializer.serialize_none(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Datelike;

    fn fixture(name: &str) -> Vec<u8> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/deck")
            .join(name);
        std::fs::read(path).expect("fixture file must exist")
    }

    #[test]
    fn boards_list_fixture_decodes() {
        let boards: Vec<Board> =
            crate::client::decode_envelope(&fixture("boards_list.json")).unwrap();
        assert_eq!(boards.len(), 2);
        assert_eq!(boards[0].title, "taskboard-sync");
        assert_eq!(
            boards[0].owner.as_ref().unwrap().primary_key,
            "taskboard-it"
        );
        assert_eq!(boards[1].id, 7);
        assert_eq!(boards[1].labels[0].title, "someday");
        assert_eq!(boards[1].labels[0].color.as_str(), "00c2e0");
        assert!(boards.iter().all(Board::is_live));
    }

    #[test]
    fn board_create_fixture_decodes_with_unknown_fields() {
        let board: Board =
            crate::client::decode_envelope(&fixture("board_create_response.json")).unwrap();
        assert_eq!(board.id, 42);
        assert_eq!(board.color.as_str(), "ff00ff");
    }

    #[test]
    fn stacks_list_fixture_decodes() {
        let stacks: Vec<Stack> =
            crate::client::decode_envelope(&fixture("stacks_list.json")).unwrap();
        assert_eq!(stacks.len(), 2);
        assert_eq!(stacks[0].title, "To do");
        assert_eq!(stacks[0].board_id, 42);
        assert_eq!(stacks[0].order, 0);
        assert_eq!(stacks[1].cards.len(), 1);
    }

    #[test]
    fn stack_detail_fixture_decodes_with_nested_cards() {
        let stack: Stack = crate::client::decode_envelope(&fixture("stack_detail.json")).unwrap();
        assert_eq!(stack.id, 9);
        assert_eq!(stack.cards.len(), 2);
        let archived = stack.cards.iter().find(|c| c.archived).unwrap();
        assert_eq!(archived.title, "kiosk layout");
        assert!(archived.duedate.is_none());
    }

    #[test]
    fn card_detail_fixture_decodes_due_and_done() {
        let card: Card = crate::client::decode_envelope(&fixture("card_detail.json")).unwrap();
        assert_eq!(card.id, 5);
        assert_eq!(card.kind, "plain");
        assert_eq!(
            card.duedate.unwrap().to_rfc3339(),
            "2020-01-20T09:52:43+00:00"
        );
        assert_eq!(card.done.unwrap().year(), 2026);
        assert_eq!(card.labels, vec![1, 2]);
        assert_eq!(card.attachment_count, 1);
        assert_eq!(card.comments_unread, 3);
        assert!(card.overdue != 0);
        assert_eq!(
            card.attachments[0].data.as_ref().unwrap().file_name,
            "spec.org"
        );
    }

    #[test]
    fn card_due_null_decodes_to_none() {
        let card: Card = crate::client::decode_envelope(&fixture("card_due_null.json")).unwrap();
        assert!(card.duedate.is_none());
        assert!(card.done.is_none());
    }

    #[test]
    fn labels_list_fixture_decodes() {
        let labels: Vec<Label> =
            crate::client::decode_envelope(&fixture("labels_list.json")).unwrap();
        assert_eq!(labels.len(), 2);
        assert_eq!(labels[0].title, "urgent");
        // Lenient read path: server strings pass through verbatim.
        assert_eq!(labels[1].color.as_str(), "00C2E0");
        assert_eq!(labels[0].board_id, 42);
    }

    #[test]
    fn attachments_list_fixture_decodes() {
        let attachments: Vec<Attachment> =
            crate::client::decode_envelope(&fixture("attachments_list.json")).unwrap();
        assert_eq!(attachments.len(), 1);
        assert_eq!(attachments[0].kind, "deck_file");
        assert_eq!(attachments[0].data.as_ref().unwrap().size, 1234);
        assert_eq!(
            attachments[0].extended_data.as_ref().unwrap().mimetype,
            "text/x-org"
        );
    }

    #[test]
    fn malformed_fixture_is_rejected() {
        let res: Result<Board, _> = crate::client::decode_envelope(&fixture("envelope_error.json"));
        assert!(res.is_err(), "truncated envelope must not decode");
    }

    #[test]
    fn explicit_null_collections_decode_as_empty() {
        // Seen on the dockerized tier: a stack whose only card was moved
        // away lists with "cards": null, and a fresh board can carry
        // "labels": null. One null must not fail the whole payload.
        let stack: Stack =
            serde_json::from_str(r#"{"id": 9, "title": "Doing", "boardId": 42, "cards": null}"#)
                .unwrap();
        assert!(stack.cards.is_empty());
        let board: Board = serde_json::from_str(
            r#"{"id": 3, "title": "t", "color": "5c2751", "labels": null, "users": null, "acl": null}"#,
        )
        .unwrap();
        assert!(board.labels.is_empty() && board.users.is_empty() && board.acl.is_empty());
        let card: Card = serde_json::from_str(
            r#"{"id": 5, "title": "c", "labels": null, "assignedUsers": null, "attachments": null}"#,
        )
        .unwrap();
        assert!(
            card.labels.is_empty() && card.assigned_users.is_empty() && card.attachments.is_empty()
        );
    }

    #[test]
    fn stack_filter_paths() {
        assert_eq!(StackFilter::Active.path_segment(), "stacks");
        assert_eq!(StackFilter::Archived.path_segment(), "stacks/archived");
    }
}
