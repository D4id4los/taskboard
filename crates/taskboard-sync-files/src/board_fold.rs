// SPDX-License-Identifier: MIT OR Apache-2.0
//! The board fold (design D-BF7, plan §5.1): pure, total,
//! **input-order-free** — per `(kind, id)` the winner is the per-field
//! max over the total order `(rank, clock, author, content)`, exactly
//! the per-user fold's discipline ([`crate::fold`]) applied to entity
//! payloads.
//!
//! **Sequence numbers never influence any merge outcome.** They are GC
//! watermarks only ([`crate::board_gc`]); the fold never sees them —
//! the backbone's named design error (a fold that sorts by seq) is
//! unrepresentable here by construction and proptested as P3.
//!
//! The order legs:
//! - `rank`: a modeled clocked field outranks an unknown relay (the
//!   verbatim floor); two relays of one slot resolve by content.
//! - `clock`: the field's embedded write clock. Tombstones (`deleted`
//!   fields) compete exactly like any other field: a later delete
//!   beats an older edit, a later edit beats an older tombstone.
//! - `author`: `None < Some(_)` (the pinned D-BF2 leg).
//! - `content`: canonical-JSON bytes — the totaliser for the
//!   same-second two-writer tie.
//!
//! Membership records fold per membership key by `(clock, by)` (the
//! per-membership LWW the OR-set semantics need); unknown entity-level
//! keys merge by canonical-content max, and whole-entity verbatim
//! relays survive as an order-free set-union — republished by
//! snapshots, so an unknown shape never dies at an older reader.
//!
//! The fold is attribution-free and idempotent: re-folding a snapshot
//! changes nothing (the subsumption law). It is the wire-level max,
//! written once, generic over the four kinds — a per-kind fork of the
//! merge would be a design finding. (The D-BF9 "reuse, not fork" rule
//! binds chunk 5's *engine ingest*, which flows through the state
//! engine's existing per-field LWW pipeline; this substrate fold is
//! new wire-level code by design.)

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use uuid::Uuid;

use crate::board_payload::{EntityEntry, EntityKind, Field, MaterializedEntity};
use crate::ids::AuthorId;

/// A folded entity in the converged board state (the payload type is
/// the fold output's shape — encode and decode share one mapping).
pub type FoldedEntity = MaterializedEntity;

/// The converged board state: one winner per `(kind, id)`, plus the
/// whole-entity verbatim relays (never folded, republished by
/// snapshots).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BoardState {
    pub entities: BTreeMap<(EntityKind, Uuid), FoldedEntity>,
    /// Whole-entity relays, keyed by their canonical JSON (dedup,
    /// order-free set-union: two distinct relays are two distinct
    /// unknown entities — both survive, in any fold order).
    pub verbatim: BTreeMap<String, Value>,
}

impl Serialize for BoardState {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        // The entities map is tuple-keyed in memory (the fold's key);
        // on the wire each winner is its own self-describing entity
        // object (kind/id included), the relays a canonical-keyed
        // list — both orders free, so the serialized fold hashes
        // order-free by construction.
        let entities: Vec<&FoldedEntity> = self.entities.values().collect();
        let verbatim: Vec<&Value> = self.verbatim.values().collect();
        (entities, verbatim).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for BoardState {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let (entities, verbatim): (Vec<EntityEntry>, Vec<Value>) =
            Deserialize::deserialize(deserializer)?;
        let mut entries = entities;
        entries.extend(verbatim.into_iter().map(EntityEntry::Verbatim));
        Ok(fold_board(entries.iter()))
    }
}

/// Folds any set of chunk/snapshot entity entries into the converged
/// board state. Pure, total, input-order-free.
pub fn fold_board<'a>(entries: impl IntoIterator<Item = &'a EntityEntry>) -> BoardState {
    let mut state = BoardState::default();
    for entry in entries {
        match entry {
            EntityEntry::Modeled(entity) => {
                let key = (entity.kind, entity.id);
                match state.entities.get(&key) {
                    Some(existing) => {
                        let merged = merge_entity(existing, entity);
                        state.entities.insert(key, merged);
                    }
                    None => {
                        state.entities.insert(key, entity.clone());
                    }
                }
            }
            EntityEntry::Verbatim(value) => {
                state
                    .verbatim
                    .entry(canonical_key(value))
                    .or_insert_with(|| value.clone());
            }
        }
    }
    state
}

/// The per-entity merge: per field the max over the fold's total
/// order; memberships per key by `(clock, by)`; unknown keys by
/// canonical-content max.
fn merge_entity(
    existing: &MaterializedEntity,
    incoming: &MaterializedEntity,
) -> MaterializedEntity {
    let mut fields = existing.fields.clone();
    for (name, field) in &incoming.fields {
        match fields.get(name) {
            Some(current) if field_order(current) >= field_order(field) => {}
            _ => {
                fields.insert(name.clone(), field.clone());
            }
        }
    }
    let memberships = match (&existing.memberships, &incoming.memberships) {
        (Some(a), Some(b)) => Some(merge_memberships(a, b)),
        (None, Some(b)) => Some(b.clone()),
        (Some(a), None) => Some(a.clone()),
        (None, None) => None,
    };
    let mut unknown = existing.unknown.clone();
    for (key, value) in &incoming.unknown {
        match unknown.get(key) {
            Some(current) if content_key(current) >= content_key(value) => {}
            _ => {
                unknown.insert(key.clone(), value.clone());
            }
        }
    }
    MaterializedEntity {
        kind: existing.kind,
        id: existing.id,
        fields,
        memberships,
        unknown,
    }
}

/// Per-membership winner = max over `(clock, by, content)` — the
/// per-membership LWW; `added` rides the content leg (an identical
/// constant per state, so the residual tie resolves deterministically).
fn merge_memberships(
    existing: &BTreeMap<String, crate::board_payload::MembershipRecord>,
    incoming: &BTreeMap<String, crate::board_payload::MembershipRecord>,
) -> BTreeMap<String, crate::board_payload::MembershipRecord> {
    let mut merged = existing.clone();
    for (key, record) in incoming {
        match merged.get(key) {
            Some(current) if current.clock > record.clock => {}
            Some(current) if current.clock == record.clock && current.by >= record.by => {}
            _ => {
                merged.insert(key.clone(), *record);
            }
        }
    }
    merged
}

/// The total order key of a field slot: `(rank, clock, author, content)`.
fn field_order(field: &Field) -> (u8, DateTime<Utc>, Option<AuthorId>, Vec<u8>) {
    match field {
        Field::Value(fv) => (1, fv.clock, fv.by, content_key(&fv.value)),
        // The verbatim floor: rank 0, clock/author legs unused (equal
        // across all relays), the value itself the tiebreak.
        Field::Unknown(value) => (0, epoch(), None, content_key(value)),
    }
}

fn epoch() -> DateTime<Utc> {
    DateTime::from_timestamp(0, 0).expect("the epoch instant always converts")
}

/// The content totaliser: canonical serialisation bytes — a total,
/// deterministic order (the `fold.rs` leg, shared with the hash).
fn content_key(value: &Value) -> Vec<u8> {
    let canonical = crate::hash::canonicalise(value);
    serde_json::to_vec(&canonical).expect("canonical JSON always serializes")
}

fn canonical_key(value: &Value) -> String {
    String::from_utf8(content_key(value)).expect("canonical JSON is UTF-8")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board_payload::{FieldValue, MembershipRecord};
    use crate::hash::StateHash;
    use crate::test_support::{author_id, clock, json_value};
    use proptest::collection;
    use proptest::prelude::*;
    use serde_json::json;
    use taskboard_domain::ids::AuthorId;
    use taskboard_domain::test_support::proptest_config;

    const AUTHOR: &str = "0193a1b2-c3d4-7cc9-8a2e-5f1e2d3c4b5a";

    fn author() -> AuthorId {
        AuthorId::parse(AUTHOR).expect("parses")
    }

    fn entry(id: u128, fields: &[(&str, Value, DateTime<Utc>, Option<AuthorId>)]) -> EntityEntry {
        EntityEntry::Modeled(MaterializedEntity {
            kind: EntityKind::Task,
            id: Uuid::from_u128(id),
            fields: fields
                .iter()
                .map(|(name, value, clock, by)| {
                    (
                        (*name).to_owned(),
                        Field::Value(FieldValue {
                            value: value.clone(),
                            clock: *clock,
                            by: *by,
                        }),
                    )
                })
                .collect(),
            memberships: None,
            unknown: BTreeMap::new(),
        })
    }

    fn model(state: &BoardState, id: u128) -> &FoldedEntity {
        &state.entities[&(EntityKind::Task, Uuid::from_u128(id))]
    }

    /// A generated entity (task-shaped, a few clocked slots, optional
    /// author and tombstone) with a fresh clock stream — plus, at the
    /// strategy level, the adversarial (clock, author)-tie cases the
    /// laws must survive.
    fn entity_strategy(id: u128) -> impl Strategy<Value = EntityEntry> {
        (
            collection::vec(
                (clock(), proptest::option::of(author_id()), json_value()),
                1..=4,
            ),
            any::<bool>(),
        )
            .prop_map(move |(slots, deleted)| {
                let mut fields: BTreeMap<String, Field> = slots
                    .into_iter()
                    .enumerate()
                    .map(|(i, (clock, by, value))| {
                        (
                            format!("slot{i}"),
                            Field::Value(FieldValue { value, clock, by }),
                        )
                    })
                    .collect();
                fields.insert(
                    "deleted".to_owned(),
                    Field::Value(FieldValue {
                        value: json!(deleted),
                        clock: DateTime::from_timestamp(1_700_000_000, 0).expect("clock"),
                        by: None,
                    }),
                );
                EntityEntry::Modeled(MaterializedEntity {
                    kind: EntityKind::Task,
                    id: Uuid::from_u128(id),
                    fields,
                    memberships: None,
                    unknown: BTreeMap::new(),
                })
            })
    }

    fn entity_set() -> impl Strategy<Value = Vec<EntityEntry>> {
        collection::vec(
            prop_oneof![
                entity_strategy(0),
                entity_strategy(1),
                entity_strategy(2),
                entity_strategy(3),
                // Occasional whole-entity relays, so the relay legs of
                // every law are exercised too.
                json_value().prop_map(EntityEntry::Verbatim),
            ],
            0..8,
        )
    }

    fn fold_all(entries: &[EntityEntry]) -> BoardState {
        fold_board(entries.iter())
    }

    proptest! {
        #![proptest_config(proptest_config(256))]

        /// P3 — fold order-freedom: any permutation folds identically,
        /// state hash included (a seq-ordered or attribution-leaking
        /// fold cannot hold this).
        #[test]
        fn fold_is_commutative_including_the_state_hash(
            (entries, mut shuffled) in entity_set().prop_flat_map(|entries| {
                let len = entries.len();
                (Just(entries.clone()), collection::vec(any::<u64>(), len))
                    .prop_map(move |(entries, keys)| {
                        let mut order: Vec<usize> = (0..entries.len()).collect();
                        order.sort_by_key(|&i| keys[i]);
                        (entries.clone(), order.iter().map(|&i| entries[i].clone()).collect::<Vec<EntityEntry>>())
                    })
            }),
        ) {
            shuffled.reverse();
            let direct = fold_all(&entries);
            let shuffled = fold_all(&shuffled);
            prop_assert_eq!(&shuffled, &direct);
            prop_assert_eq!(StateHash::of_board(&shuffled), StateHash::of_board(&direct));
        }

        /// P4 — idempotence: re-folding a fold (the snapshot
        /// republish journey) changes nothing.
        #[test]
        fn fold_is_idempotent(entries in entity_set()) {
            let folded = fold_all(&entries);
            let folded_entries: Vec<EntityEntry> = folded
                .entities
                .values()
                .map(|e| EntityEntry::Modeled(e.clone()))
                .chain(folded.verbatim.values().map(|v| EntityEntry::Verbatim(v.clone())))
                .collect();
            prop_assert_eq!(&fold_board(folded_entries.iter()), &folded);
        }

        /// P4 — associativity: partial folds compose to the whole
        /// fold (chunk- and snapshot-granularity replay).
        #[test]
        fn fold_is_associative(entries in entity_set()) {
            prop_assume!(entries.len() >= 3);
            let entries_of = |state: &BoardState| {
                let mut out: Vec<EntityEntry> = state
                    .entities
                    .values()
                    .map(|e| EntityEntry::Modeled(e.clone()))
                    .collect();
                out.extend(state.verbatim.values().map(|v| EntityEntry::Verbatim(v.clone())));
                out
            };
            let ab = fold_all(&[entries[0].clone(), entries[1].clone()]);
            let mut left_input = entries_of(&ab);
            left_input.push(entries[2].clone());
            let left = fold_board(left_input.iter());
            let bc = fold_all(&[entries[1].clone(), entries[2].clone()]);
            let mut right_input = entries_of(&bc);
            right_input.push(entries[0].clone());
            let right = fold_board(right_input.iter());
            // Associativity is a law about the three operands: both
            // association orders equal the flat fold of the same
            // three.
            let all = fold_all(&entries[..3]);
            prop_assert_eq!(&left, &all);
            prop_assert_eq!(&right, &all);
        }

        /// P5 — tombstone duels: the later delete beats the older
        /// edit, the later edit beats the older tombstone, and the
        /// outcome survives any permutation.
        #[test]
        fn tombstone_duels_resolve_by_clock_not_input_order(
            c1 in clock(),
            c2 in clock(),
            v1 in json_value(),
        ) {
            prop_assume!(c1 != c2);
            // Two competing writes of the tombstone field itself
            // (delete vs. un-delete, from two devices).
            let delete = entry(1, &[("title", v1.clone(), c1, None), ("deleted", json!(true), c1, None)]);
            let undelete = entry(1, &[("deleted", json!(false), c2, None)]);
            let forward = fold_all(&[delete.clone(), undelete.clone()]);
            let backward = fold_all(&[undelete, delete]);
            prop_assert_eq!(&forward, &backward);
            // The edit's other field never vanishes (per-field duels,
            // not whole-entity overwrite).
            prop_assert!(model(&forward, 1).fields.contains_key("title"));
            // The `deleted` field resolves by clock: true iff the
            // tombstone wrote later.
            let deleted_is_true = match model(&forward, 1).fields["deleted"] {
                Field::Value(ref fv) => fv.value == json!(true),
                Field::Unknown(_) => false,
            };
            prop_assert_eq!(deleted_is_true, c1 > c2);
        }

        /// P6 — the author leg: `Some(by)` outranks `None` at equal
        /// clocks, deterministically, and the F1 stamp round-trips.
        #[test]
        fn some_author_outranks_none_at_equal_clocks(
            c in clock(),
            v1 in json_value(),
            v2 in json_value(),
        ) {
            prop_assume!(v1 != v2);
            let authorless = entry(1, &[("title", v1, c, None)]);
            let authored = entry(1, &[("title", v2.clone(), c, Some(author()))]);
            let direct = fold_all(&[authorless.clone(), authored.clone()]);
            let reversed = fold_all(&[authored.clone(), authorless.clone()]);
            prop_assert_eq!(&direct, &reversed);
            let winner = match model(&direct, 1).fields["title"] {
                Field::Value(ref fv) => (fv.by, fv.value.clone()),
                Field::Unknown(_) => panic!("a modeled slot wins"),
            };
            prop_assert_eq!(winner.0, Some(author()));
            prop_assert_eq!(winner.1, v2, "the authored value wins the duel");
        }

        /// Unknown slot relays survive the fold (republished by
        /// snapshots) and never displace modeled content.
        #[test]
        fn unknown_slot_relays_survive_and_floor(
            c in clock(),
            v in json_value(),
        ) {
            let modeled = entry(1, &[("title", v, c, None)]);
            let raw_relay = json!({ "quantum": "spin" });
            let mut relay_entity = MaterializedEntity {
                kind: EntityKind::Task,
                id: Uuid::from_u128(1),
                fields: BTreeMap::from([("quantum".to_owned(), Field::Unknown(raw_relay.clone()))]),
                memberships: None,
                unknown: BTreeMap::new(),
            };
            let folded = fold_all(&[
                modeled,
                EntityEntry::Modeled(relay_entity.clone()),
            ]);
            prop_assert!(matches!(model(&folded, 1).fields["title"], Field::Value(_)),
                "a modeled slot is never displaced by a relay");
            prop_assert!(matches!(model(&folded, 1).fields["quantum"], Field::Unknown(_)),
                "a relay slot survives the fold");
            // Two relays of one slot resolve by canonical-content max.
            let other = json!({"quantum": "orbital"});
            relay_entity.fields.insert("quantum".to_owned(), Field::Unknown(other.clone()));
            let relayed = fold_all(&[
                EntityEntry::Modeled(relay_entity),
                EntityEntry::Modeled(MaterializedEntity {
                    kind: EntityKind::Task,
                    id: Uuid::from_u128(1),
                    fields: BTreeMap::from([("quantum".to_owned(), Field::Unknown(raw_relay.clone()))]),
                    memberships: None,
                    unknown: BTreeMap::new(),
                }),
            ]);
            let expected =
                if content_key(&raw_relay) >= content_key(&other) { raw_relay } else { other };
            let Field::Unknown(winner) = &model(&relayed, 1).fields["quantum"] else {
                panic!("relay shape");
            };
            prop_assert_eq!(winner, &expected);
        }

        /// Membership records fold per key by `(clock, by)` — a
        /// per-membership LWW, not a per-entity one.
        #[test]
        fn memberships_fold_per_key(
            c1 in clock(),
            c2 in clock(),
        ) {
            prop_assume!(c1 != c2);
            let mk = |added: bool, clock: DateTime<Utc>| MembershipRecord {
                added,
                clock,
                by: None,
            };
            let mut a = base_entity(1);
            a.memberships = Some(BTreeMap::from([
                ("label-a".to_owned(), mk(true, c1)),
                ("label-b".to_owned(), mk(true, c1)),
            ]));
            let mut b = base_entity(1);
            b.memberships = Some(BTreeMap::from([
                ("label-a".to_owned(), mk(false, c2.max(c1))),
            ]));
            let folded = fold_all(&[EntityEntry::Modeled(a.clone()), EntityEntry::Modeled(b.clone())]);
            let winner = model(&folded, 1);
            let memberships = winner.memberships.as_ref().expect("memberships survive");
            prop_assert_eq!(memberships.len(), 2, "per-key union, never whole-map overwrite");
            prop_assert_eq!(memberships["label-a"].clock, c1.max(c2));
            prop_assert_eq!(memberships["label-b"].clock, c1);
        }

        /// Whole-entity verbatim relays merge order-free by
        /// canonical-content max and survive.
        #[test]
        fn verbatim_entries_set_union_order_free(
            v1 in json_value(),
            v2 in json_value(),
        ) {
            prop_assume!(content_key(&v1) != content_key(&v2));
            let direct = fold_all(&[EntityEntry::Verbatim(v1.clone()), EntityEntry::Verbatim(v2.clone())]);
            let reversed = fold_all(&[EntityEntry::Verbatim(v2.clone()), EntityEntry::Verbatim(v1.clone())]);
            prop_assert_eq!(&direct, &reversed);
            // Set-union semantics: two distinct relays are two
            // distinct unknown entities — both survive, keyed by their
            // canonical JSON; a duplicated relay dedups to one.
            prop_assert_eq!(direct.verbatim.len(), 2);
            prop_assert!(direct.verbatim.contains_key(&canonical_key(&v1)));
            prop_assert!(direct.verbatim.contains_key(&canonical_key(&v2)));
            let doubled = fold_all(&[
                EntityEntry::Verbatim(v1.clone()),
                EntityEntry::Verbatim(v1.clone()),
            ]);
            prop_assert_eq!(doubled.verbatim.len(), 1, "identical relays dedup");
        }
    }

    fn base_entity(id: u128) -> MaterializedEntity {
        MaterializedEntity {
            kind: EntityKind::Task,
            id: Uuid::from_u128(id),
            fields: BTreeMap::new(),
            memberships: None,
            unknown: BTreeMap::new(),
        }
    }
}
