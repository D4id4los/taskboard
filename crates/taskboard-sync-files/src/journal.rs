// SPDX-License-Identifier: MIT OR Apache-2.0
//! The `journal` wire section's codec (DR-8 D-ES7): the reflection
//! entity's body ↔ a [`UserRecord`], so the transport-agnostic core —
//! and the folder/LAN successors that reuse it — can encode a device's
//! reflection home into `appdata.json` and decode peer rows back into
//! the import fold's inputs.
//!
//! Body grammar: the entity serialized **minus** `id` (the map key),
//! `updated_at` (the row `clock`), and `deleted` (the row grammar — a
//! tombstone row carries no body; the owner's local row keeps its body
//! in place). Decode is the A1 mint at the trust boundary: the map key
//! re-mints through `ReflectionId`'s UUID parse and the body re-decodes
//! through the entity's own serde (strict ratings, tolerant attribute
//! flags — a newer build's out-of-range rating is a per-row
//! [`RowRejection`], never a panic, never a whole-document failure).
//!
//! The `author` slot is ignored on import (it already ranked in the
//! files fold; the engine adopts winners verbatim). Unknown body keys
//! are tolerated by the entity derive's default serde — pinned, not
//! configured. Known leniency: two rows can be wire-different (an
//! unknown key a newer build carries) yet entity-equal after decode;
//! the engine then keeps its local row while relaying the wire one —
//! the merged view still agrees across devices, so hashes do too.

use std::collections::BTreeMap;

use taskboard_domain::ReflectionId;
use taskboard_domain::reflection::{ReflectionEntry, ReflectionState};

use crate::document::UserRecord;

/// Why one row could not become an import input. Per row, never a
/// whole-document failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum RowRejection {
    /// An unmodelable row shape (the tolerant relay's `Verbatim`): it
    /// never folds and never crosses the engine seam.
    #[error("row is not a modeled shape")]
    Verbatim,
    /// The map key is not a UUID (a hand-edited key relays but never
    /// imports).
    #[error("row key is not a UUID")]
    BadKey,
    /// The body did not decode into the entity (a newer build's
    /// out-of-range rating, or a shape-drift the derive rejects).
    #[error("row body does not decode")]
    BadBody,
}

/// The wire map key of an entry: its UUID text.
///
/// # Panics
///
/// Never: a UUID always serializes.
#[must_use]
pub fn row_key(entry: &ReflectionEntry) -> String {
    entry.id.as_uuid().to_string()
}

/// Encodes one entry as a keyed wire row: live → `Entry` (the body is
/// the entity minus `id`/`updated_at`/`deleted`, the clock is
/// `updated_at`); tombstoned → `Tombstone` (clock only; the wire
/// grammar carries no body). The `author` slot stays empty (v1 never
/// emits it).
///
/// # Panics
///
/// Never: the entity serializes to an object by construction (the
/// expects are total-function pins).
#[must_use]
pub fn encode_row(entry: &ReflectionEntry) -> (String, UserRecord) {
    let clock = entry.updated_at;
    let record = if entry.deleted {
        UserRecord::Tombstone {
            clock,
            author: None,
        }
    } else {
        let mut value = serde_json::to_value(entry).expect("an entity always serializes");
        let map = value
            .as_object_mut()
            .expect("the entity serializes to an object");
        for key in ["id", "updated_at", "deleted"] {
            map.remove(key);
        }
        let body: BTreeMap<String, serde_json::Value> = map
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        UserRecord::Entry {
            body,
            clock,
            author: None,
        }
    };
    (row_key(entry), record)
}

/// Encodes the whole reflection home: every entry, tombstones included
/// (silent absence would let a peer resurrect a deleted entry).
///
/// # Panics
///
/// Never: only [`encode_row`] is called, which never panics.
#[must_use]
pub fn journal_section(state: &ReflectionState) -> BTreeMap<String, UserRecord> {
    state
        .entries
        .values()
        .map(encode_row)
        .collect::<BTreeMap<_, _>>()
}

/// Decodes one keyed wire row into an import input. The `author` slot
/// is ignored (it ranked in the fold already).
///
/// # Errors
///
/// [`RowRejection::Verbatim`] for an unmodelable shape,
/// [`RowRejection::BadKey`] for a non-UUID key, and
/// [`RowRejection::BadBody`] when the entry body fails the entity's
/// own decode.
///
/// # Panics
///
/// Never: the id and clock always serialize; everything else is a
/// typed rejection.
pub fn decode_row(
    key: &str,
    record: &UserRecord,
) -> Result<taskboard_domain::ImportedReflection, RowRejection> {
    let id = uuid::Uuid::parse_str(key)
        .map(ReflectionId::from_uuid)
        .map_err(|_| RowRejection::BadKey)?;
    match record {
        UserRecord::Entry { body, clock, .. } => {
            let mut value = serde_json::Map::new();
            for (body_key, body_value) in body {
                value.insert(body_key.clone(), body_value.clone());
            }
            value.insert(
                "id".to_owned(),
                serde_json::to_value(id).expect("an id serializes"),
            );
            value.insert(
                "updated_at".to_owned(),
                serde_json::to_value(clock).expect("a clock serializes"),
            );
            value.insert("deleted".to_owned(), serde_json::Value::Bool(false));
            let entry: ReflectionEntry = serde_json::from_value(serde_json::Value::Object(value))
                .map_err(|_| RowRejection::BadBody)?;
            Ok(taskboard_domain::ImportedReflection::Entry(entry))
        }
        UserRecord::Tombstone { clock, .. } => {
            Ok(taskboard_domain::ImportedReflection::Tombstone { id, clock: *clock })
        }
        UserRecord::Verbatim(_) => Err(RowRejection::Verbatim),
    }
}

/// Decodes a whole keyed section into `(import inputs, rejected)`.
/// Order follows the map's key order (deterministic); rejections are
/// counted, never fatal.
#[must_use]
pub fn decode_journal(
    rows: &BTreeMap<String, UserRecord>,
) -> (Vec<taskboard_domain::ImportedReflection>, usize) {
    let mut imported = Vec::new();
    let mut rejected = 0;
    for (key, record) in rows {
        match decode_row(key, record) {
            Ok(row) => imported.push(row),
            Err(_) => rejected += 1,
        }
    }
    (imported, rejected)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::MODELED_ROW_KEYS;
    use proptest::prelude::*;
    use std::collections::BTreeSet;

    use chrono::{NaiveDate, TimeZone, Utc};
    use taskboard_domain::reflection::{HealthFlag, Rating, ReflectionAttributes, ReviewCadence};
    use taskboard_domain::test_support::{proptest_config, reflection_entry_strategy};

    /// The row keys the encoder never places in a body (the modeled
    /// envelope keys plus the three entity keys carried out-of-band).
    fn reserved_body_keys() -> Vec<&'static str> {
        let mut keys = MODELED_ROW_KEYS.to_vec();
        keys.extend_from_slice(&["id", "updated_at", "deleted"]);
        keys
    }

    fn ts(secs: i64) -> chrono::DateTime<Utc> {
        Utc.timestamp_opt(secs, 0).unwrap()
    }

    fn entry(id_raw: u128, deleted: bool) -> ReflectionEntry {
        ReflectionEntry {
            id: ReflectionId::from(uuid::Uuid::from_u128(id_raw)),
            cadence: ReviewCadence::Day,
            day: NaiveDate::from_ymd_opt(2026, 11, 5).unwrap(),
            window_start: ts(100),
            window_end: ts(100 + 86_400),
            text: "the codec round-trips".into(),
            attributes: ReflectionAttributes {
                mood: Some(Rating::try_new(4).unwrap()),
                energy: None,
                flags: BTreeSet::from([HealthFlag::GoodDay]),
                extra_flags: BTreeSet::new(),
            },
            created_at: ts(200),
            updated_at: ts(300),
            deleted,
        }
    }

    #[test]
    fn a_live_entry_encodes_and_decodes_back_identically() {
        let original = entry(7, false);
        let (key, record) = encode_row(&original);
        assert_eq!(key, original.id.as_uuid().to_string());
        let decoded = decode_row(&key, &record).expect("decodes");
        let taskboard_domain::ImportedReflection::Entry(back) = decoded else {
            panic!("a live row decodes as an entry");
        };
        assert_eq!(back, original);
    }

    /// The tombstone's whole story is id + clock; decode restores the
    /// tombstone shape exactly.
    #[test]
    fn a_tombstoned_entry_encodes_to_the_tombstone_grammar() {
        let original = entry(8, true);
        let (key, record) = encode_row(&original);
        let decoded = decode_row(&key, &record).expect("decodes");
        assert_eq!(
            decoded,
            taskboard_domain::ImportedReflection::Tombstone {
                id: original.id,
                clock: original.updated_at,
            }
        );
    }

    /// The encoded body carries none of the out-of-band keys, and the
    /// carried fields are the entity's own (structural inspection of
    /// the emitted JSON tree).
    #[test]
    fn the_encoded_body_carries_no_out_of_band_keys() {
        let (_, record) = encode_row(&entry(9, false));
        let UserRecord::Entry { body, .. } = &record else {
            panic!("a live row encodes as an entry");
        };
        for key in reserved_body_keys() {
            assert!(!body.contains_key(key), "{key} never rides the body");
        }
        let json = serde_json::to_value(record).unwrap();
        assert_eq!(json["cadence"], serde_json::json!("day"));
        assert_eq!(json["attributes"]["mood"], serde_json::json!(4));
        assert_eq!(
            json["window_start"],
            serde_json::json!("1970-01-01T00:01:40Z")
        );
    }

    #[test]
    fn the_whole_home_encodes_including_tombstones() {
        let mut state = taskboard_domain::ReflectionState::default();
        let live = entry(1, false);
        let dead = entry(2, true);
        state.entries.insert(live.id, live.clone());
        state.entries.insert(dead.id, dead.clone());
        let section = journal_section(&state);
        assert_eq!(section.len(), 2, "tombstones are encoded too");
        assert!(matches!(section[&row_key(&live)], UserRecord::Entry { .. }));
        assert!(matches!(
            section[&row_key(&dead)],
            UserRecord::Tombstone { .. }
        ));
        let (imported, rejected) = decode_journal(&section);
        assert_eq!(rejected, 0);
        assert_eq!(imported.len(), 2);
    }

    #[test]
    fn an_unknown_body_key_is_tolerated_by_the_entity_decode() {
        let (key, mut record) = encode_row(&entry(10, false));
        let UserRecord::Entry { body, clock, .. } = &mut record else {
            panic!("entry shape");
        };
        body.insert("future_field".to_owned(), serde_json::json!({"deep": [1]}));
        let _ = clock;
        let decoded = decode_row(&key, &record).expect("the unknown key is ignored");
        let taskboard_domain::ImportedReflection::Entry(back) = decoded else {
            panic!("entry shape");
        };
        assert_eq!(back, entry(10, false));
    }

    /// The strict-rating mint holds at the wire boundary: a newer
    /// build's out-of-range rating rejects the row, typed.
    #[test]
    fn an_out_of_range_rating_rejects_the_row_as_bad_body() {
        let (key, mut record) = encode_row(&entry(11, false));
        let UserRecord::Entry { body, .. } = &mut record else {
            panic!("entry shape");
        };
        body.insert("attributes".to_owned(), serde_json::json!({"mood": 6}));
        assert_eq!(decode_row(&key, &record), Err(RowRejection::BadBody));
    }

    #[test]
    fn a_non_uuid_key_rejects_as_bad_key() {
        let (_, record) = encode_row(&entry(12, false));
        assert_eq!(
            decode_row("hand-edited-key", &record),
            Err(RowRejection::BadKey)
        );
    }

    #[test]
    fn a_verbatim_row_rejects_as_verbatim() {
        let record = UserRecord::Verbatim(serde_json::json!({"opaque": true}));
        assert_eq!(
            decode_row(&uuid::Uuid::from_u128(3).to_string(), &record),
            Err(RowRejection::Verbatim)
        );
    }

    proptest::proptest! {
        #![proptest_config(proptest_config(256))]

        /// The roundtrip law over generated entries: every live entry
        /// survives encode → decode as itself.
        #[test]
        fn live_entries_roundtrip_through_the_wire(
            original in reflection_entry_strategy(),
        ) {
            let mut original = original;
            original.deleted = false;
            let (key, record) = encode_row(&original);
            let decoded = decode_row(&key, &record).expect("decodes");
            prop_assert_eq!(decoded, taskboard_domain::ImportedReflection::Entry(original));
        }

        /// Every tombstoned entry encodes to exactly its tombstone
        /// shape.
        #[test]
        fn deleted_entries_encode_to_tombstones(
            original in reflection_entry_strategy(),
        ) {
            let mut original = original;
            original.deleted = true;
            let (key, record) = encode_row(&original);
            prop_assert_eq!(
                decode_row(&key, &record).expect("decodes"),
                taskboard_domain::ImportedReflection::Tombstone {
                    id: original.id,
                    clock: original.updated_at,
                }
            );
        }

        /// Garbage rows (arbitrary JSON under arbitrary body keys, an
        /// arbitrary key) never panic — decode returns *some* typed
        /// outcome (a floor on garbage input, no ceiling on good input).
        #[test]
        fn garbage_rows_decode_to_a_typed_outcome_without_panicking(
            key in "[a-zA-Z0-9-]{0,40}",
            body in proptest::collection::btree_map(
                crate::test_support::body_key(),
                crate::test_support::json_value(),
                0..4,
            ),
            clock in crate::test_support::clock(),
        ) {
            let record = UserRecord::Entry { body, clock, author: None };
            match decode_row(&key, &record) {
                Ok(taskboard_domain::ImportedReflection::Entry(entry)) => {
                    prop_assert_eq!(entry.updated_at, clock);
                    prop_assert!(!entry.deleted);
                }
                Ok(taskboard_domain::ImportedReflection::Tombstone { .. }) => {
                    prop_assert!(false, "an Entry record never decodes as a tombstone");
                }
                Err(RowRejection::BadKey | RowRejection::BadBody) => {}
                Err(RowRejection::Verbatim) => {
                    prop_assert!(false, "an Entry record is never Verbatim");
                }
            }
        }
    }
}
