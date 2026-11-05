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

/// The tagged fingerprint string, e.g. `"fnv1a64:9e3779b97f4a7c15"`.
/// The algorithm tag is the recorded upgrade path: a future scheme
/// mints a new prefix, and differently-tagged hashes can never compare
/// equal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateHash(String);

#[cfg(test)]
impl StateHash {
    pub(crate) fn new(raw: String) -> Self {
        Self(raw)
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
