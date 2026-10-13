// SPDX-License-Identifier: MIT OR Apache-2.0
//! Reminder strategies: standalone rows and the referentially-consistent
//! builder that assembles a normalized [`ReminderState`] over a generated
//! task map (the `build_outbox` precedent).
//!
//! Generated states satisfy the persisted invariants: at most one
//! delivery row per `(reminder, occurrence)` key (first generated wins)
//! and the snooze-chain compaction of the persistence post-pass — so
//! every strategy-fed property sees only invariant-satisfying states.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, TimeDelta, Utc};
use proptest::prelude::*;

use super::{app_state_strategy, task_id_strategy, task_strategy, timestamp_strategy};
use crate::entities::Task;
use crate::ids::{ReminderRuleId, TaskId};
use crate::reminders::{
    ReminderDelivery, ReminderOffset, ReminderRef, ReminderRule, ReminderSnooze, ReminderSnoozeKey,
    ReminderState,
};
use crate::state::AppState;

/// A non-positive offset in `≤ −30 d … 0`: exercises the sign domain
/// without chrono extremes.
///
/// # Panics
///
/// Never: the range bounds are valid `TimeDelta` second counts.
pub fn reminder_offset_strategy() -> impl Strategy<Value = ReminderOffset> {
    (-2_592_000_i64..=0)
        .prop_map(|secs| ReminderOffset::before(TimeDelta::try_seconds(secs).unwrap()).unwrap())
}

/// A random rule id.
pub fn reminder_rule_id_strategy() -> impl Strategy<Value = ReminderRuleId> {
    any::<u128>().prop_map(|raw| ReminderRuleId::from(uuid::Uuid::from_u128(raw)))
}

/// A reminder pointing at an explicit rule or a task's implicit default.
pub fn reminder_ref_strategy() -> impl Strategy<Value = ReminderRef> {
    prop_oneof![
        reminder_rule_id_strategy().prop_map(ReminderRef::Rule),
        task_id_strategy().prop_map(ReminderRef::Default),
    ]
}

/// A standalone rule row (not referentially bound — the `task_strategy`
/// convention; the builder binds).
pub fn reminder_rule_strategy() -> impl Strategy<Value = ReminderRule> {
    (
        reminder_rule_id_strategy(),
        task_id_strategy(),
        reminder_offset_strategy(),
        timestamp_strategy(),
    )
        .prop_map(|(id, task, offset, created_at)| ReminderRule {
            id,
            task,
            offset,
            created_at,
        })
}

/// A standalone delivery row.
pub fn reminder_delivery_strategy() -> impl Strategy<Value = ReminderDelivery> {
    (
        reminder_ref_strategy(),
        timestamp_strategy(),
        timestamp_strategy(),
    )
        .prop_map(|(reminder, occurrence, delivered_at)| ReminderDelivery {
            reminder,
            occurrence,
            delivered_at,
        })
}

/// A standalone snooze row.
pub fn reminder_snooze_strategy() -> impl Strategy<Value = ReminderSnooze> {
    (
        reminder_ref_strategy(),
        timestamp_strategy(),
        timestamp_strategy(),
    )
        .prop_map(|(reminder, occurrence, rescheduled_to)| ReminderSnooze {
            reminder,
            occurrence,
            rescheduled_to,
        })
}

/// A non-empty task map (the referential anchor of [`build_reminders`]).
pub fn task_map_strategy() -> impl Strategy<Value = BTreeMap<TaskId, Task>> {
    prop::collection::vec(task_strategy(), 1..8)
        .prop_map(|tasks| tasks.into_iter().map(|task| (task.id, task)).collect())
}

/// An `AppState` paired with a referentially-consistent reminder section
/// over its tasks — the evaluation-proptest input while the section
/// travels beside the state (the `AppState` commit folds it into
/// `app_state_strategy`).
pub fn app_state_with_reminders_strategy() -> impl Strategy<Value = (AppState, ReminderState)> {
    (app_state_strategy(), reminder_rows_strategy()).prop_map(|(state, rows)| {
        let reminders = build_reminders(&state.tasks, rows);
        (state, reminders)
    })
}

/// One raw reminder row; resolved against a generated task map by
/// [`build_reminders`].
#[derive(Debug, Clone)]
pub struct ReminderRow {
    /// Row kind: 0 = rule, 1 = delivery, 2 = snooze, 3 = mute.
    pub kind: usize,
    /// Rule id (rule rows) / part of the reminder pick (delivery, snooze).
    pub rule: ReminderRuleId,
    /// Task anchor: the rule's task, the `Default` ref's task, or the
    /// muted task.
    pub task: TaskId,
    /// Rule offset (rule rows).
    pub offset: ReminderOffset,
    /// Rule creation stamp (rule rows).
    pub created_at: DateTime<Utc>,
    /// Reminder pick: even → a generated rule, odd → `Default` of a
    /// generated task (delivery and snooze rows).
    pub pick: usize,
    /// Occurrence (delivery and snooze rows).
    pub occurrence: DateTime<Utc>,
    /// Delivery stamp (delivery rows).
    pub delivered_at: DateTime<Utc>,
    /// Snooze target (snooze rows).
    pub rescheduled_to: DateTime<Utc>,
}

/// Raw reminder rows: `0..4` kinds × a handful of rows each.
pub fn reminder_rows_strategy() -> impl Strategy<Value = Vec<ReminderRow>> {
    prop::collection::vec(
        (
            prop_oneof![Just(0_usize), Just(1), Just(2), Just(3)],
            reminder_rule_id_strategy(),
            task_id_strategy(),
            reminder_offset_strategy(),
            timestamp_strategy(),
            any::<usize>(),
            timestamp_strategy(),
            timestamp_strategy(),
            timestamp_strategy(),
        ),
        0..12,
    )
    .prop_map(|rows| {
        rows.into_iter()
            .map(
                |(
                    kind,
                    rule,
                    task,
                    offset,
                    created_at,
                    pick,
                    occurrence,
                    delivered_at,
                    rescheduled_to,
                )| {
                    ReminderRow {
                        kind,
                        rule,
                        task,
                        offset,
                        created_at,
                        pick,
                        occurrence,
                        delivered_at,
                        rescheduled_to,
                    }
                },
            )
            .collect()
    })
}

/// Assembles the reminder section over generated tasks: rule rows anchor
/// on the picked task; delivery/snooze rows reference an existing rule or
/// the `Default` of an existing task; muted tasks ⊆ generated tasks.
/// Normalizes to the persisted invariants (≤1 delivery per key, first
/// wins; compacted snooze chains).
#[must_use]
pub fn build_reminders(tasks: &BTreeMap<TaskId, Task>, rows: Vec<ReminderRow>) -> ReminderState {
    let task_ids: Vec<TaskId> = tasks.keys().copied().collect();
    let mut state = ReminderState::default();
    if task_ids.is_empty() {
        return state;
    }
    let mut rule_ids: Vec<ReminderRuleId> = Vec::new();
    for row in rows {
        match row.kind {
            0 => {
                if let Some(&task) = task_ids.get(row.pick % task_ids.len()) {
                    let rule = ReminderRule {
                        id: row.rule,
                        task,
                        offset: row.offset,
                        created_at: row.created_at,
                    };
                    rule_ids.push(rule.id);
                    state.rules.insert(rule.id, rule);
                }
            }
            1 | 2 => {
                let Some(reminder) = pick_ref(&rule_ids, &task_ids, row.pick) else {
                    continue;
                };
                let occurrence = row.occurrence;
                if row.kind == 1 {
                    let delivery = ReminderDelivery {
                        reminder,
                        occurrence,
                        delivered_at: row.delivered_at,
                    };
                    if !state.is_delivered(&reminder, &occurrence) {
                        state.deliveries.insert(delivery);
                    }
                } else {
                    state.snoozes.insert(
                        ReminderSnoozeKey {
                            reminder,
                            occurrence,
                        },
                        row.rescheduled_to,
                    );
                }
            }
            _ => {
                state.muted.insert(task_ids[row.pick % task_ids.len()]);
            }
        }
    }
    compact(&mut state);
    state
}

/// Picks `Rule` of a generated rule for even picks, `Default` of a
/// generated task otherwise; `None` when the anchor side is empty.
fn pick_ref(rule_ids: &[ReminderRuleId], task_ids: &[TaskId], pick: usize) -> Option<ReminderRef> {
    if pick.is_multiple_of(2) {
        rule_ids
            .get(pick % rule_ids.len().max(1))
            .copied()
            .map(ReminderRef::Rule)
    } else {
        task_ids
            .get(pick % task_ids.len())
            .copied()
            .map(ReminderRef::Default)
    }
}

/// Mirrors the persistence post-pass: prunes links whose source and
/// target occurrences are both delivered (the invariant the strategy
/// world guarantees; the action world derives).
fn compact(state: &mut ReminderState) {
    let keep: BTreeSet<ReminderSnoozeKey> = state
        .snoozes
        .iter()
        .filter(|(key, target)| {
            state.is_delivered(&key.reminder, &key.occurrence)
                && state.is_delivered(&key.reminder, target)
        })
        .map(|(key, _)| *key)
        .collect();
    state.snoozes.retain(|key, _| keep.contains(key));
}

#[cfg(test)]
mod tests {
    use super::*;

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(64))]

        #[test]
        fn build_reminders_never_references_missing_tasks(
            tasks in task_map_strategy(),
            rows in reminder_rows_strategy(),
        ) {
            let state = build_reminders(&tasks, rows);
            for rule in state.rules.values() {
                prop_assert!(tasks.contains_key(&rule.task));
            }
            for delivery in &state.deliveries {
                match delivery.reminder {
                    ReminderRef::Rule(id) => prop_assert!(state.rules.contains_key(&id)),
                    ReminderRef::Default(task) => prop_assert!(tasks.contains_key(&task)),
                }
            }
            for key in state.snoozes.keys() {
                match key.reminder {
                    ReminderRef::Rule(id) => prop_assert!(state.rules.contains_key(&id)),
                    ReminderRef::Default(task) => prop_assert!(tasks.contains_key(&task)),
                }
            }
            for task in &state.muted {
                prop_assert!(tasks.contains_key(task));
            }
        }
    }
}
