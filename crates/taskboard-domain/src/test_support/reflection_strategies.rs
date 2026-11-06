// SPDX-License-Identifier: MIT OR Apache-2.0
//! Reflection-entry strategies (DR-8 chunk 3): whole entries and the
//! import-fold's decoded wire rows. Shared with the sync-files codec
//! roundtrip and the engine journeys through the `test-support`
//! feature.

use std::collections::BTreeSet;

use chrono::NaiveDate;
use proptest::prelude::*;

use crate::ids::ReflectionId;
use crate::messages::ImportedReflection;
use crate::reflection::{HealthFlag, Rating, ReflectionAttributes, ReflectionEntry, ReviewCadence};

use super::{all_health_flag_names, reflection_id_strategy, string_strategy, timestamp_strategy};

/// A rating in the mint's range, or unanswered.
///
/// # Panics
///
/// Never: the generated range is the mint's own.
pub fn rating_strategy() -> impl Strategy<Value = Option<Rating>> {
    proptest::option::of((1_u8..=5).prop_map(|n| Rating::try_new(n).expect("in range")))
}

/// Attributes over the curated flag set, with a few parked unknown
/// names (the tolerant-decode shape a newer build could have written).
///
/// # Panics
///
/// Never: every drawn flag name is a ruled member.
pub fn reflection_attributes_strategy() -> impl Strategy<Value = ReflectionAttributes> {
    (
        rating_strategy(),
        rating_strategy(),
        proptest::collection::btree_set(
            proptest::sample::select(all_health_flag_names())
                .prop_map(|name| HealthFlag::parse_text_form(name).expect("ruled name")),
            0..4,
        ),
        proptest::collection::btree_set("[a-z]{3,8}-[a-z]{2,6}", 0..2),
    )
        .prop_map(|(mood, energy, flags, extra_flags)| ReflectionAttributes {
            mood,
            energy,
            flags,
            extra_flags,
        })
}

fn cadence_strategy() -> impl Strategy<Value = ReviewCadence> {
    proptest::sample::select(&[
        ReviewCadence::Day,
        ReviewCadence::Week,
        ReviewCadence::Month,
    ])
}

/// A whole entry with an arbitrary id. The window is structurally
/// unconstrained (the import never re-mints windows — stored rows are
/// data); `updated_at >= created_at`.
pub fn reflection_entry_strategy() -> impl Strategy<Value = ReflectionEntry> {
    reflection_id_strategy().prop_flat_map(reflection_entry_with_id)
}

/// A whole entry for a fixed id (the per-id competition shape).
///
/// # Panics
///
/// Never: the date and duration arithmetic is bounded by the ranges.
pub fn reflection_entry_with_id(id: ReflectionId) -> impl Strategy<Value = ReflectionEntry> {
    (
        cadence_strategy(),
        1_u64..=40_000,
        timestamp_strategy(),
        string_strategy(),
        reflection_attributes_strategy(),
        timestamp_strategy(),
        0_i64..=86_400 * 30,
        any::<bool>(),
    )
        .prop_map(
            move |(
                cadence,
                day_offset,
                window_start,
                text,
                attributes,
                created_at,
                bump,
                deleted,
            )| {
                let day = NaiveDate::from_ymd_opt(1972, 1, 1)
                    .expect("epoch date")
                    .checked_add_days(chrono::Days::new(day_offset))
                    .expect("bounded");
                ReflectionEntry {
                    id,
                    cadence,
                    day,
                    window_start,
                    window_end: window_start + chrono::TimeDelta::try_hours(24).expect("24h"),
                    text,
                    attributes,
                    created_at,
                    updated_at: created_at + chrono::TimeDelta::try_seconds(bump).expect("bounded"),
                    deleted,
                }
            },
        )
}

/// One decoded wire row for a fixed id: a live entry or a tombstone.
pub fn imported_reflection_with_id(id: ReflectionId) -> impl Strategy<Value = ImportedReflection> {
    prop_oneof![
        3 => reflection_entry_with_id(id).prop_map(|mut entry| {
            entry.deleted = false;
            ImportedReflection::Entry(entry)
        }),
        1 => timestamp_strategy().prop_map(move |clock| ImportedReflection::Tombstone { id, clock }),
    ]
}

/// A report's rows: a small id pool so rows compete per id (repeats
/// across the vector are deliberate — the fold is order-free only for
/// unique ids, and the laws state which shape they need).
pub fn imported_reflections_strategy() -> impl Strategy<Value = Vec<ImportedReflection>> {
    proptest::collection::vec(
        (0_u128..6).prop_flat_map(|raw| {
            imported_reflection_with_id(ReflectionId::from(uuid::Uuid::from_u128(raw + 1)))
        }),
        0..8,
    )
}

/// A set of unique-id rows (the order-free law's input shape).
pub fn unique_imported_reflections_strategy() -> impl Strategy<Value = Vec<ImportedReflection>> {
    proptest::collection::btree_set(0_u128..6, 0..6).prop_flat_map(|ids| {
        ids.into_iter()
            .map(|raw| {
                imported_reflection_with_id(ReflectionId::from(uuid::Uuid::from_u128(raw + 1)))
            })
            .collect::<Vec<_>>()
    })
}

/// A reflection home over the same id pool the row strategies draw
/// from, so imports hit present and absent ids alike.
pub fn reflection_home_strategy() -> impl Strategy<Value = crate::reflection::ReflectionState> {
    proptest::collection::btree_set(0_u128..6, 0..6)
        .prop_flat_map(|ids| {
            ids.into_iter()
                .map(|raw| {
                    reflection_entry_with_id(ReflectionId::from(uuid::Uuid::from_u128(raw + 1)))
                })
                .collect::<Vec<_>>()
        })
        .prop_map(|entries| crate::reflection::ReflectionState {
            entries: entries.into_iter().map(|entry| (entry.id, entry)).collect(),
        })
}

/// The ids every pooled strategy above can draw (for assertions).
#[must_use]
pub fn pooled_reflection_ids() -> BTreeSet<ReflectionId> {
    (0_u128..6)
        .map(|raw| ReflectionId::from(uuid::Uuid::from_u128(raw + 1)))
        .collect()
}
