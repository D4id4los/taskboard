// SPDX-License-Identifier: MIT OR Apache-2.0
//! Time-tracking strategies (DR-5/1): entries with spans valid by
//! construction (start instant + positive offset; the open subset
//! arbitrary). Rejection paths are unit tests — the strategies feed
//! only the accepted sign domain, like `snooze_postpone_strategy`.

use proptest::prelude::*;

use super::task_id_strategy;
use crate::tracking::TimeEntry;

/// A random time entry; closed entries carry a strictly positive span,
/// open entries (`end: None`) accrue from `start`.
///
/// # Panics
///
/// Panics only if chrono cannot represent the bounded span offset —
/// a programming error, not an input condition.
pub fn time_entry_strategy() -> impl Strategy<Value = TimeEntry> {
    (
        any::<u128>(),
        task_id_strategy(),
        super::string_strategy(),
        super::timestamp_strategy(),
        1_i64..=86_400,
        prop::option::of(Just(())),
        super::timestamp_strategy(),
    )
        .prop_map(|(raw, task, title, start, span_secs, open, updated_at)| {
            let end = open
                .map(|()| start + chrono::TimeDelta::try_seconds(span_secs).expect("bounded span"));
            TimeEntry {
                id: crate::ids::TimeEntryId::from(uuid::Uuid::from_u128(raw)),
                task,
                task_title: title,
                start,
                end,
                created_at: updated_at,
                updated_at,
            }
        })
}

/// An arbitrary [`crate::tracking::TrackingState`]: 0..=4 entries with
/// valid spans; the open subset is arbitrary (the soft invariant's
/// transient shapes included — properties pin convergence from them).
pub fn tracking_state_strategy() -> impl Strategy<Value = crate::tracking::TrackingState> {
    prop::collection::vec(time_entry_strategy(), 0..=4).prop_map(|entries| {
        let mut state = crate::tracking::TrackingState::default();
        for entry in entries {
            state.entries.insert(entry.id, entry);
        }
        state
    })
}
