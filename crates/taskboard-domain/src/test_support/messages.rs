// SPDX-License-Identifier: MIT OR Apache-2.0
//! Strategies for the inter-actor message changesets.

use proptest::prelude::*;

use super::{string_strategy, timestamp_strategy};

/// Sparse task changes.
pub fn task_changes_strategy() -> impl Strategy<Value = crate::messages::TaskChanges> {
    (
        prop::option::of(string_strategy()),
        prop::option::of(string_strategy()),
        prop::option::of(prop::option::of(timestamp_strategy())),
        prop::option::of(any::<bool>()),
    )
        .prop_map(
            |(title, description, duedate, archived)| crate::messages::TaskChanges {
                title,
                description,
                duedate,
                archived,
            },
        )
}

/// Sparse label changes.
pub fn label_changes_strategy() -> impl Strategy<Value = crate::messages::LabelChanges> {
    (
        prop::option::of(string_strategy()),
        prop::option::of(string_strategy().prop_map(crate::entities::Color::new)),
    )
        .prop_map(|(title, color)| crate::messages::LabelChanges { title, color })
}
