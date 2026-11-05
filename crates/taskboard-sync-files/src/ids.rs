// SPDX-License-Identifier: MIT OR Apache-2.0
//! Device and author identifiers (the house A1 pattern: a private-field
//! newtype with a fallible minting constructor at the trust boundary).
//!
//! [`DeviceId`] owns the **path-safe form**: directory names in the
//! substrate tree are derived exclusively through its `Display`
//! (lowercase hyphenated UUID), which is why it is a newtype over
//! [`Uuid`] rather than a bare one. [`AuthorId`] is the fold's Q2 wire
//! slot — present in the type system and in the wire row, but minted
//! only by the future author-tagging wave; v1 never constructs one.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use uuid::Uuid;

/// One device's identity in the substrate tree: the name of its
/// `devices/` subtree, minted once per installation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DeviceId(Uuid);

impl DeviceId {
    /// The trust-boundary mint: strict UUID parsing, `None` on any
    /// non-UUID input (an unparsable device directory name classifies
    /// as `DeviceFileKind::Other` — relay-never-choke).
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        Uuid::parse_str(raw).ok().map(Self)
    }

    /// The single creation site for a *new* device identity — the
    /// crate's only nondeterminism, confined to identity minting; the
    /// codecs and the fold never call it, so every fold law stays
    /// deterministic.
    #[must_use]
    pub fn mint() -> Self {
        Self(Uuid::now_v7())
    }

    #[must_use]
    pub const fn as_uuid(&self) -> Uuid {
        self.0
    }
}

impl fmt::Display for DeviceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // `Uuid`'s `Display` is already the lowercase hyphenated
        // (filesystem-safe) form.
        self.0.fmt(f)
    }
}

impl Serialize for DeviceId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for DeviceId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Self::parse(&raw).ok_or_else(|| serde::de::Error::custom("a device id must be a UUID"))
    }
}

/// The fold's author slot (Q2's wire seam). Every v1 row carries
/// `None`; the future author-tagging wave mints these. Same A1 pattern
/// as [`DeviceId`] minus `mint` — v1 has no author minting, and its
/// absence is the pin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AuthorId(Uuid);

impl AuthorId {
    /// Strict UUID parsing — the only way an `AuthorId` exists. A wire
    /// row whose `author` value fails this parse demotes the **whole
    /// row** to `Verbatim` (never drops the author): silently
    /// de-authoring a row would change its fold rank against the
    /// author-carrying peer, so the relay fails closed.
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        Uuid::parse_str(raw).ok().map(Self)
    }

    #[must_use]
    pub const fn as_uuid(&self) -> Uuid {
        self.0
    }
}

impl fmt::Display for AuthorId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl Serialize for AuthorId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for AuthorId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Self::parse(&raw).ok_or_else(|| serde::de::Error::custom("an author id must be a UUID"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn device_id_display_form_is_lowercase_hyphenated_and_path_safe() {
        let id = DeviceId::parse("0193A1B2-C3D4-7CC9-8A2E-5F1E2D3C4B5A").expect("parses");
        let text = id.to_string();
        assert_eq!(text.len(), 36);
        assert!(
            text.chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase() || c == '-')
        );
        assert!(!text.contains('/'));
    }

    #[test]
    fn device_id_parse_rejects_non_uuids() {
        assert!(DeviceId::parse("").is_none());
        assert!(DeviceId::parse("not-a-uuid").is_none());
        assert!(DeviceId::parse("0193a1b2c3d47cc98a2e5f1e2d3c4b5a").is_some());
    }

    #[test]
    fn device_id_display_parse_roundtrips() {
        let minted = DeviceId::mint();
        assert_eq!(DeviceId::parse(&minted.to_string()), Some(minted));
    }

    #[test]
    fn device_id_serde_roundtrips_over_the_display_form() {
        let id = DeviceId::mint();
        let raw = serde_json::to_value(id).expect("serializes");
        assert_eq!(raw, json!(id.to_string()));
        assert_eq!(
            serde_json::from_value::<DeviceId>(raw).expect("decodes"),
            id
        );
    }

    #[test]
    fn device_id_serde_rejects_non_uuid_strings() {
        assert!(serde_json::from_value::<DeviceId>(json!("not-a-uuid")).is_err());
    }

    #[test]
    fn author_id_parses_strictly_and_displays_like_device_id() {
        let text = "0193a1b2-c3d4-7cc9-8a2e-5f1e2d3c4b5a";
        let author = AuthorId::parse(text).expect("parses");
        assert_eq!(author.to_string(), text);
        assert_eq!(
            serde_json::to_value(author).expect("serializes"),
            json!(text)
        );
        assert!(AuthorId::parse("nope").is_none());
        assert!(serde_json::from_value::<AuthorId>(json!(42)).is_err());
    }
}
