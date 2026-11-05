// SPDX-License-Identifier: MIT OR Apache-2.0
//! The per-device heartbeat payload (`info.json`, design D-SF14's type
//! half): refreshed by the transport on every flush, consumed for peer
//! freshness and the quiescence divergence finding.
//!
//! Exactly the design's field list — no `device_id` field (the subtree
//! path owns identity) and none of the backbone's `instance_id`/
//! `last_seq` machinery (D-SF4's fork alarm replaced it). Decode
//! posture matches the identity payloads: strict envelope, extras
//! ignored (never re-serialized by us).

use serde::{Deserialize, Serialize};

use crate::hash::StateHash;

/// The `info.json` payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeviceInfo {
    /// The publishing binary's version, free-form.
    pub app_version: String,
    /// The document format the device currently speaks.
    pub format_version: u32,
    /// The newest entry clock the device has folded.
    pub last_clock: chrono::DateTime<chrono::Utc>,
    /// The fingerprint of the device's published document
    /// ([`crate::hash`] mints it) — an opaque equality token.
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
            "last_clock": "2026-11-05T09:15:32Z",
            "state_hash": "fnv1a64:9e3779b97f4a7c15",
            "written_at": "2026-11-05T09:16:00Z"
        })
    }

    #[test]
    fn device_info_decodes_the_exact_wire_shape() {
        let info: DeviceInfo = serde_json::from_value(raw()).expect("decodes");
        assert_eq!(info.app_version, "0.2.0");
        assert_eq!(info.format_version, 1);
        assert_eq!(info.state_hash.as_ref(), "fnv1a64:9e3779b97f4a7c15");
    }

    #[test]
    fn device_info_tolerates_unknown_extra_keys() {
        let mut value = raw();
        value["instance_id"] = json!("from-a-future-peer");
        let info: DeviceInfo = serde_json::from_value(value).expect("decodes");
        assert_eq!(info.format_version, 1);
    }

    #[test]
    fn device_info_envelope_is_strict() {
        for drop_key in ["app_version", "last_clock", "state_hash", "written_at"] {
            let mut value = raw();
            value.as_object_mut().expect("object").remove(drop_key);
            assert!(
                serde_json::from_value::<DeviceInfo>(value).is_err(),
                "missing {drop_key} must be a typed failure"
            );
        }
    }

    #[test]
    fn device_info_state_hash_decodes_opaquely() {
        let mut value = raw();
        value["state_hash"] = json!("sha512-of-some-future-scheme:cafe");
        let info: DeviceInfo = serde_json::from_value(value).expect("decodes");
        assert_eq!(
            info.state_hash.as_ref(),
            "sha512-of-some-future-scheme:cafe"
        );
    }
}
