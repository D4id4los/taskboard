// SPDX-License-Identifier: MIT OR Apache-2.0
//! The n-ary merge fold (design D-SF5, decision D-FC6): pure, total,
//! **input-order-free** — per entry key the winner is the max over the
//! total order `(rank, clock, author, content)`, so no input
//! permutation can change any winner and the laws below hold by
//! construction (they are proptested anyway: they *are* the fold's
//! contract).
//!
//! The order legs:
//! - `rank`: clocked rows (`Entry`/`Tombstone`) outrank `Verbatim` —
//!   the verbatim floor. A shape this version cannot understand never
//!   displaces modeled content; a key holds a `Verbatim` row in the
//!   merged output iff no input carries a clocked row for it.
//! - `clock`: the embedded write clock; tombstones compete exactly as
//!   entries (a later delete beats an edit, a later edit beats an
//!   older tombstone — the `RuleEntry` semantics unchanged).
//! - `author` (the Q2 slot): `None` for every v1 writer; `None <
//!   Some(_)` is the pinned forward choice — at equal clocks a
//!   B-full-authored row outranks an authorless one.
//! - `content`: the residual tie at equal `(clock, author)` (two
//!   devices stamping the same second — plausible at whole-second
//!   clocks). Rows compare their canonical JSON content (`Value`'s
//!   total `Ord`; a `Tombstone` an identical constant). This is the
//!   honest reading of the design's "max by (clock, author, entry-id)":
//!   the entry-id leg is the section map key itself — constant within
//!   a per-key comparison — so content is the deterministic totaliser
//!   that keeps the fold a pure `max`.
//!
//! The fold is document-attribution-free: which device published a
//! row never influences the winner (a re-publication by a third device
//! changes nothing — idempotence). Unknown sections and unknown
//! envelope keys merge one level down by the same discipline, their
//! whole `Value` the comparable content (`Value`-max). The output
//! `format` is the max over inputs (caller-error bookkeeping, not a
//! capability mechanism — D-FC8). An empty input yields the empty
//! current-format document.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::document::{DOCUMENT_FORMAT, UserDocument, UserRecord};
use crate::ids::AuthorId;

/// Folds any set of device documents into the merged current-state
/// view. Pure, total, input-order-free.
pub fn fold<'a>(documents: impl IntoIterator<Item = &'a UserDocument>) -> UserDocument {
    let mut journal: Option<BTreeMap<String, UserRecord>> = None;
    let mut unknown_sections: BTreeMap<String, Value> = BTreeMap::new();
    let mut unknown_envelope: BTreeMap<String, Value> = BTreeMap::new();
    let mut format: Option<u32> = None;
    for document in documents {
        format = Some(format.map_or(document.format, |current| current.max(document.format)));
        if let Some(rows) = &document.sections.journal {
            let map = journal.get_or_insert_with(BTreeMap::new);
            for (key, record) in rows {
                merge_clocked(map, key, record);
            }
        }
        merge_values(&mut unknown_sections, &document.sections.unknown);
        merge_values(&mut unknown_envelope, &document.unknown);
    }
    UserDocument {
        format: format.unwrap_or(DOCUMENT_FORMAT),
        sections: crate::document::UserSections {
            journal,
            unknown: unknown_sections,
        },
        unknown: unknown_envelope,
    }
}

/// Per-key winner = max over the fold's total order.
fn merge_clocked(map: &mut BTreeMap<String, UserRecord>, key: &str, record: &UserRecord) {
    match map.get(key) {
        Some(existing) if order_key(existing) >= order_key(record) => {}
        _ => {
            map.insert(key.to_owned(), record.clone());
        }
    }
}

/// Unknown sections and envelope keys merge key-wise by
/// canonical-content max (the same totaliser the rows use).
fn merge_values(map: &mut BTreeMap<String, Value>, overlay: &BTreeMap<String, Value>) {
    for (key, value) in overlay {
        match map.get(key) {
            Some(existing) if content_key(existing) >= content_key(value) => {}
            _ => {
                map.insert(key.clone(), value.clone());
            }
        }
    }
}

/// The content totaliser: the canonical serialisation bytes of a
/// `Value` — a **total**, deterministic order. `serde_json::Value` is
/// only `PartialOrd`, so the canonical bytes are the totaliser, not a
/// nicety. Only determinism and totality matter here: the leg decides
/// otherwise-indistinguishable same-second writes, never semantic
/// ranking (documented as the novel tiebreak in the chunk plan §7).
fn content_key(value: &Value) -> Vec<u8> {
    let canonical = crate::hash::canonicalise(value);
    serde_json::to_vec(&canonical).expect("canonical JSON always serializes")
}

/// The total order key of a row: `(rank, clock, author, content)`.
fn order_key(record: &UserRecord) -> (u8, DateTime<Utc>, Option<AuthorId>, Vec<u8>) {
    match record {
        UserRecord::Entry {
            body,
            clock,
            author,
        } => (
            1,
            *clock,
            *author,
            content_key(&Value::Object(
                body.iter()
                    .map(|(key, value)| (key.clone(), value.clone()))
                    .collect(),
            )),
        ),
        // Tombstones are content-free: an identical constant makes the
        // equal-(clock, author) residual tie resolve (identically).
        UserRecord::Tombstone { clock, author } => (1, *clock, *author, b"true".to_vec()),
        // The verbatim floor: rank 0, the clock/author legs are unused
        // (equal across all verbatims), the value itself is the
        // deterministic tiebreak.
        UserRecord::Verbatim(value) => (0, epoch(), None, content_key(value)),
    }
}

fn epoch() -> DateTime<Utc> {
    DateTime::from_timestamp(0, 0).expect("the epoch instant always converts")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::UserSections;
    use crate::hash::StateHash;
    use crate::test_support::{clock, json_value, row_key, user_document_v1};
    use proptest::collection;
    use proptest::prelude::*;
    use serde_json::json;
    use std::collections::BTreeMap;

    /// A generated document set together with one of its permutations
    /// — the laws' adversarial input shape.
    fn permuted_set() -> impl Strategy<Value = (Vec<UserDocument>, Vec<UserDocument>)> {
        collection::vec(user_document_v1(), 0..5).prop_flat_map(|docs| {
            let len = docs.len();
            (Just(docs), collection::vec(any::<u64>(), len)).prop_map(|(docs, keys)| {
                let mut order: Vec<usize> = (0..docs.len()).collect();
                order.sort_by_key(|&i| keys[i]);
                let permuted = order.iter().map(|&i| docs[i].clone()).collect();
                (docs, permuted)
            })
        })
    }

    fn fold_all(docs: &[UserDocument]) -> UserDocument {
        fold(docs.iter())
    }

    fn doc_with_journal(key: &str, record: UserRecord) -> UserDocument {
        UserDocument {
            format: DOCUMENT_FORMAT,
            sections: UserSections {
                journal: Some(BTreeMap::from([(key.to_owned(), record)])),
                unknown: BTreeMap::new(),
            },
            unknown: BTreeMap::new(),
        }
    }

    fn entry(
        body: serde_json::Value,
        clock: DateTime<Utc>,
        author: Option<AuthorId>,
    ) -> UserRecord {
        // A body is an object on the wire; a generated non-object
        // value rides wrapped so the strategy stays arbitrary.
        let body = match body {
            serde_json::Value::Object(map) => map.into_iter().collect(),
            other => serde_json::json!({ "text": other })
                .as_object()
                .expect("wrapped body")
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        };
        UserRecord::Entry {
            body,
            clock,
            author,
        }
    }

    fn bytes(value: &serde_json::Value) -> Vec<u8> {
        serde_json::to_vec(&crate::hash::canonicalise(value)).expect("serializes")
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        #[test]
        fn fold_is_commutative_including_the_state_hash(
            (docs, permuted) in permuted_set()
        ) {
            let merged = fold_all(&docs);
            let merged_permuted = fold_all(&permuted);
            prop_assert_eq!(&merged_permuted, &merged);
            prop_assert_eq!(StateHash::of(&merged_permuted), StateHash::of(&merged));
        }

        #[test]
        fn fold_is_idempotent(docs in collection::vec(user_document_v1(), 0..5)) {
            let merged = fold_all(&docs);
            let mut doubled = docs.clone();
            doubled.extend(docs.iter().cloned());
            prop_assert_eq!(fold_all(&doubled), merged);
        }

        #[test]
        fn fold_is_associative(docs in collection::vec(user_document_v1(), 3..6)) {
            let (a, b, c) = (docs[0].clone(), docs[1].clone(), docs[2].clone());
            let left = fold_all(&[fold_all(&[a.clone(), b.clone()]), c.clone()]);
            let right = fold_all(&[a, fold_all(&[b, c])]);
            let all = fold_all(&[docs[0].clone(), docs[1].clone(), docs[2].clone()]);
            prop_assert_eq!(&left, &all);
            prop_assert_eq!(&right, &all);
        }

        #[test]
        fn tombstone_dominates_older_edit_and_loses_to_newer(
            key in row_key(),
            c1 in clock(),
            c2 in clock(),
            body in json_value(),
        ) {
            prop_assume!(c1 != c2);
            let edit = entry(body, c1, None);
            let tombstone = UserRecord::Tombstone { clock: c2, author: None };
            let (edit_doc, tomb_doc) = (
                doc_with_journal(&key, edit),
                doc_with_journal(&key, tombstone),
            );
            let forward = fold_all(&[edit_doc.clone(), tomb_doc.clone()]);
            let backward = fold_all(&[tomb_doc, edit_doc]);
            prop_assert_eq!(&forward, &backward);
            let expected_clock = c1.max(c2);
            let rows = forward.sections.journal.expect("journal present");
            let winner = &rows[&key];
            prop_assert_eq!(winner.clock(), Some(expected_clock));
            // `prop_assert!` renders its expression into the failure
            // message, so brace-bearing patterns are pre-computed.
            let is_tombstone = matches!(winner, UserRecord::Tombstone { .. });
            if c2 > c1 {
                prop_assert!(is_tombstone);
            } else {
                prop_assert!(!is_tombstone);
            }
        }

        #[test]
        fn verbatim_is_the_floor(
            key in row_key(),
            c in clock(),
            body in json_value(),
            opaque1 in json_value(),
            opaque2 in json_value(),
        ) {
            let edit_doc = doc_with_journal(&key, entry(body, c, None));
            let verbatim_doc =
                doc_with_journal(&key, UserRecord::Verbatim(opaque1.clone()));
            // A clocked row wins the key; the verbatim never displaces it.
            let merged = fold_all(&[verbatim_doc.clone(), edit_doc.clone()]);
            let rows = merged.sections.journal.expect("journal present");
            let is_entry = matches!(rows[&key], UserRecord::Entry { .. });
            prop_assert!(is_entry);

            // Alone, verbatims survive — deterministically across
            // permutations (covered by commutativity; here the
            // Value-max is pinned directly).
            let only_verbatim = fold_all(&[
                doc_with_journal(&key, UserRecord::Verbatim(opaque1.clone())),
                doc_with_journal(&key, UserRecord::Verbatim(opaque2.clone())),
            ]);
            let rows = only_verbatim.sections.journal.expect("journal present");
            let UserRecord::Verbatim(winner) = &rows[&key] else {
                panic!("the verbatim floor");
            };
            let expected = if bytes(&opaque1) >= bytes(&opaque2) { opaque1 } else { opaque2 };
            prop_assert_eq!(winner, &expected);
        }

        #[test]
        fn unknown_content_merges_order_free(
            key in "[a-z][a-z0-9_]{0,10}".prop_map(|k| if k == "journal" { format!("{k}_") } else { k }),
            v1 in json_value(),
            v2 in json_value(),
        ) {
            let make = |value: serde_json::Value, format: u32| UserDocument {
                format,
                sections: UserSections {
                    journal: None,
                    unknown: BTreeMap::from([(key.clone(), value)]),
                },
                unknown: BTreeMap::from([("future".to_owned(), json!(format))]),
            };
            let merged = fold_all(&[make(v1.clone(), 1), make(v2.clone(), 2)]);
            let expected = if bytes(&v1) >= bytes(&v2) { v1 } else { v2 };
            prop_assert_eq!(&merged.sections.unknown[&key], &expected);
            prop_assert_eq!(&merged.unknown["future"], &json!(2));
            prop_assert_eq!(merged.format, 2);
        }

        #[test]
        fn fold_of_generated_rows_keeps_every_well_formed_row_visible(
            docs in collection::vec(user_document_v1(), 1..4)
        ) {
            // Floors on good input: every key that exists in any input
            // exists in the merged output (the fold drops nothing).
            let merged = fold_all(&docs);
            for doc in &docs {
                if let Some(rows) = &doc.sections.journal {
                    let out = merged.sections.journal.as_ref().expect("journal present");
                    for key in rows.keys() {
                        prop_assert!(out.contains_key(key));
                    }
                }
                for key in doc.sections.unknown.keys() {
                    prop_assert!(merged.sections.unknown.contains_key(key));
                }
                for key in doc.unknown.keys() {
                    prop_assert!(merged.unknown.contains_key(key));
                }
            }
        }
    }

    proptest! {
        // The review prompt's named trap, generated *adversarially*:
        // rows that tie on `(clock, author)` = `(c, None)` and differ
        // only in body content, so a "first seen wins" fold would
        // diverge across permutations.
        #[test]
        fn author_none_totality_survives_any_permutation(
            key in row_key(),
            c in clock(),
            bodies in collection::vec(json_value(), 2..5),
        ) {
            let docs: Vec<UserDocument> = bodies
                .iter()
                .map(|body| {
                    doc_with_journal(&key, entry(json!({ "text": body }), c, None))
                })
                .collect();
            let reference = fold_all(&docs);
            let mut reversed = docs.clone();
            reversed.reverse();
            let from_reversed = fold_all(&reversed);
            prop_assert_eq!(&from_reversed, &reference);
            // The content tiebreak is visible: with distinct bodies the
            // winner is the Value-max body.
            let expected = bodies
                .iter()
                .max_by(|a, b| bytes(a).cmp(&bytes(b)))
                .expect("non-empty");
            let rows = reference.sections.journal.as_ref().expect("journal present");
            let UserRecord::Entry { body, author, .. } = &rows[&key] else {
                panic!("an entry wins");
            };
            prop_assert_eq!(author, &None);
            let expected_body: BTreeMap<String, serde_json::Value> = json!({ "text": expected })
                .as_object()
                .expect("object")
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            prop_assert_eq!(body.clone(), expected_body);
        }
    }

    /// The forward pin (Q2's seam, named as designed-forward choice,
    /// not today's-behavior debt): at equal clocks `Some(author)`
    /// outranks `None`, deterministically across permutations.
    #[test]
    fn some_author_outranks_none_at_equal_clock_forward_pins_b_full() {
        let author = AuthorId::parse("0193a1b2-c3d4-7cc9-8a2e-5f1e2d3c4b5a").expect("parses");
        let key = "0193b4c5-d6e7-7c89-ab01-2345abcdef01";
        let c = DateTime::from_timestamp(1_800_000_000, 0).expect("clock");
        let authored = doc_with_journal(key, entry(json!({"v": "authored"}), c, Some(author)));
        let authorless = doc_with_journal(key, entry(json!({"v": "authorless"}), c, None));
        let merged = fold_all(&[authorless, authored]);
        let rows = merged.sections.journal.as_ref().expect("journal present");
        let UserRecord::Entry {
            author: winner,
            body,
            ..
        } = &rows[key]
        else {
            panic!("an entry wins");
        };
        assert_eq!(*winner, Some(author));
        assert_eq!(body["v"], json!("authored"));
        let reversed = fold_all(&[
            doc_with_journal(key, entry(json!({"v": "authored"}), c, Some(author))),
            doc_with_journal(key, entry(json!({"v": "authorless"}), c, None)),
        ]);
        assert_eq!(reversed, merged);
    }

    #[test]
    fn empty_input_is_total() {
        let merged = fold(std::iter::empty());
        assert_eq!(
            merged,
            UserDocument {
                format: DOCUMENT_FORMAT,
                sections: UserSections::default(),
                unknown: BTreeMap::new(),
            }
        );
    }

    /// Document-attribution freedom: re-publishing a merged view from
    /// a third device changes nothing (the idempotence journey a fleet
    /// actually exercises).
    #[test]
    fn republished_merged_view_folds_identically() {
        let key = "0193b4c5-d6e7-7c89-ab01-2345abcdef01";
        let c = DateTime::from_timestamp(1_800_000_000, 0).expect("clock");
        let a = doc_with_journal(key, entry(json!({"v": 1}), c, None));
        let b = doc_with_journal(
            key,
            UserRecord::Tombstone {
                clock: DateTime::from_timestamp(1_800_000_001, 0).expect("clock"),
                author: None,
            },
        );
        let merged = fold_all(&[a.clone(), b]);
        // A third device republishes the merged view beside `a`'s
        // original: nothing changes.
        let again = fold_all(&[merged.clone(), a]);
        assert_eq!(again, merged);
    }
}
