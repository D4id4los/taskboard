// SPDX-License-Identifier: MIT OR Apache-2.0
//! The create-once identity payloads of the substrate tree (design
//! D-SF2/D-SF4): the folder's `identity.json` at the root and a
//! device's own `device.json` in the app data dir (the file IO is the
//! transport chunk's; the payload types are ours).
//!
//! Decode posture: **strict envelope, tolerate extras** — a missing
//! required key or a wrong-typed value is a typed decode failure (the
//! boundary that keeps two differently-versioned fleets from silently
//! misreading each other), while unknown keys are ignored because these
//! payloads are never re-serialized by us, so there is nothing to
//! relay.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::ids::DeviceId;

/// The `identity.json` payload, created once at the substrate root
/// (the transport performs the `If-None-Match: *` PUT; a losing device
/// adopts the winner).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FolderIdentity {
    /// The folder's identity — only ever echoed (never ordered, never
    /// turned into a path), so a plain `Uuid` claims no legality beyond
    /// "is a UUID".
    pub folder_id: Uuid,
    /// The tree format this folder speaks.
    pub format_version: u32,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// The `device.json` payload, created once per installation in the app
/// data dir. The device id names its `devices/` subtree; duplicating
/// this file (a cloned disk) is the fork condition the transport's
/// own-path `If-Match` guard detects.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeviceIdentity {
    pub device_id: DeviceId,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::TREE_FORMAT_VERSION;
    use serde_json::json;
    use std::str::FromStr;

    const FOLDER_ID: &str = "0193b400-0000-7c89-ab01-2345abcdef01";

    #[test]
    fn folder_identity_decodes_the_exact_wire_shape() {
        let raw = json!({
            "folder_id": FOLDER_ID,
            "format_version": TREE_FORMAT_VERSION,
            "created_at": "2026-11-05T09:15:32Z"
        });
        let identity: FolderIdentity = serde_json::from_value(raw).expect("decodes");
        assert_eq!(identity.folder_id, Uuid::from_str(FOLDER_ID).expect("uuid"));
        assert_eq!(identity.format_version, TREE_FORMAT_VERSION);
    }

    #[test]
    fn folder_identity_tolerates_unknown_extra_keys() {
        let raw = json!({
            "folder_id": FOLDER_ID,
            "format_version": 1,
            "created_at": "2026-11-05T09:15:32Z",
            "unknown_future_key": {"nested": true}
        });
        let identity: FolderIdentity = serde_json::from_value(raw).expect("decodes");
        assert_eq!(identity.format_version, 1);
    }

    #[test]
    fn folder_identity_envelope_is_strict() {
        let missing_version = json!({
            "folder_id": FOLDER_ID,
            "created_at": "2026-11-05T09:15:32Z"
        });
        assert!(serde_json::from_value::<FolderIdentity>(missing_version).is_err());
        let wrong_typed = json!({
            "folder_id": FOLDER_ID,
            "format_version": "1",
            "created_at": "2026-11-05T09:15:32Z"
        });
        assert!(serde_json::from_value::<FolderIdentity>(wrong_typed).is_err());
        assert!(serde_json::from_value::<FolderIdentity>(json!(42)).is_err());
    }

    #[test]
    fn device_identity_roundtrips_over_the_newtype_serde() {
        let identity = DeviceIdentity {
            device_id: DeviceId::mint(),
            created_at: "2026-11-05T09:15:32Z".parse().expect("rfc3339"),
        };
        let raw = serde_json::to_value(&identity).expect("serializes");
        assert_eq!(
            serde_json::from_value::<DeviceIdentity>(raw).expect("decodes"),
            identity
        );
    }

    #[test]
    fn device_identity_envelope_is_strict() {
        let raw = json!({ "created_at": "2026-11-05T09:15:32Z" });
        assert!(serde_json::from_value::<DeviceIdentity>(raw).is_err());
    }
}
