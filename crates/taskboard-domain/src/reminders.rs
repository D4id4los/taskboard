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
        let reminder = Self::parse_ref(reminder)
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

impl ReminderSnoozeKey {
    fn parse_ref(raw: &str) -> Option<ReminderRef> {
        if let Some(id) = raw.strip_prefix("rule:") {
            return Some(ReminderRef::Rule(ReminderRuleId::from_uuid(
                uuid::Uuid::parse_str(id).ok()?,
            )));
        }
        let task = raw.strip_prefix("default:")?;
        Some(ReminderRef::Default(TaskId::from_uuid(
            uuid::Uuid::parse_str(task).ok()?,
        )))
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
    }
}
