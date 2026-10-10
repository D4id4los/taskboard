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
    #[serde(default, deserialize_with = "null_to_zero")]
    pub deleted_at: i64,
    /// Unix timestamp of the last modification.
    #[serde(default, deserialize_with = "null_to_zero")]
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
    #[serde(default, deserialize_with = "null_to_zero")]
    pub deleted_at: i64,
    #[serde(default)]
    pub order: i64,
    /// Unix timestamp of the last modification.
    #[serde(default, deserialize_with = "null_to_zero")]
    pub last_modified: i64,
    #[serde(default)]
    pub archived: bool,
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
    pub labels: Vec<CardLabel>,
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

/// A label reference on a card. Deck versions disagree on the wire shape:
/// card responses inline the full label object, others carry bare ids (and
/// writes take ids) — only the id is consumed and re-emitted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardLabel {
    pub id: u64,
}

impl From<u64> for CardLabel {
    fn from(id: u64) -> Self {
        Self { id }
    }
}

impl serde::Serialize for CardLabel {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u64(self.id)
    }
}

impl<'de> Deserialize<'de> for CardLabel {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct LabelRefVisitor;

        impl<'de> serde::de::Visitor<'de> for LabelRefVisitor {
            type Value = CardLabel;

            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("a label id or label object")
            }

            fn visit_u64<E: serde::de::Error>(self, id: u64) -> Result<CardLabel, E> {
                Ok(CardLabel { id })
            }

            fn visit_map<A>(self, mut map: A) -> Result<CardLabel, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                let mut id = None;
                while let Some(key) = map.next_key::<String>()? {
                    if key == "id" {
                        id = Some(map.next_value::<u64>()?);
                    } else {
                        let _ = map.next_value::<serde::de::IgnoredAny>()?;
                    }
                }
                id.map(|id| CardLabel { id })
                    .ok_or_else(|| serde::de::Error::missing_field("id"))
            }
        }
        deserializer.deserialize_any(LabelRefVisitor)
    }
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
    /// Unix timestamp of the last modification; the live tier's default
    /// board labels carry an explicit `null` (no stamp).
    #[serde(default, deserialize_with = "null_to_zero")]
    pub last_modified: i64,
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
/// depending on the endpoint that embedded it. Some responses carry a bare
/// user id string instead of an object — that decodes as the id in all
/// three fields.
#[derive(Debug, Clone)]
pub struct Participant {
    pub primary_key: String,
    pub uid: String,
    pub displayname: String,
}

// Deck's card PUT takes the owner/participant as the bare user id string
// (`update(string $title, $type, string $owner, ...)`, Deck 1.17.5), while
// reads return either a string or a full object — so writes re-emit the id.
impl serde::Serialize for Participant {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if !self.primary_key.is_empty() {
            serializer.serialize_str(&self.primary_key)
        } else if !self.uid.is_empty() {
            serializer.serialize_str(&self.uid)
        } else {
            serializer.serialize_str(&self.displayname)
        }
    }
}

impl<'de> Deserialize<'de> for Participant {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw: serde_json::Value = serde_json::Value::deserialize(deserializer)?;
        match raw {
            serde_json::Value::String(id) => Ok(Self {
                primary_key: id.clone(),
                uid: id.clone(),
                displayname: id,
            }),
            serde_json::Value::Object(_) => Ok(Self {
                primary_key: raw["primaryKey"].as_str().unwrap_or_default().to_owned(),
                uid: raw["uid"].as_str().unwrap_or_default().to_owned(),
                displayname: raw["displayname"].as_str().unwrap_or_default().to_owned(),
            }),
            other => {
                let unexpected = match other {
                    serde_json::Value::Null => serde::de::Unexpected::Unit,
                    serde_json::Value::Bool(b) => serde::de::Unexpected::Bool(b),
                    serde_json::Value::Number(n) => {
                        serde::de::Unexpected::Unsigned(n.as_u64().unwrap_or_default())
                    }
                    serde_json::Value::Array(_) => serde::de::Unexpected::Seq,
                    _ => serde::de::Unexpected::Map,
                };
                Err(serde::de::Error::invalid_type(
                    unexpected,
                    &"a participant id string or object",
                ))
            }
        }
    }
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

/// Some servers send timestamp fields as an explicit `null` where the
/// dockerized tier sends `0` or omits them (the live tier's default board
/// labels carry `"lastModified": null`); decode those as `0` — the
/// documented no-stamp sentinel the read mapping treats as "loses every
/// LWW comparison".
fn null_to_zero<'de, D>(deserializer: D) -> Result<i64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Option::<i64>::deserialize(deserializer)?.unwrap_or_default())
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
    fn explicit_null_timestamps_decode_as_the_zero_sentinel() {
        // The live tier's freshly created boards carry default labels with
        // `"lastModified": null` (tier-3 observed 2026-10-10); `#[serde(
        // default)]` alone rejects an explicit null, which failed every
        // board create decode there.
        let board: Board = serde_json::from_str(
            r#"{"id": 1, "title": "b", "color": "00c2e0",
                "lastModified": null, "deletedAt": null,
                "labels": [{"id": 2, "title": "Finished", "color": "31CC7C",
                            "boardId": 1, "cardId": null, "lastModified": null}]}"#,
        )
        .unwrap();
        assert_eq!(board.last_modified, 0);
        assert_eq!(board.deleted_at, 0);
        assert!(board.is_live());
        assert_eq!(board.labels[0].last_modified, 0);

        let stack: Stack = serde_json::from_str(
            r#"{"id": 3, "title": "s", "boardId": 1, "lastModified": null,
                "deletedAt": null}"#,
        )
        .unwrap();
        assert_eq!(stack.last_modified, 0);
        assert_eq!(stack.deleted_at, 0);
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
        assert_eq!(card.labels, vec![CardLabel::from(1), CardLabel::from(2)]);
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
    fn card_labels_decode_from_ids_and_objects() {
        // Some Deck versions inline full label objects on cards, others
        // carry bare ids; both must decode to the label id.
        let card: Card = serde_json::from_str(
            r#"{"id": 5, "title": "c", "labels": [7, {"id": 9, "title": "x", "color": "ff0000"}]}"#,
        )
        .unwrap();
        assert_eq!(card.labels, vec![CardLabel::from(7), CardLabel::from(9)]);
        // Writes re-emit the ids only.
        let json = serde_json::to_string(&card.labels).unwrap();
        assert_eq!(json, "[7,9]");
    }

    #[test]
    fn participant_decodes_from_id_string_or_object() {
        let p: Participant = serde_json::from_str(r#""taskboard-it""#).unwrap();
        assert_eq!(p.primary_key, "taskboard-it");
        let p: Participant =
            serde_json::from_str(r#"{"primaryKey": "u", "uid": "u", "displayname": "U"}"#).unwrap();
        assert_eq!(p.displayname, "U");
        // Writes re-emit the user id string (Deck's card PUT signature).
        assert_eq!(serde_json::to_string(&p).unwrap(), r#""u""#);
    }

    #[test]
    fn card_label_decodes_bare_id() {
        let l: CardLabel = serde_json::from_str("7").unwrap();
        assert_eq!(l, CardLabel::from(7));
    }

    #[test]
    fn participant_serializes_as_first_non_empty_id_field() {
        let full = Participant {
            primary_key: "pk".into(),
            uid: "uid".into(),
            displayname: "dn".into(),
        };
        assert_eq!(serde_json::to_string(&full).unwrap(), r#""pk""#);
        assert_eq!(
            serde_json::to_string(&Participant {
                primary_key: String::new(),
                uid: "uid".into(),
                displayname: "dn".into(),
            })
            .unwrap(),
            r#""uid""#
        );
        assert_eq!(
            serde_json::to_string(&Participant {
                primary_key: String::new(),
                uid: String::new(),
                displayname: "dn".into(),
            })
            .unwrap(),
            r#""dn""#
        );
    }

    #[test]
    fn participant_rejects_other_json_shapes() {
        for bad in ["null", "true", "5", r#"["u"]"#] {
            assert!(
                serde_json::from_str::<Participant>(bad).is_err(),
                "{bad} must not decode as a participant"
            );
        }
    }

    #[test]
    fn explicit_null_collections_decode_as_empty() {
        // Seen on the dockerized tier: a stack whose only card was moved
        // away lists with "cards": null, and a fresh board can carry
        // "labels": null. One null must not fail the whole payload.
        let stack: Stack =
            serde_json::from_str(r#"{"id": 9, "title": "Doing", "boardId": 42, "cards": null}"#)
                .unwrap();
        // Not `assert!(stack.cards.is_empty())`: CI's newer stable clippy
        // denies that shape (clippy::assert_is_empty); the length check is
        // accepted by both toolchains.
        assert_eq!(stack.cards.len(), 0, "null cards decode as empty");
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
