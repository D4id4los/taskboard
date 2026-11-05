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
    /// # Panics
    /// Never: an in-memory document and canonical JSON always
    /// serialize; the expects are total-function pins.
    ///
    /// (The doc comment above is the contract line; the expects in the
    /// body cannot fire for any constructible document.)
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
    let mut hash: u64 = 0xcbf2_9ce6_8447_3675;
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
