// SPDX-License-Identifier: MIT OR Apache-2.0
//! Validated Deck color type.
//!
//! Deck colors are six hex digits (no `#`). Writes are strict
//! ([`DeckColor::from_hex`]); reads are lenient so one malformed server
//! color cannot fail decoding a whole listing.

use std::fmt;

use serde::de::{self, Deserializer, Visitor};
use serde::ser::Serializer;
use serde::{Deserialize, Serialize};

/// A Deck color as six lowercase hex digits, constructed without `#`.
///
/// Construct with [`DeckColor::from_hex`] or [`FromStr`]; `Display`
/// round-trips the normalized (lowercase) form.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DeckColor(String);

/// A color string that is not six hex digits (a leading `#` is also
/// rejected; Deck expects the bare value).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("color must be six hex digits without '#'")]
pub struct ParseColorError;

impl DeckColor {
    /// Validates a six-digit hex color (`#` rejected, case-insensitive,
    /// stored lowercase).
    ///
    /// # Errors
    ///
    /// Returns [`ParseColorError`] for anything but exactly six hex digits.
    pub fn from_hex(input: &str) -> Result<Self, ParseColorError> {
        Self::validate(input).map(Self)
    }

    /// Wraps a color string read from the server without validation.
    ///
    /// Deserialization is lenient by design: an unexpected server color must
    /// surface as a pass-through value, not a decode failure of the whole
    /// payload. Only ever produced by reading; [`Display`] re-emits the
    /// string verbatim.
    #[must_use]
    pub(crate) fn from_server(input: String) -> Self {
        Self(input)
    }

    fn validate(input: &str) -> Result<String, ParseColorError> {
        if input.len() == 6
            && input.bytes().all(|b| b.is_ascii_hexdigit())
            && !input.starts_with('#')
        {
            Ok(input.to_ascii_lowercase())
        } else {
            Err(ParseColorError)
        }
    }

    /// The normalized (lowercase) six-hex-digit value.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for DeckColor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::str::FromStr for DeckColor {
    type Err = ParseColorError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_hex(s)
    }
}

impl Serialize for DeckColor {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for DeckColor {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ColorVisitor;

        impl Visitor<'_> for ColorVisitor {
            type Value = DeckColor;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a color string")
            }

            fn visit_str<E: de::Error>(self, v: &str) -> Result<DeckColor, E> {
                Ok(DeckColor::from_server(v.to_owned()))
            }
        }
        deserializer.deserialize_str(ColorVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn valid_hex_is_normalized_to_lowercase() {
        let c = DeckColor::from_hex("00C2E0").unwrap();
        assert_eq!(c.as_str(), "00c2e0");
        assert_eq!(c.to_string(), "00c2e0");
    }

    #[test]
    fn invalid_classes_are_rejected() {
        for bad in [
            "#00c2e0", "00c2e", "00c2e00", "00c2eg", "00 c2e", "", "0000", "zzzzzz",
        ] {
            assert_eq!(
                DeckColor::from_hex(bad),
                Err(ParseColorError),
                "{bad:?} must be rejected"
            );
        }
    }

    #[test]
    fn from_str_matches_from_hex() {
        assert_eq!("ff00ff".parse::<DeckColor>(), DeckColor::from_hex("ff00ff"));
        assert!("#ff00ff".parse::<DeckColor>().is_err());
    }

    #[test]
    fn display_round_trips() {
        let c = DeckColor::from_hex("ABCDEF").unwrap();
        let rt = c.to_string().parse::<DeckColor>().unwrap();
        assert_eq!(c, rt);
    }

    #[test]
    fn deserialization_is_lenient_and_passes_through() {
        let c: DeckColor = serde_json::from_str("\"not-a-color\"").unwrap();
        assert_eq!(c.as_str(), "not-a-color");
        let c: DeckColor = serde_json::from_str("\"#FF0000\"").unwrap();
        assert_eq!(c.as_str(), "#FF0000");
    }

    #[test]
    fn serialization_emits_normalized_string() {
        let json = serde_json::to_string(&DeckColor::from_hex("00C2E0").unwrap()).unwrap();
        assert_eq!(json, "\"00c2e0\"");
    }

    proptest! {
        #[test]
        fn valid_inputs_round_trip(lower in "[0-9a-f]{6}", upper in "[0-9A-F]{6}") {
            let c = DeckColor::from_hex(&lower).unwrap();
            prop_assert_eq!(c.as_str(), lower);
            let c = DeckColor::from_hex(&upper).unwrap();
            prop_assert_eq!(c.as_str(), upper.to_ascii_lowercase());
        }
    }
}
