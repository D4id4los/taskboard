// SPDX-License-Identifier: MIT OR Apache-2.0
//! The per-user current-state document (`appdata.json`, design D-SF3,
//! decisions D-FC5/D-FC8): a versioned envelope of named sections, the
//! `journal` section typed as a map of [`UserRecord`] rows, everything
//! else carried verbatim.
//!
//! Decode posture — **strict envelope, lenient rows** (the
//! `AppDataDocument`/`RuleEntry` recipe from `taskboard-domain`): the
//! envelope is the boundary (`format` required, an integer, the
//! document a JSON object — anything else is a typed decode failure);
//! rows are lenient (an unmodelable shape stays [`UserRecord::Verbatim`],
//! never an error that kills a document).
//!
//! Tolerance is **structural relay, not ignoring** (the recorded
//! upgrade of the dispatch letter): a device re-publishes its whole
//! current view, so unknown envelope keys, unknown/reserved sections,
//! unknown body keys, and `Verbatim` rows are all *carried* — never
//! errorized, never silently dropped. Only the never-republished
//! identity/heartbeat payloads ignore extras. Reserved section names
//! (`settings`, `proofs`) ride `sections.unknown` like any unknown
//! section and are never emitted by this version.
//!
//! Wire row grammar (mirrors `RuleEntry`):
//! - `Entry`: an object with a decodable RFC 3339 `clock`; **every
//!   other key rides `body` preserved** (the section's own typed
//!   decode of exactly this body is a later chunk's business).
//! - `Tombstone`: `{ "deleted": true, "clock": … }`.
//! - `Verbatim`: anything else.
//! - The optional `author` wire slot (Q2) decodes when present;
//!   a *present but unparseable* author demotes the whole row to
//!   `Verbatim` (fail-closed — silently de-authoring would change the
//!   row's fold rank); `author: null` decodes as `None`. Emission
//!   writes the `author` key only when `Some` (v1 never emits it).
//!
//! Map keys are plain strings: this version emits UUIDs but decode
//! tolerates any key (a hand-edited non-UUID key relays through fold
//! untouched — deliberate leniency). Modeled names
//! (`format`, `sections`, `journal`, `clock`, `author`) win on the
//! wire, so an unknown namespace cannot collide with them; the
//! document generators in `test_support` respect this.
//!
//! Codecs are version-blind (D-FC8): any `format` value decodes; the
//! consumer gates on it, never the codec.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

use crate::ids::AuthorId;

/// The document format this version writes.
pub const DOCUMENT_FORMAT: u32 = 1;

/// The v1 passenger section's wire name (the domain vocabulary stays
/// `reflection`/`ReflectionEntry` — `journal` is taken by the undo
/// journal).
pub const JOURNAL_SECTION: &str = "journal";

/// Section names this version never emits (Q3's second slice); when
/// seen on the wire they relay verbatim via `sections.unknown`.
pub const RESERVED_SECTION_NAMES: &[&str] = &["settings", "proofs"];

/// The row object's modeled keys — an `Entry` body cannot carry them.
pub const MODELED_ROW_KEYS: &[&str] = &["clock", "author"];

#[must_use]
pub fn is_reserved_section_name(name: &str) -> bool {
    RESERVED_SECTION_NAMES.contains(&name)
}

/// A device's whole current view of the per-user sections.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UserDocument {
    /// The document format version. Strict-envelope: missing or
    /// non-integer is a typed decode failure.
    pub format: u32,
    pub sections: UserSections,
    /// Unknown envelope keys: carried verbatim (the relay rule).
    #[serde(flatten, default, skip_serializing_if = "BTreeMap::is_empty")]
    pub unknown: BTreeMap<String, Value>,
}

/// The section registry: `journal` typed, everything else verbatim.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct UserSections {
    /// The v1 passenger, keyed by entry UUID string.
    pub journal: Option<BTreeMap<String, UserRecord>>,
    /// Unknown and reserved sections: carried verbatim.
    pub unknown: BTreeMap<String, Value>,
}

impl Serialize for UserSections {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serde_json::Map::new();
        if let Some(rows) = &self.journal {
            let section = serde_json::to_value(rows).map_err(serde::ser::Error::custom)?;
            map.insert(JOURNAL_SECTION.to_owned(), section);
        }
        for (key, value) in &self.unknown {
            map.insert(key.clone(), value.clone());
        }
        Value::Object(map).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for UserSections {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = BTreeMap::<String, Value>::deserialize(deserializer)?;
        let mut sections = Self::default();
        for (key, value) in raw {
            if key == JOURNAL_SECTION {
                // Leniency (the `DismissalsEntry` non-object
                // precedent): a journal body that is not a map of rows
                // relays verbatim, never fails the document.
                match value {
                    Value::Object(rows) => {
                        sections.journal = Some(
                            rows.into_iter()
                                .map(|(key, row)| (key, UserRecord::decode(row)))
                                .collect(),
                        );
                    }
                    other => {
                        sections.unknown.insert(key, other);
                    }
                }
            } else {
                sections.unknown.insert(key, value);
            }
        }
        Ok(sections)
    }
}

/// One row of a per-user section: a modeled clocked row, a tombstone,
/// or an unmodelable shape preserved verbatim (never folded).
#[derive(Debug, Clone, PartialEq)]
pub enum UserRecord {
    /// A modeled row: `clock` (+ the Q2 `author` slot) are the
    /// competition metadata; every other key of the row object rides
    /// `body` preserved.
    Entry {
        body: BTreeMap<String, Value>,
        clock: DateTime<Utc>,
        author: Option<AuthorId>,
    },
    /// `{ "deleted": true, "clock": … }` — the `RuleEntry` tombstone
    /// grammar.
    Tombstone {
        clock: DateTime<Utc>,
        author: Option<AuthorId>,
    },
    /// A shape this version does not model: preserved verbatim, never
    /// folded, never an error.
    Verbatim(Value),
}

impl UserRecord {
    /// The row's write clock; `None` for verbatim content (which the
    /// fold therefore never orders against a clocked row).
    #[must_use]
    pub fn clock(&self) -> Option<DateTime<Utc>> {
        match self {
            Self::Entry { clock, .. } | Self::Tombstone { clock, .. } => Some(*clock),
            Self::Verbatim(_) => None,
        }
    }

    /// Decodes one row; an unmodelable shape stays `Verbatim` with the
    /// original value (never an error). Priority order per the plan's
    /// decode rules: tombstone arm, then entry arm, else verbatim.
    fn decode(raw: Value) -> Self {
        let Value::Object(map) = &raw else {
            return Self::Verbatim(raw);
        };
        // The author rule (D-FC2): absent or `null` → `None`
        // (tolerant); parseable → `Some`; present and unparseable →
        // the whole row demotes to `Verbatim` (fail-closed).
        let author = match map.get("author") {
            None | Some(Value::Null) => Some(None),
            Some(Value::String(text)) => AuthorId::parse(text).map(Some),
            Some(_) => None,
        };
        let clock = || {
            serde_json::from_value::<DateTime<Utc>>(
                map.get("clock").cloned().unwrap_or(Value::Null),
            )
        };
        if map.get("deleted") == Some(&Value::Bool(true)) {
            if let (Some(author), Ok(clock)) = (author, clock()) {
                return Self::Tombstone { clock, author };
            }
            return Self::Verbatim(raw);
        }
        if let (Some(author), Ok(clock)) = (author, clock()) {
            let body = map
                .iter()
                .filter(|(key, _)| !MODELED_ROW_KEYS.contains(&key.as_str()))
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect();
            return Self::Entry {
                body,
                clock,
                author,
            };
        }
        Self::Verbatim(raw)
    }
}

impl Serialize for UserRecord {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Entry {
                body,
                clock,
                author,
            } => {
                let mut map = serde_json::Map::new();
                for (key, value) in body {
                    map.insert(key.clone(), value.clone());
                }
                map.insert(
                    "clock".to_owned(),
                    serde_json::to_value(clock).map_err(serde::ser::Error::custom)?,
                );
                // v1 never emits the author key (the grep-pin target);
                // the key exists on the wire only for `Some`.
                if let Some(author) = author {
                    map.insert(
                        "author".to_owned(),
                        serde_json::to_value(author).map_err(serde::ser::Error::custom)?,
                    );
                }
                Value::Object(map).serialize(serializer)
            }
            Self::Tombstone { clock, author } => {
                let mut map = serde_json::Map::new();
                map.insert("deleted".to_owned(), Value::Bool(true));
                map.insert(
                    "clock".to_owned(),
                    serde_json::to_value(clock).map_err(serde::ser::Error::custom)?,
                );
                if let Some(author) = author {
                    map.insert(
                        "author".to_owned(),
                        serde_json::to_value(author).map_err(serde::ser::Error::custom)?,
                    );
                }
                Value::Object(map).serialize(serializer)
            }
            Self::Verbatim(value) => value.serialize(serializer),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::str::FromStr;

    const ENTRY_KEY: &str = "0193b4c5-d6e7-7c89-ab01-2345abcdef01";
    const TOMBSTONE_KEY: &str = "0193b4c6-f0e1-7d2c-ab4a-59687a89bcde";
    const OPAQUE_KEY: &str = "0193b4c7-1223-7e45-8679-0b1c2d3e4f50";
    const AUTHORED_KEY: &str = "0193b4c8-a7b8-7c9d-ab0e-f1e2d3c4b5a6";
    const AUTHOR: &str = "0193a1b2-c3d4-7cc9-8a2e-5f1e2d3c4b5a";

    fn wire_example() -> serde_json::Value {
        serde_json::json!({
            "format": 1,
            "sections": {
                "journal": {
                    ENTRY_KEY: {
                        "kind": "reflection",
                        "text": "The fold laws went green.",
                        "clock": "2026-11-05T09:15:32Z"
                    },
                    TOMBSTONE_KEY: {
                        "deleted": true,
                        "clock": "2026-11-05T10:00:00Z"
                    },
                    OPAQUE_KEY: {
                        "opaque": "future-peer-row",
                        "v2": [1, 2, 3]
                    },
                    AUTHORED_KEY: {
                        "text": "authored by a B-full peer someday",
                        "clock": "2026-11-05T11:00:00Z",
                        "author": AUTHOR
                    }
                },
                "proofs": { "not": "understood here", "relayed": true }
            }
        })
    }

    #[test]
    fn the_wire_example_decodes_row_by_row() {
        let doc: UserDocument =
            serde_json::from_value(wire_example()).expect("the example decodes");
        assert_eq!(doc.format, DOCUMENT_FORMAT);
        let rows = doc.sections.journal.as_ref().expect("journal present");
        assert_eq!(rows.len(), 4);
        let UserRecord::Entry {
            body,
            clock,
            author,
        } = &rows[ENTRY_KEY]
        else {
            panic!("row 1 must be an Entry");
        };
        assert_eq!(clock.to_rfc3339(), "2026-11-05T09:15:32+00:00");
        assert_eq!(author, &None);
        assert_eq!(body.len(), 2);
        assert_eq!(body["kind"], json!("reflection"));
        assert!(matches!(rows[TOMBSTONE_KEY], UserRecord::Tombstone { .. }));
        assert!(matches!(rows[OPAQUE_KEY], UserRecord::Verbatim(_)));
        let UserRecord::Entry { author, .. } = &rows[AUTHORED_KEY] else {
            panic!("row 4 must be an Entry");
        };
        assert_eq!(
            author.as_ref().map(AuthorId::to_string),
            Some(AUTHOR.to_owned())
        );
        // The reserved section relays verbatim.
        assert!(doc.sections.unknown.contains_key("proofs"));
        assert!(is_reserved_section_name("proofs"));
        assert!(is_reserved_section_name("settings"));
        assert!(!is_reserved_section_name("journal"));
    }

    #[test]
    fn envelope_strictness_the_typed_failures() {
        let not_an_object = json!([1, 2, 3]);
        assert!(serde_json::from_value::<UserDocument>(not_an_object).is_err());
        let missing_format = json!({ "sections": {} });
        assert!(serde_json::from_value::<UserDocument>(missing_format).is_err());
        let float_format = json!({ "format": 1.5, "sections": {} });
        assert!(serde_json::from_value::<UserDocument>(float_format).is_err());
        let string_format = json!({ "format": "1", "sections": {} });
        assert!(serde_json::from_value::<UserDocument>(string_format).is_err());
        let missing_sections = json!({ "format": 1 });
        assert!(serde_json::from_value::<UserDocument>(missing_sections).is_err());
    }

    #[test]
    fn garbage_rows_never_kill_the_document() {
        let doc: UserDocument = serde_json::from_value(json!({
            "format": 1,
            "sections": {
                "journal": {
                    "k1": "bare string",
                    "k2": 42,
                    "k3": null,
                    "k4": { "deleted": true },
                    "k5": { "deleted": true, "clock": "not-a-clock" },
                    "k6": { "clock": "not-a-clock", "text": "x" },
                    "k7": { "clock": "2026-11-05T09:15:32Z", "author": "not-a-uuid" },
                    "k8": { "clock": "2026-11-05T09:15:32Z", "author": 42 },
                    "k9": { "clock": "2026-11-05T09:15:32Z", "author": null, "text": "ok" }
                }
            }
        }))
        .expect("the envelope survives any row garbage");
        let rows = doc.sections.journal.as_ref().expect("journal present");
        for key in ["k1", "k2", "k3", "k4", "k5", "k6", "k7", "k8"] {
            assert!(
                matches!(rows[key], UserRecord::Verbatim(_)),
                "{key} must demote to Verbatim"
            );
        }
        let UserRecord::Entry { author, .. } = &rows["k9"] else {
            panic!("a well-formed row still models");
        };
        assert_eq!(author, &None, "author: null decodes as None");
    }

    #[test]
    fn journal_body_that_is_not_a_row_map_relays_verbatim() {
        let doc: UserDocument = serde_json::from_value(json!({
            "format": 1,
            "sections": { "journal": 42 }
        }))
        .expect("decodes");
        assert!(doc.sections.journal.is_none());
        assert_eq!(doc.sections.unknown["journal"], json!(42));
    }

    #[test]
    fn non_uuid_map_keys_relay_through_decode_and_encode() {
        let raw = json!({
            "format": 1,
            "sections": {
                "journal": { "hand-edited-key": { "clock": "2026-11-05T09:15:32Z", "text": "x" } }
            }
        });
        let doc: UserDocument = serde_json::from_value(raw.clone()).expect("decodes");
        let encoded = serde_json::to_value(&doc).expect("encodes");
        assert_eq!(
            encoded["sections"]["journal"]["hand-edited-key"],
            raw["sections"]["journal"]["hand-edited-key"]
        );
    }

    proptest::proptest! {
        #[test]
        fn emission_of_generated_documents_is_v1_clean(
            doc in crate::test_support::user_document_v1_authorless()
        ) {
            let encoded = serde_json::to_value(&doc).expect("encodes");
            assert!(!contains_key(&encoded, "author"));
            assert!(!contains_key(&encoded, "base_clock"));
            let sections = encoded["sections"].as_object().expect("sections object");
            for name in RESERVED_SECTION_NAMES {
                assert!(!sections.contains_key(*name));
            }
        }

        #[test]
        fn roundtrip_law_decode_encode_decode_is_identity(
            doc in crate::test_support::user_document_v1()
        ) {
            let encoded = serde_json::to_value(&doc).expect("encodes");
            let decoded: UserDocument = serde_json::from_value(encoded).expect("re-decodes");
            assert_eq!(decoded, doc);
        }
    }

    #[test]
    fn authored_rows_emit_the_author_key() {
        let author = AuthorId::parse(AUTHOR).expect("parses");
        let doc = UserDocument {
            format: DOCUMENT_FORMAT,
            sections: UserSections {
                journal: Some(BTreeMap::from([(
                    ENTRY_KEY.to_owned(),
                    UserRecord::Entry {
                        body: BTreeMap::new(),
                        clock: DateTime::<Utc>::from_str("2026-11-05T11:00:00Z").expect("clock"),
                        author: Some(author),
                    },
                )])),
                unknown: BTreeMap::new(),
            },
            unknown: BTreeMap::new(),
        };
        let encoded = serde_json::to_value(&doc).expect("encodes");
        let row = &encoded["sections"]["journal"][ENTRY_KEY];
        assert_eq!(row["author"], json!(AUTHOR));
    }

    #[test]
    fn unknown_content_is_carried_back_out() {
        let raw = json!({
            "format": 1,
            "future-envelope-key": [1, {"x": 2}],
            "sections": {
                "journal": {
                    ENTRY_KEY: { "clock": "2026-11-05T09:15:32Z", "extra": {"deep": true} }
                },
                "proofs": { "relayed": true }
            }
        });
        let doc: UserDocument = serde_json::from_value(raw).expect("decodes");
        let encoded = serde_json::to_value(&doc).expect("encodes");
        assert_eq!(encoded["future-envelope-key"], json!([1, {"x": 2}]));
        assert_eq!(encoded["sections"]["proofs"], json!({ "relayed": true }));
        let row = &encoded["sections"]["journal"][ENTRY_KEY];
        assert_eq!(row["extra"], json!({"deep": true}));
    }

    /// Structural key search (no text assertions — this inspects the
    /// emitted JSON tree).
    fn contains_key(value: &serde_json::Value, needle: &str) -> bool {
        match value {
            serde_json::Value::Object(map) => map
                .iter()
                .any(|(k, v)| k == needle || contains_key(v, needle)),
            serde_json::Value::Array(items) => items.iter().any(|v| contains_key(v, needle)),
            _ => false,
        }
    }
}
