// SPDX-License-Identifier: MIT OR Apache-2.0
//! The board snapshot (`snapshot-<seq>.json`, design D-BF7): the
//! owner's converged fold up to and including its own chunk `<seq>` —
//! the late-join floor and the GC arm.
//!
//! Publishing a snapshot subsumes the device's prior chunks
//! (self-healing: the fold is idempotent) and makes them GC-eligible.
//! The entity grammar is the payload schema's, verbatim relays
//! included — snapshots subsume and heal ([`crate::board_payload`]).
//! Decode posture: strict envelope, version-blind (the consumer
//! gates); unknown envelope keys relay. Immutability-as-published
//! rides the chunk discipline: snapshots are overwritten only by a
//! *new* seq's file, never in place.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::board_payload::EntityEntry;
use crate::ids::DeviceId;

/// The snapshot's `state` object: the fold output serialized.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SnapshotState {
    /// Every modeled entity and verbatim relay the owner's fold holds.
    pub entities: Vec<EntityEntry>,
    /// Unknown `state` keys: carried verbatim (the relay rule).
    #[serde(flatten, default, skip_serializing_if = "BTreeMap::is_empty")]
    pub unknown: BTreeMap<String, Value>,
}

/// One published snapshot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BoardSnapshot {
    pub format_version: u32,
    pub device_id: DeviceId,
    /// The snapshot's watermark: everything this device has seen up to
    /// and including its own chunk `seq`. The owner's chunks `≤ seq`
    /// are subsumed (GC-eligible); late joiners floor here.
    pub seq: u64,
    pub written_at: DateTime<Utc>,
    pub state: SnapshotState,
    /// Unknown envelope keys: carried verbatim (the relay rule).
    #[serde(flatten, default, skip_serializing_if = "BTreeMap::is_empty")]
    pub unknown: BTreeMap<String, Value>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board_payload::{EntityKind, Field, FieldValue, MaterializedEntity};
    use serde_json::json;
    use uuid::Uuid;

    const DEVICE: &str = "0193a1b2-c3d4-7cc9-8a2e-5f1e2d3c4b5a";

    fn device() -> DeviceId {
        DeviceId::parse(DEVICE).expect("parses")
    }

    fn raw_snapshot() -> serde_json::Value {
        json!({
            "format_version": 1,
            "device_id": DEVICE,
            "seq": 141,
            "written_at": "2026-11-08T09:15:32Z",
            "state": { "entities": [] }
        })
    }

    fn modeled_entity() -> EntityEntry {
        EntityEntry::Modeled(MaterializedEntity {
            kind: EntityKind::Task,
            id: Uuid::from_u128(1),
            fields: BTreeMap::from([(
                "deleted".to_owned(),
                Field::Value(FieldValue {
                    value: json!(true),
                    clock: "2026-11-08T09:15:31Z".parse().expect("rfc3339"),
                    by: None,
                }),
            )]),
            memberships: None,
            unknown: BTreeMap::new(),
        })
    }

    #[test]
    fn a_snapshot_decodes_the_exact_wire_shape() {
        let snapshot: BoardSnapshot = serde_json::from_value(raw_snapshot()).expect("decodes");
        assert_eq!(snapshot.device_id, device());
        assert_eq!(snapshot.seq, 141);
        assert_eq!(snapshot.state.entities, [] as [EntityEntry; 0]);
    }

    #[test]
    fn the_envelope_is_strict() {
        for drop_key in ["format_version", "device_id", "seq", "written_at", "state"] {
            let mut value = raw_snapshot();
            value.as_object_mut().expect("object").remove(drop_key);
            assert!(
                serde_json::from_value::<BoardSnapshot>(value).is_err(),
                "missing {drop_key} must be a typed failure"
            );
        }
        let mut missing_entities = raw_snapshot();
        missing_entities["state"] = json!({});
        assert!(serde_json::from_value::<BoardSnapshot>(missing_entities).is_err());
    }

    #[test]
    fn a_snapshot_roundtrips_with_relays_included() {
        let snapshot = BoardSnapshot {
            format_version: 1,
            device_id: device(),
            seq: 16,
            written_at: "2026-11-08T09:15:32Z".parse().expect("rfc3339"),
            state: SnapshotState {
                entities: vec![
                    modeled_entity(),
                    EntityEntry::Verbatim(json!({ "future": "entity-shape" })),
                ],
                unknown: BTreeMap::new(),
            },
            unknown: BTreeMap::from([("future_envelope".to_owned(), json!({ "x": 1 }))]),
        };
        let back: BoardSnapshot =
            serde_json::from_value(serde_json::to_value(&snapshot).expect("encodes"))
                .expect("decodes");
        assert_eq!(back, snapshot);
        assert_eq!(
            back.unknown["future_envelope"],
            json!({ "x": 1 }),
            "unknown envelope keys relay through the snapshot"
        );
    }

    #[test]
    fn garbage_entities_never_kill_the_snapshot() {
        let mut value = raw_snapshot();
        value["state"]["entities"] = json!(["junk", 42]);
        let snapshot: BoardSnapshot = serde_json::from_value(value).expect("decodes");
        assert_eq!(snapshot.state.entities.len(), 2);
        assert!(matches!(
            snapshot.state.entities[0],
            EntityEntry::Verbatim(_)
        ));
    }

    /// The tombstone rides snapshots like any field (tombstones are
    /// never GC'd — the resurrection defense *is* the snapshot's
    /// carried `deleted` field).
    #[test]
    fn tombstones_ride_the_snapshot_state() {
        let snapshot = BoardSnapshot {
            format_version: 1,
            device_id: device(),
            seq: 16,
            written_at: "2026-11-08T09:15:32Z".parse().expect("rfc3339"),
            state: SnapshotState {
                entities: vec![modeled_entity()],
                unknown: BTreeMap::new(),
            },
            unknown: BTreeMap::new(),
        };
        let back: BoardSnapshot =
            serde_json::from_value(serde_json::to_value(&snapshot).expect("encodes"))
                .expect("decodes");
        let EntityEntry::Modeled(entity) = &back.state.entities[0] else {
            panic!("modeled shape");
        };
        let Field::Value(deleted) = &entity.fields["deleted"] else {
            panic!("modeled slot");
        };
        assert_eq!(deleted.value, json!(true));
    }
}
#[cfg(test)]
mod fuzz {
    use super::*;
    use crate::test_support::json_value;
    use proptest::prelude::*;
    use taskboard_domain::test_support::proptest_config;

    proptest! {
        #![proptest_config(proptest_config(256))]

        /// P8 — arbitrary JSON through the snapshot decode: typed
        /// outcome, never a panic.
        #[test]
        fn arbitrary_snapshot_values_never_panic(raw in json_value()) {
            let outcome = serde_json::from_value::<BoardSnapshot>(raw);
            let _ = outcome.is_ok();
        }
    }
}
