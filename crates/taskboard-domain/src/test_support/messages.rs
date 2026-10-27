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
        prop::option::of(prop::option::of(timestamp_strategy())),
        prop::option::of(prop::option::of(super::entities::effort_strategy())),
        prop::option::of(prop::option::of(super::entities::importance_strategy())),
        prop::option::of(prop::option::of(super::entities::urgency_strategy())),
        prop::option::of(any::<bool>()),
    )
        .prop_map(
            |(title, description, duedate, scheduled, effort, importance, urgency, archived)| {
                crate::messages::TaskChanges {
                    title,
                    description,
                    duedate,
                    scheduled,
                    effort,
                    importance,
                    urgency,
                    recur: None,
                    archived,
                }
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
/// # Panics
///
/// Panics only if chrono cannot represent the bounded span offsets —
/// a programming error, not an input condition.
#[allow(clippy::too_many_lines)] // one strategy arm per verb family
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
        (
            super::board_id_strategy(),
            string_strategy(),
            -100_i64..=100,
        )
            .prop_map(|(board, title, order)| StateCommand::CreateStack {
                board,
                title,
                order
            },),
        (super::stack_id_strategy(), string_strategy())
            .prop_map(|(id, new_title)| StateCommand::RenameStack { id, new_title }),
        super::stack_id_strategy().prop_map(|id| StateCommand::DeleteStack { id }),
        (super::stack_id_strategy(), -100_i64..=100)
            .prop_map(|(id, order)| StateCommand::MoveStack { id, order }),
        // 3: label commands
        (
            super::board_id_strategy(),
            string_strategy(),
            string_strategy(),
        )
            .prop_map(|(board, title, color)| {
                StateCommand::CreateLabel {
                    board,
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
        // 2: board commands
        string_strategy().prop_map(|title| StateCommand::CreateBoard {
            title,
            color: crate::entities::Color::new("3366cc"),
        }),
        (super::board_id_strategy(), string_strategy())
            .prop_map(|(board, title)| StateCommand::RenameBoard { board, title }),
        (
            super::board_id_strategy(),
            super::remote_board_id_strategy(),
            string_strategy(),
        )
            .prop_map(|(board, remote, connector)| StateCommand::BindBoard {
                board,
                remote,
                connector: crate::ids::ConnectorId::from(connector),
            }),
        // 2: reminder commands
        (
            super::reminders::reminder_ref_strategy(),
            super::timestamp_strategy(),
            snooze_postpone_strategy(),
        )
            .prop_map(
                |(reminder, occurrence, postpone)| StateCommand::SnoozeReminder {
                    reminder,
                    occurrence,
                    postpone,
                }
            ),
        (super::task_id_strategy(), any::<bool>())
            .prop_map(|(task, muted)| StateCommand::SetTaskRemindersMuted { task, muted }),
        (super::task_id_strategy(), reminder_offset_strategy())
            .prop_map(|(task, offset)| StateCommand::AddReminderRule { task, offset }),
        super::task_id_strategy().prop_map(|task| StateCommand::ClearReminderRules { task }),
        // 5: tracking commands (DR-5/1)
        super::task_id_strategy().prop_map(|task| StateCommand::StartTracking { task }),
        Just(StateCommand::StopTracking),
        (
            super::task_id_strategy(),
            super::timestamp_strategy(),
            1_i64..=86_400,
        )
            .prop_map(|(task, start, span)| {
                // Valid spans only: the rejection paths are unit tests
                // (the strategy feeds the accepted sign domain, like the
                // snooze postpone).
                StateCommand::LogTime {
                    task,
                    start,
                    end: start + chrono::TimeDelta::try_seconds(span).unwrap(),
                }
            }),
        (
            super::time_entry_id_strategy(),
            prop::option::of(super::timestamp_strategy()),
            prop::option::of(super::timestamp_strategy()),
        )
            .prop_map(|(id, start, end)| StateCommand::AdjustTimeEntry { id, start, end }),
        super::time_entry_id_strategy().prop_map(|id| StateCommand::DeleteTimeEntry { id }),
        // 1: sync request
        Just(StateCommand::RequestSync),
    ]
}

/// A bounded positive postpone (1 s … 24 h): the planner rejects
/// non-positive ones, so the strategy feeds only the accepted sign domain.
fn snooze_postpone_strategy() -> impl Strategy<Value = chrono::TimeDelta> {
    (1_i64..=86_400).prop_map(|secs| chrono::TimeDelta::try_seconds(secs).unwrap())
}

/// A due-anchored reminder offset (−24 h … 0): `ReminderOffset` admits
/// only non-positive durations, so the strategy feeds only its sign
/// domain.
fn reminder_offset_strategy() -> impl Strategy<Value = crate::reminders::ReminderOffset> {
    (-86_400_i64..=0).prop_map(|secs| {
        crate::reminders::ReminderOffset::before(chrono::TimeDelta::try_seconds(secs).unwrap())
            .unwrap()
    })
}
