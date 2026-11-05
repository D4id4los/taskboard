// SPDX-License-Identifier: MIT OR Apache-2.0
//! Proptest strategies over the substrate's document shapes, shared
//! with downstream crates via the `test-support` feature (the
//! `taskboard-domain` precedent; the seam chunk's engine tests
//! consume these).

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use proptest::prelude::*;
use serde_json::Value;

use crate::document::{UserDocument, UserRecord, UserSections};
use crate::ids::AuthorId;

/// Arbitrary JSON leaves — no floats (a non-finite float is
/// unconstructible via serde and never rides the wire; keeping numbers
/// integral keeps `Value`'s `Ord` transparent).
pub fn json_leaf() -> impl Strategy<Value = Value> {
    prop_oneof![
        Just(Value::Null),
        any::<bool>().prop_map(Value::Bool),
        any::<i64>().prop_map(Value::from),
        any::<String>().prop_map(Value::String),
    ]
}

/// Arbitrary JSON values (recursive), with sorted object maps.
pub fn json_value() -> impl Strategy<Value = Value> {
    json_leaf().prop_recursive(3, 8, 4, |inner| {
        prop_oneof![
            proptest::collection::vec(inner.clone(), 0..4).prop_map(Value::Array),
            proptest::collection::btree_map("[a-z][a-z0-9_]{0,8}", inner, 0..4)
                .prop_map(|map| Value::Object(map.into_iter().collect())),
        ]
    })
}

/// A legal `Entry` body key: the row's modeled keys (`clock`,
/// `author`) are consumed as metadata on decode, so a body cannot
/// carry them.
pub fn body_key() -> impl Strategy<Value = String> {
    "[a-z][a-z0-9_]{0,10}".prop_map(|key| {
        if key == "clock" || key == "author" {
            format!("{key}_")
        } else {
            key
        }
    })
}

/// A legal unknown-section key: never the modeled `journal`.
pub fn unknown_section_key() -> impl Strategy<Value = String> {
    "[a-z][a-z0-9_]{0,10}".prop_map(|key| {
        if key == "journal" {
            format!("{key}_")
        } else {
            key
        }
    })
}

/// A legal unknown-envelope key: never the modeled `format`/
/// `sections`.
pub fn unknown_envelope_key() -> impl Strategy<Value = String> {
    "[a-z][a-z0-9_]{0,10}".prop_map(|key| match key.as_str() {
        "format" | "sections" => format!("{key}_"),
        _ => key,
    })
}

/// A row-map key — this version emits UUID strings; decode tolerates
/// any key, the generator stays inside the emitted grammar.
pub fn row_key() -> impl Strategy<Value = String> {
    any::<u128>().prop_map(|seed| uuid::Uuid::from_u128(seed).to_string())
}

/// Write clocks over a wide but sane range (no far-future years).
///
/// # Panics
/// Never in practice: the whole range converts; the `expect` is the
/// total-function pin, not a real failure path.
pub fn clock() -> impl Strategy<Value = DateTime<Utc>> {
    (0..4_000_000_000_i64).prop_map(|secs| {
        DateTime::from_timestamp(secs, 0).expect("in-range seconds always convert")
    })
}

/// Author ids (the Q2 slot's forward shape; v1 strategies also cover
/// the `None` writer).
///
/// # Panics
/// Never in practice: a `Uuid`'s `Display` always parses back.
pub fn author_id() -> impl Strategy<Value = AuthorId> {
    any::<u128>().prop_map(|seed| {
        AuthorId::parse(&uuid::Uuid::from_u128(seed).to_string())
            .expect("a Display UUID always parses")
    })
}

/// A row: entries, tombstones, verbatims, and both author arms.
pub fn user_record() -> impl Strategy<Value = UserRecord> {
    prop_oneof![
        (
            proptest::collection::btree_map(body_key(), json_value(), 0..4),
            clock()
        )
            .prop_map(|(body, c)| UserRecord::Entry {
                body,
                clock: c,
                author: None,
            }),
        clock().prop_map(|c| UserRecord::Tombstone {
            clock: c,
            author: None
        }),
        json_value().prop_map(UserRecord::Verbatim),
        (
            proptest::collection::btree_map(body_key(), json_value(), 0..4),
            clock(),
            author_id()
        )
            .prop_map(|(body, c, a)| UserRecord::Entry {
                body,
                clock: c,
                author: Some(a),
            }),
        (clock(), author_id()).prop_map(|(c, a)| UserRecord::Tombstone {
            clock: c,
            author: Some(a)
        }),
    ]
}

/// A journal section: rows keyed by UUID strings.
pub fn journal_rows() -> impl Strategy<Value = BTreeMap<String, UserRecord>> {
    proptest::collection::btree_map(row_key(), user_record(), 0..6)
}

/// A section registry with a `journal` presence arm and unknown
/// sections.
pub fn user_sections() -> impl Strategy<Value = UserSections> {
    (
        proptest::option::of(journal_rows()),
        proptest::collection::btree_map(unknown_section_key(), json_value(), 0..3),
    )
        .prop_map(|(journal, unknown)| UserSections { journal, unknown })
}

/// A whole document in the current format (v1 never emits another).
pub fn user_document_v1() -> impl Strategy<Value = UserDocument> {
    (
        user_sections(),
        proptest::collection::btree_map(unknown_envelope_key(), json_value(), 0..2),
    )
        .prop_map(|(sections, unknown)| UserDocument {
            format: crate::document::DOCUMENT_FORMAT,
            sections,
            unknown,
        })
}

/// A whole document in any format the codec accepts (the fold's
/// `format = max` bookkeeping needs higher values sometimes).
pub fn user_document() -> impl Strategy<Value = UserDocument> {
    (
        1..3_u32,
        user_sections(),
        proptest::collection::btree_map(unknown_envelope_key(), json_value(), 0..2),
    )
        .prop_map(|(format, sections, unknown)| UserDocument {
            format,
            sections,
            unknown,
        })
}

/// A v1-authored document: every row's author slot is `None` (the
/// emission pins assert against exactly what this version writes).
pub fn user_document_v1_authorless() -> impl Strategy<Value = UserDocument> {
    user_document_v1().prop_map(|mut doc| {
        if let Some(rows) = &mut doc.sections.journal {
            for row in rows.values_mut() {
                if let UserRecord::Entry { author, .. } | UserRecord::Tombstone { author, .. } = row
                {
                    *author = None;
                }
            }
        }
        doc
    })
}
