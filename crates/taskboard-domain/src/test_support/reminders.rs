// SPDX-License-Identifier: MIT OR Apache-2.0
//! Reminder strategies: standalone rows and the referentially-consistent
//! builder that assembles a normalized [`ReminderState`] over a generated
//! task map (the `build_outbox` precedent).
//!
//! Generated states satisfy the persisted invariants: at most one
//! delivery row per `(reminder, occurrence)` key (first generated wins)
//! and the snooze-chain compaction of the persistence post-pass — so
//! every strategy-fed property sees only invariant-satisfying states.

use std::collections::BTreeMap;

use chrono::{DateTime, TimeDelta, Utc};
use proptest::prelude::*;

use super::{task_id_strategy, task_strategy, timestamp_strategy};
use crate::entities::Task;
use crate::ids::{AdhocAlarmId, ReminderRuleId, TaskId};
use crate::reminders::{
    AdhocAlarm, Insistence, ReminderDelivery, ReminderOffset, ReminderRef, ReminderRule,
    ReminderSnooze, ReminderSnoozeKey, ReminderState,
};

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

/// A random insistence rung (the flavour ladder's two levels).
pub fn insistence_strategy() -> impl proptest::strategy::Strategy<Value = Insistence> {
    proptest::prop_oneof![
        proptest::strategy::Just(Insistence::Normal),
        proptest::strategy::Just(Insistence::Alarm),
    ]
}

/// A random adhoc-alarm id.
pub fn adhoc_alarm_id_strategy() -> impl Strategy<Value = AdhocAlarmId> {
    any::<u128>().prop_map(|raw| AdhocAlarmId::from(uuid::Uuid::from_u128(raw)))
}

/// A reminder pointing at an explicit rule, a task's implicit default,
/// or a standalone adhoc alarm.
pub fn reminder_ref_strategy() -> impl Strategy<Value = ReminderRef> {
    prop_oneof![
        reminder_rule_id_strategy().prop_map(ReminderRef::Rule),
        task_id_strategy().prop_map(ReminderRef::Default),
        adhoc_alarm_id_strategy().prop_map(ReminderRef::Adhoc),
    ]
}

/// A standalone adhoc alarm row (label from a short alphanumeric string;
/// referentially self-contained — no task anchor).
///
/// # Panics
///
/// Only via `expect` on the constant label regex: a compile-time-known
/// valid pattern.
pub fn adhoc_alarm_strategy() -> impl Strategy<Value = AdhocAlarm> {
    (
        adhoc_alarm_id_strategy(),
        proptest::string::string_regex("[a-z0-9]{0,10}").expect("valid regex"),
        timestamp_strategy(),
        insistence_strategy(),
        timestamp_strategy(),
    )
        .prop_map(|(id, label, fire_at, insistence, created_at)| AdhocAlarm {
            id,
            label,
            fire_at,
            insistence,
            created_at,
        })
}

/// A standalone rule row (not referentially bound — the `task_strategy`
/// convention; the builder binds).
pub fn reminder_rule_strategy() -> impl Strategy<Value = ReminderRule> {
    (
        reminder_rule_id_strategy(),
        task_id_strategy(),
        reminder_offset_strategy(),
        insistence_strategy(),
        timestamp_strategy(),
    )
        .prop_map(|(id, task, offset, insistence, created_at)| ReminderRule {
            id,
            task,
            offset,
            insistence,
            created_at,
            updated_at: created_at,
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

/// One raw reminder row; resolved against a generated task map by
/// [`build_reminders`].
#[derive(Debug, Clone)]
pub struct ReminderRow {
    /// Row kind: 0 = rule, 1 = delivery, 2 = snooze, 3 = mute,
    /// 4 = adhoc alarm, 5 = dismissal.
    pub kind: usize,
    /// Rule id (rule rows) / part of the reminder pick (delivery, snooze).
    pub rule: ReminderRuleId,
    /// Task anchor: the rule's task, the `Default` ref's task, or the
    /// muted task.
    pub task: TaskId,
    /// Alarm id (alarm rows) / part of the reminder pick (delivery,
    /// snooze).
    pub alarm: AdhocAlarmId,
    /// Rule offset (rule rows).
    pub offset: ReminderOffset,
    /// Rule creation stamp (rule rows) / alarm creation stamp (alarm
    /// rows).
    pub created_at: DateTime<Utc>,
    /// Reminder pick: `pick % 3` — 0 → a generated rule, 1 → `Default`
    /// of a generated task, 2 → a generated alarm (delivery and snooze
    /// rows).
    pub pick: usize,
    /// Occurrence (delivery and snooze rows) / fire-at instant (alarm
    /// rows).
    pub occurrence: DateTime<Utc>,
    /// Delivery stamp (delivery rows).
    pub delivered_at: DateTime<Utc>,
    /// Snooze target (snooze rows).
    pub rescheduled_to: DateTime<Utc>,
    /// Alarm label (alarm rows).
    pub label: String,
}

/// Raw reminder rows: `0..=5` kinds × a handful of rows each.
///
/// # Panics
///
/// Only via `expect` on the constant label regex: a compile-time-known
/// valid pattern.
pub fn reminder_rows_strategy() -> impl Strategy<Value = Vec<ReminderRow>> {
    prop::collection::vec(
        (
            prop_oneof![Just(0_usize), Just(1), Just(2), Just(3), Just(4), Just(5)],
            reminder_rule_id_strategy(),
            task_id_strategy(),
            adhoc_alarm_id_strategy(),
            reminder_offset_strategy(),
            timestamp_strategy(),
            any::<usize>(),
            timestamp_strategy(),
            timestamp_strategy(),
            timestamp_strategy(),
            proptest::string::string_regex("[a-z0-9]{0,10}").expect("valid regex"),
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
                    alarm,
                    offset,
                    created_at,
                    pick,
                    occurrence,
                    delivered_at,
                    rescheduled_to,
                    label,
                )| {
                    ReminderRow {
                        kind,
                        rule,
                        task,
                        alarm,
                        offset,
                        created_at,
                        pick,
                        occurrence,
                        delivered_at,
                        rescheduled_to,
                        label,
                    }
                },
            )
            .collect()
    })
}

/// Assembles the reminder section over generated tasks: rule rows anchor
/// on the picked task; delivery/snooze rows reference an existing rule,
/// the `Default` of an existing task, or an existing alarm; muted tasks
/// ⊆ generated tasks; alarm rows are referentially self-contained (the
/// alarm map never references tasks). Normalizes to the persisted
/// invariants (≤1 delivery and ≤1 dismissal per key, first wins;
/// compacted snooze chains).
#[must_use]
pub fn build_reminders(tasks: &BTreeMap<TaskId, Task>, rows: Vec<ReminderRow>) -> ReminderState {
    let task_ids: Vec<TaskId> = tasks.keys().copied().collect();
    let mut state = ReminderState::default();
    if task_ids.is_empty() {
        return state;
    }
    let mut rule_ids: Vec<ReminderRuleId> = Vec::new();
    let mut alarm_ids: Vec<AdhocAlarmId> = Vec::new();
    for row in rows {
        match row.kind {
            0 => {
                if let Some(&task) = task_ids.get(row.pick % task_ids.len()) {
                    let rule = ReminderRule {
                        insistence: Insistence::Normal,
                        id: row.rule,
                        task,
                        offset: row.offset,
                        updated_at: row.created_at,
                        created_at: row.created_at,
                    };
                    rule_ids.push(rule.id);
                    state.rules.insert(rule.id, rule);
                }
            }
            1 | 2 => {
                let Some(reminder) = pick_ref(&rule_ids, &task_ids, &alarm_ids, row.pick) else {
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
            4 => {
                let alarm = AdhocAlarm {
                    insistence: Insistence::Normal,
                    id: row.alarm,
                    label: row.label,
                    fire_at: row.occurrence,
                    created_at: row.created_at,
                };
                alarm_ids.push(alarm.id);
                state.alarms.insert(alarm.id, alarm);
            }
            5 => {
                let Some(reminder) = pick_ref(&rule_ids, &task_ids, &alarm_ids, row.pick) else {
                    continue;
                };
                let dismissal = crate::reminders::ReminderDismissal {
                    reminder,
                    occurrence: row.occurrence,
                    dismissed_at: row.delivered_at,
                };
                if !state.is_dismissed(&reminder, &row.occurrence) {
                    state.dismissals.insert(dismissal);
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

/// Picks `Rule` of a generated rule (`pick % 3 == 0`), `Default` of a
/// generated task (`== 1`), or `Adhoc` of a generated alarm (`== 2`);
/// `None` when the picked anchor side is empty.
fn pick_ref(
    rule_ids: &[ReminderRuleId],
    task_ids: &[TaskId],
    alarm_ids: &[AdhocAlarmId],
    pick: usize,
) -> Option<ReminderRef> {
    match pick % 3 {
        0 => rule_ids
            .get(pick % rule_ids.len().max(1))
            .copied()
            .map(ReminderRef::Rule),
        1 => task_ids
            .get(pick % task_ids.len())
            .copied()
            .map(ReminderRef::Default),
        _ => alarm_ids
            .get(pick % alarm_ids.len().max(1))
            .copied()
            .map(ReminderRef::Adhoc),
    }
}

/// The strategy world normalizes through the same primitive the action
/// world derives with — one compaction definition site.
fn compact(state: &mut ReminderState) {
    crate::reminders::compact_snooze_chains(state);
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
                    ReminderRef::Adhoc(id) => prop_assert!(state.alarms.contains_key(&id)),
                }
            }
            for dismissal in &state.dismissals {
                match dismissal.reminder {
                    ReminderRef::Rule(id) => prop_assert!(state.rules.contains_key(&id)),
                    ReminderRef::Default(task) => prop_assert!(tasks.contains_key(&task)),
                    ReminderRef::Adhoc(id) => prop_assert!(state.alarms.contains_key(&id)),
                }
            }
            for key in state.snoozes.keys() {
                match key.reminder {
                    ReminderRef::Rule(id) => prop_assert!(state.rules.contains_key(&id)),
                    ReminderRef::Default(task) => prop_assert!(tasks.contains_key(&task)),
                    ReminderRef::Adhoc(id) => prop_assert!(state.alarms.contains_key(&id)),
                }
            }
            for task in &state.muted {
                prop_assert!(tasks.contains_key(task));
            }
        }
    }
}
