// SPDX-License-Identifier: MIT OR Apache-2.0
//! Reminder model: due-anchored rules, occurrence-keyed delivery proofs,
//! snooze chains, and mute — pure data plus the pure evaluator that turns
//! them into firings.
//!
//! Time is always a parameter (the `clock.rs` contract): nothing here
//! reads a clock, generates ids, or touches IO. The suppression guard is
//! the `(reminder, occurrence)` pair, so moving a due date or snoozing
//! re-arms a reminder by construction; explicit rules replace the task's
//! implicit default reminder (derived, nothing stored).

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::entities::Task;
use crate::ids::{ReminderRuleId, TaskId};

/// A due-anchored offset that can only be "at or before" the due date:
/// the inner `chrono::TimeDelta` is ≤ 0 by construction — [`before`] is
/// the only non-zero construction path.
///
/// [`before`]: ReminderOffset::before
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
pub struct ReminderOffset(chrono::TimeDelta);

impl ReminderOffset {
    /// The only construction path for a non-zero offset: `dur` must not
    /// be positive ("after due" nagging is the repeat-policy question,
    /// not an offset sign).
    ///
    /// # Errors
    ///
    /// [`InvalidReminderOffset`] when `dur` is after the due date.
    pub fn before(dur: chrono::TimeDelta) -> Result<Self, InvalidReminderOffset> {
        if dur > chrono::TimeDelta::zero() {
            Err(InvalidReminderOffset::AfterDue)
        } else {
            Ok(Self(dur))
        }
    }

    /// Zero offset: fire exactly at the due date.
    #[must_use]
    pub fn at_due() -> Self {
        Self::default()
    }

    /// The wrapped duration (≤ 0 always).
    #[must_use]
    pub fn as_duration(self) -> chrono::TimeDelta {
        self.0
    }
}

/// `dur > zero` — positive offsets are not constructible.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum InvalidReminderOffset {
    /// The proposed offset lies after the due date.
    #[error("reminder offset must not be after the due date")]
    AfterDue,
}

/// Which reminder a firing/delivery/snooze refers to: an explicit rule,
/// or a task's implicit default (no stored row — its identity is the
/// task).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ReminderRef {
    /// An explicit rule.
    Rule(ReminderRuleId),
    /// The task's implicit default reminder.
    Default(TaskId),
}

impl ReminderRef {
    /// The text form shared by the storage codec and the snooze-map key
    /// encoding (`"rule:<uuid>"` / `"default:<task-uuid>"`).
    #[must_use]
    pub fn text_form(self) -> String {
        match self {
            Self::Rule(id) => format!("rule:{}", id.as_uuid()),
            Self::Default(task) => format!("default:{}", task.as_uuid()),
        }
    }

    /// Parses [`text_form`](ReminderRef::text_form) back. One definition
    /// site for the text encoding: the snooze-map key serde and the
    /// storage codec both delegate here.
    #[must_use]
    pub fn parse_text_form(raw: &str) -> Option<Self> {
        if let Some(id) = raw.strip_prefix("rule:") {
            return Some(Self::Rule(ReminderRuleId::from(
                uuid::Uuid::parse_str(id).ok()?,
            )));
        }
        let task = raw.strip_prefix("default:")?;
        Some(Self::Default(TaskId::from(
            uuid::Uuid::parse_str(task).ok()?,
        )))
    }
}

/// A reminder rule anchored on a task's due date. N rules per task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReminderRule {
    /// Local identity.
    pub id: ReminderRuleId,
    /// The watched task.
    pub task: TaskId,
    /// Fire this far before the due date; zero = at due (≤ 0 by
    /// construction via [`ReminderOffset`]).
    pub offset: ReminderOffset,
    /// Write time. Per-field write clocks are added additively when a
    /// connector demands rule sync; v1 needs only the creation stamp.
    pub created_at: DateTime<Utc>,
}

/// Proof that an occurrence was notified — the at-most-once guard.
/// Keyed by `(reminder, occurrence)`, NOT by rule alone: moving the due
/// date or snoozing creates a new occurrence and re-arms. Additive,
/// set-union-mergeable by construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ReminderDelivery {
    /// Which reminder fired.
    pub reminder: ReminderRef,
    /// The fire instant this delivery settled.
    pub occurrence: DateTime<Utc>,
    /// When the engine recorded it (stamped at fire time).
    pub delivered_at: DateTime<Utc>,
}

/// A "show me again later": the occurrence is *rescheduled*, not
/// suppressed — the evaluator walks the chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReminderSnooze {
    /// Which reminder was snoozed.
    pub reminder: ReminderRef,
    /// The occurrence being snoozed.
    pub occurrence: DateTime<Utc>,
    /// When it fires again (`now + postpone`, stamped by the engine).
    pub rescheduled_to: DateTime<Utc>,
}

/// Snooze-chain map key: `(reminder, occurrence)`. Serializes as a
/// single string (`"<reminder text form>|<rfc3339 occurrence>"`) because
/// JSON map keys must be strings — the `ValidatorKey` precedent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ReminderSnoozeKey {
    /// Which reminder the link belongs to.
    pub reminder: ReminderRef,
    /// The occurrence the link reschedules.
    pub occurrence: DateTime<Utc>,
}

impl Serialize for ReminderSnoozeKey {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&format!(
            "{}|{}",
            self.reminder.text_form(),
            self.occurrence.to_rfc3339()
        ))
    }
}

impl<'de> Deserialize<'de> for ReminderSnoozeKey {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        let (reminder, occurrence) = raw
            .split_once('|')
            .ok_or_else(|| serde::de::Error::custom("invalid snooze key"))?;
        let reminder = ReminderRef::parse_text_form(reminder)
            .ok_or_else(|| serde::de::Error::custom("invalid snooze key reminder form"))?;
        let occurrence = DateTime::parse_from_rfc3339(occurrence)
            .map_err(|_| serde::de::Error::custom("invalid snooze key occurrence"))?
            .with_timezone(&Utc);
        Ok(Self {
            reminder,
            occurrence,
        })
    }
}

/// Per-device defaults for the implicit reminder. Injected config data,
/// never persisted in [`ReminderState`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ReminderDefaults {
    /// Offset before due for the implicit default reminder (≤ 0).
    pub default_before: ReminderOffset,
}

/// One due firing: exactly the triple that becomes a
/// [`ReminderDelivery`] row plus a notification payload downstream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ReminderFiring {
    /// Which reminder fires.
    pub reminder: ReminderRef,
    /// The task it belongs to.
    pub task: TaskId,
    /// The effective occurrence being fired.
    pub occurrence: DateTime<Utc>,
}

/// Persisted reminder facts carried in `AppState`/`PersistedState`.
/// Field order is the snapshot order: rules, deliveries, snoozes, muted.
/// Delivered-ness and snoozed-ness are plain set/map membership checks.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReminderState {
    /// Explicit rules by id.
    pub rules: BTreeMap<ReminderRuleId, ReminderRule>,
    /// Delivery proofs (≤ 1 row per `(reminder, occurrence)` — enforced
    /// by [`apply_actions`](crate::persistence::apply_actions), pinned by
    /// property).
    pub deliveries: BTreeSet<ReminderDelivery>,
    /// Snooze chain links: `(reminder, occurrence) → rescheduled`
    /// instant. Chains compose (a snooze target is itself snoozable).
    pub snoozes: BTreeMap<ReminderSnoozeKey, DateTime<Utc>>,
    /// Tasks muted out of firings entirely. The overdue *banner* ignores
    /// this set — mute means "stop nagging", not "hide reality".
    pub muted: BTreeSet<TaskId>,
}

impl ReminderState {
    /// Delivery lookup on the `(reminder, occurrence)` key. A linear scan
    /// is deliberate: the ≤1-row-per-key invariant makes any hit a
    /// prefix hit, and kiosk-scale sets don't warrant a range dance.
    #[must_use]
    pub fn is_delivered(&self, reminder: &ReminderRef, occurrence: &DateTime<Utc>) -> bool {
        self.deliveries
            .iter()
            .any(|d| &d.reminder == reminder && &d.occurrence == occurrence)
    }

    /// Effective occurrence of this reminder: the base occurrence, or
    /// wherever the snooze chain has moved it (see [`effective_occurrence`]).
    #[must_use]
    pub fn effective_occurrence(
        &self,
        reminder: &ReminderRef,
        tasks: &BTreeMap<TaskId, Task>,
        defaults: &ReminderDefaults,
    ) -> Option<DateTime<Utc>> {
        effective_occurrence(reminder, self, tasks, defaults)
    }
}

/// Outbound reminder notification: the payload a sink delivers. Title is
/// a snapshot at fire time (the task may be edited later). Pure data —
/// the engine's notifier *channel* lives in the actor crates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Notification {
    /// Which reminder fired — correlates a later dismiss/snooze reply
    /// with this payload (both carry the ref + occurrence).
    pub reminder: ReminderRef,
    /// The task it belongs to.
    pub task: TaskId,
    /// Title snapshot at fire time.
    pub title: String,
    /// The effective occurrence that fired.
    pub occurrence: DateTime<Utc>,
}

/// Pure payload assembly: one [`Notification`] per firing, with the
/// title snapshot from the task map. A firing whose task is missing is
/// skipped (total on arbitrary input; unreachable through the engine's
/// sequential tick, which evaluates and assembles in one message).
#[must_use]
pub fn notifications_for(
    firings: &[ReminderFiring],
    tasks: &BTreeMap<TaskId, Task>,
) -> Vec<Notification> {
    firings
        .iter()
        .filter_map(|firing| {
            tasks.get(&firing.task).map(|task| Notification {
                reminder: firing.reminder,
                task: firing.task,
                title: task.title.clone(),
                occurrence: firing.occurrence,
            })
        })
        .collect()
}

/// Prunes snooze links whose source AND target occurrences are both
/// delivered: such a link can never influence evaluation again — both
/// endpoints suppress on their own. The compaction primitive shared by
/// [`apply_actions`](crate::persistence::apply_actions)' post-batch pass
/// and the test-support state normalizer; one definition site.
///
/// Target-only pruning would be observable: a snoozed-but-undelivered
/// source would re-arm the moment its target settles. Both-endpoints
/// pruning is unconditionally unobservable (property-pinned).
pub fn compact_snooze_chains(reminders: &mut ReminderState) {
    let keep: Vec<ReminderSnoozeKey> = reminders
        .snoozes
        .iter()
        .filter(|(key, target)| {
            !(reminders.is_delivered(&key.reminder, &key.occurrence)
                && reminders.is_delivered(&key.reminder, target))
        })
        .map(|(key, _)| *key)
        .collect();
    reminders.snoozes.retain(|key, _| keep.contains(key));
}

/// The instant an explicit rule fires at, given the task's current
/// due date. `None` ⇔ no due date, or a fire instant outside the
/// representable range (the evaluator is total on arbitrary input).
/// Pure in `(offset, duedate)`.
#[must_use]
pub fn occurrence_of(rule: &ReminderRule, duedate: Option<DateTime<Utc>>) -> Option<DateTime<Utc>> {
    duedate.and_then(|d| d.checked_add_signed(rule.offset.as_duration()))
}

/// Effective occurrence of any reminder (explicit or default): the base
/// occurrence, or wherever the snooze chain has moved it. The chain walk
/// is cycle-bounded (a `visited` set): on a cyclic link chain —
/// unsanctionable data that no sanctioned write can produce — the walk
/// stops at the last node before the repeat, keeping the evaluator total
/// on arbitrary input.
#[must_use]
pub fn effective_occurrence(
    reminder: &ReminderRef,
    reminders: &ReminderState,
    tasks: &BTreeMap<TaskId, Task>,
    defaults: &ReminderDefaults,
) -> Option<DateTime<Utc>> {
    let base = base_occurrence(reminder, reminders, tasks, defaults)?;
    let mut effective = base;
    let mut visited = BTreeSet::new();
    visited.insert(base);
    while let Some(&target) = reminders.snoozes.get(&ReminderSnoozeKey {
        reminder: *reminder,
        occurrence: effective,
    }) {
        if !visited.insert(target) {
            break;
        }
        effective = target;
    }
    Some(effective)
}

fn base_occurrence(
    reminder: &ReminderRef,
    reminders: &ReminderState,
    tasks: &BTreeMap<TaskId, Task>,
    defaults: &ReminderDefaults,
) -> Option<DateTime<Utc>> {
    match *reminder {
        ReminderRef::Rule(id) => {
            let rule = reminders.rules.get(&id)?;
            occurrence_of(rule, tasks.get(&rule.task)?.duedate)
        }
        ReminderRef::Default(task) => {
            let duedate = tasks.get(&task)?.duedate?;
            duedate.checked_add_signed(defaults.default_before.as_duration())
        }
    }
}

/// Everything that must fire at `now`: explicit rules plus implicit
/// defaults, occurrence-keyed, snooze- and mute-aware. Sorted by
/// `(occurrence, reminder)` — a total order, so output is fully
/// deterministic.
///
/// Eligibility: the task must be live and open with a due date and not
/// muted; a task's explicit rules (in rule-id order) replace its
/// implicit default. A reminder fires iff its effective occurrence is ≤
/// `now` and undelivered.
#[must_use]
pub fn due_firings(
    tasks: &BTreeMap<TaskId, Task>,
    reminders: &ReminderState,
    defaults: &ReminderDefaults,
    now: DateTime<Utc>,
) -> Vec<ReminderFiring> {
    let mut firings = Vec::new();
    for task in tasks.values() {
        if !is_remindable(task) || reminders.muted.contains(&task.id) {
            continue;
        }
        let rules: Vec<&ReminderRule> = reminders
            .rules
            .values()
            .filter(|rule| rule.task == task.id)
            .collect();
        let candidates: Vec<ReminderRef> = if rules.is_empty() {
            // Explicit rules replace the default — derived, nothing stored.
            vec![ReminderRef::Default(task.id)]
        } else {
            rules.iter().map(|r| ReminderRef::Rule(r.id)).collect()
        };
        for reminder in candidates {
            if let Some(effective) = reminders.effective_occurrence(&reminder, tasks, defaults)
                && effective <= now
                && !reminders.is_delivered(&reminder, &effective)
            {
                firings.push(ReminderFiring {
                    reminder,
                    task: task.id,
                    occurrence: effective,
                });
            }
        }
    }
    firings.sort_by_key(|f| (f.occurrence, f.reminder));
    firings
}

/// Kiosk banner: live, open tasks with due date < `now`, in canonical
/// order (`(order, id)` — the `AppState::live_tasks` comparator).
/// Ignores rules, deliveries, snoozes, and mute — board truth, not
/// nagging policy.
#[must_use]
pub fn overdue_tasks(tasks: &BTreeMap<TaskId, Task>, now: DateTime<Utc>) -> Vec<&Task> {
    filter_sorted(tasks, |t| {
        t.is_live() && t.is_open() && t.duedate.is_some_and(|d| d < now)
    })
}

/// Desktop "due soon" strip: same eligibility, due date in
/// `(now, now + horizon]`. A horizon beyond the representable range is
/// saturated to it (the strip degrades to "everything ahead", never a
/// panic — chunk 3 feeds this from user config).
#[must_use]
pub fn due_soon(
    tasks: &BTreeMap<TaskId, Task>,
    now: DateTime<Utc>,
    horizon: chrono::TimeDelta,
) -> Vec<&Task> {
    let limit = now
        .checked_add_signed(horizon)
        .unwrap_or(DateTime::<Utc>::MAX_UTC);
    filter_sorted(tasks, |t| {
        t.is_live() && t.is_open() && t.duedate.is_some_and(|d| d > now && d <= limit)
    })
}

/// The firing/banner eligibility filter: not tombstoned, not completed,
/// has a due date. Mute is a firings-only concern and lives in
/// [`due_firings`].
fn is_remindable(task: &Task) -> bool {
    task.is_live() && task.is_open() && task.duedate.is_some()
}

/// Shared banner/strip filter + the canonical `(order, id)` sort. Pinned
/// by property to `AppState::live_tasks`' comparator.
fn filter_sorted(tasks: &BTreeMap<TaskId, Task>, keep: impl Fn(&Task) -> bool) -> Vec<&Task> {
    let mut picked: Vec<&Task> = tasks.values().filter(|t| keep(t)).collect();
    picked.sort_by_key(|t| (t.order, t.id));
    picked
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support;
    use chrono::TimeZone;
    use proptest::prelude::*;

    fn ts(secs: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(secs, 0).unwrap()
    }

    fn ref_of(raw: u128) -> ReminderRef {
        ReminderRef::Rule(ReminderRuleId::from(uuid::Uuid::from_u128(raw)))
    }

    // ---- Evaluation (P1-P4, P7, P8) -----------------------------------

    fn live_task(raw: u128, duedate: Option<DateTime<Utc>>, order: i64) -> Task {
        Task {
            id: TaskId::from(uuid::Uuid::from_u128(raw)),
            remote: None,
            title: format!("task {raw}"),
            description: String::new(),
            duedate,
            done: None,
            stack: crate::ids::StackId::from(uuid::Uuid::from_u128(1)),
            order,
            labels: BTreeSet::default(),
            archived: false,
            deleted: false,
            clocks: crate::entities::TaskClocks {
                title: ts(100),
                description: ts(100),
                duedate: ts(100),
                done: ts(100),
                position: ts(100),
                labels: ts(100),
                archived: ts(100),
                deleted: ts(100),
            },
            remote_seen: None,
        }
    }

    /// The evaluation-proptest input: referentially-consistent
    /// `(tasks, reminders)` plus injected defaults and `now`.
    fn eval_input() -> impl proptest::strategy::Strategy<
        Value = (
            std::collections::BTreeMap<TaskId, Task>,
            ReminderState,
            ReminderDefaults,
            DateTime<Utc>,
        ),
    > {
        (
            test_support::app_state_strategy(),
            test_support::reminder_offset_strategy(),
            test_support::timestamp_strategy(),
        )
            .prop_map(|(app, offset, now)| {
                let reminders = app.reminders.clone();
                (
                    app.tasks.clone(),
                    reminders,
                    ReminderDefaults {
                        default_before: offset,
                    },
                    now,
                )
            })
    }

    /// Independent re-derivation of the eligible set: does not call
    /// `effective_occurrence` or `due_firings` — walks base occurrence
    /// and the snooze chain by hand.
    fn expected_firings(
        tasks: &std::collections::BTreeMap<TaskId, Task>,
        reminders: &ReminderState,
        defaults: &ReminderDefaults,
        now: DateTime<Utc>,
    ) -> Vec<(DateTime<Utc>, ReminderRef)> {
        let mut expected = Vec::new();
        for task in tasks.values() {
            if !(task.is_live() && task.is_open() && task.duedate.is_some()) {
                continue;
            }
            if reminders.muted.contains(&task.id) {
                continue;
            }
            let mut rule_refs: Vec<ReminderRef> = reminders
                .rules
                .values()
                .filter(|r| r.task == task.id)
                .map(|r| ReminderRef::Rule(r.id))
                .collect();
            if rule_refs.is_empty() {
                rule_refs.push(ReminderRef::Default(task.id));
            }
            for reminder in rule_refs {
                let base = match reminder {
                    ReminderRef::Rule(id) => {
                        let Some(rule) = reminders.rules.get(&id) else {
                            continue;
                        };
                        let Some(duedate) = tasks.get(&rule.task).and_then(|t| t.duedate) else {
                            continue;
                        };
                        duedate + rule.offset.as_duration()
                    }
                    ReminderRef::Default(task_id) => {
                        let Some(duedate) = tasks.get(&task_id).and_then(|t| t.duedate) else {
                            continue;
                        };
                        duedate + defaults.default_before.as_duration()
                    }
                };
                // Hand-rolled snooze walk (stops at first repeat).
                let mut effective = base;
                let mut seen = BTreeSet::from([base]);
                while let Some(&target) = reminders.snoozes.get(&ReminderSnoozeKey {
                    reminder,
                    occurrence: effective,
                }) {
                    if !seen.insert(target) {
                        break;
                    }
                    effective = target;
                }
                if effective <= now && !reminders.is_delivered(&reminder, &effective) {
                    expected.push((effective, reminder));
                }
            }
        }
        expected.sort();
        expected
    }

    proptest! {
        #![proptest_config(test_support::proptest_config(256))]

        #[test]
        fn p1_settled_firings_never_refire(
            (tasks, reminders, defaults, now) in eval_input(),
        ) {
            let f1 = due_firings(&tasks, &reminders, &defaults, now);
            for f in &f1 {
                prop_assert!(!reminders.is_delivered(&f.reminder, &f.occurrence));
            }
            let mut settled = reminders.clone();
            for f in &f1 {
                settled.deliveries.insert(ReminderDelivery {
                    reminder: f.reminder,
                    occurrence: f.occurrence,
                    delivered_at: now,
                });
            }
            let f2 = due_firings(&tasks, &settled, &defaults, now);
            prop_assert!(f2.is_empty(), "settled firings must not re-fire at the same now");
        }

        #[test]
        fn p2_due_firings_equal_the_independently_derived_set(
            (tasks, reminders, defaults, now) in eval_input(),
        ) {
            let got: Vec<(DateTime<Utc>, ReminderRef)> = due_firings(&tasks, &reminders, &defaults, now)
                .into_iter()
                .map(|f| (f.occurrence, f.reminder))
                .collect();
            prop_assert_eq!(expected_firings(&tasks, &reminders, &defaults, now), got);
        }

        #[test]
        fn p3_occurrence_is_monotone_in_the_duedate(
            rule in test_support::reminder_rule_strategy(),
            d1 in test_support::timestamp_strategy(),
            d2 in test_support::timestamp_strategy(),
        ) {
            let (early, late) = if d1 <= d2 { (d1, d2) } else { (d2, d1) };
            prop_assert!(occurrence_of(&rule, Some(early)) <= occurrence_of(&rule, Some(late)));
            prop_assert_eq!(
                occurrence_of(&rule, Some(early)),
                Some(early + rule.offset.as_duration())
            );
            prop_assert_eq!(occurrence_of(&rule, None), None);
        }

        #[test]
        fn p4_overdue_equals_the_canonically_sorted_eligible_set(
            (tasks, reminders, defaults, now) in eval_input(),
        ) {
            let _ = reminders;
            let _ = defaults;
            let mut expected: Vec<TaskId> = tasks
                .values()
                .filter(|t| t.is_live() && t.is_open() && t.duedate.is_some_and(|d| d < now))
                .map(|t| t.id)
                .collect();
            expected.sort_by_key(|id| (tasks[id].order, *id));
            let got = overdue_tasks(&tasks, now);
            prop_assert_eq!(got.iter().map(|t| t.id).collect::<Vec<_>>(), expected);
        }

        #[test]
        fn p7_snoozed_occurrence_is_suppressed_until_its_target(
            offset in test_support::reminder_offset_strategy(),
            now in test_support::timestamp_strategy(),
            postpone_secs in 1_i64..=3_600,
            probe_secs in 0_i64..3_600,
            use_rule in any::<bool>(),
        ) {
            let postpone = chrono::TimeDelta::try_seconds(postpone_secs).unwrap();
            let task = live_task(1, Some(now), 0);
            let tasks = BTreeMap::from([(task.id, task.clone())]);
            let defaults = ReminderDefaults { default_before: offset };
            let mut reminders = ReminderState::default();
            let reminder = if use_rule {
                let rule = ReminderRule {
                    id: ReminderRuleId::from(uuid::Uuid::from_u128(10)),
                    task: task.id,
                    offset,
                    created_at: now,
                };
                let r = ReminderRef::Rule(rule.id);
                reminders.rules.insert(rule.id, rule);
                r
            } else {
                ReminderRef::Default(task.id)
            };
            let base = reminders.effective_occurrence(&reminder, &tasks, &defaults).unwrap();
            prop_assert!(base <= now);
            reminders
                .snoozes
                .insert(ReminderSnoozeKey { reminder, occurrence: base }, now + postpone);

            // Any instant before the target: suppressed.
            let probe = now + chrono::TimeDelta::try_seconds(probe_secs).unwrap().min(postpone - chrono::TimeDelta::try_seconds(1).unwrap());
            let during = due_firings(&tasks, &reminders, &defaults, probe);
            prop_assert!(during.is_empty(), "snoozed reminder must not fire before its target");

            // At the target: fires (undelivered), then settles forever.
            let at_target = due_firings(&tasks, &reminders, &defaults, now + postpone);
            prop_assert_eq!(
                at_target.iter().map(|f| f.reminder).collect::<Vec<_>>(),
                vec![reminder]
            );
            reminders.deliveries.insert(ReminderDelivery {
                reminder,
                occurrence: now + postpone,
                delivered_at: now + postpone,
            });
            prop_assert!(due_firings(&tasks, &reminders, &defaults, now + postpone).is_empty());
            prop_assert!(
                due_firings(&tasks, &reminders, &defaults, now + postpone * 2).is_empty()
            );
        }

        #[test]
        fn p8_muted_tasks_never_fire_but_stay_in_the_banner(
            (tasks, mut reminders, defaults, now) in eval_input(),
        ) {
            let overdue: Vec<TaskId> = tasks
                .values()
                .filter(|t| t.is_live() && t.is_open() && t.duedate.is_some_and(|d| d < now))
                .map(|t| t.id)
                .collect();
            reminders.muted = overdue.iter().copied().collect();
            let fired: Vec<TaskId> = due_firings(&tasks, &reminders, &defaults, now)
                .into_iter()
                .map(|f| f.task)
                .collect();
            prop_assert!(fired.iter().all(|t| !reminders.muted.contains(t)));
            // Banner truth ignores mute entirely.
            let banner: BTreeSet<TaskId> =
                overdue_tasks(&tasks, now).into_iter().map(|t| t.id).collect();
            prop_assert_eq!(banner, overdue.into_iter().collect::<BTreeSet<_>>());
        }
    }

    #[test]
    fn due_soon_window_is_half_open_at_both_edges() {
        let now = ts(10_000);
        let horizon = chrono::TimeDelta::try_minutes(5).unwrap();
        let task =
            |raw: u128, duedate: Option<DateTime<Utc>>, order: i64| live_task(raw, duedate, order);
        let tasks = BTreeMap::from([
            (uuid::Uuid::from_u128(1), task(1, Some(now), 0)), // at now: below window
            (
                uuid::Uuid::from_u128(2),
                task(2, Some(now + horizon), 1), // at horizon: inside
            ),
            (
                uuid::Uuid::from_u128(3),
                task(
                    3,
                    Some(now + horizon + chrono::TimeDelta::try_seconds(1).unwrap()),
                    2,
                ), // past horizon
            ),
            (
                uuid::Uuid::from_u128(4),
                task(4, Some(now + chrono::TimeDelta::try_seconds(1).unwrap()), 3), // just above now: inside
            ),
        ]);
        let tasks: BTreeMap<TaskId, Task> = tasks.into_values().map(|t| (t.id, t)).collect();
        let soon: Vec<TaskId> = due_soon(&tasks, now, horizon)
            .into_iter()
            .map(|t| t.id)
            .collect();
        let in_window: Vec<TaskId> = vec![
            TaskId::from(uuid::Uuid::from_u128(2)),
            TaskId::from(uuid::Uuid::from_u128(4)),
        ];
        assert_eq!(soon, in_window);
        // Completed and tombstoned tasks never appear.
        let mut closed = live_task(9, Some(now + chrono::TimeDelta::try_minutes(1).unwrap()), 4);
        closed.done = Some(now);
        let mut tomb = live_task(8, Some(now + chrono::TimeDelta::try_minutes(1).unwrap()), 5);
        tomb.deleted = true;
        let tasks: BTreeMap<TaskId, Task> = BTreeMap::from([(closed.id, closed), (tomb.id, tomb)]);
        assert_eq!(due_soon(&tasks, now, horizon), Vec::<&Task>::new());
        assert_eq!(overdue_tasks(&tasks, now), Vec::<&Task>::new());
    }

    #[test]
    fn occurrence_arithmetic_stays_total_at_the_representable_edge() {
        let rule = ReminderRule {
            id: ReminderRuleId::from(uuid::Uuid::from_u128(1)),
            task: TaskId::from(uuid::Uuid::from_u128(2)),
            offset: ReminderOffset::before(chrono::TimeDelta::try_days(-30).unwrap()).unwrap(),
            created_at: ts(100),
        };
        // A fire instant below chrono's representable floor suppresses
        // the reminder instead of panicking the evaluator.
        assert_eq!(occurrence_of(&rule, Some(DateTime::<Utc>::MIN_UTC)), None);
    }

    #[test]
    fn due_soon_saturates_an_unrepresentable_horizon() {
        let now = ts(10_000);
        let task = live_task(1, Some(now + chrono::TimeDelta::try_seconds(1).unwrap()), 0);
        let tasks = BTreeMap::from([(task.id, task)]);
        let soon: Vec<TaskId> = due_soon(&tasks, now, chrono::TimeDelta::MAX)
            .into_iter()
            .map(|t| t.id)
            .collect();
        assert_eq!(soon, vec![TaskId::from(uuid::Uuid::from_u128(1))]);
    }

    #[test]
    fn due_firings_are_sorted_by_occurrence_then_reminder() {
        let now = ts(20_000);
        let task_a = live_task(1, Some(ts(1_000)), 5);
        let task_b = live_task(2, Some(ts(2_000)), 1);
        let tasks = BTreeMap::from([(task_a.id, task_a.clone()), (task_b.id, task_b.clone())]);
        let mut reminders = ReminderState::default();
        // task_a (later order) fires earlier: output must not follow task order.
        let rule_a = ReminderRule {
            id: ReminderRuleId::from(uuid::Uuid::from_u128(10)),
            task: task_a.id,
            offset: ReminderOffset::at_due(),
            created_at: ts(100),
        };
        let rule_b = ReminderRule {
            id: ReminderRuleId::from(uuid::Uuid::from_u128(11)),
            task: task_b.id,
            offset: ReminderOffset::at_due(),
            created_at: ts(100),
        };
        reminders.rules.insert(rule_a.id, rule_a);
        reminders.rules.insert(rule_b.id, rule_b);
        let firings = due_firings(&tasks, &reminders, &ReminderDefaults::default(), now);
        let occurrences: Vec<DateTime<Utc>> = firings.iter().map(|f| f.occurrence).collect();
        assert_eq!(
            occurrences,
            vec![ts(1_000), ts(2_000)],
            "sorted by occurrence, not task or rule order"
        );
        for pair in firings.windows(2) {
            assert!(pair[0] < pair[1], "total (occurrence, reminder) order");
        }
    }

    #[test]
    fn offset_guard_accepts_non_positive_and_rejects_positive() {
        assert!(matches!(
            ReminderOffset::before(chrono::TimeDelta::try_seconds(1).unwrap()),
            Err(InvalidReminderOffset::AfterDue)
        ));
        for secs in [0_i64, -1, -300, -2_592_000] {
            let offset = ReminderOffset::before(chrono::TimeDelta::try_seconds(secs).unwrap());
            assert!(offset.is_ok());
            assert_eq!(offset.unwrap().as_duration().num_seconds(), secs);
        }
        assert_eq!(
            ReminderOffset::at_due().as_duration(),
            chrono::TimeDelta::zero()
        );
    }

    #[test]
    fn is_delivered_keys_on_the_pair_not_delivered_at() {
        let mut state = ReminderState::default();
        let r = ref_of(1);
        state.deliveries.insert(ReminderDelivery {
            reminder: r,
            occurrence: ts(1_000),
            delivered_at: ts(1_001),
        });
        assert!(state.is_delivered(&r, &ts(1_000)));
        assert!(!state.is_delivered(&r, &ts(2_000)), "other occurrence");
        assert!(
            !state.is_delivered(&ref_of(2), &ts(1_000)),
            "other reminder"
        );
        // A replay with a different stamp does not change the pair's
        // delivered-ness (first write wins is decided by the caller).
        let replay = ReminderDelivery {
            reminder: r,
            occurrence: ts(1_000),
            delivered_at: ts(9_999),
        };
        assert_ne!(replay, *state.deliveries.iter().next().unwrap());
    }

    #[test]
    fn snooze_key_text_form_roundtrips() {
        let key = ReminderSnoozeKey {
            reminder: ref_of(7),
            occurrence: ts(1_234_567),
        };
        let json = serde_json::to_string(&key).unwrap();
        let back: ReminderSnoozeKey = serde_json::from_str(&json).unwrap();
        assert_eq!(back, key);
        let default_key = ReminderSnoozeKey {
            reminder: ReminderRef::Default(TaskId::from(uuid::Uuid::from_u128(9))),
            occurrence: ts(42),
        };
        let back: ReminderSnoozeKey =
            serde_json::from_str(&serde_json::to_string(&default_key).unwrap()).unwrap();
        assert_eq!(back, default_key);
        assert!(serde_json::from_str::<ReminderSnoozeKey>("\"garbage\"").is_err());
    }

    // ---- Notification payload + text-form parsing (chunk 2) ------------

    #[test]
    fn reminder_ref_text_form_roundtrips_and_rejects_garbage() {
        let rule = ref_of(11);
        let default = ReminderRef::Default(TaskId::from(uuid::Uuid::from_u128(12)));
        for reminder in [rule, default] {
            assert_eq!(
                ReminderRef::parse_text_form(&reminder.text_form()),
                Some(reminder),
                "parse is the inverse of text_form"
            );
        }
        for garbage in [
            "",
            "garbage",
            "rule:",
            "default:",
            "rule:not-a-uuid",
            "default:not-a-uuid",
            "rule:1|default:2",
        ] {
            assert_eq!(
                ReminderRef::parse_text_form(garbage),
                None,
                "{garbage:?} must be rejected"
            );
        }
    }

    #[test]
    fn notification_roundtrips_through_serde() {
        let notification = Notification {
            reminder: ref_of(3),
            task: TaskId::from(uuid::Uuid::from_u128(4)),
            title: "ship it".into(),
            occurrence: ts(1_000),
        };
        let back: Notification =
            serde_json::from_str(&serde_json::to_string(&notification).unwrap()).unwrap();
        assert_eq!(back, notification);
    }

    #[test]
    fn notifications_for_snapshots_titles_and_skips_missing_tasks() {
        let task = live_task(1, Some(ts(500)), 0);
        let tasks = BTreeMap::from([(task.id, task.clone())]);
        let stranger = TaskId::from(uuid::Uuid::from_u128(999));
        let firings = vec![
            ReminderFiring {
                reminder: ref_of(10),
                task: task.id,
                occurrence: ts(400),
            },
            ReminderFiring {
                reminder: ReminderRef::Default(stranger),
                task: stranger,
                occurrence: ts(401),
            },
        ];
        let notes = notifications_for(&firings, &tasks);
        assert_eq!(
            notes,
            vec![Notification {
                reminder: ref_of(10),
                task: task.id,
                title: task.title.clone(),
                occurrence: ts(400),
            }],
            "one payload per firing with a live task; the missing task is skipped"
        );
        assert_eq!(notifications_for(&[], &tasks), Vec::new());
    }

    proptest! {
        #![proptest_config(test_support::proptest_config(256))]

        #[test]
        fn reminder_types_roundtrip_through_serde(
            rule in test_support::reminder_rule_strategy(),
            delivery in test_support::reminder_delivery_strategy(),
            snooze in test_support::reminder_snooze_strategy(),
            offset in test_support::reminder_offset_strategy(),
            firing_occurrence in test_support::timestamp_strategy(),
        ) {
            fn roundtrip<
                T: serde::Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug,
            >(
                value: &T,
            ) {
                let back: T =
                    serde_json::from_str(&serde_json::to_string(value).unwrap()).unwrap();
                assert_eq!(back, *value);
            }
            roundtrip(&rule);
            roundtrip(&delivery);
            roundtrip(&snooze);
            roundtrip(&offset);
            roundtrip(&ReminderDefaults {
                default_before: offset,
            });
            roundtrip(&ReminderFiring {
                reminder: delivery.reminder,
                task: rule.task,
                occurrence: firing_occurrence,
            });
        }

        #[test]
        fn reminder_ref_roundtrips_through_serde(rule_raw in any::<u128>(), task_raw in any::<u128>()) {
            for reminder in [
                ref_of(rule_raw),
                ReminderRef::Default(TaskId::from(uuid::Uuid::from_u128(task_raw))),
            ] {
                let back: ReminderRef =
                    serde_json::from_str(&serde_json::to_string(&reminder).unwrap()).unwrap();
                prop_assert_eq!(back, reminder);
            }
        }

        #[test]
        fn reminder_state_roundtrips_through_serde(
            tasks in test_support::task_map_strategy(),
            rows in test_support::reminder_rows_strategy(),
        ) {
            let state = test_support::build_reminders(&tasks, rows);
            let back: ReminderState =
                serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
            prop_assert_eq!(back, state);
        }

        #[test]
        fn notifications_for_matches_the_firings_and_the_task_map(
            tasks in test_support::task_map_strategy(),
            occurrences in prop::collection::vec(test_support::timestamp_strategy(), 1..8),
            any_ref in test_support::reminder_ref_strategy(),
            stranger in test_support::task_id_strategy(),
        ) {
            // The first firing references a task absent from the map (its
            // payload must be skipped); the rest reference a present task
            // and must mirror their firing triple + the map's title.
            let anchor = *tasks.keys().next().unwrap();
            let mut firings = Vec::new();
            firings.push(ReminderFiring {
                reminder: any_ref,
                task: stranger,
                occurrence: occurrences[0],
            });
            for occurrence in &occurrences[1..] {
                firings.push(ReminderFiring {
                    reminder: any_ref,
                    task: anchor,
                    occurrence: *occurrence,
                });
            }
            let notes = notifications_for(&firings, &tasks);
            prop_assert_eq!(notes.len() + 1, firings.len());
            for (note, firing) in notes.iter().zip(&firings[1..]) {
                prop_assert_eq!(note.reminder, firing.reminder);
                prop_assert_eq!(note.task, firing.task);
                prop_assert_eq!(note.occurrence, firing.occurrence);
                prop_assert_eq!(&note.title, &tasks[&firing.task].title);
            }
        }
    }

    // ---- #303 adversarial arithmetic (issue #303, plan §proptests) ----

    fn adversarial_offset_rule(
        rule_raw: u128,
        offset_secs: i64,
        task: TaskId,
        created_at: DateTime<Utc>,
    ) -> ReminderRule {
        let offset =
            ReminderOffset::before(chrono::TimeDelta::try_seconds(offset_secs).unwrap()).unwrap();
        ReminderRule {
            id: ReminderRuleId::from(uuid::Uuid::from_u128(rule_raw)),
            task,
            offset,
            created_at,
        }
    }

    proptest! {
        #![proptest_config(test_support::proptest_config(256))]

        #[test]
        fn occurrence_of_is_total_and_saturating(
            offset_secs in prop_oneof![
                Just(0_i64),
                (-2_592_000_i64..=-1),
                (chrono::TimeDelta::MIN.num_seconds()..=-2_592_001),
            ],
            due_secs in test_support::adversarial_timestamp_seconds(),
            d1 in test_support::adversarial_timestamp_seconds(),
            d2 in test_support::adversarial_timestamp_seconds(),
        ) {
            let rule =
                adversarial_offset_rule(400, offset_secs, TaskId::from(uuid::Uuid::from_u128(1)), ts(100));
            let due = ts(due_secs);
            // Library oracle: the contract *is* chrono's checked
            // arithmetic — no clamping, `None` ⇔ overflow.
            prop_assert_eq!(
                occurrence_of(&rule, Some(due)),
                due.checked_add_signed(rule.offset.as_duration()),
            );
            // Totality covers `None` dues trivially; monotonicity in the
            // due, `None` sorting below `Some` (the saturation contract).
            let (early, late) = if d1 <= d2 { (d1, d2) } else { (d2, d1) };
            prop_assert!(
                occurrence_of(&rule, Some(ts(early))) <= occurrence_of(&rule, Some(ts(late))),
            );
        }

        #[test]
        fn effective_occurrence_terminates_on_arbitrary_chains(
            due_secs in 1_000_000_i64..=2_000_000,
            links in prop::collection::vec((0_usize..=5, 0_usize..=5), 0..=6),
        ) {
            // Snooze links over a small instant set drawn *with
            // replacement*, so self-links and cycles generate naturally:
            // the cycle bound (the `visited` set) is under test.
            let due = ts(due_secs);
            let task = live_task(1, Some(due), 0);
            let task_id = task.id;
            let tasks = BTreeMap::from([(task.id, task)]);
            let rule = ReminderRule {
                id: ReminderRuleId::from(uuid::Uuid::from_u128(401)),
                task: task_id,
                offset: ReminderOffset::at_due(),
                created_at: due,
            };
            let reminder = ReminderRef::Rule(rule.id);
            let instants: Vec<DateTime<Utc>> = (0..=6)
                .map(|i| due + chrono::TimeDelta::try_hours(i).unwrap())
                .collect();
            let mut reminders = ReminderState::default();
            reminders.rules.insert(rule.id, rule);
            for &(source, target) in &links {
                reminders.snoozes.insert(
                    ReminderSnoozeKey {
                        reminder,
                        occurrence: instants[source],
                    },
                    instants[target],
                );
            }

            let result = reminders.effective_occurrence(&reminder, &tasks, &ReminderDefaults::default());
            prop_assert!(result.is_some(), "total: the walk returns the base at worst");

            // Bounded ≤ len(links)-hop manual walk reaches the same fixed
            // point, and the walked path never repeats a node.
            let mut walked = due;
            let mut visited = BTreeSet::from([due]);
            let mut hops = 0_usize;
            while hops <= links.len() {
                let Some(&next) = reminders.snoozes.get(&ReminderSnoozeKey {
                    reminder,
                    occurrence: walked,
                }) else {
                    break;
                };
                if !visited.insert(next) {
                    break;
                }
                walked = next;
                hops += 1;
            }
            prop_assert_eq!(result, Some(walked));
            prop_assert_eq!(visited.len(), hops + 1, "no node repeats on the path");
        }

        #[test]
        fn due_soon_saturates_the_horizon_never_panics(
            now_secs in test_support::adversarial_timestamp_seconds(),
            horizon in prop_oneof![
                (1_i64..=2_592_000).prop_map(|s| chrono::TimeDelta::try_seconds(s).unwrap()),
                Just(chrono::TimeDelta::try_seconds(i64::MAX / 1000).unwrap()),
                Just(-chrono::TimeDelta::try_seconds(i64::MAX / 1000).unwrap()),
                Just(chrono::TimeDelta::zero()),
            ],
        ) {
            let now = ts(now_secs);
            let edge = DateTime::<Utc>::MAX_UTC - chrono::TimeDelta::try_seconds(1).unwrap();
            let task = live_task(1, Some(edge), 0);
            let task_id = task.id;
            let tasks = BTreeMap::from([(task.id, task)]);

            let got = due_soon(&tasks, now, horizon);
            let limit = now
                .checked_add_signed(horizon)
                .unwrap_or(DateTime::<Utc>::MAX_UTC);
            for t in &got {
                prop_assert!(
                    t.duedate.is_some_and(|d| d > now && d <= limit),
                    "library oracle: the half-open window in `(now, limit]`",
                );
            }
            if horizon == chrono::TimeDelta::try_seconds(i64::MAX / 1000).unwrap() && now < edge {
                prop_assert!(
                    got.iter().any(|t| t.id == task_id),
                    "the max horizon degrades to everything ahead",
                );
            }
        }

        #[test]
        fn due_firings_total_deterministic_ordered_on_adversarial_instants(
            now_secs in test_support::adversarial_timestamp_seconds(),
            dues in prop::collection::vec(test_support::adversarial_timestamp_seconds(), 1..=4),
            offset_secs in prop_oneof![
                Just(0_i64),
                (-2_592_000_i64..=-1),
                (chrono::TimeDelta::MIN.num_seconds()..=-2_592_001),
            ],
        ) {
            let now = ts(now_secs);
            let mut tasks: BTreeMap<TaskId, Task> = BTreeMap::new();
            let mut reminders = ReminderState::default();
            for (i, due_secs) in dues.iter().enumerate() {
                let order = i64::try_from(i).unwrap();
                let task = live_task(u128::try_from(i).unwrap() + 1, Some(ts(*due_secs)), order);
                // Distinct rule ids: the rules map is keyed by id, so a
                // shared id would silently keep only the last task's
                // explicit rule and drop the rest to default reminders.
                let rule = adversarial_offset_rule(
                    u128::try_from(i).unwrap() + 401,
                    offset_secs,
                    task.id,
                    ts(100),
                );
                reminders.rules.insert(rule.id, rule);
                tasks.insert(task.id, task);
            }
            let defaults = ReminderDefaults::default();

            let a = due_firings(&tasks, &reminders, &defaults, now);
            let b = due_firings(&tasks, &reminders, &defaults, now);
            prop_assert_eq!(&a, &b, "deterministic");
            for pair in a.windows(2) {
                prop_assert!(
                    (pair[0].occurrence, pair[0].reminder) < (pair[1].occurrence, pair[1].reminder),
                    "total (occurrence, reminder) order",
                );
            }
            for firing in &a {
                prop_assert!(firing.occurrence <= now);
                prop_assert!(!reminders.is_delivered(&firing.reminder, &firing.occurrence));
                let task = &tasks[&firing.task];
                prop_assert!(task.is_live() && task.is_open() && task.duedate.is_some());
                prop_assert!(!reminders.muted.contains(&firing.task));
            }
        }

        #[test]
        fn reminder_offset_before_rejects_exactly_positive_durations(
            secs in (-(i64::MAX / 1000)..=(i64::MAX / 1000)),
        ) {
            // The full `try_seconds` domain: `before` is `Ok` iff the
            // duration is not positive; the error is exactly AfterDue.
            let dur = chrono::TimeDelta::try_seconds(secs).unwrap();
            match ReminderOffset::before(dur) {
                Ok(offset) => {
                    prop_assert!(dur <= chrono::TimeDelta::zero());
                    prop_assert!(offset.as_duration() <= chrono::TimeDelta::zero());
                    prop_assert_eq!(offset.as_duration(), dur);
                }
                Err(err) => {
                    prop_assert!(matches!(err, InvalidReminderOffset::AfterDue));
                    prop_assert!(dur > chrono::TimeDelta::zero());
                }
            }
        }
    }

    #[test]
    fn dst_boundary_instants_are_ordinary_instants() {
        // Known-answer battery over the 2026 DST transitions: the
        // arithmetic is zone-free, so every battery instant behaves like
        // any other — occurrence is instant-exact and firings fire at the
        // offset instant, never at a wall-clock reading of it.
        let one_hour = chrono::TimeDelta::try_hours(1).unwrap();
        for instant in test_support::dst_boundary_instants() {
            let task = live_task(31, Some(instant), 0);
            let task_id = task.id;
            let tasks = BTreeMap::from([(task.id, task)]);
            let rule = ReminderRule {
                id: ReminderRuleId::from(uuid::Uuid::from_u128(402)),
                task: task_id,
                offset: ReminderOffset::before(-one_hour).unwrap(),
                created_at: instant,
            };
            assert_eq!(
                occurrence_of(&rule, Some(instant)),
                Some(instant - one_hour),
                "instant-exact at {instant}"
            );
            let mut reminders = ReminderState::default();
            reminders.rules.insert(rule.id, rule);

            let firings = due_firings(&tasks, &reminders, &ReminderDefaults::default(), instant);
            assert_eq!(firings.len(), 1, "exactly one firing at {instant}");
            assert_eq!(firings[0].occurrence, instant - one_hour);
        }
    }
}
