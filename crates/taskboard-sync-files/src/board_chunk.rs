// SPDX-License-Identifier: MIT OR Apache-2.0
//! The immutable operation chunk (`ops/<seq>.json`, design D-BF6/D-BF7):
//! a contiguous seq range of one device's materialized entity writes on
//! one board.
//!
//! **Immutability is the contract**: once published, a chunk is never
//! rewritten or renamed by any code path in this crate — the type has
//! no mutation surface and no IO exists here; the transport's
//! publish-once/If-None-Match discipline (chunk 5) owns the write.
//! Subsumption, not revision, is how a device corrects history: a
//! newer snapshot plus GC of the owner's own chunk files below its
//! watermark.
//!
//! Decode posture: **strict envelope** (all keys required, an integer
//! `format_version`, `first_seq ≤ last_seq` — a chunk that cannot name
//! its watermark is unusable for GC, a typed failure); **lenient
//! entity list** (garbage entity shapes relay per the payload rules —
//! a torn chunk that still parses its envelope folds its good
//! entities; the rest heals via the next snapshot). Version-blind
//! decode (the consumer gates, never the codec). Unknown envelope keys
//! relay (never re-serialized by chunks themselves, carried so a
//! snapshot republish never drops them). `device_id` redundancy: the
//! path owns identity (the heartbeat precedent); the field is the
//! clone-detection handle — carried here, equality-checked by the
//! reader (chunk 5's lane), never by the codec.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

use crate::board_payload::EntityEntry;
use crate::ids::DeviceId;

/// One published chunk.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BoardChunk {
    pub format_version: u32,
    pub device_id: DeviceId,
    /// The chunk's first sequence number (inclusive, gap-free per
    /// device: `prev_last + 1`).
    pub first_seq: u64,
    /// The chunk's last sequence number (inclusive).
    pub last_seq: u64,
    pub written_at: DateTime<Utc>,
    /// The materialized entity writes; leniently decoded (relays
    /// included, never a chunk-killing error).
    pub entities: Vec<EntityEntry>,
    /// Unknown envelope keys: carried verbatim (the relay rule).
    #[serde(flatten, default, skip_serializing_if = "BTreeMap::is_empty")]
    pub unknown: BTreeMap<String, Value>,
}

impl BoardChunk {
    /// The chunk's seq range as a closed interval.
    #[must_use]
    pub const fn seqs(&self) -> std::ops::RangeInclusive<u64> {
        self.first_seq..=self.last_seq
    }
}

impl<'de> Deserialize<'de> for BoardChunk {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // The derive handles the strict envelope (every field required,
        // typed); this pass adds the one envelope-level invariant.
        #[derive(Deserialize)]
        struct Raw {
            format_version: u32,
            device_id: DeviceId,
            first_seq: u64,
            last_seq: u64,
            written_at: DateTime<Utc>,
            entities: Vec<EntityEntry>,
            #[serde(flatten, default)]
            unknown: BTreeMap<String, Value>,
        }
        let raw = Raw::deserialize(deserializer)?;
        if raw.first_seq > raw.last_seq {
            return Err(serde::de::Error::custom(
                "a chunk's first_seq must not exceed its last_seq",
            ));
        }
        Ok(Self {
            format_version: raw.format_version,
            device_id: raw.device_id,
            first_seq: raw.first_seq,
            last_seq: raw.last_seq,
            written_at: raw.written_at,
            entities: raw.entities,
            unknown: raw.unknown,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board_payload::{Field, FieldValue, MaterializedEntity};
    use serde_json::json;
    use taskboard_domain::ids::AuthorId;
    use uuid::Uuid;

    const DEVICE: &str = "0193a1b2-c3d4-7cc9-8a2e-5f1e2d3c4b5a";

    fn device() -> DeviceId {
        DeviceId::parse(DEVICE).expect("parses")
    }

    fn authored_entity() -> EntityEntry {
        EntityEntry::Modeled(MaterializedEntity {
            kind: crate::board_payload::EntityKind::Task,
            id: Uuid::from_u128(1),
            fields: BTreeMap::from([(
                "title".to_owned(),
                Field::Value(FieldValue {
                    value: json!("ship"),
                    clock: "2026-11-08T09:15:31Z".parse().expect("rfc3339"),
                    by: Some(AuthorId::parse(DEVICE).expect("parses")),
                }),
            )]),
            memberships: None,
            unknown: BTreeMap::new(),
        })
    }

    fn raw_chunk() -> serde_json::Value {
        json!({
            "format_version": 1,
            "device_id": DEVICE,
            "first_seq": 129,
            "last_seq": 141,
            "written_at": "2026-11-08T09:15:32Z",
            "entities": []
        })
    }

    #[test]
    fn a_chunk_decodes_the_exact_wire_shape() {
        let chunk: BoardChunk = serde_json::from_value(raw_chunk()).expect("decodes");
        assert_eq!(chunk.device_id, device());
        assert_eq!(chunk.first_seq, 129);
        assert_eq!(chunk.last_seq, 141);
        assert!(chunk.seqs().contains(&130));
        assert!(!chunk.seqs().contains(&142));
    }

    #[test]
    fn a_chunk_roundtrips_over_entities_and_relays() {
        let chunk = BoardChunk {
            format_version: 1,
            device_id: device(),
            first_seq: 1,
            last_seq: 2,
            written_at: "2026-11-08T09:15:32Z".parse().expect("rfc3339"),
            entities: vec![
                authored_entity(),
                EntityEntry::Verbatim(json!({ "opaque": "future-peer-shape" })),
            ],
            unknown: BTreeMap::new(),
        };
        let back: BoardChunk =
            serde_json::from_value(serde_json::to_value(&chunk).expect("encodes"))
                .expect("decodes");
        assert_eq!(back, chunk);
    }

    #[test]
    fn the_envelope_is_strict() {
        for drop_key in [
            "format_version",
            "device_id",
            "first_seq",
            "last_seq",
            "written_at",
            "entities",
        ] {
            let mut value = raw_chunk();
            value.as_object_mut().expect("object").remove(drop_key);
            assert!(
                serde_json::from_value::<BoardChunk>(value).is_err(),
                "missing {drop_key} must be a typed failure"
            );
        }
    }

    /// A chunk that cannot name its watermark is unusable for GC —
    /// `first_seq > last_seq` is a typed decode failure, not data.
    #[test]
    fn an_inverted_seq_range_is_a_typed_failure() {
        let mut value = raw_chunk();
        value["first_seq"] = json!(142);
        value["last_seq"] = json!(129);
        assert!(serde_json::from_value::<BoardChunk>(value).is_err());
    }

    #[test]
    fn a_zero_seq_chunk_is_representable_and_ordered() {
        let mut value = raw_chunk();
        value["first_seq"] = json!(0);
        value["last_seq"] = json!(0);
        let chunk: BoardChunk = serde_json::from_value(value).expect("decodes");
        assert_eq!(chunk.first_seq, 0);
        assert_eq!(chunk.last_seq, 0);
    }

    #[test]
    fn garbage_entities_never_kill_the_chunk() {
        let mut value = raw_chunk();
        value["entities"] = json!([
            "not an entity",
            { "kind": "task", "id": Uuid::from_u128(1).to_string(),
              "fields": { "title": { "value": "ok", "clock": "2026-11-08T09:15:31Z" } } }
        ]);
        let chunk: BoardChunk = serde_json::from_value(value).expect("decodes");
        assert_eq!(
            chunk.entities.len(),
            2,
            "good entities fold, garbage relays"
        );
        assert!(matches!(chunk.entities[0], EntityEntry::Verbatim(_)));
        assert!(matches!(chunk.entities[1], EntityEntry::Modeled(_)));
    }
}
