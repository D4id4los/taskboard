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

/// An arbitrary [`StateCommand`]: ids are drawn from the full space (they
/// may reference entities that do not exist in the state under test —
/// exercising the typed-rejection paths), payloads are bounded strings
/// and sane orders.
pub fn state_command_strategy() -> impl Strategy<Value = crate::messages::StateCommand> {
    use crate::messages::StateCommand;

    prop_oneof![
        // 10: task commands
        (
            string_strategy(),
            super::stack_id_strategy(),
            -100_i64..=100
        )
            .prop_map(|(title, stack, order)| StateCommand::CreateTask {
                title,
                stack,
                order
            },),
        (super::task_id_strategy(), super::task_changes_strategy())
            .prop_map(|(id, changes)| { StateCommand::UpdateTask { id, changes } }),
        (super::task_id_strategy(), any::<bool>())
            .prop_map(|(id, done)| StateCommand::SetTaskDone { id, done }),
        (
            super::task_id_strategy(),
            super::stack_id_strategy(),
            -100_i64..=100
        )
            .prop_map(|(id, stack, order)| StateCommand::MoveTask { id, stack, order },),
        super::task_id_strategy().prop_map(|id| StateCommand::DeleteTask { id }),
        // 5: stack commands
        (string_strategy(), -100_i64..=100)
            .prop_map(|(title, order)| StateCommand::CreateStack { title, order }),
        (super::stack_id_strategy(), string_strategy())
            .prop_map(|(id, new_title)| StateCommand::RenameStack { id, new_title }),
        super::stack_id_strategy().prop_map(|id| StateCommand::DeleteStack { id }),
        // 3: label commands
        (string_strategy(), string_strategy()).prop_map(|(title, color)| {
            StateCommand::CreateLabel {
                title,
                color: crate::entities::Color::new(color),
            }
        }),
        (super::label_id_strategy(), super::label_changes_strategy())
            .prop_map(|(id, changes)| StateCommand::UpdateLabel { id, changes }),
        super::label_id_strategy().prop_map(|id| StateCommand::DeleteLabel { id }),
        (super::task_id_strategy(), super::label_id_strategy())
            .prop_map(|(task, label)| StateCommand::AssignLabel { task, label }),
        (super::task_id_strategy(), super::label_id_strategy())
            .prop_map(|(task, label)| StateCommand::UnassignLabel { task, label }),
        // 1: sync request
        Just(StateCommand::RequestSync),
    ]
}
