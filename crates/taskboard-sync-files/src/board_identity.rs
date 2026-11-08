// SPDX-License-Identifier: MIT OR Apache-2.0
//! The board subtree's create-once identity payload
//! (`boards/<board>/identity.json`, design D-BF6): attached at
//! board-subtree adoption, mirroring the root folder's identity
//! grammar one level down.
//!
//! Decode posture is the identity payloads' own: **strict envelope,
//! extras ignored** — these payloads are never re-serialized by us, so
//! there is nothing to relay. `board_id` mirrors the directory
//! component; a mismatch between the two is the *reader's* typed
//! finding (chunk 5's lane), never a decode failure — the codec stays
//! total over any well-formed envelope.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The `boards/<board>/identity.json` payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BoardIdentity {
    /// The board's identity — the directory component it was attached
    /// under (only ever echoed, never turned into a path here).
    pub board_id: Uuid,
    /// The tree format this board subtree speaks (shared constant with
    /// the root identity).
    pub format_version: u32,
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Set when this subtree replaces a Deck board; such a subtree is
    /// never discovered as a separate board.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub moved_from: Option<crate::board_move::MovedFrom>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{BOARDS_DIR_NAME, TREE_FORMAT_VERSION, board_identity_path};
    use serde_json::json;
    use std::str::FromStr;

    const BOARD_ID: &str = "0193b400-0000-7c89-ab01-2345abcdef01";

    fn raw() -> serde_json::Value {
        json!({
            "board_id": BOARD_ID,
            "format_version": TREE_FORMAT_VERSION,
            "created_at": "2026-11-08T09:15:32Z"
        })
    }

    #[test]
    fn board_identity_decodes_the_exact_wire_shape() {
        let identity: BoardIdentity = serde_json::from_value(raw()).expect("decodes");
        assert_eq!(identity.board_id, Uuid::from_str(BOARD_ID).expect("uuid"));
        assert_eq!(identity.format_version, TREE_FORMAT_VERSION);
    }

    #[test]
    fn board_identity_tolerates_unknown_extra_keys() {
        let mut value = raw();
        value["instance_id"] = json!("from-a-future-peer");
        let identity: BoardIdentity = serde_json::from_value(value).expect("decodes");
        assert_eq!(identity.format_version, TREE_FORMAT_VERSION);
    }

    #[test]
    fn board_identity_envelope_is_strict() {
        for drop_key in ["board_id", "format_version", "created_at"] {
            let mut value = raw();
            value.as_object_mut().expect("object").remove(drop_key);
            assert!(
                serde_json::from_value::<BoardIdentity>(value).is_err(),
                "missing {drop_key} must be a typed failure"
            );
        }
        assert!(serde_json::from_value::<BoardIdentity>(json!(42)).is_err());
        let wrong_typed = json!({
            "board_id": BOARD_ID,
            "format_version": "1",
            "created_at": "2026-11-08T09:15:32Z"
        });
        assert!(serde_json::from_value::<BoardIdentity>(wrong_typed).is_err());
    }

    #[test]
    fn board_identity_roundtrips_over_the_serde_form() {
        let identity = BoardIdentity {
            board_id: Uuid::from_str(BOARD_ID).expect("uuid"),
            format_version: TREE_FORMAT_VERSION,
            created_at: "2026-11-08T09:15:32Z".parse().expect("rfc3339"),
            moved_from: None,
        };
        let back: BoardIdentity =
            serde_json::from_value(serde_json::to_value(&identity).expect("encodes"))
                .expect("decodes");
        assert_eq!(back, identity);
    }

    #[test]
    fn the_builder_path_carries_the_board_id() {
        let board =
            taskboard_domain::ids::BoardId::from_uuid(Uuid::from_str(BOARD_ID).expect("uuid"));
        assert!(
            board_identity_path(board).starts_with(&format!("{BOARDS_DIR_NAME}/{BOARD_ID}/")),
            "the path builder emits the lowercase hyphenated board component"
        );
    }
}
