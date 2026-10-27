// SPDX-License-Identifier: MIT OR Apache-2.0
//! Reminder model: due-anchored rules, occurrence-keyed delivery proofs,
//! synced dismissals, snooze chains, and mute — pure data plus the pure
//! evaluator that turns them into firings and the derived notification
//! surface.
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
use crate::ids::{AdhocAlarmId, ReminderRuleId, TaskId};

/// A due-anchored offset that can only be "at or before" the due date:
/// the inner `chrono::TimeDelta` is ≤ 0 and whole-second by
/// construction — [`before`] is the only non-zero construction path.
/// Whole-second matters because every persistence lane (the sqlite
/// `offset_seconds` column, the sidecar document's integer field, the
/// text form) stores whole seconds; a sub-second offset would diverge
/// from its persisted form on reload and re-fire a delivered
/// occurrence.
///
/// [`before`]: ReminderOffset::before
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct ReminderOffset(chrono::TimeDelta);

impl ReminderOffset {
    /// The only construction path for a non-zero offset: `dur` must not
    /// be positive ("after due" nagging is the repeat-policy question,
    /// not an offset sign) and must be a whole number of seconds — the
    /// sub-second component would be truncated by every persistence
    /// lane. The positive check runs first: a positive sub-second input
    /// answers [`InvalidReminderOffset::AfterDue`].
    ///
    /// # Errors
    ///
    /// [`InvalidReminderOffset::AfterDue`] when `dur` is after the due
    /// date; [`InvalidReminderOffset::NotWholeSecond`] when `dur`
    /// carries a sub-second component.
    pub fn before(dur: chrono::TimeDelta) -> Result<Self, InvalidReminderOffset> {
        if dur > chrono::TimeDelta::zero() {
            Err(InvalidReminderOffset::AfterDue)
        } else if dur.subsec_nanos() != 0 {
            Err(InvalidReminderOffset::NotWholeSecond)
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

    /// The canonical text form: the before-magnitude rendered as
    /// `<h>h<m>m<s>s`, zero components omitted, `"0s"` for zero
    /// (`"10m"`, `"1h30m"`, `"48h"` for two days). One definition site
    /// shared by the list rows, the receipts, and the UI rule list; the
    /// [`parse_text_form`](Self::parse_text_form) inverse round-trips it.
    ///
    /// The form is defined on whole seconds, which the type guarantees:
    /// every offset is whole-second by construction, so the rendering
    /// never discards information.
    #[must_use]
    pub fn text_form(self) -> String {
        use std::fmt::Write as _;
        // The offset is non-positive by construction; the text form
        // renders the before-magnitude.
        let secs = self.0.num_seconds().abs();
        let (h, m, s) = (secs / 3_600, (secs % 3_600) / 60, secs % 60);
        let mut out = String::new();
        if h > 0 {
            let _ = write!(out, "{h}h");
        }
        if m > 0 {
            let _ = write!(out, "{m}m");
        }
        if s > 0 || out.is_empty() {
            let _ = write!(out, "{s}s");
        }
        out
    }

    /// Parses the offset text form — the one grammar of
    /// `[reminders] default_before` and the CLI `--before` flag: a
    /// non-empty sequence of `<unsigned-int><unit>` components, unit ∈
    /// `s | m | h | d`, no sign, no spaces; the total must fit a
    /// `TimeDelta`. The before-magnitude is negated into the offset's
    /// sign domain (`"10m"` → −600 s, `"0s"` → [`at_due`](Self::at_due));
    /// canonical forms are content, not input echoes (`"2d"` parses to
    /// the same offset [`text_form`](Self::text_form) renders `"48h"`).
    #[must_use]
    pub fn parse_text_form(raw: &str) -> Option<Self> {
        if raw.is_empty() {
            return None;
        }
        let bytes = raw.as_bytes();
        let mut total: i64 = 0;
        let mut i = 0;
        while i < bytes.len() {
            let digits_start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i == digits_start {
                return None; // a component without its number
            }
            let value = i64::try_from(raw[digits_start..i].parse::<u64>().ok()?).ok()?;
            let unit_secs = match bytes.get(i)? {
                b's' => 1,
                b'm' => 60,
                b'h' => 3_600,
                b'd' => 86_400,
                _ => return None,
            };
            i += 1;
            total = total.checked_add(value.checked_mul(unit_secs)?)?;
        }
        Self::before(chrono::TimeDelta::try_seconds(-total)?).ok()
    }
}

/// Positive offsets and sub-second offsets are not constructible; both
/// rejections happen at the [`before`](ReminderOffset::before) mint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum InvalidReminderOffset {
    /// The proposed offset lies after the due date.
    #[error("reminder offset must not be after the due date")]
    AfterDue,
    /// The proposed offset carries a sub-second component. Offsets are
    /// whole-second so the storage column, the text form, and the synced
    /// document field can never diverge from the evaluated value (a
    /// reload would recompute a different occurrence and re-fire a
    /// delivered reminder).
    #[error("reminder offset must be a whole number of seconds")]
    NotWholeSecond,
}

impl Serialize for ReminderOffset {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        // The same `(seconds, nanoseconds)` tuple the derived chrono
        // impl produced — the wire shape is unchanged.
        <(i64, i32) as Serialize>::serialize(
            &(self.0.num_seconds(), self.0.subsec_nanos()),
            serializer,
        )
    }
}

impl<'de> Deserialize<'de> for ReminderOffset {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let (secs, nanos) = <(i64, i32) as Deserialize>::deserialize(deserializer)?;
        let nanos = u32::try_from(nanos)
            .map_err(|_| serde::de::Error::custom("invalid reminder offset"))?;
        let dur = chrono::TimeDelta::new(secs, nanos)
            .ok_or_else(|| serde::de::Error::custom("TimeDelta out of bounds"))?;
        // The mint is the integrity check: positive or sub-second data
        // is corruption, not a clamped value (the storage codec's
        // standing precedent, applied to the serde lane).
        Self::before(dur).map_err(|_| serde::de::Error::custom("invalid reminder offset"))
    }
}

/// Which reminder a firing/delivery/snooze refers to: an explicit rule,
/// a task's implicit default (no stored row — its identity is the task),
/// or a standalone adhoc alarm (device-local; the label lives on the
/// alarm row).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ReminderRef {
    /// An explicit rule.
    Rule(ReminderRuleId),
    /// The task's implicit default reminder.
    Default(TaskId),
    /// A standalone adhoc alarm.
    Adhoc(AdhocAlarmId),
}

impl ReminderRef {
    /// The text form shared by the storage codec and the snooze-map key
    /// encoding (`"rule:<uuid>"` / `"default:<task-uuid>"` /
    /// `"alarm:<uuid>"`).
    #[must_use]
    pub fn text_form(self) -> String {
        match self {
            Self::Rule(id) => format!("rule:{}", id.as_uuid()),
            Self::Default(task) => format!("default:{}", task.as_uuid()),
            Self::Adhoc(id) => format!("alarm:{}", id.as_uuid()),
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
        if let Some(id) = raw.strip_prefix("alarm:") {
            return Some(Self::Adhoc(AdhocAlarmId::from(
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
    /// The rule's embedded LWW write clock (D-S9): consumed by the
    /// sidecar document lane — every rule mutation stamps it, creation
    /// stamps it equal to `created_at`; no rule-edit verbs exist yet
    /// (removals delete, they never stamp a survivor).
    pub updated_at: DateTime<Utc>,
}

/// The [`CreateAdhocAlarm`](crate::messages::StateCommand::CreateAdhocAlarm)
/// timing input: absolute-vs-duration is a CLI concern, never a domain
/// concern — the planner stamps `In`. Serde is externally tagged (the
/// `StateCommand` default).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum AdhocWhen {
    /// Fire at this absolute instant (a past instant is legal data: it
    /// fires on the next tick).
    At(DateTime<Utc>),
    /// Fire `now + dur`, stamped by the engine's injected clock (the
    /// `SnoozeReminder` stamp precedent). `dur ≤ 0` is a typed
    /// rejection.
    In(chrono::TimeDelta),
}

/// A standalone alarm: fires once at its absolute `fire_at` instant
/// (plus whatever snooze-chain movement follows), then dies by
/// settlement — the post-batch prune removes it once its effective
/// occurrence is delivered **and** dismissed (the R3 settle: between
/// fire and dismiss the row and its delivery are the live in-app
/// surface, snoozable and command-resolvable). Cancel is the
/// off-switch; there is no mute (mute is task-shaped) and no
/// done/archived state (map membership is liveness).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdhocAlarm {
    /// Local identity.
    pub id: AdhocAlarmId,
    /// The notification title (no task to snapshot a title from).
    pub label: String,
    /// The absolute fire instant.
    pub fire_at: DateTime<Utc>,
    /// Creation stamp.
    pub created_at: DateTime<Utc>,
}

/// Proof that an occurrence was notified — the per-device at-most-once
/// guard for the OS toast. Keyed by `(reminder, occurrence)`, NOT by rule
/// alone: moving the due date or snoozing creates a new occurrence and
/// re-arms. Additive, set-union-mergeable by construction. Stays out of
/// the sync document entirely (R3: fire receipts are per-device facts).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ReminderDelivery {
    /// Which reminder fired.
    pub reminder: ReminderRef,
    /// The fire instant this delivery settled.
    pub occurrence: DateTime<Utc>,
    /// When the engine recorded it (stamped at fire time).
    pub delivered_at: DateTime<Utc>,
}

/// A user dismissal of a fired occurrence — the fleet-wide "stop showing
/// this" fact (the R3 two-fact split: the delivery receipt stays local,
/// the dismissal is the synced surface). Keyed by `(reminder,
/// occurrence)` like [`ReminderDelivery`]; a dismissal is permanent for
/// its occurrence (there is no un-dismiss), so the set is additive and
/// tombstone-free by construction.
///
/// The state-side type is representable for **all** [`ReminderRef`]
/// kinds: local dismissal of default-reminder and adhoc-alarm
/// notifications must clear them in-app, and their occurrences are
/// per-device facts (they never roam). Only `Rule`-ref rows ever
/// serialize into the synced `dismissals` document section — the wire
/// type carries a bare `ReminderRuleId` (the fail-closed mint, design
/// D-4), and the section's emission filter enforces the split; nothing in
/// this crate ever serializes a `Default`/`Adhoc` row to the document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ReminderDismissal {
    /// Which reminder's notification was dismissed.
    pub reminder: ReminderRef,
    /// The occurrence the dismissal settles.
    pub occurrence: DateTime<Utc>,
    /// When the user dismissed (stamped at the dismiss command) — the
    /// row's LWW write clock for the document lane.
    pub dismissed_at: DateTime<Utc>,
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
    /// What the firing is anchored on (the watched task, or the alarm).
    pub anchor: ReminderAnchor,
    /// The effective occurrence being fired.
    pub occurrence: DateTime<Utc>,
}

/// What a firing/notification is anchored on: the task a reminder
/// watches, or the alarm that owns its label. Alarms carry the typed
/// anchor so consumers match without re-matching the ref.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ReminderAnchor {
    /// A task-anchored reminder (explicit rule or implicit default).
    Task(TaskId),
    /// A standalone alarm (the label lives on the [`AdhocAlarm`] row).
    Alarm(AdhocAlarmId),
}

/// Persisted reminder facts carried in `AppState`/`PersistedState`.
/// Field order is the snapshot order: rules, deliveries, dismissals,
/// snoozes, muted, alarms. Delivered-ness, dismissed-ness, and
/// snoozed-ness are plain set/map membership checks.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReminderState {
    /// Explicit rules by id.
    pub rules: BTreeMap<ReminderRuleId, ReminderRule>,
    /// Delivery proofs (≤ 1 row per `(reminder, occurrence)` — enforced
    /// by [`apply_actions`](crate::persistence::apply_actions), pinned by
    /// property).
    pub deliveries: BTreeSet<ReminderDelivery>,
    /// User dismissals (≤ 1 row per `(reminder, occurrence)` — the same
    /// first-write-wins discipline as `deliveries`). Superset of the
    /// synced lane by construction: Rule-ref rows roam, Default/Adhoc
    /// rows stay device-local.
    pub dismissals: BTreeSet<ReminderDismissal>,
    /// Snooze chain links: `(reminder, occurrence) → rescheduled`
    /// instant. Chains compose (a snooze target is itself snoozable).
    pub snoozes: BTreeMap<ReminderSnoozeKey, DateTime<Utc>>,
    /// Tasks muted out of firings entirely. The overdue *banner* ignores
    /// this set — mute means "stop nagging", not "hide reality".
    pub muted: BTreeSet<TaskId>,
    /// Standalone alarms by id. Map membership is liveness: a removed
    /// row cannot fire, and no done/tombstone state exists.
    pub alarms: BTreeMap<AdhocAlarmId, AdhocAlarm>,
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

    /// Dismissal lookup on the `(reminder, occurrence)` key — the
    /// [`is_delivered`](Self::is_delivered) sibling, same linear-scan
    /// rationale.
    #[must_use]
    pub fn is_dismissed(&self, reminder: &ReminderRef, occurrence: &DateTime<Utc>) -> bool {
        self.dismissals
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
/// a snapshot at fire time (the task may be edited later; an alarm's
/// label is its own row). Pure data — the engine's notifier *channel*
/// lives in the actor crates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Notification {
    /// Which reminder fired — correlates a later dismiss/snooze reply
    /// with this payload (both carry the ref + occurrence).
    pub reminder: ReminderRef,
    /// What the notification is anchored on (the watched task, or the
    /// alarm that owns the label).
    pub anchor: ReminderAnchor,
    /// Title snapshot at fire time.
    pub title: String,
    /// The effective occurrence that fired.
    pub occurrence: DateTime<Utc>,
}

/// The engine→notifier channel payload (R3, design D-10): the fire and
/// withdraw directions share the notifier channel as a two-variant
/// enum. `Fire` is the v1 toast (unchanged semantics); `Withdraw` is
/// the best-effort OS-toast withdrawal a landing dismissal asks for —
/// best-effort by contract: a sink that cannot withdraw no-ops, and a
/// failed withdrawal is never an error surface (the in-app notification
/// is the universal source of truth; closing a toast natively is not a
/// dismissal).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotifierMessage {
    /// Render one reminder notification.
    Fire(Notification),
    /// A dismissal landed (a local `DismissReminder`, the Mark Done
    /// planner's rows, or a folded synced fact): withdraw the toast for
    /// the occurrence.
    Withdraw {
        /// Which reminder's notification to withdraw.
        reminder: ReminderRef,
        /// The occurrence whose toast should go.
        occurrence: DateTime<Utc>,
    },
}

/// Pure payload assembly: one [`Notification`] per firing, with the
/// title snapshot from the task map (task anchors) or the alarm row's
/// label (alarm anchors). A firing whose anchor cannot resolve — a task
/// missing from the map, an alarm missing from the alarms map — is
/// skipped (total on arbitrary input; unreachable through the engine's
/// sequential tick, which evaluates and assembles in one message).
#[must_use]
pub fn notifications_for(
    firings: &[ReminderFiring],
    tasks: &BTreeMap<TaskId, Task>,
    reminders: &ReminderState,
) -> Vec<Notification> {
    firings
        .iter()
        .filter_map(|firing| match firing.anchor {
            ReminderAnchor::Task(task_id) => tasks.get(&task_id).map(|task| Notification {
                reminder: firing.reminder,
                anchor: firing.anchor,
                title: task.title.clone(),
                occurrence: firing.occurrence,
            }),
            ReminderAnchor::Alarm(alarm_id) => {
                reminders.alarms.get(&alarm_id).map(|alarm| Notification {
                    reminder: firing.reminder,
                    anchor: firing.anchor,
                    title: alarm.label.clone(),
                    occurrence: firing.occurrence,
                })
            }
        })
        .collect()
}

/// Prunes snooze links whose source AND target occurrences are both
/// dismissed (the R3 settle): such a link can never influence
/// evaluation again — both endpoints are dead in both readers (no fire,
/// `due_firings`; no row, `active_notifications`). The compaction
/// primitive shared by
/// [`apply_actions`](crate::persistence::apply_actions)' post-batch pass
/// and the test-support state normalizer; one definition site.
///
/// Delivered-ness is deliberately not a criterion: a merely-delivered
/// occurrence is the live in-app surface under R3, not a dead one, and
/// pruning its links would let the chain walk revert to an earlier node
/// and resurrect a dismissed notification (the regression trace).
/// Target-only pruning would be observable: a snoozed-but-unsettled
/// source would re-arm the moment its target settles. Both-endpoints
/// pruning is unconditionally unobservable (property-pinned, both
/// readers).
pub fn compact_snooze_chains(reminders: &mut ReminderState) {
    let keep: Vec<ReminderSnoozeKey> = reminders
        .snoozes
        .iter()
        .filter(|(key, target)| {
            !(reminders.is_dismissed(&key.reminder, &key.occurrence)
                && reminders.is_dismissed(&key.reminder, target))
        })
        .map(|(key, _)| *key)
        .collect();
    reminders.snoozes.retain(|key, _| keep.contains(key));
}

/// Removes an alarm row and prunes its `Adhoc`-ref deliveries,
/// dismissals, and snooze links with it — the `DeleteReminderRule`
/// cascade shape, one definition site shared by the
/// [`DeleteAdhocAlarm`](crate::persistence::PersistenceAction::DeleteAdhocAlarm)
/// arm and [`prune_settled_alarms`] (nothing can re-fire from a pruned
/// map row).
pub(crate) fn remove_alarm(reminders: &mut ReminderState, id: AdhocAlarmId) {
    reminders.alarms.remove(&id);
    let reminder = ReminderRef::Adhoc(id);
    reminders.deliveries.retain(|d| d.reminder != reminder);
    reminders.dismissals.retain(|d| d.reminder != reminder);
    reminders.snoozes.retain(|key, _| key.reminder != reminder);
}

/// Prunes alarms whose effective occurrence is delivered **and**
/// dismissed: such an alarm is settled (the R3 two-fact split — the
/// delivery receipt alone is the per-device toast guard, not the
/// settle) and dead in both readers. Between fire and dismiss the row
/// and its delivery live: `active_notifications`' alarm arm is
/// reachable and `SnoozeReminder`/`CancelAdhocAlarm` on a fired alarm
/// resolve. The [`compact_snooze_chains`] sibling — the caller orders
/// it AFTER compaction so a just-settled chain reverts to its base
/// before the check reads it (the composition: both endpoints dismissed
/// → link compacts → effective reverts → base delivered ∧ dismissed →
/// alarm prunes). Pruning an alarm removes its deliveries, dismissals,
/// and snooze links with it. Unobservable in [`due_firings`] and
/// [`active_notifications`] (property-pinned).
pub fn prune_settled_alarms(reminders: &mut ReminderState) {
    let settled: Vec<AdhocAlarmId> = reminders
        .alarms
        .keys()
        .copied()
        .filter(|&id| {
            alarm_occurrence(reminders, id).is_some_and(|eff| {
                let reminder = ReminderRef::Adhoc(id);
                reminders.is_delivered(&reminder, &eff) && reminders.is_dismissed(&reminder, &eff)
            })
        })
        .collect();
    for id in settled {
        remove_alarm(reminders, id);
    }
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
    Some(walk_chain(base, *reminder, &reminders.snoozes))
}

/// The snooze-chain walk shared by every occurrence reader: follows
/// `(reminder, occurrence) → target` links from `base`, cycle-bounded
/// (a `visited` set — on a cyclic link chain, unsanctionable data no
/// sanctioned write can produce, the walk stops at the last node before
/// the repeat, keeping the reader total on arbitrary input). Pure
/// motion out of `effective_occurrence`.
fn walk_chain(
    base: DateTime<Utc>,
    reminder: ReminderRef,
    snoozes: &BTreeMap<ReminderSnoozeKey, DateTime<Utc>>,
) -> DateTime<Utc> {
    let mut effective = base;
    let mut visited = BTreeSet::new();
    visited.insert(base);
    while let Some(&target) = snoozes.get(&ReminderSnoozeKey {
        reminder,
        occurrence: effective,
    }) {
        if !visited.insert(target) {
            break;
        }
        effective = target;
    }
    effective
}

/// Effective occurrence of one adhoc alarm: its absolute `fire_at` base,
/// or wherever the snooze chain has moved it. The alarms-side reading
/// used by the settlement prune, the CLI list rows, and
/// [`notifications_for`]'s label lookup — no `tasks`/`defaults` context,
/// unlike [`effective_occurrence`]. `None` ⇔ the alarm row is missing.
#[must_use]
pub fn alarm_occurrence(reminders: &ReminderState, id: AdhocAlarmId) -> Option<DateTime<Utc>> {
    let base = reminders.alarms.get(&id)?.fire_at;
    Some(walk_chain(base, ReminderRef::Adhoc(id), &reminders.snoozes))
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
        // Absolute: no arithmetic, so `None` is only the missing row.
        ReminderRef::Adhoc(id) => Some(reminders.alarms.get(&id)?.fire_at),
    }
}

/// Everything that must fire at `now`: explicit rules plus implicit
/// defaults, occurrence-keyed, snooze- and mute-aware — plus the
/// standalone alarms (eligibility = alarms-map membership; mute does
/// not apply to alarms, cancel is their off-switch). Sorted by
/// `(occurrence, reminder)` — a total order, so output is fully
/// deterministic.
///
/// Task-side eligibility: the task must be live and open with a due date
/// and not muted; a task's explicit rules (in rule-id order) replace its
/// implicit default. A reminder fires iff its effective occurrence is ≤
/// `now` and neither delivered nor dismissed (the local receipt guards
/// the OS toast on this device; a dismissal — local or synced-in —
/// suppresses the occurrence everywhere, R3).
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
                && !reminders.is_dismissed(&reminder, &effective)
            {
                firings.push(ReminderFiring {
                    reminder,
                    anchor: ReminderAnchor::Task(task.id),
                    occurrence: effective,
                });
            }
        }
    }
    // The alarms pass: map membership is liveness, no mute check (the
    // muted set is task-shaped; cancel removes the row).
    for &id in reminders.alarms.keys() {
        if let Some(effective) = alarm_occurrence(reminders, id)
            && effective <= now
            && !reminders.is_delivered(&ReminderRef::Adhoc(id), &effective)
            && !reminders.is_dismissed(&ReminderRef::Adhoc(id), &effective)
        {
            firings.push(ReminderFiring {
                reminder: ReminderRef::Adhoc(id),
                anchor: ReminderAnchor::Alarm(id),
                occurrence: effective,
            });
        }
    }
    firings.sort_by_key(|f| (f.occurrence, f.reminder));
    firings
}

/// The in-app notification surface (R3, design D-6): one
/// [`Notification`] per occurrence this device **fired** (a delivery
/// receipt exists for `(reminder, effective occurrence)`) that is **not
/// dismissed** — derived at read time, never stored, so nothing can
/// drift. The pure view the dumb presentation client reads (exactly like
/// [`overdue_tasks`]) and the Mark Done planner consults for the
/// currently-active occurrences its dismissal rows must cover (one
/// definition, two consumers).
///
/// Eligibility mirrors [`due_firings`]: the anchor must still resolve
/// (a live, open, due-anchored task; or a live alarm row) and mute hides
/// a task's notifications while it holds (mute = stop nagging; the
/// receipts survive, so un-muting restores the surface). A dismissed
/// occurrence never returns — there is no un-dismiss. Sorted by
/// `(occurrence, reminder)` — the house deterministic order.
#[must_use]
pub fn active_notifications(
    tasks: &BTreeMap<TaskId, Task>,
    reminders: &ReminderState,
    defaults: &ReminderDefaults,
    now: DateTime<Utc>,
) -> Vec<Notification> {
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
            vec![ReminderRef::Default(task.id)]
        } else {
            rules.iter().map(|r| ReminderRef::Rule(r.id)).collect()
        };
        for reminder in candidates {
            if let Some(effective) = reminders.effective_occurrence(&reminder, tasks, defaults)
                && effective <= now
                && reminders.is_delivered(&reminder, &effective)
                && !reminders.is_dismissed(&reminder, &effective)
            {
                firings.push(ReminderFiring {
                    reminder,
                    anchor: ReminderAnchor::Task(task.id),
                    occurrence: effective,
                });
            }
        }
    }
    for &id in reminders.alarms.keys() {
        if let Some(effective) = alarm_occurrence(reminders, id)
            && effective <= now
            && reminders.is_delivered(&ReminderRef::Adhoc(id), &effective)
            && !reminders.is_dismissed(&ReminderRef::Adhoc(id), &effective)
        {
            firings.push(ReminderFiring {
                reminder: ReminderRef::Adhoc(id),
                anchor: ReminderAnchor::Alarm(id),
                occurrence: effective,
            });
        }
    }
    firings.sort_by_key(|f| (f.occurrence, f.reminder));
    notifications_for(&firings, tasks, reminders)
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
            scheduled: None,
            done: None,
            stack: crate::ids::StackId::from(uuid::Uuid::from_u128(1)),
            order,
            labels: BTreeSet::default(),
            effort: None,
            importance: None,
            urgency: None,
            recur: None,

            archived: false,
            deleted: false,
            clocks: crate::entities::TaskClocks {
                title: ts(100),
                description: ts(100),
                duedate: ts(100),
                scheduled: ts(100),
                done: ts(100),
                position: ts(100),
                labels: ts(100),
                effort: ts(100),
                importance: ts(100),
                urgency: ts(100),
                recur: ts(100),

                archived: ts(100),
                deleted: ts(100),
            },
            remote_seen: None,
            remote_baseline: None,
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
                    ReminderRef::Adhoc(_) => continue,
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
                if effective <= now
                    && !reminders.is_delivered(&reminder, &effective)
                    && !reminders.is_dismissed(&reminder, &effective)
                {
                    expected.push((effective, reminder));
                }
            }
        }
        // The alarms loop: absolute base, same walk, same gate; mute
        // does not apply.
        for (&id, alarm) in &reminders.alarms {
            let reminder = ReminderRef::Adhoc(id);
            let mut effective = alarm.fire_at;
            let mut seen = BTreeSet::from([alarm.fire_at]);
            while let Some(&target) = reminders.snoozes.get(&ReminderSnoozeKey {
                reminder,
                occurrence: effective,
            }) {
                if !seen.insert(target) {
                    break;
                }
                effective = target;
            }
            if effective <= now
                && !reminders.is_delivered(&reminder, &effective)
                && !reminders.is_dismissed(&reminder, &effective)
            {
                expected.push((effective, reminder));
            }
        }
        expected.sort();
        expected
    }

    /// Independent re-derivation of the active-notification surface: does
    /// not call `effective_occurrence` or `active_notifications` — walks
    /// base occurrence and the snooze chain by hand, gates on
    /// delivered ∧ ¬dismissed, resolves titles from the task/alarm rows,
    /// and skips missing anchors (the `notifications_for` contract).
    fn expected_active(
        tasks: &std::collections::BTreeMap<TaskId, Task>,
        reminders: &ReminderState,
        defaults: &ReminderDefaults,
        now: DateTime<Utc>,
    ) -> Vec<Notification> {
        let mut expected = Vec::new();
        let mut active: Vec<(DateTime<Utc>, ReminderRef, ReminderAnchor)> = Vec::new();
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
                    ReminderRef::Adhoc(_) => continue,
                };
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
                if effective <= now
                    && reminders.is_delivered(&reminder, &effective)
                    && !reminders.is_dismissed(&reminder, &effective)
                {
                    active.push((effective, reminder, ReminderAnchor::Task(task.id)));
                }
            }
        }
        for (&id, alarm) in &reminders.alarms {
            let reminder = ReminderRef::Adhoc(id);
            let mut effective = alarm.fire_at;
            let mut seen = BTreeSet::from([alarm.fire_at]);
            while let Some(&target) = reminders.snoozes.get(&ReminderSnoozeKey {
                reminder,
                occurrence: effective,
            }) {
                if !seen.insert(target) {
                    break;
                }
                effective = target;
            }
            if effective <= now
                && reminders.is_delivered(&reminder, &effective)
                && !reminders.is_dismissed(&reminder, &effective)
            {
                active.push((effective, reminder, ReminderAnchor::Alarm(id)));
            }
        }
        active.sort();
        for (occurrence, reminder, anchor) in active {
            let title = match anchor {
                ReminderAnchor::Task(task_id) => {
                    let Some(task) = tasks.get(&task_id) else {
                        continue;
                    };
                    task.title.clone()
                }
                ReminderAnchor::Alarm(alarm_id) => {
                    let Some(alarm) = reminders.alarms.get(&alarm_id) else {
                        continue;
                    };
                    alarm.label.clone()
                }
            };
            expected.push(Notification {
                reminder,
                anchor,
                title,
                occurrence,
            });
        }
        expected
    }

    // ---- Offset text form ---------------------------------------------

    fn offset_before(secs: i64) -> ReminderOffset {
        ReminderOffset::before(chrono::TimeDelta::try_seconds(-secs).expect("valid offset"))
            .expect("negative duration is a legal offset")
    }

    #[test]
    fn text_form_renders_the_canonical_forms() {
        assert_eq!(ReminderOffset::at_due().text_form(), "0s");
        assert_eq!(offset_before(600).text_form(), "10m");
        assert_eq!(offset_before(5_400).text_form(), "1h30m");
        // Canonical forms are content, not input echoes: two days
        // renders as 48 hours.
        assert_eq!(offset_before(2 * 86_400).text_form(), "48h");
    }

    #[test]
    fn parse_text_form_accepts_the_grammar_and_canonicalizes() {
        assert_eq!(
            ReminderOffset::parse_text_form("0s"),
            Some(ReminderOffset::at_due())
        );
        assert_eq!(
            ReminderOffset::parse_text_form("10m"),
            Some(offset_before(600))
        );
        assert_eq!(
            ReminderOffset::parse_text_form("1h30m"),
            Some(offset_before(5_400))
        );
        assert_eq!(
            ReminderOffset::parse_text_form("2d"),
            Some(offset_before(2 * 86_400))
        );
        assert_eq!(
            ReminderOffset::parse_text_form("90s"),
            Some(offset_before(90))
        );
    }

    #[test]
    fn parse_text_form_rejects_garbage_and_overflow() {
        for raw in [
            "",
            "5min",
            "10 m",
            "m",
            "10",
            "-10m",
            "+10m",
            "10x",
            "9223372036854775807d9223372036854775807s",
        ] {
            assert!(
                ReminderOffset::parse_text_form(raw).is_none(),
                "{raw} must be rejected"
            );
        }
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
            which in 0_u8..3,
        ) {
            let postpone = chrono::TimeDelta::try_seconds(postpone_secs).unwrap();
            // The alarm arm needs no task anchor (alarms are
            // task-independent): no duedate, so no default competes.
            let task = live_task(1, (which != 2).then_some(now), 0);
            let tasks = BTreeMap::from([(task.id, task.clone())]);
            let defaults = ReminderDefaults { default_before: offset };
            let mut reminders = ReminderState::default();
            let reminder = match which {
                0 => {
                    let rule = ReminderRule {
                        id: ReminderRuleId::from(uuid::Uuid::from_u128(10)),
                        task: task.id,
                        offset,
                        created_at: now,
                        updated_at: now,
                    };
                    let r = ReminderRef::Rule(rule.id);
                    reminders.rules.insert(rule.id, rule);
                    r
                }
                1 => ReminderRef::Default(task.id),
                _ => {
                    let alarm = AdhocAlarm {
                        id: AdhocAlarmId::from(uuid::Uuid::from_u128(10)),
                        label: "p7".into(),
                        fire_at: now,
                        created_at: now,
                    };
                    let r = ReminderRef::Adhoc(alarm.id);
                    reminders.alarms.insert(alarm.id, alarm);
                    r
                }
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
            // Alarm firings are observed before the mute: muting every
            // task must not silence any of them.
            let alarm_firings: Vec<ReminderRef> =
                due_firings(&tasks, &reminders, &defaults, now)
                    .into_iter()
                    .filter(|f| matches!(f.anchor, ReminderAnchor::Alarm(_)))
                    .map(|f| f.reminder)
                    .collect();
            reminders.muted = overdue.iter().copied().collect();
            let firings = due_firings(&tasks, &reminders, &defaults, now);
            let fired_tasks: Vec<TaskId> =
                firings.iter().filter_map(|f| match f.anchor {
                    ReminderAnchor::Task(t) => Some(t),
                    ReminderAnchor::Alarm(_) => None,
                }).collect();
            prop_assert!(fired_tasks.iter().all(|t| !reminders.muted.contains(t)));
            let fired_alarms: Vec<ReminderRef> = firings
                .into_iter()
                .filter(|f| matches!(f.anchor, ReminderAnchor::Alarm(_)))
                .map(|f| f.reminder)
                .collect();
            prop_assert_eq!(
                fired_alarms, alarm_firings,
                "muting every task does not silence any alarm",
            );
            // Banner truth ignores mute entirely.
            let banner: BTreeSet<TaskId> =
                overdue_tasks(&tasks, now).into_iter().map(|t| t.id).collect();
            prop_assert_eq!(banner, overdue.into_iter().collect::<BTreeSet<_>>());
        }

        #[test]
        fn p9_text_form_roundtrips_grammar_magnitudes(
            secs in 0_i64..=400 * 86_400,
        ) {
            let offset = ReminderOffset::before(
                chrono::TimeDelta::try_seconds(-secs).unwrap(),
            )
            .unwrap();
            prop_assert_eq!(ReminderOffset::parse_text_form(&offset.text_form()), Some(offset));
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
            updated_at: ts(100),
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
            updated_at: ts(100),
        };
        let rule_b = ReminderRule {
            id: ReminderRuleId::from(uuid::Uuid::from_u128(11)),
            task: task_b.id,
            offset: ReminderOffset::at_due(),
            created_at: ts(100),
            updated_at: ts(100),
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

    /// The double-fire impossibility, pinned at the mint: a sub-second
    /// offset — which every persistence lane would truncate, diverging
    /// the reload-recomputed occurrence — is rejected typed, not
    /// truncated or clamped.
    #[test]
    fn before_rejects_sub_second_durations() {
        assert!(matches!(
            ReminderOffset::before(chrono::TimeDelta::try_milliseconds(-500).unwrap()),
            Err(InvalidReminderOffset::NotWholeSecond)
        ));
        // Whole-second inputs (including zero) stay accepted.
        assert!(ReminderOffset::before(chrono::TimeDelta::zero()).is_ok());
        assert!(
            ReminderOffset::before(chrono::TimeDelta::try_seconds(-90).unwrap()).is_ok(),
            "whole-second negative still mints"
        );
    }

    /// Precedence pin: the positive check dominates, so a positive
    /// sub-second input answers `AfterDue` — same as any other positive
    /// input.
    #[test]
    fn after_due_dominates_the_precedence() {
        assert!(matches!(
            ReminderOffset::before(chrono::TimeDelta::try_milliseconds(500).unwrap()),
            Err(InvalidReminderOffset::AfterDue)
        ));
    }

    /// The text-form grammar only ever produces whole-second offsets:
    /// every accepted form mints a value whose evaluated duration has no
    /// sub-second component (deterministic sweep over the grammar's
    /// units and magnitudes; P9 round-trips the forms).
    #[test]
    fn text_form_and_parse_stay_whole_second() {
        for form in [
            "0s",
            "1s",
            "90s",
            "10m",
            "59m",
            "1h",
            "1h30m",
            "23h59m59s",
            "1d",
            "2d",
            "48h",
            "1d2h3m4s",
        ] {
            let offset = ReminderOffset::parse_text_form(form).unwrap_or_else(|| {
                panic!("grammar accepts {form}");
            });
            assert_eq!(
                offset.as_duration().subsec_nanos(),
                0,
                "{form} parses whole-second"
            );
        }
    }

    /// Serde keeps the derived chrono tuple wire shape
    /// (`(seconds, nanoseconds)`) but mints through `before`: sub-second
    /// or positive tuples are corruption, rejected typed — never
    /// clamped.
    #[test]
    fn serde_roundtrips_whole_second_and_rejects_corruption() {
        for secs in [0_i64, -1, -90, -86_400] {
            let offset = ReminderOffset::before(chrono::TimeDelta::try_seconds(secs).unwrap())
                .expect("whole-second");
            let json = serde_json::to_string(&offset).expect("serializes");
            assert_eq!(json, format!("[{secs},0]"), "unchanged tuple wire shape");
            let back: ReminderOffset = serde_json::from_str(&json).expect("round-trips");
            assert_eq!(back, offset);
        }
        for corrupt in ["[-600,500000000]", "[1,0]", "[0,-500000000]"] {
            let err = serde_json::from_str::<ReminderOffset>(corrupt)
                .expect_err("corrupt tuple rejected");
            assert!(
                err.is_data(),
                "corruption is a data error, not a syntax one: {corrupt}"
            );
        }
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
    fn is_dismissed_keys_on_the_pair_not_dismissed_at() {
        let mut state = ReminderState::default();
        let r = ref_of(3);
        state.dismissals.insert(ReminderDismissal {
            reminder: r,
            occurrence: ts(1_000),
            dismissed_at: ts(1_001),
        });
        assert!(state.is_dismissed(&r, &ts(1_000)));
        assert!(!state.is_dismissed(&r, &ts(2_000)), "other occurrence");
        assert!(
            !state.is_dismissed(&ref_of(4), &ts(1_000)),
            "other reminder"
        );
    }

    #[test]
    fn dismissal_suppresses_refire_and_clears_the_active_surface() {
        let now = ts(20_000);
        let task = live_task(1, Some(ts(19_000)), 0);
        let tasks = BTreeMap::from([(task.id, task.clone())]);
        let rule = ReminderRule {
            id: ReminderRuleId::from(uuid::Uuid::from_u128(10)),
            task: task.id,
            offset: ReminderOffset::at_due(),
            created_at: ts(100),
            updated_at: ts(100),
        };
        let reminder = ReminderRef::Rule(rule.id);
        let mut reminders = ReminderState::default();
        reminders.rules.insert(rule.id, rule);
        let defaults = ReminderDefaults::default();

        // Fires at the due instant...
        let fired = due_firings(&tasks, &reminders, &defaults, now);
        assert_eq!(fired.len(), 1);
        let occurrence = fired[0].occurrence;
        reminders.deliveries.insert(ReminderDelivery {
            reminder,
            occurrence,
            delivered_at: now,
        });
        // ...is quiet afterwards, but active on the surface...
        assert_eq!(due_firings(&tasks, &reminders, &defaults, now), Vec::new());
        let active = active_notifications(&tasks, &reminders, &defaults, now);
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].reminder, reminder);
        assert_eq!(active[0].occurrence, occurrence);
        assert_eq!(active[0].title, task.title);
        // ...until the dismissal clears it everywhere: no refire, no
        // surface entry, at `now` or ever after.
        reminders.dismissals.insert(ReminderDismissal {
            reminder,
            occurrence,
            dismissed_at: now,
        });
        assert_eq!(due_firings(&tasks, &reminders, &defaults, now), Vec::new());
        assert_eq!(
            due_firings(
                &tasks,
                &reminders,
                &defaults,
                now + chrono::TimeDelta::try_hours(1).unwrap()
            ),
            Vec::new()
        );
        assert_eq!(
            active_notifications(&tasks, &reminders, &defaults, now),
            Vec::new()
        );
        // The dismissal is permanent for the occurrence only: a new
        // occurrence (the due date moved) re-arms by construction.
        let mut moved = tasks.get(&task.id).unwrap().clone();
        moved.duedate = Some(ts(40_000));
        let tasks = BTreeMap::from([(moved.id, moved)]);
        assert_eq!(
            due_firings(&tasks, &reminders, &defaults, ts(40_000)).len(),
            1,
            "a new occurrence re-arms"
        );
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
        let alarm = ReminderRef::Adhoc(AdhocAlarmId::from(uuid::Uuid::from_u128(13)));
        for reminder in [rule, default, alarm] {
            assert_eq!(
                ReminderRef::parse_text_form(&reminder.text_form()),
                Some(reminder),
                "parse is the inverse of text_form"
            );
        }
        assert!(alarm.text_form().starts_with("alarm:"));
        for garbage in [
            "",
            "garbage",
            "rule:",
            "default:",
            "alarm:",
            "rule:not-a-uuid",
            "default:not-a-uuid",
            "alarm:not-a-uuid",
            "alarm:1|default:2",
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
            anchor: ReminderAnchor::Task(TaskId::from(uuid::Uuid::from_u128(4))),
            title: "ship it".into(),
            occurrence: ts(1_000),
        };
        let back: Notification =
            serde_json::from_str(&serde_json::to_string(&notification).unwrap()).unwrap();
        assert_eq!(back, notification);
    }

    #[test]
    fn notifications_for_snapshots_titles_skips_missing_and_labels_alarms() {
        let task = live_task(1, Some(ts(500)), 0);
        let tasks = BTreeMap::from([(task.id, task.clone())]);
        let alarm = crate::reminders::AdhocAlarm {
            id: AdhocAlarmId::from(uuid::Uuid::from_u128(20)),
            label: "steep the tea".into(),
            fire_at: ts(300),
            created_at: ts(100),
        };
        let mut reminders = ReminderState::default();
        reminders.alarms.insert(alarm.id, alarm.clone());
        let stranger = TaskId::from(uuid::Uuid::from_u128(999));
        let stranger_alarm = AdhocAlarmId::from(uuid::Uuid::from_u128(998));
        let firings = vec![
            ReminderFiring {
                reminder: ref_of(10),
                anchor: ReminderAnchor::Task(task.id),
                occurrence: ts(400),
            },
            ReminderFiring {
                reminder: ReminderRef::Default(stranger),
                anchor: ReminderAnchor::Task(stranger),
                occurrence: ts(401),
            },
            ReminderFiring {
                reminder: ReminderRef::Adhoc(alarm.id),
                anchor: ReminderAnchor::Alarm(alarm.id),
                occurrence: ts(402),
            },
            ReminderFiring {
                reminder: ReminderRef::Adhoc(stranger_alarm),
                anchor: ReminderAnchor::Alarm(stranger_alarm),
                occurrence: ts(403),
            },
        ];
        let notes = notifications_for(&firings, &tasks, &reminders);
        assert_eq!(
            notes,
            vec![
                Notification {
                    reminder: ref_of(10),
                    anchor: ReminderAnchor::Task(task.id),
                    title: task.title.clone(),
                    occurrence: ts(400),
                },
                Notification {
                    reminder: ReminderRef::Adhoc(alarm.id),
                    anchor: ReminderAnchor::Alarm(alarm.id),
                    title: alarm.label.clone(),
                    occurrence: ts(402),
                },
            ],
            "task payloads carry the task title; alarm payloads the alarm label; \
             missing anchors are skipped"
        );
        assert_eq!(notifications_for(&[], &tasks, &reminders), Vec::new());
    }

    proptest! {
        #![proptest_config(test_support::proptest_config(256))]

        #[test]
        fn reminder_types_roundtrip_through_serde(
            rule in test_support::reminder_rule_strategy(),
            delivery in test_support::reminder_delivery_strategy(),
            snooze in test_support::reminder_snooze_strategy(),
            offset in test_support::reminder_offset_strategy(),
            alarm in test_support::adhoc_alarm_strategy(),
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
            roundtrip(&crate::reminders::ReminderDismissal {
                reminder: delivery.reminder,
                occurrence: delivery.occurrence,
                dismissed_at: delivery.delivered_at,
            });
            roundtrip(&snooze);
            roundtrip(&offset);
            roundtrip(&alarm);
            roundtrip(&ReminderAnchor::Task(rule.task));
            roundtrip(&ReminderAnchor::Alarm(alarm.id));
            roundtrip(&ReminderDefaults {
                default_before: offset,
            });
            roundtrip(&ReminderFiring {
                reminder: delivery.reminder,
                anchor: ReminderAnchor::Task(rule.task),
                occurrence: firing_occurrence,
            });
            roundtrip(&Notification {
                reminder: delivery.reminder,
                anchor: ReminderAnchor::Alarm(alarm.id),
                title: alarm.label.clone(),
                occurrence: firing_occurrence,
            });
        }

        #[test]
        fn reminder_ref_roundtrips_through_serde(
            rule_raw in any::<u128>(),
            task_raw in any::<u128>(),
            alarm_raw in any::<u128>(),
        ) {
            for reminder in [
                ref_of(rule_raw),
                ReminderRef::Default(TaskId::from(uuid::Uuid::from_u128(task_raw))),
                ReminderRef::Adhoc(AdhocAlarmId::from(uuid::Uuid::from_u128(alarm_raw))),
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
                anchor: ReminderAnchor::Task(stranger),
                occurrence: occurrences[0],
            });
            for occurrence in &occurrences[1..] {
                firings.push(ReminderFiring {
                    reminder: any_ref,
                    anchor: ReminderAnchor::Task(anchor),
                    occurrence: *occurrence,
                });
            }
            let notes = notifications_for(&firings, &tasks, &ReminderState::default());
            prop_assert_eq!(notes.len() + 1, firings.len());
            for (note, firing) in notes.iter().zip(&firings[1..]) {
                prop_assert_eq!(note.reminder, firing.reminder);
                prop_assert_eq!(note.anchor, firing.anchor);
                prop_assert_eq!(note.occurrence, firing.occurrence);
                let ReminderAnchor::Task(task_id) = firing.anchor else {
                    unreachable!("the fixture only builds Task anchors");
                };
                prop_assert_eq!(&note.title, &tasks[&task_id].title);
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
            updated_at: created_at,
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
                updated_at: due,
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
                let ReminderAnchor::Task(task_id) = firing.anchor else {
                    // Alarm firings are covered by the delivered/order
                    // checks above (the fixture mints no alarms).
                    continue;
                };
                let task = &tasks[&task_id];
                prop_assert!(task.is_live() && task.is_open() && task.duedate.is_some());
                prop_assert!(!reminders.muted.contains(&task_id));
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

    proptest! {
        #![proptest_config(test_support::proptest_config(256))]

        #[test]
        fn p10_dismissed_occurrences_never_refire(
            (tasks, reminders, defaults, now) in eval_input(),
        ) {
            let f1 = due_firings(&tasks, &reminders, &defaults, now);
            let mut dismissed = reminders.clone();
            for f in &f1 {
                dismissed.dismissals.insert(ReminderDismissal {
                    reminder: f.reminder,
                    occurrence: f.occurrence,
                    dismissed_at: now,
                });
            }
            let f2 = due_firings(&tasks, &dismissed, &defaults, now);
            prop_assert!(f2.is_empty(), "dismissed firings must not re-fire at the same now");
            // And the surface is empty for exactly those occurrences...
            let active = active_notifications(&tasks, &dismissed, &defaults, now);
            prop_assert!(
                active
                    .iter()
                    .all(|n| !f1.iter().any(|f| f.reminder == n.reminder && f.occurrence == n.occurrence)),
                "a just-dismissed occurrence must not surface as active",
            );
        }

        #[test]
        fn p11_active_notifications_equal_the_independently_derived_set(
            (tasks, reminders, defaults, now) in eval_input(),
        ) {
            prop_assert_eq!(
                active_notifications(&tasks, &reminders, &defaults, now),
                expected_active(&tasks, &reminders, &defaults, now),
            );
        }

        #[test]
        fn p12_due_and_active_are_disjoint(
            (tasks, reminders, defaults, now) in eval_input(),
        ) {
            // Due = ¬delivered ∧ ¬dismissed; active = delivered ∧
            // ¬dismissed: no (reminder, occurrence) pair can be both.
            let due: BTreeSet<_> = due_firings(&tasks, &reminders, &defaults, now)
                .into_iter()
                .map(|f| (f.reminder, f.occurrence))
                .collect();
            let active: BTreeSet<_> = active_notifications(&tasks, &reminders, &defaults, now)
                .into_iter()
                .map(|n| (n.reminder, n.occurrence))
                .collect();
            prop_assert!(due.is_disjoint(&active));
        }

        #[test]
        fn p13_mark_done_dismissal_rows_cover_the_active_surface(
            (tasks, reminders, defaults, now) in eval_input(),
        ) {
            // The planner's dismissal emission derives from the same view
            // the UI reads (one definition, two consumers): completing a
            // task dismisses exactly the notifications that were active
            // for it — so after the batch, the surface has no entry for
            // the completed task's reminders.
            let Some((&task_id, _)) = tasks.iter().next() else {
                return Ok(());
            };
            let mut done = tasks.clone();
            let mut updated = done[&task_id].clone();
            updated.done = Some(now);
            done.insert(task_id, updated);
            let active_before: BTreeSet<_> = active_notifications(&tasks, &reminders, &defaults, now)
                .into_iter()
                .filter(|n| matches!(n.anchor, ReminderAnchor::Task(t) if t == task_id))
                .map(|n| (n.reminder, n.occurrence))
                .collect();
            let mut after = reminders.clone();
            for (reminder, occurrence) in &active_before {
                after.dismissals.insert(ReminderDismissal {
                    reminder: *reminder,
                    occurrence: *occurrence,
                    dismissed_at: now,
                });
            }
            let still_active: BTreeSet<_> =
                active_notifications(&done, &after, &defaults, now)
                    .into_iter()
                    .filter(|n| matches!(n.anchor, ReminderAnchor::Task(t) if t == task_id))
                    .map(|n| (n.reminder, n.occurrence))
                    .collect();
            prop_assert!(still_active.is_disjoint(&active_before));
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
                updated_at: instant,
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
