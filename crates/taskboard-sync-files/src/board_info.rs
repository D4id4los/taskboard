// SPDX-License-Identifier: MIT OR Apache-2.0
//! The board-lane heartbeat payload (`boards/<board>/devices/<device>/
//! info.json`, design D-BF6): the root [`crate::heartbeat::DeviceInfo`]
//! shape **plus `last_seq`** — the per-(board, device) GC watermark and
//! late-join advertisement.
//!
//! `last_seq` exists only where chunks exist: the per-user root's
//! `DeviceInfo` stays current-state and is *not* touched — the root
//! carries no sequence numbers at all (mixed kinds across subtrees,
//! never within one). Refreshed by the transport on every chunk or
//! snapshot publish (chunk 5); the core only types it. Decode posture:
//! strict envelope, extras ignored (the backbone's `instance_id`
//! forward-compat rides the extras rule; never re-serialized by us).

use serde::{Deserialize, Serialize};

use crate::hash::StateHash;

/// The board-lane `info.json` payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BoardInfo {
    /// The publishing binary's version, free-form.
    pub app_version: String,
    /// The tree format the device currently speaks.
    pub format_version: u32,
    /// The newest chunk sequence number the device has published on
    /// this board — the GC watermark and the late-join advertisement.
    pub last_seq: u64,
    /// The newest entity-write clock the device has folded on this board.
    pub last_clock: chrono::DateTime<chrono::Utc>,
    /// The fingerprint of the device's board state
    /// ([`crate::hash::StateHash::of_board`]) — an opaque equality token.
    pub state_hash: StateHash,
    /// When this heartbeat was written.
    pub written_at: chrono::DateTime<chrono::Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn raw() -> serde_json::Value {
        json!({
            "app_version": "0.2.0",
            "format_version": 1,
            "last_seq": 141,
            "last_clock": "2026-11-08T09:15:32Z",
            "state_hash": "fnv1a64:9e3779b97f4a7c15",
            "written_at": "2026-11-08T09:16:00Z"
        })
    }

    #[test]
    fn board_info_decodes_the_exact_wire_shape() {
        let info: BoardInfo = serde_json::from_value(raw()).expect("decodes");
        assert_eq!(info.app_version, "0.2.0");
        assert_eq!(info.format_version, 1);
        assert_eq!(info.last_seq, 141);
    }

    #[test]
    fn board_info_tolerates_unknown_extra_keys() {
        let mut value = raw();
        value["instance_id"] = json!("from-the-backbone-shape");
        let info: BoardInfo = serde_json::from_value(value).expect("decodes");
        assert_eq!(info.last_seq, 141);
    }

    #[test]
    fn board_info_envelope_is_strict() {
        for drop_key in [
            "app_version",
            "format_version",
            "last_seq",
            "last_clock",
            "state_hash",
            "written_at",
        ] {
            let mut value = raw();
            value.as_object_mut().expect("object").remove(drop_key);
            assert!(
                serde_json::from_value::<BoardInfo>(value).is_err(),
                "missing {drop_key} must be a typed failure"
            );
        }
        let wrong_typed_seq = json!({
            "app_version": "0.2.0",
            "format_version": 1,
            "last_seq": "141",
            "last_clock": "2026-11-08T09:15:32Z",
            "state_hash": "fnv1a64:9e3779b97f4a7c15",
            "written_at": "2026-11-08T09:16:00Z"
        });
        assert!(serde_json::from_value::<BoardInfo>(wrong_typed_seq).is_err());
    }

    /// The root `DeviceInfo` stays current-state: its wire shape pins
    /// **no** `last_seq` (mixed kinds across subtrees, never within
    /// one — D-BF6). Pinned here so the board-lane growth can never
    /// leak into the root lane by accident.
    #[test]
    fn the_root_device_info_shape_carries_no_last_seq() {
        let root = json!({
            "app_version": "0.2.0",
            "format_version": 1,
            "last_clock": "2026-11-08T09:15:32Z",
            "state_hash": "fnv1a64:9e3779b97f4a7c15",
            "written_at": "2026-11-08T09:16:00Z"
        });
        let info: crate::heartbeat::DeviceInfo = serde_json::from_value(root).expect("decodes");
        assert_eq!(info.format_version, 1);
        // The two lanes' types differ structurally, not by a flag.
        assert!(
            serde_json::to_value(&info)
                .expect("encodes")
                .get("last_seq")
                .is_none()
        );
    }
}
