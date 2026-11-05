// SPDX-License-Identifier: MIT OR Apache-2.0
//! The device state hash: an opaque equality fingerprint of a device's
//! published per-user document, carried in every `info.json` heartbeat
//! (design D-SF14). The canonicalisation and the minting function live
//! at the bottom of this module's story (D-FC7); the type itself is
//! deliberately opaque on decode so a peer running a future hash scheme
//! can never spuriously compare equal — the safe failure direction
//! (worst case a divergence finding, never a false "converged").

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

/// The tagged fingerprint string, e.g. `"fnv1a64:9e3779b97f4a7c15"`.
/// The algorithm tag is the recorded upgrade path: a future scheme
/// mints a new prefix, and differently-tagged hashes can never compare
/// equal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateHash(String);

impl StateHash {
    pub(crate) fn new(raw: String) -> Self {
        Self(raw)
    }

    /// Mints the tagged fingerprint of a document: FNV-1a 64 over the
    /// canonicalised serialisation (D-FC7). No cryptographic pretension
    /// — divergence detection needs a *stable equality fingerprint*,
    /// not strength (the folder is plaintext anyway, Q7); a collision
    /// can only mask a divergence (≈2⁻⁶⁴ per pair), never fire a false
    /// alarm (equal hashes ⇒ equal canonical bytes).
    ///
    /// # Panics
    /// Never: an in-memory document and canonical JSON always
    /// serialize; the expects are total-function pins.
    #[must_use]
    pub fn of(document: &crate::document::UserDocument) -> Self {
        let raw = serde_json::to_value(document).expect("an in-memory document always serializes");
        let bytes = serde_json::to_vec(&canonicalise(&raw)).expect("serializable");
        Self::new(format!("fnv1a64:{:016x}", fnv1a64(&bytes)))
    }
}

/// The FNV-1a 64 hash over raw bytes (the `taskboard-app` IPC
/// precedent's rationale: `DefaultHasher` has no cross-version
/// stability). Must stay byte-for-byte this algorithm forever — the
/// `fnv1a64:` tag is the upgrade path, not the evolution space.
pub(crate) fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
    }
    hash
}

/// Recursively rebuilds every JSON object through a sorted
/// `BTreeMap<String, Value>` — key-order-free serialisation. Maps in
/// the model are already `BTreeMap`s; this pass pins the raw `Value`
/// leaves (`Verbatim` rows, unknown sections) and immunises the hash
/// against any future `serde_json/preserve_order` feature leak (with
/// that feature on, `Map` becomes insertion-ordered and naive
/// `to_string` would make two devices that parsed
/// differently-ordered files compute different hashes — false
/// divergence alarms).
pub(crate) fn canonicalise(value: &Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            // The std `BTreeMap` detour is the point: it sorts
            // regardless of how `serde_json::Map` is backed.
            map.iter()
                .map(|(key, inner)| (key.clone(), canonicalise(inner)))
                .collect::<std::collections::BTreeMap<_, _>>()
                .into_iter()
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(canonicalise).collect()),
        other => other.clone(),
    }
}

impl fmt::Display for StateHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for StateHash {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl Serialize for StateHash {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for StateHash {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // Opaque: any non-empty string, no prefix validation. The
        // empty string carries no information and demotes to the typed
        // decode failure.
        let raw = String::deserialize(deserializer)?;
        if raw.is_empty() {
            return Err(serde::de::Error::custom(
                "a state hash must be a non-empty string",
            ));
        }
        Ok(Self(raw))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn state_hash_decodes_opaquely_across_tag_schemes() {
        let future = serde_json::from_value::<StateHash>(json!("sha256:abcd")).expect("decodes");
        assert_eq!(future.as_ref(), "sha256:abcd");
        assert_eq!(
            serde_json::to_value(&future).expect("serializes"),
            json!("sha256:abcd")
        );
    }

    #[test]
    fn state_hash_rejects_the_empty_string() {
        assert!(serde_json::from_value::<StateHash>(json!("")).is_err());
        assert!(serde_json::from_value::<StateHash>(json!(42)).is_err());
    }

    #[test]
    fn state_hash_display_roundtrips() {
        let hash = StateHash::new("fnv1a64:0123456789abcdef".to_owned());
        assert_eq!(hash.to_string(), "fnv1a64:0123456789abcdef");
        assert_eq!(hash.as_ref(), hash.to_string());
    }
}

#[cfg(test)]
mod hash_pins {
    use super::*;
    use crate::document::{DOCUMENT_FORMAT, UserDocument, UserRecord, UserSections};
    use crate::test_support::user_document_v1;
    use chrono::{DateTime, Utc};
    use proptest::prelude::*;
    use serde_json::json;
    use std::collections::BTreeMap;
    use std::str::FromStr;

    /// The FNV-1a 64 algorithm is pinned byte-for-byte by the fixed
    /// vectors (the ipc precedent's vectors) — the `fnv1a64:` tag
    /// promises this exact function forever.
    #[test]
    fn fnv1a64_matches_the_fixed_vectors() {
        assert_eq!(fnv1a64(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a64(b"a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fnv1a64(b"foobar"), 0x8594_4171_f739_67e8);
    }

    /// The mint is a tagged string: `fnv1a64:` + 32 lowercase hex —
    /// the structural pin on the upgrade path (a future scheme mints a
    /// new prefix and can never compare equal).
    #[test]
    fn state_hash_format_is_tagged() {
        let doc = UserDocument {
            format: DOCUMENT_FORMAT,
            sections: UserSections::default(),
            unknown: BTreeMap::new(),
        };
        let hash = StateHash::of(&doc);
        let text = hash.to_string();
        let (tag, hex) = text.split_once(':').expect("tagged");
        assert_eq!(tag, "fnv1a64");
        assert_eq!(hex.len(), 16);
        assert!(
            hex.bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        );
    }

    /// Same-key-different-order wire bytes produce equal hashes — the
    /// `preserve_order` immunisation expressed as behavior at the raw
    /// `Value` leaves (unknown sections/verbatim rows).
    #[test]
    fn hash_is_stable_across_wire_key_order() {
        let forward: UserDocument = serde_json::from_value(json!({
            "format": 1,
            "sections": {
                "journal": {
                    "0193b4c5-d6e7-7c89-ab01-2345abcdef01": {
                        "clock": "2026-11-05T09:15:32Z",
                        "extra": { "z": 1, "a": 2 }
                    }
                }
            }
        }))
        .expect("decodes");
        // The same document re-encoded with the unknown section's raw
        // object keys in the opposite order on the wire.
        let backward: UserDocument = serde_json::from_str(
            r#"{"sections":{"journal":{"0193b4c5-d6e7-7c89-ab01-2345abcdef01":{
                "extra":{"a":2,"z":1},"clock":"2026-11-05T09:15:32Z"}}},"format":1}"#,
        )
        .expect("decodes");
        assert_eq!(StateHash::of(&forward), StateHash::of(&backward));
    }

    /// The hash survives the codec: the published bytes decode back to
    /// an identically-fingerprinted model.
    #[test]
    fn hash_of_decoded_roundtrip_matches() {
        proptest!(|(doc in user_document_v1())| {
            let encoded = serde_json::to_value(&doc).expect("encodes");
            let decoded: UserDocument = serde_json::from_value(encoded).expect("re-decodes");
            prop_assert_eq!(StateHash::of(&decoded), StateHash::of(&doc));
        });
    }

    /// The *useful* direction: changing one row's body changes the
    /// hash. Collision-freeness is never asserted (no quality
    /// ceilings); one deterministic body change per case.
    #[test]
    fn unequal_documents_hash_unequally() {
        proptest!(|(mut doc in user_document_v1())| {
            let baseline = StateHash::of(&doc);
            let key = "0193b4c5-d6e7-7c89-ab01-2345abcdef01";
            doc.sections.journal.get_or_insert_with(BTreeMap::new).insert(
                key.to_owned(),
                UserRecord::Entry {
                    body: BTreeMap::from([(
                        "mutant".to_owned(),
                        json!({"marker": true}),
                    )]),
                    clock: DateTime::<Utc>::from_str("2026-11-05T09:15:32Z")
                        .expect("clock"),
                    author: None,
                },
            );
            prop_assert_ne!(StateHash::of(&doc), baseline);
        });
    }

    /// The D-SF5(b) seam for the later chunks: the fold's output hash
    /// is the merged view's hash — computed here over a fixed set with
    /// the permutation-invariance the commutativity law already
    /// proptests.
    #[test]
    fn fold_output_hash_is_the_merged_view_hash() {
        let key = "0193b4c5-d6e7-7c89-ab01-2345abcdef01";
        let c = DateTime::<Utc>::from_str("2026-11-05T09:15:32Z").expect("clock");
        let row = UserRecord::Entry {
            body: BTreeMap::from([("text".to_owned(), json!("hello"))]),
            clock: c,
            author: None,
        };
        let a = UserDocument {
            format: DOCUMENT_FORMAT,
            sections: UserSections {
                journal: Some(BTreeMap::from([(key.to_owned(), row)])),
                unknown: BTreeMap::new(),
            },
            unknown: BTreeMap::new(),
        };
        let merged_direct = crate::fold::fold([&a]);
        let merged_via_set = crate::fold::fold([&a, &a]);
        assert_eq!(
            StateHash::of(&merged_direct),
            StateHash::of(&merged_via_set)
        );
    }
}
