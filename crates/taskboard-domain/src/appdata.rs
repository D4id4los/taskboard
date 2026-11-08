// SPDX-License-Identifier: MIT OR Apache-2.0
//! The app-data sidecar document: versioned envelope, section registry,
//! and the pure union/GC primitives (D-S2/D-S3 of the sidecar design).
//!
//! The document is *transport*, never truth: local state keeps only what
//! it models, and everything a reader does not understand is carried
//! through **verbatim** (JSON value equality, not byte equality —
//! re-serialization normalizes formatting). The codec is version-blind:
//! it never names capability flags; the emission/fold sites gate (the
//! lane decision lives in the sync/pipeline layers).
//!
//! Modeled task-field keys are exactly [`MODELED_TASK_FIELD_KEYS`]
//! (`effort`, `importance`, `urgency`, and `scheduled` — the fourth key
//! is the D-V2 fallback lane). A modeled key whose `{v, clock}` shape
//! does not decode is demoted to verbatim unmodeled content, never a
//! whole-document failure; the strict boundary is the **envelope**
//! (missing `format`, non-document JSON).

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::entities::{Effort, Importance, Urgency};
use crate::ids::RemoteBoardId;
use crate::recur::RecurRule;
use crate::reminders::{Insistence, ReminderOffset};

/// The document format version. Additive changes never bump it; a reader
/// seeing a greater `format` folds nothing (the fence lives in the
/// consumers, the typed evidence carries the reason).
pub const APPDATA_FORMAT: u32 = 1;

/// The convention title of the per-board anchor card (exact-match scan —
/// the string is machine-owned; no normalization ever applies).
pub const ANCHOR_CARD_TITLE: &str = "[taskboard] app-data";

/// The reserved, never-a-card entry key inside the `dismissals`
/// section: break-ref dismissal rows ride the board-level entry under
/// it (#588 D-9). Deck card ids are decimal strings, so `"break"` can
/// never collide; the per-card fold loop looks up only card keys of
/// present tasks, so the entry is invisible to it by construction.
pub const BREAK_DISMISSALS_ENTRY_KEY: &str = "break";

/// The codec's modeled task-field keys — its single dispatch source and
/// the subject of the manifest lockstep assertion: [`crate::entities::
/// TASK_FIELDS`]' `AppData` rows plus `scheduled` (the Native row whose
/// sync lane is dynamic per D-V2; the lane decision never lives here).
pub const MODELED_TASK_FIELD_KEYS: &[&str] = &[
    "effort",
    "importance",
    "urgency",
    "recur",
    "kind",
    "scheduled",
];

/// A per-board sidecar document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppDataDocument {
    /// The document format version ([`APPDATA_FORMAT`] for anything this
    /// version writes).
    pub format: u32,
    /// The remote board the document belongs to (a foreign board folds
    /// nothing — the fence lives in the consumers).
    pub board: RemoteBoardId,
    /// The section registry.
    pub sections: AppDataSections,
    /// Unknown envelope keys: carried verbatim (the D5 relay rule).
    #[serde(flatten, default, skip_serializing_if = "BTreeMap::is_empty")]
    pub unknown: BTreeMap<String, Value>,
}

/// The day-one sections. The `proofs` name stays **reserved-but-
/// unclaimed** (it misnames the synced fact under the R3 model — the
/// user dismissal, not a mechanical fire proof — and no future wave may
/// invent a colliding meaning for it); `settings` stays reserved. Both,
/// and any unknown section, land in `unknown` verbatim.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct AppDataSections {
    /// Per-card app-field entries, keyed by decimal remote card id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tasks: Option<BTreeMap<String, TaskEntry>>,
    /// Per-card reminder-rule entries, keyed by decimal remote card id.
    #[serde(
        rename = "reminder-rules",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub reminder_rules: Option<BTreeMap<String, RulesEntry>>,
    /// Per-card synced user-dismissal entries, keyed by decimal remote
    /// card id (the `dismissals` section, design D-1/D-9).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dismissals: Option<BTreeMap<String, DismissalsEntry>>,
    /// Board-level synced time entries, keyed by entry UUID (design
    /// D-T10) — the one section whose keys are not card ids: history
    /// outlives task deletion, so `prune_absent_cards` must never see it.
    #[serde(
        rename = "time-entries",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub time_entries: Option<BTreeMap<String, TimeEntryRecord>>,
    /// Board-level habit tracker definitions, keyed by the habit UUID
    /// string (DR-6 §4.5, D-C9) — the `RuleEntry` row pattern. Like
    /// `time_entries`, the keys are not card ids: history (and the
    /// deletion tombstones) outlive any card, so `prune_absent_cards`
    /// must never see them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub habits: Option<BTreeMap<String, HabitRecord>>,
    /// Board-level habit count facts, keyed by
    /// [`crate::habit::HabitLogKey::wire_form`] (§4.5) — set-union facts
    /// with row-clock LWW, under the 180-day wire retention (D-H14).
    #[serde(rename = "habit-log", default, skip_serializing_if = "Option::is_none")]
    pub habit_log: Option<BTreeMap<String, HabitLogRecord>>,
    /// Board-level break-rule definitions, keyed by the `BreakRuleId`
    /// UUID string — the rule's remote identity (D-S9's device, #588
    /// D-4). Like `time_entries`/`habits`, the keys are not card ids:
    /// rules outlive any card, so `prune_absent_cards` must never see
    /// the section.
    #[serde(
        rename = "break-rules",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub break_rules: Option<BTreeMap<String, BreakRuleEntry>>,
    /// Unknown sections: carried verbatim.
    #[serde(flatten, default, skip_serializing_if = "BTreeMap::is_empty")]
    pub unknown: BTreeMap<String, Value>,
}

/// One card's app-field entry: modeled keys typed, everything else
/// preserved verbatim.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TaskEntry {
    /// The modeled keys only — exactly [`MODELED_TASK_FIELD_KEYS`]
    /// (manifest assertion 3).
    pub fields: BTreeMap<String, TaskFieldWrite>,
    /// Unmodeled content: unknown keys *beside* `fields` in the entry
    /// object top-level, and undecodable/unknown keys *inside* `fields`
    /// nested under the `"fields"` key — byte-for-value verbatim. (The
    /// top-level key `"fields"` is modeled, so the namespace cannot
    /// collide.)
    pub unmodeled: BTreeMap<String, Value>,
}

impl Serialize for TaskEntry {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serde_json::Map::new();
        let demoted_fields = self.unmodeled.contains_key("fields");
        if !self.fields.is_empty() || demoted_fields {
            let mut fields = serde_json::Map::new();
            for (key, write) in &self.fields {
                fields.insert(
                    key.clone(),
                    serde_json::to_value(write).map_err(serde::ser::Error::custom)?,
                );
            }
            // Demoted/unknown keys ride back into the fields object.
            if let Some(Value::Object(nested)) = self.unmodeled.get("fields") {
                for (key, value) in nested {
                    fields.entry(key.clone()).or_insert_with(|| value.clone());
                }
            }
            map.insert("fields".to_owned(), Value::Object(fields));
        }
        for (key, value) in &self.unmodeled {
            if key == "fields" {
                continue;
            }
            map.insert(key.clone(), value.clone());
        }
        Value::Object(map).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for TaskEntry {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = Value::deserialize(deserializer)?;
        let mut entry = TaskEntry::default();
        let Value::Object(map) = raw else {
            return Err(serde::de::Error::custom("a task entry must be an object"));
        };
        for (key, value) in map {
            if key == "fields" {
                let Value::Object(fields) = value else {
                    // A non-object `fields` is unmodeled content.
                    entry.unmodeled.insert("fields".to_owned(), Value::Null);
                    continue;
                };
                let mut demoted = serde_json::Map::new();
                for (field_key, field_value) in fields {
                    match decode_field_write(&field_key, &field_value) {
                        Some(write) => {
                            entry.fields.insert(field_key, write);
                        }
                        None => {
                            // Leniency (D-S2's materialization rule): one
                            // malformed modeled key demotes to verbatim,
                            // never fails the document.
                            demoted.insert(field_key, field_value);
                        }
                    }
                }
                if !demoted.is_empty() {
                    entry
                        .unmodeled
                        .insert("fields".to_owned(), Value::Object(demoted));
                }
            } else {
                entry.unmodeled.insert(key, value);
            }
        }
        Ok(entry)
    }
}

/// A modeled key's `{ "v": <encoding>, "clock": "<rfc3339>" }` write.
///
/// The *key* selects the [`ModeledValue`] arm — the encoding is the
/// domain field's own serde shape. Deserialization is key-directed (see
/// [`decode_field_write`]); serialization emits the envelope verbatim.
#[derive(Debug, Clone, PartialEq)]
pub struct TaskFieldWrite {
    /// The field's value; `None` = a cleared field (an ordinary write,
    /// never a tombstone — the tasks section carries no tombstones).
    pub value: ModeledValue,
    /// The per-field write clock (rfc3339 on the wire).
    pub clock: DateTime<Utc>,
    /// Who wrote the field (F1's `by` leg, D-BF2): the author slot of
    /// the duel tuple. Default-absent (every pre-F1 row decodes
    /// `None`); present-but-unparseable demotes the whole row to
    /// unmodeled — silently de-authoring would change fold rank.
    pub by: Option<crate::ids::AuthorId>,
}

impl Serialize for TaskFieldWrite {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let value = encode_modeled(&self.value);
        let mut object = serde_json::Map::new();
        object.insert("v".into(), value);
        object.insert(
            "clock".into(),
            serde_json::to_value(self.clock).unwrap_or(Value::Null),
        );
        if let Some(by) = self.by {
            object.insert("by".into(), serde_json::to_value(by).unwrap_or(Value::Null));
        }
        Value::Object(object).serialize(serializer)
    }
}

/// A modeled field's value: the key selects the arm (see
/// [`modeled_key`]); `None` = a cleared field (an ordinary write, never
/// a tombstone). `scheduled` is the fourth modeled key (D-V2's fallback
/// lane): the same envelope as the others, `null` = cleared.
#[derive(Debug, Clone, PartialEq)]
pub enum ModeledValue {
    Effort(Option<Effort>),
    Importance(Option<Importance>),
    Urgency(Option<Urgency>),
    Recur(Option<RecurRule>),
    /// The tracker task kind (issue #492); `None` = a reset to the
    /// `Standard` default — an ordinary write, never a tombstone.
    Kind(Option<crate::entities::TaskKind>),
    Scheduled(Option<DateTime<Utc>>),
}

/// The wire key a modeled value rides.
#[must_use]
pub fn modeled_key(value: &ModeledValue) -> &'static str {
    match value {
        ModeledValue::Effort(_) => "effort",
        ModeledValue::Importance(_) => "importance",
        ModeledValue::Urgency(_) => "urgency",
        ModeledValue::Recur(_) => "recur",
        ModeledValue::Kind(_) => "kind",
        ModeledValue::Scheduled(_) => "scheduled",
    }
}

/// The key-directed decode of one modeled field's `{v, clock}` object.
/// `None` = not decodable under this key (demotes to unmodeled).
fn decode_field_write(key: &str, raw: &Value) -> Option<TaskFieldWrite> {
    let Value::Object(map) = raw else {
        return None;
    };
    let clock: DateTime<Utc> = serde_json::from_value(map.get("clock")?.clone()).ok()?;
    let value = decode_modeled(key, map.get("v").unwrap_or(&Value::Null))?;
    let by = decode_by(map).ok()?;
    Some(TaskFieldWrite { value, clock, by })
}

/// The `by` decode leg shared by every authored row: absent/`null` →
/// `None` (the tolerant default; old documents carry no key), a
/// parseable UUID → `Some`, **present but unparseable → the caller
/// demotes the whole row** (never a silent de-authoring: the fold's
/// `None < Some` leg would change rank against the author-carrying
/// peer).
pub(crate) fn decode_by(
    map: &serde_json::Map<String, Value>,
) -> Result<Option<crate::ids::AuthorId>, ()> {
    match map.get("by") {
        None | Some(Value::Null) => Ok(None),
        Some(raw) => match raw.as_str().and_then(crate::ids::AuthorId::parse) {
            Some(by) => Ok(Some(by)),
            None => Err(()),
        },
    }
}

/// Encodes a modeled value in the domain field's own serde shape
/// (`null` = cleared).
fn encode_modeled(value: &ModeledValue) -> Value {
    match value {
        ModeledValue::Effort(effort) => effort.map_or(Value::Null, |e| {
            serde_json::to_value(e).unwrap_or(Value::Null)
        }),
        ModeledValue::Importance(level) => level.map_or(Value::Null, |l| {
            serde_json::to_value(l).unwrap_or(Value::Null)
        }),
        ModeledValue::Urgency(level) => level.map_or(Value::Null, |l| {
            serde_json::to_value(l).unwrap_or(Value::Null)
        }),
        ModeledValue::Recur(rule) => rule.as_ref().map_or(Value::Null, |r| {
            serde_json::to_value(r).unwrap_or(Value::Null)
        }),
        ModeledValue::Kind(kind) => kind.map_or(Value::Null, |k| {
            serde_json::to_value(k).unwrap_or(Value::Null)
        }),
        ModeledValue::Scheduled(dt) => dt.map_or(Value::Null, |d| {
            serde_json::to_value(d).unwrap_or(Value::Null)
        }),
    }
}

/// The key-directed value decode: `null` and the field's own serde shape
/// decode into the arm; anything else (a string effort, an object, …)
/// demotes. The key must be modeled.
fn decode_modeled(key: &str, v: &Value) -> Option<ModeledValue> {
    let value = match key {
        "effort" => ModeledValue::Effort(
            (v != &Value::Null)
                .then(|| serde_json::from_value(v.clone()).ok())
                .flatten(),
        ),
        "importance" => ModeledValue::Importance(
            (v != &Value::Null)
                .then(|| serde_json::from_value(v.clone()).ok())
                .flatten(),
        ),
        "urgency" => ModeledValue::Urgency(
            (v != &Value::Null)
                .then(|| serde_json::from_value(v.clone()).ok())
                .flatten(),
        ),
        "recur" => ModeledValue::Recur(
            (v != &Value::Null)
                .then(|| serde_json::from_value(v.clone()).ok())
                .flatten(),
        ),
        "kind" => ModeledValue::Kind(
            (v != &Value::Null)
                .then(|| serde_json::from_value(v.clone()).ok())
                .flatten(),
        ),
        "scheduled" => ModeledValue::Scheduled(
            (v != &Value::Null)
                .then(|| serde_json::from_value(v.clone()).ok())
                .flatten(),
        ),
        _ => return None,
    };
    // A typed shape that failed to decode (e.g. a string effort) demotes.
    let decoded_shape = match &value {
        ModeledValue::Effort(e) => e.is_some(),
        ModeledValue::Importance(e) => e.is_some(),
        ModeledValue::Urgency(e) => e.is_some(),
        ModeledValue::Recur(e) => e.is_some(),
        ModeledValue::Kind(e) => e.is_some(),
        ModeledValue::Scheduled(e) => e.is_some(),
    };
    if v != &Value::Null && !decoded_shape {
        None
    } else {
        Some(value)
    }
}

/// One card's reminder-rule entry.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RulesEntry {
    /// Keyed by the `ReminderRuleId` UUID string (D-S9: the document
    /// *is* the rule's remote identity).
    pub rules: BTreeMap<String, RuleEntry>,
    /// Unknown keys beside `rules`: carried verbatim.
    pub unknown: BTreeMap<String, Value>,
}

impl Serialize for RulesEntry {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serde_json::Map::new();
        if !self.rules.is_empty() {
            let mut rules = serde_json::Map::new();
            for (key, entry) in &self.rules {
                rules.insert(
                    key.clone(),
                    serde_json::to_value(entry).map_err(serde::ser::Error::custom)?,
                );
            }
            map.insert("rules".to_owned(), Value::Object(rules));
        }
        for (key, value) in &self.unknown {
            map.insert(key.clone(), value.clone());
        }
        Value::Object(map).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for RulesEntry {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = Value::deserialize(deserializer)?;
        let mut entry = RulesEntry::default();
        let Value::Object(map) = raw else {
            return Err(serde::de::Error::custom("a rules entry must be an object"));
        };
        for (key, value) in map {
            if key == "rules" {
                let Value::Object(rules) = value else {
                    entry.unknown.insert("rules".to_owned(), Value::Null);
                    continue;
                };
                for (rule_key, rule_value) in rules {
                    entry
                        .rules
                        .insert(rule_key, RuleEntry::deserialize_verbatim(rule_value));
                }
            } else {
                entry.unknown.insert(key, value);
            }
        }
        Ok(entry)
    }
}

/// One reminder rule's document shape, or a deletion tombstone, or a
/// shape this version does not model (preserved verbatim, never folded —
/// D-S2's materialization rule).
#[derive(Debug, Clone, PartialEq)]
pub enum RuleEntry {
    /// `{ "offset": <seconds ≤ 0>, "created": rfc3339, "clock": rfc3339,
    /// "insistence": "normal"|"alarm" }` for a `DueOffset` rule — or
    /// `{ "at": <rfc3339>, "created": rfc3339, "clock": rfc3339,
    /// "insistence": "normal"|"alarm" }` for an `At` rule (r4 decision
    /// 2). The `at` key is additive with a twist: an `At` rule emits
    /// **no** `offset` key, so documents carrying only due-offset rules
    /// stay byte-identical and an old reader's `decode_offset` fails —
    /// the whole entry demotes to [`RuleEntry::Verbatim`] (D-S2's
    /// materialization rule, both directions safe). Decode-side, a
    /// missing `at` key defaults-on-missing to `DueOffset` (the
    /// insistence precedent); a present-but-unmodelable `at` value
    /// demotes the whole entry (never a silent downgrade).
    Rule {
        kind: crate::reminders::ReminderKind,
        created: DateTime<Utc>,
        clock: DateTime<Utc>,
        insistence: Insistence,
        /// The row's author (F1's `by` leg): default-absent, emitted
        /// only when known.
        by: Option<crate::ids::AuthorId>,
    },
    /// `{ "deleted": true, "clock": rfc3339 }` — rule *membership* needs
    /// explicit tombstones (absence from a possibly-truncated document
    /// must never delete locally).
    Tombstone {
        clock: DateTime<Utc>,
        /// The tombstone's author (F1's `by` leg).
        by: Option<crate::ids::AuthorId>,
    },
    /// A shape this version does not model: preserved verbatim.
    Verbatim(Value),
}

impl RuleEntry {
    /// The entry's write clock; `None` for verbatim content (which the
    /// union therefore never orders against a clocked entry).
    #[must_use]
    pub fn clock(&self) -> Option<DateTime<Utc>> {
        match self {
            Self::Rule { clock, .. } | Self::Tombstone { clock, .. } => Some(*clock),
            Self::Verbatim(_) => None,
        }
    }

    /// Decodes one rule entry; an unmodelable shape stays `Verbatim`
    /// with the original value (never an error).
    fn deserialize_verbatim(raw: Value) -> Self {
        let Value::Object(map) = &raw else {
            return Self::Verbatim(raw);
        };
        if map.get("deleted") == Some(&Value::Bool(true)) {
            if let (Ok(clock), Ok(by)) =
                (serde_json::from_value(map["clock"].clone()), decode_by(map))
            {
                return Self::Tombstone { clock, by };
            }
            return Self::Verbatim(raw);
        }
        if let (Ok(kind), Ok(created), Ok(clock), Ok(insistence), Ok(by)) = (
            decode_kind(map.get("at"), map.get("offset")),
            serde_json::from_value(map.get("created").cloned().unwrap_or(Value::Null)),
            serde_json::from_value(map.get("clock").cloned().unwrap_or(Value::Null)),
            // Default-on-missing (the additive-section precedent): an
            // old document folds `Normal`. A *present* but unmodelable
            // value is not defaulted — it falls through to `Verbatim`.
            decode_insistence_default_on_missing(map.get("insistence")),
            decode_by(map),
        ) {
            return Self::Rule {
                kind,
                created,
                clock,
                insistence,
                by,
            };
        }
        Self::Verbatim(raw)
    }
}

impl Serialize for RuleEntry {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Rule {
                kind,
                created,
                clock,
                insistence,
                by,
            } => {
                // The modeled shape gains the `by` key only when known:
                // authorless rows stay byte-identical to the pre-F1
                // documents (old readers ignore unknown keys).
                let mut object = match *kind {
                    // A `DueOffset` rule emits today's shape verbatim: no
                    // new key, byte-identical documents for due-offset-only
                    // rules, old readers unaffected.
                    crate::reminders::ReminderKind::DueOffset(offset) => {
                        serde_json::json!({
                            "offset": offset.as_duration().num_seconds(),
                            "created": created,
                            "clock": clock,
                            "insistence": insistence.text_form(),
                        })
                    }
                    // An `At` rule emits the `at` key instead of `offset`:
                    // an old reader's decode fails and the entry demotes to
                    // `Verbatim` — preserved, never folded.
                    crate::reminders::ReminderKind::At(at) => {
                        serde_json::json!({
                            "at": at.as_datetime().to_rfc3339(),
                            "created": created,
                            "clock": clock,
                            "insistence": insistence.text_form(),
                        })
                    }
                };
                if let Some(map) = object.as_object_mut()
                    && let Some(by) = by
                {
                    map.insert("by".into(), serde_json::to_value(by).unwrap_or(Value::Null));
                }
                object.serialize(serializer)
            }
            Self::Tombstone { clock, by } => {
                let mut object = serde_json::json!({ "deleted": true, "clock": clock });
                if let Some(map) = object.as_object_mut()
                    && let Some(by) = by
                {
                    map.insert("by".into(), serde_json::to_value(by).unwrap_or(Value::Null));
                }
                object.serialize(serializer)
            }
            Self::Verbatim(value) => value.serialize(serializer),
        }
    }
}

/// Decodes the rule entry's insistence rung: absent → `Normal`
/// (default-on-missing, the DR-1 additive precedent); present and a
/// valid serde unit → the rung; present and unknown → `Err` (the entry
/// demotes to `Verbatim`, never a silent downgrade).
fn decode_insistence_default_on_missing(raw: Option<&Value>) -> Result<Insistence, ()> {
    match raw {
        None | Some(&Value::Null) => Ok(Insistence::Normal),
        Some(value) => serde_json::from_value(value.clone()).map_err(|_| ()),
    }
}

/// Decodes the rule offset's integer-seconds wire shape; a positive
/// offset is unconstructible in the domain and demotes the entry.
fn decode_offset(raw: Option<&Value>) -> Result<ReminderOffset, ()> {
    let Some(Value::Number(n)) = raw else {
        return Err(());
    };
    let secs = n.as_i64().ok_or(())?;
    ReminderOffset::before(chrono::TimeDelta::seconds(secs)).map_err(|_| ())
}

/// Decodes the rule entry's kind: an `at` key carries the rfc3339
/// instant (minted — sub-second or unparseable demotes the entry); no
/// `at` key defaults-on-missing to the legacy due-offset decode (an
/// old document's `offset` key). The kind is decided by `at`'s
/// presence alone, so the two shapes can never blend.
fn decode_kind(
    at: Option<&Value>,
    offset: Option<&Value>,
) -> Result<crate::reminders::ReminderKind, ()> {
    match at {
        Some(Value::Null) | None => Ok(crate::reminders::ReminderKind::DueOffset(decode_offset(
            offset,
        )?)),
        Some(value) => {
            let raw = serde_json::from_value::<DateTime<Utc>>(value.clone()).map_err(|_| ())?;
            Ok(crate::reminders::ReminderKind::At(
                crate::reminders::AtInstant::whole_second(raw).map_err(|_| ())?,
            ))
        }
    }
}

/// One card's synced-dismissal entry (the `dismissals` section, design
/// D-1/D-9) — the [`RulesEntry`] shape discipline over a tombstone-free
/// payload: dismissals are permanent for their occurrence, so there are
/// no deletion tombstones to model.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DismissalsEntry {
    /// Keyed by the row grammar the *entry* scopes: card entries key
    /// by [`dismissal_wire_key`] — `"<rule uuid>|<rfc3339
    /// occurrence>"` — and the reserved board-level entry
    /// ([`BREAK_DISMISSALS_ENTRY_KEY`]) keys by
    /// [`break_dismissal_wire_key`] — `"break:<uuid>|<rfc3339>"` (#588
    /// D-9; JSON map keys must be strings, the `ReminderSnoozeKey`
    /// string-encoding precedent). The UUID's document presence *is*
    /// its remote identity (D-S9); no local `TaskId`/`AdhocAlarmId`
    /// text form is ever representable here.
    pub dismissals: BTreeMap<String, DismissalEntry>,
    /// Unknown keys beside `dismissals`: carried verbatim.
    pub unknown: BTreeMap<String, Value>,
}

impl Serialize for DismissalsEntry {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serde_json::Map::new();
        if !self.dismissals.is_empty() {
            let mut dismissals = serde_json::Map::new();
            for (key, entry) in &self.dismissals {
                dismissals.insert(
                    key.clone(),
                    serde_json::to_value(entry).map_err(serde::ser::Error::custom)?,
                );
            }
            map.insert("dismissals".to_owned(), Value::Object(dismissals));
        }
        for (key, value) in &self.unknown {
            map.insert(key.clone(), value.clone());
        }
        Value::Object(map).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for DismissalsEntry {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = Value::deserialize(deserializer)?;
        let mut entry = DismissalsEntry::default();
        let Value::Object(map) = raw else {
            return Err(serde::de::Error::custom(
                "a dismissals entry must be an object",
            ));
        };
        for (key, value) in map {
            if key == "dismissals" {
                let Value::Object(dismissals) = value else {
                    // A non-object section body is unmodeled content,
                    // never a document failure.
                    entry.unknown.insert("dismissals".to_owned(), value);
                    continue;
                };
                for (row_key, row_value) in dismissals {
                    entry
                        .dismissals
                        .insert(row_key, DismissalEntry::deserialize_verbatim(row_value));
                }
            } else {
                entry.unknown.insert(key, value);
            }
        }
        Ok(entry)
    }
}

/// One synced dismissal's document shape, or a shape this version does
/// not model (preserved verbatim, never folded — D-S2's materialization
/// rule). Wire shape: `{ "dismissed_at": rfc3339 }` — the embedded LWW
/// clock (D-7); the `(rule, occurrence)` identity lives in the map key,
/// so the wire carries a bare rule UUID and can never carry a local
/// `TaskId`/`AdhocAlarmId` (the fail-closed mint, design D-4).
#[derive(Debug, Clone, PartialEq)]
pub enum DismissalEntry {
    /// `{ "dismissed_at": rfc3339 }`.
    Dismissed {
        dismissed_at: DateTime<Utc>,
        /// The row's author (F1's `by` leg): the dismissal fold is
        /// additive, so the leg rides the wire neutrally and is never
        /// used in a comparison.
        by: Option<crate::ids::AuthorId>,
    },
    /// A shape this version does not model: preserved verbatim.
    Verbatim(Value),
}

impl DismissalEntry {
    /// The entry's write clock; `None` for verbatim content (which the
    /// union therefore never orders against a clocked entry).
    #[must_use]
    pub fn clock(&self) -> Option<DateTime<Utc>> {
        match self {
            Self::Dismissed { dismissed_at, .. } => Some(*dismissed_at),
            Self::Verbatim(_) => None,
        }
    }

    /// Decodes one dismissal row; an unmodelable shape stays `Verbatim`
    /// with the original value (never an error).
    fn deserialize_verbatim(raw: Value) -> Self {
        let Value::Object(map) = &raw else {
            return Self::Verbatim(raw);
        };
        if let (Ok(dismissed_at), Ok(by)) = (
            serde_json::from_value(map.get("dismissed_at").cloned().unwrap_or(Value::Null)),
            decode_by(map),
        ) {
            return Self::Dismissed { dismissed_at, by };
        }
        Self::Verbatim(raw)
    }
}

impl Serialize for DismissalEntry {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Dismissed { dismissed_at, by } => {
                let mut object = serde_json::json!({ "dismissed_at": dismissed_at });
                if let Some(map) = object.as_object_mut()
                    && let Some(by) = by
                {
                    map.insert("by".into(), serde_json::to_value(by).unwrap_or(Value::Null));
                }
                object.serialize(serializer)
            }
            Self::Verbatim(value) => value.serialize(serializer),
        }
    }
}

/// One synced time entry's document shape, or a deletion tombstone, or a
/// shape this version does not model (preserved verbatim, never folded —
/// D-S2's materialization rule). Board-level section keyed by the entry
/// UUID (design D-T10); `task` is the decimal remote card id.
#[derive(Debug, Clone, PartialEq)]
pub enum TimeEntryRecord {
    /// `{ "task": "<decimal card>", "title": str, "start": rfc3339,
    /// "end": rfc3339|null, "created": rfc3339, "clock": rfc3339,
    /// "kind": "work"|"respite" }`. The `kind` key is additive (D-2): a
    /// pre-kind row omits it and folds `Work`; an unknown kind demotes
    /// the whole row to [`TimeEntryRecord::Verbatim`] (never a silent
    /// downgrade — the `RuleEntry` insistence ruling). `task: None`
    /// decodes tolerantly (a peer row without the key) but is never
    /// emitted by this version — an unbound-task entry has no identity
    /// another device could resolve (D-T10).
    Entry {
        task: Option<String>,
        title: String,
        start: DateTime<Utc>,
        end: Option<DateTime<Utc>>,
        created: DateTime<Utc>,
        clock: DateTime<Utc>,
        kind: crate::tracking::TimeEntryKind,
        /// The row's author (F1's `by` leg): default-absent.
        by: Option<crate::ids::AuthorId>,
    },
    /// `{ "deleted": true, "clock": rfc3339 }` — a global delete (D-T12:
    /// deliberate deletes roam; retention aging never does).
    Tombstone {
        clock: DateTime<Utc>,
        /// The tombstone's author (F1's `by` leg).
        by: Option<crate::ids::AuthorId>,
    },
    /// A shape this version does not model: preserved verbatim.
    Verbatim(Value),
}

impl TimeEntryRecord {
    /// The row's write clock; `None` for verbatim content (which the
    /// union therefore never orders against a clocked row).
    #[must_use]
    pub fn clock(&self) -> Option<DateTime<Utc>> {
        match self {
            Self::Entry { clock, .. } | Self::Tombstone { clock, .. } => Some(*clock),
            Self::Verbatim(_) => None,
        }
    }

    /// Decodes one time-entry row; an unmodelable shape stays `Verbatim`
    /// with the original value (never an error). Span validity is **not**
    /// a decode input — synced rows fold as-is (the §6 skew ruling); the
    /// read-side projections clamp.
    fn deserialize_verbatim(raw: Value) -> Self {
        let Value::Object(map) = &raw else {
            return Self::Verbatim(raw);
        };
        if map.get("deleted") == Some(&Value::Bool(true)) {
            if let (Ok(clock), Ok(by)) =
                (serde_json::from_value(map["clock"].clone()), decode_by(map))
            {
                return Self::Tombstone { clock, by };
            }
            return Self::Verbatim(raw);
        }
        let task = match map.get("task") {
            None | Some(Value::Null) => Some(None),
            Some(Value::String(text)) => Some(Some(text.clone())),
            _ => None, // present but not a string: unmodelable
        };
        let end = match map.get("end") {
            None | Some(Value::Null) => Some(None),
            Some(value) => serde_json::from_value(value.clone()).ok().map(Some),
        };
        if let (
            Some(task),
            Some(end),
            Ok(title),
            Ok(start),
            Ok(created),
            Ok(clock),
            Ok(kind),
            Ok(by),
        ) = (
            task,
            end,
            serde_json::from_value(map.get("title").cloned().unwrap_or(Value::Null)),
            serde_json::from_value(map.get("start").cloned().unwrap_or(Value::Null)),
            serde_json::from_value(map.get("created").cloned().unwrap_or(Value::Null)),
            serde_json::from_value(map.get("clock").cloned().unwrap_or(Value::Null)),
            // Default-on-missing (the additive-section precedent): a
            // pre-kind row folds `Work`. A *present* but unmodeled kind
            // is not defaulted — it falls through to `Verbatim`, never
            // a silent downgrade (the `RuleEntry` insistence ruling).
            decode_time_entry_kind_default_on_missing(map.get("kind")),
            decode_by(map),
        ) {
            return Self::Entry {
                task,
                title,
                start,
                end,
                created,
                clock,
                kind,
                by,
            };
        }
        Self::Verbatim(raw)
    }
}

/// Decodes the time-entry row's kind: absent →
/// [`crate::tracking::TimeEntryKind::Work`] (default-off, the D-2
/// fold); present and a valid serde unit → the kind; present and
/// unknown → `Err` (the row demotes to `Verbatim`, never a silent
/// downgrade).
fn decode_time_entry_kind_default_on_missing(
    raw: Option<&Value>,
) -> Result<crate::tracking::TimeEntryKind, ()> {
    match raw {
        None | Some(&Value::Null) => Ok(crate::tracking::TimeEntryKind::Work),
        Some(value) => serde_json::from_value(value.clone()).map_err(|_| ()),
    }
}

impl<'de> Deserialize<'de> for TimeEntryRecord {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // The tolerant boundary: any shape this version does not model
        // decodes as `Verbatim`, never a document failure.
        Ok(Self::deserialize_verbatim(Value::deserialize(
            deserializer,
        )?))
    }
}

impl Serialize for TimeEntryRecord {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Entry {
                task,
                title,
                start,
                end,
                created,
                clock,
                kind,
                by,
            } => {
                let mut object = serde_json::json!({
                    "task": task,
                    "title": title,
                    "start": start,
                    "end": end,
                    "created": created,
                    "clock": clock,
                    "kind": kind.text_form(),
                });
                if let Some(map) = object.as_object_mut()
                    && let Some(by) = by
                {
                    map.insert("by".into(), serde_json::to_value(by).unwrap_or(Value::Null));
                }
                object.serialize(serializer)
            }
            Self::Tombstone { clock, by } => {
                let mut object = serde_json::json!({ "deleted": true, "clock": clock });
                if let Some(map) = object.as_object_mut()
                    && let Some(by) = by
                {
                    map.insert("by".into(), serde_json::to_value(by).unwrap_or(Value::Null));
                }
                object.serialize(serializer)
            }
            Self::Verbatim(value) => value.serialize(serializer),
        }
    }
}

/// One board's habit tracker definition, or its deletion tombstone, or a
/// shape this version does not model (preserved verbatim, never folded —
/// D-S2's materialization rule). Board-level section keyed by the habit
/// UUID (DR-6 §4.5); the document belongs to one remote board, so no
/// `board` rides the wire (D-H11 — the fold stamps the locally-bound
/// board).
#[derive(Debug, Clone, PartialEq)]
pub enum HabitRecord {
    /// `{ "title": str, "target": HabitTarget, "order": i64,
    /// "anchor_time": "HH:MM:SS"|null, "clocks": HabitClocks,
    /// "continued_from": uuid|null }` — the whole row, all five
    /// embedded clocks riding it (they are the row's competition data).
    Live {
        title: String,
        target: crate::habit::HabitTarget,
        order: i64,
        anchor_time: Option<chrono::NaiveTime>,
        clocks: crate::habit::HabitClocks,
        continued_from: Option<crate::ids::SeriesId>,
        /// The row's author (F1's `by` leg): default-absent.
        by: Option<crate::ids::AuthorId>,
    },
    /// `{ "deleted": true, "clock": rfc3339 }` — deletion roams forever
    /// (D-H10): tombstone rows are tiny and must reach late-adopting
    /// devices, so definitions carry **no retention**.
    Tombstone {
        clock: DateTime<Utc>,
        /// The tombstone's author (F1's `by` leg).
        by: Option<crate::ids::AuthorId>,
    },
    /// A shape this version does not model: preserved verbatim.
    Verbatim(Value),
}

impl HabitRecord {
    /// The row's competition clock: the **effective clock** — `max` of
    /// the five embedded clocks for a `Live` row, the tombstone's clock
    /// otherwise; `None` for verbatim content (which the union never
    /// orders against).
    #[must_use]
    pub fn competition_clock(&self) -> Option<DateTime<Utc>> {
        match self {
            Self::Live { clocks, .. } => Some(
                clocks
                    .title
                    .max(clocks.target)
                    .max(clocks.order)
                    .max(clocks.anchor_time)
                    .max(clocks.deleted),
            ),
            Self::Tombstone { clock, .. } => Some(*clock),
            Self::Verbatim(_) => None,
        }
    }

    /// The row's author leg (F1's `by`): `None` for verbatim content.
    #[must_use]
    pub const fn by(&self) -> Option<crate::ids::AuthorId> {
        match self {
            Self::Live { by, .. } | Self::Tombstone { by, .. } => *by,
            Self::Verbatim(_) => None,
        }
    }

    /// Decodes one habit row; an unmodelable shape stays `Verbatim`
    /// with the original value (never an error — one malformed row
    /// demotes, the document never fails).
    fn deserialize_verbatim(raw: Value) -> Self {
        let Value::Object(map) = &raw else {
            return Self::Verbatim(raw);
        };
        if map.get("deleted") == Some(&Value::Bool(true)) {
            if let (Ok(clock), Ok(by)) =
                (serde_json::from_value(map["clock"].clone()), decode_by(map))
            {
                return Self::Tombstone { clock, by };
            }
            return Self::Verbatim(raw);
        }
        let anchor_time = match map.get("anchor_time") {
            None | Some(Value::Null) => Some(None),
            Some(value) => serde_json::from_value(value.clone()).ok().map(Some),
        };
        let continued_from = match map.get("continued_from") {
            None | Some(Value::Null) => Some(None),
            Some(value) => serde_json::from_value(value.clone()).ok().map(Some),
        };
        if let (
            Ok(title),
            Ok(target),
            Ok(order),
            Some(anchor_time),
            Ok(clocks),
            Some(continued_from),
            Ok(by),
        ) = (
            serde_json::from_value(map.get("title").cloned().unwrap_or(Value::Null)),
            // The strict target decode is load-bearing: the mint rejects
            // a stored 0, so a non-decoding target demotes the row.
            serde_json::from_value(map.get("target").cloned().unwrap_or(Value::Null)),
            serde_json::from_value(map.get("order").cloned().unwrap_or(Value::Null)),
            anchor_time,
            serde_json::from_value(map.get("clocks").cloned().unwrap_or(Value::Null)),
            continued_from,
            decode_by(map),
        ) {
            return Self::Live {
                title,
                target,
                order,
                anchor_time,
                clocks,
                continued_from,
                by,
            };
        }
        Self::Verbatim(raw)
    }
}

impl<'de> Deserialize<'de> for HabitRecord {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // The tolerant boundary: any shape this version does not model
        // decodes as `Verbatim`, never a document failure.
        Ok(Self::deserialize_verbatim(Value::deserialize(
            deserializer,
        )?))
    }
}

impl Serialize for HabitRecord {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Live {
                title,
                target,
                order,
                anchor_time,
                clocks,
                continued_from,
                by,
            } => {
                let mut object = serde_json::json!({
                    "title": title,
                    "target": target,
                    "order": order,
                    "anchor_time": anchor_time,
                    "clocks": clocks,
                    "continued_from": continued_from,
                });
                if let Some(map) = object.as_object_mut()
                    && let Some(by) = by
                {
                    map.insert("by".into(), serde_json::to_value(by).unwrap_or(Value::Null));
                }
                object.serialize(serializer)
            }
            Self::Tombstone { clock, by } => {
                let mut object = serde_json::json!({ "deleted": true, "clock": clock });
                if let Some(map) = object.as_object_mut()
                    && let Some(by) = by
                {
                    map.insert("by".into(), serde_json::to_value(by).unwrap_or(Value::Null));
                }
                object.serialize(serializer)
            }
            Self::Verbatim(value) => value.serialize(serializer),
        }
    }
}

/// One habit log row's document shape, or a shape this version does not
/// model (preserved verbatim). **No tombstone arm** (D-H5): no command
/// deletes a log row (`DeleteHabit` keeps the log — the log outlives the
/// tracker), so nothing can emit one; a future delete shape decodes as
/// `Verbatim` and relays — the documented leniency boundary, never a
/// decode failure.
#[derive(Debug, Clone, PartialEq)]
pub enum HabitLogRecord {
    /// `{ "count": u32, "clock": rfc3339 }` — the row's LWW clock; the
    /// `(habit, window)` identity lives in the map key.
    Entry {
        count: u32,
        clock: DateTime<Utc>,
        /// The row's author (F1's `by` leg): default-absent.
        by: Option<crate::ids::AuthorId>,
    },
    /// A shape this version does not model: preserved verbatim.
    Verbatim(Value),
}

impl HabitLogRecord {
    /// The row's write clock; `None` for verbatim content (which the
    /// union therefore never orders against a clocked row).
    #[must_use]
    pub fn clock(&self) -> Option<DateTime<Utc>> {
        match self {
            Self::Entry { clock, .. } => Some(*clock),
            Self::Verbatim(_) => None,
        }
    }
}

impl<'de> Deserialize<'de> for HabitLogRecord {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // The tolerant boundary: any shape this version does not model
        // — a future `{"deleted": true}` row included — decodes as
        // `Verbatim`, never a document failure.
        let raw = Value::deserialize(deserializer)?;
        if let Value::Object(map) = &raw
            && let (Some(Ok(count)), Some(Ok(clock)), Ok(by)) = (
                map.get("count")
                    .map(|v| serde_json::from_value::<u32>(v.clone())),
                map.get("clock")
                    .map(|v| serde_json::from_value::<DateTime<Utc>>(v.clone())),
                decode_by(map),
            )
            && !map.contains_key("deleted")
        {
            return Ok(Self::Entry { count, clock, by });
        }
        Ok(Self::Verbatim(raw))
    }
}

impl Serialize for HabitLogRecord {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Entry { count, clock, by } => {
                let mut object = serde_json::json!({ "count": count, "clock": clock });
                if let Some(map) = object.as_object_mut()
                    && let Some(by) = by
                {
                    map.insert("by".into(), serde_json::to_value(by).unwrap_or(Value::Null));
                }
                object.serialize(serializer)
            }
            Self::Verbatim(value) => value.serialize(serializer),
        }
    }
}

/// One break rule's document shape, or a deletion tombstone, or a shape
/// this version does not model (preserved verbatim, never folded —
/// D-S2's materialization rule). Board-level section keyed by the rule
/// UUID (#588 D-4); the document belongs to one board, so no `board`
/// rides the wire (D-H11's device — the fold stamps the locally-bound
/// board). Wire shape: `{ "label": str, "threshold_seconds": i64 > 0,
/// "created": rfc3339, "clock": rfc3339, "insistence":
/// "normal"|"alarm" }` — the [`RuleEntry`] row grammar at board level.
#[derive(Debug, Clone, PartialEq)]
pub enum BreakRuleEntry {
    /// The live rule row. `clock` is the row's competition metadata
    /// (D-5): v1's only verbs are create and delete, so every row this
    /// version writes carries `clock == created`; the slot is the
    /// growth path a future edit-verb chunk fills without a wire
    /// change.
    Rule {
        label: String,
        threshold: crate::tracking::WorkThreshold,
        created: DateTime<Utc>,
        clock: DateTime<Utc>,
        insistence: Insistence,
        /// The row's author (F1's `by` leg): default-absent.
        by: Option<crate::ids::AuthorId>,
    },
    /// `{ "deleted": true, "clock": rfc3339 }` — rule *membership*
    /// needs explicit tombstones (absence from a possibly-truncated
    /// document must never delete locally — the `RuleEntry` ruling).
    /// A re-created rule is a *new UUID* (no edit verbs), so a
    /// tombstone's decision is final for its key.
    Tombstone {
        clock: DateTime<Utc>,
        /// The tombstone's author (F1's `by` leg).
        by: Option<crate::ids::AuthorId>,
    },
    /// A shape this version does not model: preserved verbatim,
    /// never folded.
    Verbatim(Value),
}

impl BreakRuleEntry {
    /// The entry's write clock; `None` for verbatim content (which the
    /// union therefore never orders against a clocked entry).
    #[must_use]
    pub fn clock(&self) -> Option<DateTime<Utc>> {
        match self {
            Self::Rule { clock, .. } | Self::Tombstone { clock, .. } => Some(*clock),
            Self::Verbatim(_) => None,
        }
    }

    /// The row's author leg (F1's `by`): `None` for verbatim content.
    #[must_use]
    pub const fn by(&self) -> Option<crate::ids::AuthorId> {
        match self {
            Self::Rule { by, .. } | Self::Tombstone { by, .. } => *by,
            Self::Verbatim(_) => None,
        }
    }

    /// Decodes one break-rule entry; an unmodelable shape stays
    /// `Verbatim` with the original value (never an error — one
    /// malformed row demotes, the document never fails). A zero or
    /// negative threshold is domain-inconstructible and demotes the
    /// whole row (the `decode_offset` positivity ruling); a missing
    /// `insistence` defaults to `Normal` (the additive-section
    /// precedent) while a present-but-unmodelable one demotes the row
    /// (never a silent downgrade).
    fn deserialize_verbatim(raw: Value) -> Self {
        let Value::Object(map) = &raw else {
            return Self::Verbatim(raw);
        };
        if map.get("deleted") == Some(&Value::Bool(true)) {
            if let (Ok(clock), Ok(by)) =
                (serde_json::from_value(map["clock"].clone()), decode_by(map))
            {
                return Self::Tombstone { clock, by };
            }
            return Self::Verbatim(raw);
        }
        let threshold = map
            .get("threshold_seconds")
            .and_then(serde_json::Value::as_i64)
            .and_then(|secs| {
                chrono::TimeDelta::try_seconds(secs)
                    .and_then(|dur| crate::tracking::WorkThreshold::try_new(dur).ok())
            });
        if let (Some(threshold), Ok(label), Ok(created), Ok(clock), Ok(insistence), Ok(by)) = (
            threshold,
            serde_json::from_value(map.get("label").cloned().unwrap_or(Value::Null)),
            serde_json::from_value(map.get("created").cloned().unwrap_or(Value::Null)),
            serde_json::from_value(map.get("clock").cloned().unwrap_or(Value::Null)),
            // Default-on-missing (the additive-section precedent): an
            // old document folds `Normal`. A *present* but unmodelable
            // value is not defaulted — it falls through to `Verbatim`.
            decode_insistence_default_on_missing(map.get("insistence")),
            decode_by(map),
        ) {
            return Self::Rule {
                label,
                threshold,
                created,
                clock,
                insistence,
                by,
            };
        }
        Self::Verbatim(raw)
    }
}

impl<'de> Deserialize<'de> for BreakRuleEntry {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // The tolerant boundary: any shape this version does not model
        // decodes as `Verbatim`, never a document failure.
        Ok(Self::deserialize_verbatim(Value::deserialize(
            deserializer,
        )?))
    }
}

impl Serialize for BreakRuleEntry {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Rule {
                label,
                threshold,
                created,
                clock,
                insistence,
                by,
            } => {
                let mut object = serde_json::json!({
                    "label": label,
                    "threshold_seconds": threshold.as_duration().num_seconds(),
                    "created": created,
                    "clock": clock,
                    "insistence": insistence.text_form(),
                });
                if let Some(map) = object.as_object_mut()
                    && let Some(by) = by
                {
                    map.insert("by".into(), serde_json::to_value(by).unwrap_or(Value::Null));
                }
                object.serialize(serializer)
            }
            Self::Tombstone { clock, by } => {
                let mut object = serde_json::json!({ "deleted": true, "clock": clock });
                if let Some(map) = object.as_object_mut()
                    && let Some(by) = by
                {
                    map.insert("by".into(), serde_json::to_value(by).unwrap_or(Value::Null));
                }
                object.serialize(serializer)
            }
            Self::Verbatim(value) => value.serialize(serializer),
        }
    }
}

/// The dismissal row's wire key: `"<rule uuid>|<rfc3339 occurrence>"`.
/// Neither component can contain `|` (a UUID is hex+dashes, rfc3339 is
/// digits/punct), so the encoding is injective.
#[must_use]
pub fn dismissal_wire_key(rule: crate::ids::ReminderRuleId, occurrence: DateTime<Utc>) -> String {
    format!("{}|{}", rule.as_uuid(), occurrence.to_rfc3339())
}

/// Parses [`dismissal_wire_key`]; `None` for any key this version does
/// not model (a `default:`/`alarm:`/`break:`-prefixed row a hand-edited
/// document might carry, a malformed pair, …) — the fold ignores it,
/// the codec relays it verbatim.
pub(crate) fn parse_dismissal_wire_key(
    key: &str,
) -> Option<(crate::ids::ReminderRuleId, DateTime<Utc>)> {
    let (uuid_part, occurrence_part) = key.split_once('|')?;
    // The explicit `break:` rejection names the old-peer fail-closed
    // contract (D-9/D-10): every widening key fails the strict
    // bare-UUID parse by construction — an old peer needs zero
    // cooperation, and can never decode a break dismissal as a Rule-ref
    // dismissal for a rule it may not have.
    if uuid_part.starts_with("break:") {
        return None;
    }
    let uuid = uuid::Uuid::parse_str(uuid_part).ok()?;
    let occurrence = DateTime::parse_from_rfc3339(occurrence_part).ok()?;
    Some((
        crate::ids::ReminderRuleId::from(uuid),
        occurrence.with_timezone(&Utc),
    ))
}

/// The break dismissal row's wire key:
/// `"break:<uuid>|<rfc3339 occurrence>"` (#588 D-9). The `break:`
/// prefix is the typed family marker — an old peer's strict bare-UUID
/// parse fails on it (the fail-closed old-peer mechanism, D-1), and no
/// [`dismissal_wire_key`] can ever collide with it. Injective for the
/// same component reason as [`dismissal_wire_key`] (a UUID is
/// hex+dashes, rfc3339 is digits/punct, the prefix is fixed text).
#[must_use]
pub fn break_dismissal_wire_key(
    rule: crate::ids::BreakRuleId,
    occurrence: DateTime<Utc>,
) -> String {
    format!("break:{}|{}", rule.as_uuid(), occurrence.to_rfc3339())
}

/// Parses [`break_dismissal_wire_key`]; `None` for any key this version
/// does not model (a bare rule key, a `default:`/`alarm:`-prefixed row,
/// a malformed pair, …) — the fold ignores it, the codec relays it
/// verbatim.
pub(crate) fn parse_break_dismissal_wire_key(
    key: &str,
) -> Option<(crate::ids::BreakRuleId, DateTime<Utc>)> {
    let rest = key.strip_prefix("break:")?;
    let (uuid_part, occurrence_part) = rest.split_once('|')?;
    let uuid = uuid::Uuid::parse_str(uuid_part).ok()?;
    let occurrence = DateTime::parse_from_rfc3339(occurrence_part).ok()?;
    Some((
        crate::ids::BreakRuleId::from(uuid),
        occurrence.with_timezone(&Utc),
    ))
}

/// The read-modify-write merge the flush performs (D-S4 item 4): per-key
/// embedded-clock LWW — the newer clock wins, equal clocks resolve to
/// `overlay` (the local side being flushed); every part the union does
/// not understand is *additive*: unknown envelope keys, unknown
/// sections, unmodeled entry keys, and verbatim rule shapes survive
/// from `base` and are extended (never overwritten) by `overlay`.
#[must_use]
#[allow(clippy::too_many_lines)] // one match arm per section; splitting hides the relay rule
pub fn union(base: &AppDataDocument, overlay: &AppDataDocument) -> AppDataDocument {
    let mut unknown = base.unknown.clone();
    for (key, value) in &overlay.unknown {
        unknown.entry(key.clone()).or_insert_with(|| value.clone());
    }
    let sections = AppDataSections {
        tasks: match (&base.sections.tasks, &overlay.sections.tasks) {
            (Some(b), Some(o)) => {
                let mut merged = b.clone();
                for (card, overlay_entry) in o {
                    merged
                        .entry(card.clone())
                        .and_modify(|base_entry| union_task_entry(base_entry, overlay_entry))
                        .or_insert_with(|| overlay_entry.clone());
                }
                Some(merged)
            }
            (Some(b), None) => Some(b.clone()),
            (None, Some(o)) => Some(o.clone()),
            (None, None) => None,
        },
        reminder_rules: match (
            &base.sections.reminder_rules,
            &overlay.sections.reminder_rules,
        ) {
            (Some(b), Some(o)) => {
                let mut merged = b.clone();
                for (card, overlay_entry) in o {
                    merged
                        .entry(card.clone())
                        .and_modify(|base_entry| union_rules_entry(base_entry, overlay_entry))
                        .or_insert_with(|| overlay_entry.clone());
                }
                Some(merged)
            }
            (Some(b), None) => Some(b.clone()),
            (None, Some(o)) => Some(o.clone()),
            (None, None) => None,
        },
        dismissals: match (&base.sections.dismissals, &overlay.sections.dismissals) {
            (Some(b), Some(o)) => {
                let mut merged = b.clone();
                for (card, overlay_entry) in o {
                    merged
                        .entry(card.clone())
                        .and_modify(|base_entry| union_dismissals_entry(base_entry, overlay_entry))
                        .or_insert_with(|| overlay_entry.clone());
                }
                Some(merged)
            }
            (Some(b), None) => Some(b.clone()),
            (None, Some(o)) => Some(o.clone()),
            (None, None) => None,
        },
        time_entries: match (&base.sections.time_entries, &overlay.sections.time_entries) {
            (Some(b), Some(o)) => {
                let mut merged = b.clone();
                union_time_entries(&mut merged, o);
                Some(merged)
            }
            (Some(b), None) => Some(b.clone()),
            (None, Some(o)) => Some(o.clone()),
            (None, None) => None,
        },
        habits: match (&base.sections.habits, &overlay.sections.habits) {
            (Some(b), Some(o)) => {
                let mut merged = b.clone();
                union_habits(&mut merged, o);
                Some(merged)
            }
            (Some(b), None) => Some(b.clone()),
            (None, Some(o)) => Some(o.clone()),
            (None, None) => None,
        },
        habit_log: match (&base.sections.habit_log, &overlay.sections.habit_log) {
            (Some(b), Some(o)) => {
                let mut merged = b.clone();
                union_habit_log(&mut merged, o);
                Some(merged)
            }
            (Some(b), None) => Some(b.clone()),
            (None, Some(o)) => Some(o.clone()),
            (None, None) => None,
        },
        break_rules: match (&base.sections.break_rules, &overlay.sections.break_rules) {
            (Some(b), Some(o)) => {
                let mut merged = b.clone();
                union_break_rules(&mut merged, o);
                Some(merged)
            }
            (Some(b), None) => Some(b.clone()),
            (None, Some(o)) => Some(o.clone()),
            (None, None) => None,
        },
        unknown: {
            let mut sections_unknown = base.sections.unknown.clone();
            for (key, value) in &overlay.sections.unknown {
                sections_unknown
                    .entry(key.clone())
                    .or_insert_with(|| value.clone());
            }
            sections_unknown
        },
    };
    // The flush writes its own envelope facts; the union's *content*
    // rules above are what the relay guarantee needs.
    AppDataDocument {
        format: overlay.format,
        board: overlay.board,
        sections,
        unknown,
    }
}

/// Per-field embedded-clock LWW over one card's entry; unmodeled content
/// is additive (base preserved, overlay extends).
fn union_task_entry(base: &mut TaskEntry, overlay: &TaskEntry) {
    for (key, overlay_write) in &overlay.fields {
        match base.fields.get(key) {
            Some(base_write) if base_write.clock > overlay_write.clock => {}
            // Equal clocks resolve to the overlay (the flush's side).
            _ => {
                base.fields.insert(key.clone(), overlay_write.clone());
            }
        }
    }
    for (key, value) in &overlay.unmodeled {
        base.unmodeled
            .entry(key.clone())
            .or_insert_with(|| value.clone());
    }
}

/// Per-rule embedded-clock LWW (tombstones compete by the same clock);
/// verbatim shapes are additive and never displaced by overlay content
/// this version cannot order.
fn union_rules_entry(base: &mut RulesEntry, overlay: &RulesEntry) {
    for (key, overlay_entry) in &overlay.rules {
        let winner = match base.rules.get(key) {
            None => Some(overlay_entry.clone()),
            Some(base_entry) => match (base_entry, overlay_entry) {
                // Verbatim content is never ordered against a clocked
                // entry: whichever side holds it keeps the slot.
                (RuleEntry::Verbatim(_), _) | (_, RuleEntry::Verbatim(_)) => None,
                (b, o) => match (b.clock(), o.clock()) {
                    (Some(bc), Some(oc)) if bc >= oc => None,
                    _ => Some(o.clone()),
                },
            },
        };
        if let Some(winner) = winner {
            base.rules.insert(key.clone(), winner);
        }
    }
    for (key, value) in &overlay.unknown {
        base.unknown
            .entry(key.clone())
            .or_insert_with(|| value.clone());
    }
}

/// Per-row embedded-clock LWW on `dismissed_at` (D-7 — the same
/// machinery as the rules union; a dismissal's single write site makes
/// LWW degenerate to set-union in practice). Verbatim shapes are
/// additive and never displaced by overlay content this version cannot
/// order.
fn union_dismissals_entry(base: &mut DismissalsEntry, overlay: &DismissalsEntry) {
    for (key, overlay_entry) in &overlay.dismissals {
        let winner = match base.dismissals.get(key) {
            None => Some(overlay_entry.clone()),
            Some(base_entry) => match (base_entry, overlay_entry) {
                (DismissalEntry::Verbatim(_), _) | (_, DismissalEntry::Verbatim(_)) => None,
                (b, o) => match (b.clock(), o.clock()) {
                    (Some(bc), Some(oc)) if bc >= oc => None,
                    _ => Some(o.clone()),
                },
            },
        };
        if let Some(winner) = winner {
            base.dismissals.insert(key.clone(), winner);
        }
    }
    for (key, value) in &overlay.unknown {
        base.unknown
            .entry(key.clone())
            .or_insert_with(|| value.clone());
    }
}

/// Per-row embedded-clock LWW on the entry UUID key (D-T11: newer
/// `clock` wins; tombstones compete by the same clock — a delete after
/// an adjust wins, a later-dated live row beats an older tombstone; the
/// remote stop of an open entry is just a newer write). Verbatim shapes
/// are additive and never displaced by overlay content this version
/// cannot order.
///
/// Deliberate divergence (D-7) from `union_rules_entry`/
/// `union_dismissals_entry`: **equal clocks resolve to the overlay**,
/// matching `union_task_entry` and D-T11's normative text — the flush's
/// just-stamped local writes must win the pull-side tie.
fn union_time_entries(
    base: &mut BTreeMap<String, TimeEntryRecord>,
    overlay: &BTreeMap<String, TimeEntryRecord>,
) {
    for (key, overlay_row) in overlay {
        let winner = match base.get(key) {
            None => Some(overlay_row.clone()),
            Some(base_row) => match (base_row, overlay_row) {
                // Verbatim content is never ordered against a clocked
                // row: whichever side holds it keeps the slot.
                (TimeEntryRecord::Verbatim(_), _) | (_, TimeEntryRecord::Verbatim(_)) => None,
                (b, o) => match (b.clock(), o.clock()) {
                    (Some(bc), Some(oc)) if bc > oc => None,
                    _ => Some(o.clone()),
                },
            },
        };
        if let Some(winner) = winner {
            base.insert(key.clone(), winner);
        }
    }
}

/// Per-row effective-clock LWW over the `habits` section (DR-6 §4.5):
/// a `Live` row competes on the `max` of its five embedded clocks, a
/// tombstone on its own clock — a delete after any edit wins, a newer
/// edit resurrects. Deliberately the [`union_time_entries`] flavor
/// (D-T11): **equal clocks resolve to the overlay** — the flush's
/// just-stamped local writes must win the pull-side tie. Verbatim shapes
/// are never ordered against and never displaced.
fn union_habits(base: &mut BTreeMap<String, HabitRecord>, overlay: &BTreeMap<String, HabitRecord>) {
    for (key, overlay_row) in overlay {
        let winner = match base.get(key) {
            None => Some(overlay_row.clone()),
            Some(base_row) => match (base_row, overlay_row) {
                (HabitRecord::Verbatim(_), _) | (_, HabitRecord::Verbatim(_)) => None,
                (b, o) => match (b.competition_clock(), o.competition_clock()) {
                    (Some(bc), Some(oc)) if bc > oc => None,
                    _ => Some(o.clone()),
                },
            },
        };
        if let Some(winner) = winner {
            base.insert(key.clone(), winner);
        }
    }
}

/// Per-row LWW on the row clock over the `habit-log` section (§4.5):
/// whole-row LWW that degenerates to set-union in practice (one write
/// site per key per device — the planner's fresh `now`). Equal clocks
/// resolve to the overlay, the [`union_time_entries`] flavor. Verbatim
/// shapes — a future delete row included — are never ordered against
/// and never displaced.
fn union_habit_log(
    base: &mut BTreeMap<String, HabitLogRecord>,
    overlay: &BTreeMap<String, HabitLogRecord>,
) {
    for (key, overlay_row) in overlay {
        let winner = match base.get(key) {
            None => Some(overlay_row.clone()),
            Some(base_row) => match (base_row, overlay_row) {
                (HabitLogRecord::Verbatim(_), _) | (_, HabitLogRecord::Verbatim(_)) => None,
                (b, o) => match (b.clock(), o.clock()) {
                    (Some(bc), Some(oc)) if bc > oc => None,
                    _ => Some(o.clone()),
                },
            },
        };
        if let Some(winner) = winner {
            base.insert(key.clone(), winner);
        }
    }
}

/// Per-row embedded-clock LWW over the `break-rules` section (#588
/// D-6, the [`union_habits`] shape): newer `clock` wins, equal clocks
/// resolve to the overlay (the flush's just-stamped local writes must
/// win the pull-side tie), tombstones compete by the same clock, and
/// `Verbatim` rows are never ordered against and never displaced.
fn union_break_rules(
    base: &mut BTreeMap<String, BreakRuleEntry>,
    overlay: &BTreeMap<String, BreakRuleEntry>,
) {
    for (key, overlay_row) in overlay {
        let winner = match base.get(key) {
            None => Some(overlay_row.clone()),
            Some(base_row) => match (base_row, overlay_row) {
                (BreakRuleEntry::Verbatim(_), _) | (_, BreakRuleEntry::Verbatim(_)) => None,
                (b, o) => match (b.clock(), o.clock()) {
                    (Some(bc), Some(oc)) if bc > oc => None,
                    _ => Some(o.clone()),
                },
            },
        };
        if let Some(winner) = winner {
            base.insert(key.clone(), winner);
        }
    }
}

/// The flush GC (D-S2): removes the entries — both known sections,
/// tombstones included — whose decimal card key is absent from
/// `present_cards` (the cycle's complete snapshot's card set). Entries
/// for present cards, the reserved board-level `break` dismissals
/// entry (#588 D-12), and unknown sections are never touched.
#[must_use]
pub fn prune_absent_cards(
    document: &AppDataDocument,
    present_cards: &std::collections::BTreeSet<String>,
) -> AppDataDocument {
    fn keep<V: Clone>(
        map: &BTreeMap<String, V>,
        present: &std::collections::BTreeSet<String>,
    ) -> BTreeMap<String, V> {
        map.iter()
            .filter(|(card, _)| present.contains(card.as_str()))
            .map(|(card, entry)| (card.clone(), entry.clone()))
            .collect()
    }
    let mut pruned = document.clone();
    if let Some(tasks) = &document.sections.tasks {
        let kept = keep(tasks, present_cards);
        pruned.sections.tasks = (!kept.is_empty()).then_some(kept);
    }
    if let Some(rules) = &document.sections.reminder_rules {
        let kept = keep(rules, present_cards);
        pruned.sections.reminder_rules = (!kept.is_empty()).then_some(kept);
    }
    if let Some(dismissals) = &document.sections.dismissals {
        // The one carve-out (#588 D-12): the reserved board-level
        // `break` entry is not a card entry — it survives regardless
        // of the present-card set. The `break-rules` section needs no
        // arm at all (board-level sections are invisible to this GC —
        // the D-T10 fact, in its third application).
        let mut kept: BTreeMap<String, DismissalsEntry> = keep(dismissals, present_cards);
        if let Some(reserved) = dismissals.get(BREAK_DISMISSALS_ENTRY_KEY) {
            kept.insert(BREAK_DISMISSALS_ENTRY_KEY.to_owned(), reserved.clone());
        }
        pruned.sections.dismissals = (!kept.is_empty()).then_some(kept);
    }
    pruned
}

/// Builds one card's overlay `TaskEntry` from the task's current local
/// truth (chunk 4's D11 overlay assembly): the three always-app keys and
/// `recur` (no native lane — always a document write) as `{v, clock}`
/// full-send writes out of the task's values + `TaskClocks` slots, and
/// `scheduled` only while `!native_scheduled` (the D-V2
/// emission gating — the decision is the caller's, the transform stays
/// version-blind data).
#[must_use]
pub fn task_entry_for(task: &crate::entities::Task, native_scheduled: bool) -> TaskEntry {
    let mut fields = BTreeMap::new();
    let effort = ModeledValue::Effort(task.effort);
    fields.insert(
        "effort".to_owned(),
        TaskFieldWrite {
            value: effort,
            clock: task.clocks.effort,
            by: task.authors.effort,
        },
    );
    let importance = ModeledValue::Importance(task.importance);
    fields.insert(
        "importance".to_owned(),
        TaskFieldWrite {
            value: importance,
            clock: task.clocks.importance,
            by: task.authors.importance,
        },
    );
    let urgency = ModeledValue::Urgency(task.urgency);
    fields.insert(
        "urgency".to_owned(),
        TaskFieldWrite {
            value: urgency,
            clock: task.clocks.urgency,
            by: task.authors.urgency,
        },
    );
    // Recurrence has no native Deck lane (design fact 11): the rule is
    // always a document write. `None` serializes as the typed clear —
    // an ordinary write on the remote side, never a tombstone.
    let recur = ModeledValue::Recur(task.recur.clone());
    fields.insert(
        "recur".to_owned(),
        TaskFieldWrite {
            value: recur,
            clock: task.clocks.recur,
            by: task.authors.recur,
        },
    );
    // The task kind (issue #492) has no native Deck lane either: the
    // write is always emitted, `Standard` as the typed reset — an
    // ordinary write on the remote side, never a tombstone.
    let kind = ModeledValue::Kind(Some(task.kind));
    fields.insert(
        "kind".to_owned(),
        TaskFieldWrite {
            value: kind,
            clock: task.clocks.kind,
            by: task.authors.kind,
        },
    );
    if !native_scheduled {
        let scheduled = ModeledValue::Scheduled(task.scheduled);
        fields.insert(
            "scheduled".to_owned(),
            TaskFieldWrite {
                value: scheduled,
                clock: task.clocks.scheduled,
                by: task.authors.scheduled,
            },
        );
    }
    TaskEntry {
        fields,
        unmodeled: BTreeMap::new(),
    }
}

/// Builds one card's overlay `RulesEntry` from the task's local rules:
/// each as `Rule { offset, created, clock: updated_at }` keyed by the
/// rule UUID (D-S9). Tombstones are *derived* separately (see
/// [`derive_rule_tombstones`]) — the overlay never invents them.
#[must_use]
pub fn rules_entry_for(
    rules: &BTreeMap<crate::ids::ReminderRuleId, crate::reminders::ReminderRule>,
    task: crate::ids::TaskId,
    author: Option<crate::ids::AuthorId>,
) -> RulesEntry {
    let mapped: BTreeMap<String, RuleEntry> = rules
        .values()
        .filter(|rule| rule.task == task)
        .map(|rule| {
            (
                rule.id.as_uuid().to_string(),
                RuleEntry::Rule {
                    kind: rule.kind,
                    created: rule.created_at,
                    clock: rule.updated_at,
                    insistence: rule.insistence,
                    by: author,
                },
            )
        })
        .collect();
    RulesEntry {
        rules: mapped,
        unknown: BTreeMap::new(),
    }
}

/// Builds one card's overlay `DismissalsEntry` from the local dismissal
/// set: the task's **Rule-ref** dismissals only, each as
/// `Dismissed { dismissed_at }` keyed by [`dismissal_wire_key`] (D-11).
///
/// **Fail-closed emission filter (design D-4, #588 D-11).** The
/// state-side `ReminderDismissal` is representable for all
/// [`ReminderRef`] kinds (local Default/Adhoc dismissals must clear
/// in-app), so the filter — not the type — is the boundary:
/// `Default(TaskId)` and `Adhoc(AdhocAlarmId)` rows are skipped here
/// and can never reach a card entry, because the card-entry wire key
/// shape admits a rule UUID only. The rule's task scoping mirrors
/// [`rules_entry_for`] (`rule.task == task`), and the proptest pin
/// (`no_local_ids_reach_the_dismissals_wire`) is the tripwire against
/// a future widening. Orphan rows (rule deleted locally) are filtered
/// by the same scoping; their already-uploaded rows persist in the
/// base document via the union's additivity.
#[must_use]
pub fn dismissals_entry_for(
    dismissals: &std::collections::BTreeSet<crate::reminders::ReminderDismissal>,
    rules: &BTreeMap<crate::ids::ReminderRuleId, crate::reminders::ReminderRule>,
    task: crate::ids::TaskId,
    author: Option<crate::ids::AuthorId>,
) -> DismissalsEntry {
    let mapped: BTreeMap<String, DismissalEntry> = dismissals
        .iter()
        .filter_map(|d| match d.reminder {
            crate::reminders::ReminderRef::Rule(rule_id) => {
                rules.get(&rule_id).filter(|rule| rule.task == task)?;
                Some((
                    dismissal_wire_key(rule_id, d.occurrence),
                    DismissalEntry::Dismissed {
                        dismissed_at: d.dismissed_at,
                        by: author,
                    },
                ))
            }
            // The fail-closed arm keeps `Break ⇒ None` (the dispatch's
            // contract, #588 D-11): break dismissals ride the
            // board-level reserved entry
            // ([`BREAK_DISMISSALS_ENTRY_KEY`], see
            // [`break_dismissals_entry_for`]), never a card entry — a
            // card entry is scoped to the card's task's rules.
            crate::reminders::ReminderRef::Default(_)
            | crate::reminders::ReminderRef::Adhoc(_)
            | crate::reminders::ReminderRef::Break(_)
            // Tracker dismissals stay device-local (#504 D-5 — the
            // occurrence keys derive from the device's day calendar, so
            // a synced key cannot compose across day configs): the
            // fail-closed arm, exactly the Default/Adhoc lane.
            | crate::reminders::ReminderRef::Tracker(_) => None,
        })
        .collect();
    DismissalsEntry {
        dismissals: mapped,
        unknown: BTreeMap::new(),
    }
}

/// Builds the reserved board-level `break` entry's overlay rows from
/// the local dismissal set (#588 D-10): the local set's **Break-ref
/// rows only**, each keyed by [`break_dismissal_wire_key`], filtered
/// to rules that exist locally **with `rule.board == board`** — the
/// orphan + scoping filter mirroring [`dismissals_entry_for`]'s
/// `rule.task == task`. (The orphan arm is belt-and-braces —
/// `DeleteBreakRule` cascades break dismissals locally — but it keeps
/// the property shape identical to the recipe and scopes the lane to
/// the flushing board.) The flush inserts the entry under
/// [`BREAK_DISMISSALS_ENTRY_KEY`] only when non-empty (the
/// `overlay_dismissals` gate).
#[must_use]
pub fn break_dismissals_entry_for(
    dismissals: &std::collections::BTreeSet<crate::reminders::ReminderDismissal>,
    break_rules: &BTreeMap<crate::ids::BreakRuleId, crate::reminders::BreakRule>,
    board: crate::ids::BoardId,
    author: Option<crate::ids::AuthorId>,
) -> DismissalsEntry {
    let mapped: BTreeMap<String, DismissalEntry> = dismissals
        .iter()
        .filter_map(|d| match d.reminder {
            crate::reminders::ReminderRef::Break(rule_id) => {
                break_rules
                    .get(&rule_id)
                    .filter(|rule| rule.board == board)?;
                Some((
                    break_dismissal_wire_key(rule_id, d.occurrence),
                    DismissalEntry::Dismissed {
                        dismissed_at: d.dismissed_at,
                        by: author,
                    },
                ))
            }
            // Break-ref rows are this entry's whole grammar; every
            // other family stays in its own lane (the per-card
            // entries' filter).
            crate::reminders::ReminderRef::Default(_)
            | crate::reminders::ReminderRef::Adhoc(_)
            | crate::reminders::ReminderRef::Rule(_)
            // Tracker-ref rows are nobody's lane here (per-device
            // dismissals, #504 D-5) — the fail-closed arm.
            | crate::reminders::ReminderRef::Tracker(_) => None,
        })
        .collect();
    DismissalsEntry {
        dismissals: mapped,
        unknown: BTreeMap::new(),
    }
}

/// Derives the removal tombstones for one card's flush (chunk 4's D8):
/// every key in `base.rules` that decodes as a *live* `Rule` and has no
/// local rule with that UUID gets `{ deleted: true, clock }` — the
/// removing command's op stamp is the removal instant. Existing
/// tombstones pass through untouched (no re-stamp); `Verbatim` entries
/// are never ordered against. Absence-from-base is never an input: only
/// base-*present* live rules missing locally tombstone, so a truncated
/// foreign view can never cause a deletion here.
#[must_use]
pub fn derive_rule_tombstones(
    base: &RulesEntry,
    local: &BTreeMap<crate::ids::ReminderRuleId, crate::reminders::ReminderRule>,
    clock: DateTime<Utc>,
    author: Option<crate::ids::AuthorId>,
) -> BTreeMap<String, RuleEntry> {
    base.rules
        .iter()
        .filter_map(|(key, entry)| {
            let RuleEntry::Rule { .. } = entry else {
                return None; // tombstones pass through, verbatim never ordered
            };
            let uuid = uuid::Uuid::parse_str(key).ok()?;
            let id = crate::ids::ReminderRuleId::from(uuid);
            (!local.contains_key(&id))
                .then(|| (key.clone(), RuleEntry::Tombstone { clock, by: author }))
        })
        .collect()
}

/// The flush's lane GC (D-V2): while the connector natively owns
/// `startdate`, every modeled `scheduled` key is removed from the tasks
/// entries (native owns the field; the document must not carry a stale
/// key). `drop == false` returns the document unchanged. The flag is a
/// parameter — the lane decision lives in the sync layer.
#[must_use]
pub fn lane_gc_scheduled(document: &AppDataDocument, drop: bool) -> AppDataDocument {
    let mut gc = document.clone();
    if drop && let Some(tasks) = gc.sections.tasks.as_mut() {
        for entry in tasks.values_mut() {
            entry.fields.remove("scheduled");
        }
    }
    gc
}

/// The time-entries retention window in days (design D-T13): a named
/// constant, never a `[tracking]` config knob (the surface is fenced —
/// retention aging is a wire-compaction concern, not a policy). The
/// overlay builder and the lane GC filter by this one window so an aged
/// row is never re-emitted and the union cannot resurrect it. **The
/// local database never drops history** — this constant bounds the
/// document only; `export` reads full local history.
pub const TRACKING_SYNC_RETENTION_DAYS: i64 = 180;

/// `now − TRACKING_SYNC_RETENTION_DAYS`: rows whose window instant
/// (`start` for live entries, `clock` for tombstones) falls **before**
/// this instant are aged; the boundary itself is inside.
fn time_entry_window_start(now: DateTime<Utc>) -> DateTime<Utc> {
    now - chrono::TimeDelta::try_days(TRACKING_SYNC_RETENTION_DAYS)
        .expect("180 days is within TimeDelta's range")
}

/// Builds the flush overlay's `time-entries` section from the local
/// tracking state (design D-T10/D-T13). The emission filter is
/// fail-closed by construction:
///
/// - the entry must be **within the retention window**
///   (`start >= now − TRACKING_SYNC_RETENTION_DAYS`; aged rows are
///   never re-emitted — overlay and lane GC agree on the one window);
/// - the entry's task must exist **and bind to `board`**
///   (`task.remote` is `Some` with `.board == board`): unbound entries
///   are device-local and never emitted, other boards' entries are not
///   this document's, and a dangling task id (possible after
///   `ClearBoardContent`, D-15) emits nothing;
/// - **`task.deleted` does not filter**: a soft-deleted-but-bound
///   task's entries keep flowing so post-deletion adjusts/deletes roam
///   (soft-deleted tasks keep local rows precisely so their ops stay
///   resolvable);
/// - the emitted row's `task` is the decimal remote card id; `clock`
///   is the entry's LWW mutation clock — the remote stop is just a
///   newer write (D-T11).
#[must_use]
pub fn time_entries_for(
    entries: &BTreeMap<crate::ids::TimeEntryId, crate::tracking::TimeEntry>,
    tasks: &BTreeMap<crate::ids::TaskId, crate::entities::Task>,
    board: RemoteBoardId,
    now: DateTime<Utc>,
    author: Option<crate::ids::AuthorId>,
) -> BTreeMap<String, TimeEntryRecord> {
    time_entries_for_lane(entries, tasks, now, author, |task| {
        let binding = task.remote.as_ref()?;
        if binding.board != board {
            return None; // unbound or another board's document
        }
        // The card key is the Deck wire number (the sidecar is a
        // Deck-endpoint artifact); a Files payload has no Deck
        // number and no sidecar home — the entry is skipped.
        binding.card.as_deck().map(|n| n.to_string())
    })
}

/// The files lane's time-entries builder (D-BF9): the sidecar's
/// retention window and row codec verbatim, but the card key is the
/// task's *domain UUID* (files rows carry no remote child ids) and
/// membership is the board's own stacks. Pair of
/// [`time_entries_for`] — the two share [`time_entries_for_lane`],
/// diverging only in the lane's card-key shape.
#[must_use]
pub fn files_time_entries_for(
    entries: &BTreeMap<crate::ids::TimeEntryId, crate::tracking::TimeEntry>,
    tasks: &BTreeMap<crate::ids::TaskId, crate::entities::Task>,
    stacks: &BTreeMap<crate::ids::StackId, crate::entities::Stack>,
    board: crate::ids::BoardId,
    now: DateTime<Utc>,
    author: Option<crate::ids::AuthorId>,
) -> BTreeMap<String, TimeEntryRecord> {
    time_entries_for_lane(entries, tasks, now, author, |task| {
        let stack = stacks.get(&task.stack)?;
        if stack.board != board {
            return None; // another board's document
        }
        Some(task.id.as_uuid().to_string())
    })
}

/// The lane-shared time-entries builder body: retention window and row
/// codec once, with each sync lane supplying its own card key +
/// membership predicate (Deck keys by wire card number, files by the
/// domain UUID).
fn time_entries_for_lane(
    entries: &BTreeMap<crate::ids::TimeEntryId, crate::tracking::TimeEntry>,
    tasks: &BTreeMap<crate::ids::TaskId, crate::entities::Task>,
    now: DateTime<Utc>,
    author: Option<crate::ids::AuthorId>,
    card_key_of: impl Fn(&crate::entities::Task) -> Option<String>,
) -> BTreeMap<String, TimeEntryRecord> {
    let window_start = time_entry_window_start(now);
    entries
        .iter()
        .filter_map(|(id, entry)| {
            if entry.start < window_start {
                return None; // aged: never re-emitted (D-T13)
            }
            let task = tasks.get(&entry.task)?;
            let card_key = card_key_of(task)?;
            Some((
                id.as_uuid().to_string(),
                TimeEntryRecord::Entry {
                    task: Some(card_key),
                    title: entry.task_title.clone(),
                    start: entry.start,
                    end: entry.end,
                    created: entry.created_at,
                    clock: entry.updated_at,
                    kind: entry.kind,
                    by: author,
                },
            ))
        })
        .collect()
}

/// The flush's time-entries lane GC (design D-T13, the
/// `lane_gc_scheduled` shape): strips section rows whose window instant
/// falls out of the retention window — `start` for live `Entry` rows,
/// `clock` for `Tombstone` rows. `Verbatim` rows pass untouched (never
/// ordered against); an emptied section becomes `None`. **Wire-only**:
/// nothing here or anywhere else deletes a local row. Open entries get
/// no exemption (D-11): a 180-day-open entry is out of service by
/// definition; its local truth is unaffected.
#[must_use]
pub fn lane_gc_old_time_entries(document: &AppDataDocument, now: DateTime<Utc>) -> AppDataDocument {
    let mut gc = document.clone();
    if let Some(section) = gc.sections.time_entries.take() {
        let window_start = time_entry_window_start(now);
        let kept: BTreeMap<String, TimeEntryRecord> = section
            .into_iter()
            .filter(|(_, row)| match row {
                TimeEntryRecord::Entry { start, .. } => *start >= window_start,
                TimeEntryRecord::Tombstone { clock, .. } => *clock >= window_start,
                TimeEntryRecord::Verbatim(_) => true,
            })
            .collect();
        gc.sections.time_entries = (!kept.is_empty()).then_some(kept);
    }
    gc
}

/// Derives the removal tombstones for the `time-entries` section (design
/// D-T12, the truncated-view guard): every base key gets
/// `{ deleted: true, clock }` **iff every clause of the fail-closed
/// conjunction holds** — a narrower view can never erase another
/// device's history:
///
/// 1. the base row decodes as a live `Entry` (base tombstones pass
///    through untouched — no re-stamp; `Verbatim` is never ordered
///    against);
/// 2. the key parses as a UUID (an unparseable key is not an entry
///    identity);
/// 3. the **base row's own `start` is within the retention window**
///    (a row the GC is about to strip is not tombstoned — guard on
///    what the base row carries, not the local copy, so the check
///    survives a locally-unknown row);
/// 4. the row's `task` card **resolves locally** (present in
///    `resolvable_cards` — unresolvable entries are neither imported
///    by the fold nor deleted here; they age out instead, D-T13);
/// 5. **no local entry with that UUID exists** (absence-from-base is
///    never an input — only base-*present* rows missing locally
///    tombstone).
#[must_use]
pub fn derive_time_entry_tombstones(
    base: &BTreeMap<String, TimeEntryRecord>,
    local: &BTreeMap<crate::ids::TimeEntryId, crate::tracking::TimeEntry>,
    resolvable_cards: &std::collections::BTreeSet<String>,
    now: DateTime<Utc>,
    clock: DateTime<Utc>,
    author: Option<crate::ids::AuthorId>,
) -> BTreeMap<String, TimeEntryRecord> {
    let window_start = time_entry_window_start(now);
    base.iter()
        .filter_map(|(key, row)| {
            let TimeEntryRecord::Entry {
                start,
                task: Some(task),
                ..
            } = row
            else {
                return None; // tombstones pass through, verbatim never ordered
            };
            let uuid = uuid::Uuid::parse_str(key).ok()?;
            if *start < window_start {
                return None; // aged rows are stripped, not tombstoned (D-T13)
            }
            if !resolvable_cards.contains(task) {
                return None; // an unresolvable card never deletes
            }
            let id = crate::ids::TimeEntryId::from(uuid);
            (!local.contains_key(&id)).then(|| {
                (
                    key.clone(),
                    TimeEntryRecord::Tombstone { clock, by: author },
                )
            })
        })
        .collect()
}

/// The habit-log wire retention in days (design §4.5, mirroring DR-5's
/// D-T13 ruling): a named constant, never a config knob. The overlay
/// builder and the lane GC filter by this one window so an aged row is
/// never re-emitted and the union cannot resurrect it. **The local
/// database never drops history** — this constant bounds the document
/// only.
pub const HABIT_LOG_SYNC_RETENTION_DAYS: i64 = 180;

/// `today − HABIT_LOG_SYNC_RETENTION_DAYS` in the **local-date
/// vocabulary** (D-H8): a row's window date at or after this boundary is
/// in retention; the boundary itself is inside. Saturates at
/// [`chrono::NaiveDate::MIN`] (the `period_before` leniency).
fn habit_log_cutoff(today: chrono::NaiveDate) -> chrono::NaiveDate {
    today
        .checked_sub_days(chrono::Days::new(
            u64::try_from(HABIT_LOG_SYNC_RETENTION_DAYS).expect("positive constant"),
        ))
        .unwrap_or(chrono::NaiveDate::MIN)
}

/// Builds the flush overlay's `habits` section from the local habit
/// state (DR-6 §4.5, D-H10): **every** habit of `board`, live rows as
/// [`HabitRecord::Live`] and tombstoned rows as
/// [`HabitRecord::Tombstone { clock: clocks.deleted }`] — definitions
/// carry no retention, because deletions must roam forever so
/// late-adopting devices learn them. Foreign-board habits never emit.
#[must_use]
pub fn habits_for(
    habits: &BTreeMap<crate::ids::HabitId, crate::habit::Habit>,
    board: crate::ids::BoardId,
    author: Option<crate::ids::AuthorId>,
) -> BTreeMap<String, HabitRecord> {
    habits
        .values()
        .filter(|habit| habit.board == board)
        .map(|habit| {
            let row = if habit.is_live() {
                HabitRecord::Live {
                    title: habit.title.clone(),
                    target: habit.target,
                    order: habit.order,
                    anchor_time: habit.anchor_time,
                    clocks: habit.clocks,
                    continued_from: habit.continued_from,
                    by: author,
                }
            } else {
                HabitRecord::Tombstone {
                    clock: habit.clocks.deleted,
                    by: author,
                }
            };
            (habit.id.as_uuid().to_string(), row)
        })
        .collect()
}

/// Builds the flush overlay's `habit-log` section from the local habit
/// state (§4.5). The emission filter is fail-closed by construction:
/// a log row emits only if
///
/// - its habit **exists locally** (an orphaned fact never emits —
///   `ClearBoardContent` fallout included);
/// - that habit's `board == board` (other boards' facts are not this
///   document's);
/// - the window is **in retention**: `window.as_date() >= today − 180 d`
///   (D-H14 — aged rows are never re-emitted; overlay and lane GC agree
///   on the one window, so the union cannot resurrect them).
#[must_use]
pub fn habit_log_for(
    habits: &BTreeMap<crate::ids::HabitId, crate::habit::Habit>,
    log: &BTreeMap<crate::habit::HabitLogKey, crate::habit::HabitLogEntry>,
    board: crate::ids::BoardId,
    today: chrono::NaiveDate,
    author: Option<crate::ids::AuthorId>,
) -> BTreeMap<String, HabitLogRecord> {
    let cutoff = habit_log_cutoff(today);
    log.iter()
        .filter(|(key, _)| {
            habits
                .get(&key.habit)
                .is_some_and(|habit| habit.board == board)
                && key.window.as_date() >= cutoff
        })
        .map(|(key, entry)| {
            (
                key.wire_form(),
                HabitLogRecord::Entry {
                    count: entry.count,
                    clock: entry.clock,
                    by: author,
                },
            )
        })
        .collect()
}

/// The flush's habit-log lane GC (D-H14, the `lane_gc_old_time_entries`
/// shape): strips section rows whose window date — parsed from the map
/// key — falls before the retention boundary. `Verbatim` rows pass
/// untouched (never ordered against), and so does a clocked row whose
/// key does not parse: we cannot date it, and stripping it would forge
/// a deletion the union could not resurrect. An emptied section becomes
/// `None`. **Wire-only**: nothing here or anywhere else deletes a local
/// row.
#[must_use]
pub fn lane_gc_old_habit_log(
    document: &AppDataDocument,
    today: chrono::NaiveDate,
) -> AppDataDocument {
    let mut gc = document.clone();
    if let Some(section) = gc.sections.habit_log.take() {
        let cutoff = habit_log_cutoff(today);
        let kept: BTreeMap<String, HabitLogRecord> = section
            .into_iter()
            .filter(|(key, row)| match row {
                HabitLogRecord::Verbatim(_) => true,
                HabitLogRecord::Entry { .. } => crate::habit::HabitLogKey::parse_wire_form(key)
                    .is_none_or(|parsed| parsed.window.as_date() >= cutoff),
            })
            .collect();
        gc.sections.habit_log = (!kept.is_empty()).then_some(kept);
    }
    gc
}

/// Builds the flush overlay's `break-rules` section from the local
/// break-rule state (#588 D-2/D-5, the `habits_for` device): every
/// live row of `board` (rows carry no local tombstone state — deletion
/// is a hard delete, the tombstones *derive* from the base document,
/// see [`derive_break_rule_tombstones`]). The wire row carries no
/// board (D-H11's device — the fold stamps it); `clock` is the row's
/// creation stamp (v1's only verbs are create and delete, D-5).
#[must_use]
pub fn break_rules_for(
    break_rules: &BTreeMap<crate::ids::BreakRuleId, crate::reminders::BreakRule>,
    board: crate::ids::BoardId,
    author: Option<crate::ids::AuthorId>,
) -> BTreeMap<String, BreakRuleEntry> {
    break_rules
        .values()
        .filter(|rule| rule.board == board)
        .map(|rule| {
            (
                rule.id.as_uuid().to_string(),
                BreakRuleEntry::Rule {
                    label: rule.label.clone(),
                    threshold: rule.threshold,
                    created: rule.created_at,
                    clock: rule.created_at,
                    insistence: rule.insistence,
                    by: author,
                },
            )
        })
        .collect()
}

/// Derives the removal tombstones for the `break-rules` section (#588
/// D-7, the [`derive_rule_tombstones`] shape with its truncated-view
/// guard carried over): every base key that decodes as a *live* `Rule`
/// row and has no local rule with that UUID gets
/// `{ deleted: true, clock }` — the removing command's op stamp is the
/// removal instant. Existing tombstones pass through untouched (no
/// re-stamp); `Verbatim` entries are never ordered against.
/// Absence-from-base is never an input: only base-*present* live rows
/// missing locally tombstone, so a truncated foreign view can never
/// cause a deletion here.
#[must_use]
pub fn derive_break_rule_tombstones(
    base: &BTreeMap<String, BreakRuleEntry>,
    local: &BTreeMap<crate::ids::BreakRuleId, crate::reminders::BreakRule>,
    clock: DateTime<Utc>,
    author: Option<crate::ids::AuthorId>,
) -> BTreeMap<String, BreakRuleEntry> {
    base.iter()
        .filter_map(|(key, entry)| {
            let BreakRuleEntry::Rule { .. } = entry else {
                return None; // tombstones pass through, verbatim never ordered
            };
            let uuid = uuid::Uuid::parse_str(key).ok()?;
            let id = crate::ids::BreakRuleId::from(uuid);
            (!local.contains_key(&id))
                .then(|| (key.clone(), BreakRuleEntry::Tombstone { clock, by: author }))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::TASK_FIELDS;
    use crate::reminders::Insistence;
    use chrono::TimeZone;
    use proptest::prelude::*;

    fn ts(secs: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(secs, 0).unwrap()
    }

    fn write(value: ModeledValue, clock: i64) -> TaskFieldWrite {
        TaskFieldWrite {
            value,
            clock: ts(clock),

            by: None,
        }
    }

    fn document(board: u64, sections: AppDataSections) -> AppDataDocument {
        AppDataDocument {
            format: APPDATA_FORMAT,
            board: RemoteBoardId::deck(board),
            sections,
            unknown: BTreeMap::new(),
        }
    }

    fn entry(fields: &[(&str, TaskFieldWrite)]) -> TaskEntry {
        TaskEntry {
            fields: fields
                .iter()
                .map(|(k, w)| ((*k).to_owned(), w.clone()))
                .collect(),
            unmodeled: BTreeMap::new(),
        }
    }

    fn break_rule_row(raw: u128, board: u64) -> crate::reminders::BreakRule {
        crate::reminders::BreakRule {
            id: crate::ids::BreakRuleId::from(uuid::Uuid::from_u128(raw)),
            board: crate::ids::BoardId::from(uuid::Uuid::from_u128(board.into())),
            label: "time for a break".to_owned(),
            threshold: crate::tracking::WorkThreshold::try_new(
                chrono::TimeDelta::try_seconds(1_500).unwrap(),
            )
            .unwrap(),
            insistence: Insistence::Alarm,
            created_at: ts(100),
        }
    }

    fn break_rule_entry() -> BreakRuleEntry {
        BreakRuleEntry::Rule {
            label: "time for a break".to_owned(),
            threshold: crate::tracking::WorkThreshold::try_new(
                chrono::TimeDelta::try_seconds(1_500).unwrap(),
            )
            .unwrap(),
            created: ts(100),
            clock: ts(100),
            insistence: Insistence::Alarm,

            by: None,
        }
    }

    #[test]
    fn envelope_and_sections_round_trip_through_serde() {
        let sections = AppDataSections {
            tasks: Some(
                [(
                    "55".to_owned(),
                    entry(&[(
                        "effort",
                        write(ModeledValue::Effort(Some(Effort::new(900))), 100),
                    )]),
                )]
                .into_iter()
                .collect(),
            ),
            reminder_rules: Some(
                [(
                    "55".to_owned(),
                    RulesEntry {
                        rules: [(
                            "018f0000-0000-7000-8000-000000000001".to_owned(),
                            RuleEntry::Rule {
                                insistence: Insistence::Normal,
                                kind: crate::reminders::ReminderKind::DueOffset(
                                    ReminderOffset::before(chrono::TimeDelta::seconds(-900))
                                        .unwrap(),
                                ),
                                created: ts(100),
                                clock: ts(120),

                                by: None,
                            },
                        )]
                        .into(),
                        unknown: BTreeMap::new(),
                    },
                )]
                .into_iter()
                .collect(),
            ),
            ..Default::default()
        };
        let doc = document(2, sections);
        let json = serde_json::to_value(&doc).unwrap();
        // The wire shapes are the design's illustrations.
        assert_eq!(json["format"], 1);
        assert_eq!(json["board"], 2, "the remote board id rides bare");
        assert!(json["sections"]["tasks"]["55"]["fields"]["effort"]["v"].is_u64());
        assert_eq!(
            json["sections"]["reminder-rules"]["55"]["rules"]["018f0000-0000-7000-8000-000000000001"]
                ["offset"],
            -900
        );
        let back: AppDataDocument = serde_json::from_value(json).unwrap();
        assert_eq!(back, doc);
    }

    #[test]
    fn cleared_modeled_values_encode_as_null_and_decode_cleared() {
        let e = entry(&[
            ("effort", write(ModeledValue::Effort(None), 100)),
            ("scheduled", write(ModeledValue::Scheduled(None), 101)),
        ]);
        let json = serde_json::to_value(&e).unwrap();
        assert_eq!(json["fields"]["effort"]["v"], Value::Null);
        assert_eq!(json["fields"]["scheduled"]["v"], Value::Null);
        let back: TaskEntry = serde_json::from_value(json).unwrap();
        assert_eq!(back, e, "a cleared field round-trips as the None arm");
    }

    #[test]
    fn the_four_modeled_keys_decode_into_their_typed_arms() {
        let json = serde_json::json!({
            "fields": {
                "effort": { "v": 900, "clock": "2026-01-01T00:00:00Z" },
                "importance": { "v": "high", "clock": "2026-01-01T00:00:01Z" },
                "urgency": { "v": "very_low", "clock": "2026-01-01T00:00:02Z" },
                "scheduled": { "v": "2026-04-01T08:00:00Z", "clock": "2026-01-01T00:00:03Z" }
            }
        });
        let back: TaskEntry = serde_json::from_value(json).unwrap();
        assert_eq!(back.fields.len(), 4);
        assert_eq!(back.unmodeled.len(), 0);
        assert_eq!(
            back.fields["effort"].value,
            ModeledValue::Effort(Some(Effort::new(900)))
        );
        assert_eq!(
            back.fields["importance"].value,
            ModeledValue::Importance(Some(Importance::High))
        );
        assert_eq!(
            back.fields["urgency"].value,
            ModeledValue::Urgency(Some(Urgency::VeryLow))
        );
        assert_eq!(
            back.fields["scheduled"].value,
            ModeledValue::Scheduled(Some(
                "2026-04-01T08:00:00Z".parse().expect("fixed test instant")
            ))
        );
        // Same envelope, `scheduled` cleared (D-V2's fallback lane).
        let cleared: TaskEntry = serde_json::from_value(serde_json::json!({
            "fields": { "scheduled": { "v": null, "clock": "2026-01-01T00:00:03Z" } }
        }))
        .unwrap();
        assert_eq!(
            cleared.fields["scheduled"].value,
            ModeledValue::Scheduled(None)
        );
    }

    #[test]
    fn the_kind_key_decodes_reset_typed_and_demotes_unknown_values() {
        // `null` is the typed reset to `Standard`; the unit names decode
        // typed; an unknown text demotes the field to unmodeled — the
        // document never fails (D-S2's materialization rule).
        let reset: TaskEntry = serde_json::from_value(serde_json::json!({
            "fields": { "kind": { "v": null, "clock": "2026-01-01T00:00:00Z" } }
        }))
        .unwrap();
        assert_eq!(
            reset.fields["kind"].value,
            ModeledValue::Kind(None),
            "null decodes the reset write"
        );
        let typed: TaskEntry = serde_json::from_value(serde_json::json!({
            "fields": { "kind": { "v": "tracker", "clock": "2026-01-01T00:00:00Z" } }
        }))
        .unwrap();
        assert_eq!(
            typed.fields["kind"].value,
            ModeledValue::Kind(Some(crate::entities::TaskKind::Tracker))
        );
        let bogus: TaskEntry = serde_json::from_value(serde_json::json!({
            "fields": { "kind": { "v": "bogus", "clock": "2026-01-01T00:00:00Z" } }
        }))
        .unwrap();
        assert!(bogus.fields.is_empty());
        assert!(
            bogus.unmodeled.contains_key("fields"),
            "an unknown kind value demotes verbatim"
        );
        // The wire key round-trips through the envelope losslessly.
        let write = TaskFieldWrite {
            value: ModeledValue::Kind(Some(crate::entities::TaskKind::Tracker)),
            clock: "2026-01-01T00:00:00Z".parse().expect("fixed instant"),

            by: None,
        };
        let encoded = serde_json::to_value(&write).unwrap();
        assert_eq!(encoded["v"], serde_json::json!("tracker"));
    }

    #[test]
    fn the_recur_key_decodes_into_its_typed_arm_and_clears_via_null() {
        let rule_json = serde_json::json!({
            "series": "018f0000-0000-7000-8000-000000000001",
            "anchor": "2026-01-01",
            "pattern": { "Daily": { "every": 2 } },
            "bound": "None",
            "continued_from": null
        });
        let json = serde_json::json!({
            "fields": {
                "recur": { "v": rule_json, "clock": "2026-01-01T00:00:00Z" }
            }
        });
        let back: TaskEntry = serde_json::from_value(json).unwrap();
        assert_eq!(back.unmodeled.len(), 0);
        assert_eq!(
            back.fields["recur"].value,
            ModeledValue::Recur(Some(serde_json::from_value(rule_json).unwrap()))
        );
        // Cleared: `v: null` is an ordinary write of the None arm.
        let cleared: TaskEntry = serde_json::from_value(serde_json::json!({
            "fields": { "recur": { "v": null, "clock": "2026-01-01T00:00:00Z" } }
        }))
        .unwrap();
        assert_eq!(cleared.fields["recur"].value, ModeledValue::Recur(None));
    }

    #[test]
    fn an_undecodable_recur_value_demotes_to_verbatim() {
        // A peer wrote an `every: 0` rule (the mints reject it at the
        // decode boundary): the key demotes verbatim, siblings decode —
        // the document never fails (§6.1).
        let bad = serde_json::json!({ "v": {"series": "x", "pattern": {"Daily": {"every": 0}}}, "clock": "2026-01-01T00:00:00Z" });
        let doc: AppDataDocument = serde_json::from_value(serde_json::json!({
            "format": 1, "board": 2,
            "sections": { "tasks": { "55": { "fields": {
                "recur": bad,
                "urgency": { "v": "high", "clock": "2026-01-01T00:00:01Z" }
            } } } }
        }))
        .unwrap();
        let task = &doc.sections.tasks.as_ref().unwrap()["55"];
        assert_eq!(
            task.fields["urgency"].value,
            ModeledValue::Urgency(Some(Urgency::High))
        );
        assert_eq!(
            task.unmodeled.get("fields").unwrap()["recur"],
            serde_json::json!({ "v": {"series": "x", "pattern": {"Daily": {"every": 0}}}, "clock": "2026-01-01T00:00:00Z" })
        );
    }

    #[test]
    fn reserved_names_and_unknown_content_stay_verbatim() {
        // The reserved field key and section names never collide with a
        // modeled meaning; everything unknown survives decode→encode
        // value-identically.
        let json = serde_json::json!({
            "format": 1,
            "board": 2,
            "envelope-future": { "a": [1, 2] },
            "sections": {
                "tasks": {
                    "55": {
                        "fields": {
                            "task-kind": "normal",
                            "effort": { "v": 1, "clock": "2026-01-01T00:00:00Z" }
                        },
                        "unknown-beside-fields": true
                    }
                },
                "proofs": { "hash": "abc" },
                "settings": { "theme": "dark" }
            }
        });
        let doc: AppDataDocument = serde_json::from_value(json.clone()).unwrap();
        assert!(doc.unknown.contains_key("envelope-future"));
        assert!(doc.sections.unknown.contains_key("proofs"));
        assert!(doc.sections.unknown.contains_key("settings"));
        let task = &doc.sections.tasks.as_ref().unwrap()["55"];
        assert_eq!(task.unmodeled.get("fields").unwrap()["task-kind"], "normal");
        assert_eq!(task.unmodeled["unknown-beside-fields"], true);
        let re = serde_json::to_value(&doc).unwrap();
        assert_eq!(re["envelope-future"], json["envelope-future"]);
        assert_eq!(re["sections"]["proofs"], json["sections"]["proofs"]);
        assert_eq!(
            re["sections"]["tasks"]["55"]["fields"]["task-kind"],
            json["sections"]["tasks"]["55"]["fields"]["task-kind"]
        );
        assert_eq!(
            re["sections"]["tasks"]["55"]["unknown-beside-fields"],
            json["sections"]["tasks"]["55"]["unknown-beside-fields"]
        );
    }

    #[test]
    fn a_malformed_modeled_key_demotes_instead_of_failing() {
        // A peer wrote the effort as a string (D9's leniency rule): the
        // key demotes to verbatim; the sibling keys still decode.
        let doc: AppDataDocument = serde_json::from_value(serde_json::json!({
            "format": 1, "board": 2,
            "sections": { "tasks": { "55": { "fields": {
                "effort": { "v": "nine hundred", "clock": "2026-01-01T00:00:00Z" },
                "urgency": { "v": "high", "clock": "2026-01-01T00:00:01Z" }
            } } } }
        }))
        .unwrap();
        let task = &doc.sections.tasks.as_ref().unwrap()["55"];
        assert_eq!(
            task.fields["urgency"].value,
            ModeledValue::Urgency(Some(Urgency::High))
        );
        assert_eq!(
            task.unmodeled.get("fields").unwrap()["effort"],
            serde_json::json!({ "v": "nine hundred", "clock": "2026-01-01T00:00:00Z" })
        );
    }

    #[test]
    fn the_strict_boundary_is_the_envelope() {
        // Missing format: the strict Decode-class failure.
        let missing: Result<AppDataDocument, _> =
            serde_json::from_value(serde_json::json!({ "board": 2, "sections": {} }));
        assert!(missing.is_err(), "a format-less document must not decode");
        // A non-document JSON value: the same.
        let garbage: Result<AppDataDocument, _> =
            serde_json::from_value(serde_json::json!([1, 2, 3]));
        assert!(garbage.is_err());
    }

    #[test]
    fn rule_shapes_decode_and_tombstones_keep_the_boolean_tag() {
        let entry: RulesEntry = serde_json::from_value(serde_json::json!({
            "rules": {
                "018f0000-0000-7000-8000-000000000001":
                    { "offset": -900, "created": "2026-01-01T00:00:00Z",
                      "clock": "2026-01-01T00:01:00Z" },
                "018f0000-0000-7000-8000-000000000002":
                    { "deleted": true, "clock": "2026-01-01T00:02:00Z" },
                "018f0000-0000-7000-8000-000000000003":
                    { "future-shape": { "whatever": 1 } }
            }
        }))
        .unwrap();
        assert!(matches!(
            entry.rules["018f0000-0000-7000-8000-000000000001"],
            RuleEntry::Rule { .. }
        ));
        assert!(matches!(
            entry.rules["018f0000-0000-7000-8000-000000000002"],
            RuleEntry::Tombstone { .. }
        ));
        assert!(matches!(
            entry.rules["018f0000-0000-7000-8000-000000000003"],
            RuleEntry::Verbatim(_)
        ));
        // The tombstone re-encodes with the boolean tag, the rule with
        // the integer-seconds offset.
        let json = serde_json::to_value(&entry).unwrap();
        assert_eq!(
            json["rules"]["018f0000-0000-7000-8000-000000000002"]["deleted"],
            true
        );
        assert_eq!(
            json["rules"]["018f0000-0000-7000-8000-000000000001"]["offset"],
            -900
        );
    }

    #[test]
    fn a_positive_rule_offset_demotes_to_verbatim() {
        // Positive offsets are unconstructible in the domain; the shape
        // is unmodelable and stays verbatim.
        let entry: RulesEntry = serde_json::from_value(serde_json::json!({
            "rules": { "r": { "offset": 900, "created": "2026-01-01T00:00:00Z",
                              "clock": "2026-01-01T00:01:00Z" } }
        }))
        .unwrap();
        assert!(matches!(entry.rules["r"], RuleEntry::Verbatim(_)));
    }

    #[test]
    fn rule_entry_insistence_default_on_missing_roundtrips_and_emits() {
        // The exact pre-insistence document shape: no insistence key.
        let entry: RulesEntry = serde_json::from_value(serde_json::json!({
            "rules": { "r": { "offset": -900, "created": "2026-01-01T00:00:00Z",
                              "clock": "2026-01-01T00:01:00Z" } }
        }))
        .unwrap();
        assert_eq!(
            entry.rules["r"],
            RuleEntry::Rule {
                kind: crate::reminders::ReminderKind::DueOffset(
                    crate::reminders::ReminderOffset::before(
                        chrono::TimeDelta::try_minutes(-15).unwrap(),
                    )
                    .unwrap()
                ),
                created: "2026-01-01T00:00:00Z".parse().unwrap(),
                clock: "2026-01-01T00:01:00Z".parse().unwrap(),
                insistence: crate::reminders::Insistence::Normal,

                by: None,
            },
            "the missing key folds Normal"
        );
        // An explicit alarm rung roundtrips.
        let entry: RulesEntry = serde_json::from_value(serde_json::json!({
            "rules": { "r": { "offset": -900, "created": "2026-01-01T00:00:00Z",
                              "clock": "2026-01-01T00:01:00Z",
                              "insistence": "alarm" } }
        }))
        .unwrap();
        assert_eq!(
            entry.rules["r"],
            RuleEntry::Rule {
                kind: crate::reminders::ReminderKind::DueOffset(
                    crate::reminders::ReminderOffset::before(
                        chrono::TimeDelta::try_minutes(-15).unwrap(),
                    )
                    .unwrap()
                ),
                created: "2026-01-01T00:00:00Z".parse().unwrap(),
                clock: "2026-01-01T00:01:00Z".parse().unwrap(),
                insistence: crate::reminders::Insistence::Alarm,

                by: None,
            }
        );
        // Emission writes the key unconditionally on a clocked entry.
        let json = serde_json::to_value(&entry).unwrap();
        assert_eq!(json["rules"]["r"]["insistence"], "alarm");
    }

    #[test]
    fn an_unknown_insistence_value_demotes_to_verbatim() {
        // A rung this version does not know is unmodelable content —
        // preserved verbatim, never silently folded down to Normal
        // (the positive-offset precedent).
        let entry: RulesEntry = serde_json::from_value(serde_json::json!({
            "rules": { "r": { "offset": -900, "created": "2026-01-01T00:00:00Z",
                              "clock": "2026-01-01T00:01:00Z",
                              "insistence": "critical" } }
        }))
        .unwrap();
        assert!(matches!(entry.rules["r"], RuleEntry::Verbatim(_)));
    }

    #[test]
    fn mixed_fleet_old_shape_folds_and_unknown_sibling_relays_verbatim() {
        // One pre-round device's old-shape rule entry + one entry whose
        // rung this version does not model: the old shape folds Normal
        // locally, and the union re-emits the unknown entry verbatim
        // (the D-S2 relay, extended to the insistence rung).
        let raw = serde_json::json!({
            "rules": {
                "018f0000-0000-7000-8000-000000000001":
                    { "offset": -900, "created": "2026-01-01T00:00:00Z",
                      "clock": "2026-01-01T00:01:00Z" },
                "018f0000-0000-7000-8000-000000000002":
                    { "offset": -60, "created": "2026-01-01T00:00:00Z",
                      "clock": "2026-01-01T00:01:00Z", "insistence": "siren" }
            }
        });
        let entry: RulesEntry = serde_json::from_value(raw.clone()).unwrap();
        let folded = entry.rules["018f0000-0000-7000-8000-000000000001"].clone();
        assert!(matches!(
            folded,
            RuleEntry::Rule {
                insistence: crate::reminders::Insistence::Normal,
                ..
            }
        ));
        // The document union (the flush's merge) relays the unknown
        // entry untouched.
        let base: crate::appdata::AppDataDocument = serde_json::from_value(serde_json::json!({
            "format": 2, "board": 7, "sections": { "reminder-rules": { "t": raw } }
        }))
        .unwrap();
        let overlay: crate::appdata::AppDataDocument = serde_json::from_value(serde_json::json!({
            "format": 2, "board": 7, "sections": {}
        }))
        .unwrap();
        let merged = crate::appdata::union(&base, &overlay);
        let relayed = &merged.sections.reminder_rules.as_ref().unwrap()["t"];
        assert_eq!(
            serde_json::to_value(relayed).unwrap(),
            serde_json::json!({
                "rules": {
                    // The known old shape folds Normal and re-emits the
                    // fully-owned shape (emission is unconditional).
                    "018f0000-0000-7000-8000-000000000001":
                        { "offset": -900, "created": "2026-01-01T00:00:00Z",
                          "clock": "2026-01-01T00:01:00Z",
                          "insistence": "normal" },
                    // The unknown rung survives the union value-for-value.
                    "018f0000-0000-7000-8000-000000000002":
                        { "offset": -60, "created": "2026-01-01T00:00:00Z",
                          "clock": "2026-01-01T00:01:00Z", "insistence": "siren" }
                }
            }),
        );
    }

    #[test]
    fn union_picks_per_key_newer_clocks_and_resolves_ties_to_the_overlay() {
        let base = document(
            2,
            AppDataSections {
                tasks: Some(
                    [(
                        "55".to_owned(),
                        entry(&[
                            (
                                "effort",
                                write(ModeledValue::Effort(Some(Effort::new(1))), 100),
                            ),
                            ("urgency", write(ModeledValue::Urgency(None), 90)),
                        ]),
                    )]
                    .into(),
                ),
                ..Default::default()
            },
        );
        let overlay = document(
            2,
            AppDataSections {
                tasks: Some(
                    [(
                        "55".to_owned(),
                        entry(&[
                            // Older: base keeps its value.
                            (
                                "effort",
                                write(ModeledValue::Effort(Some(Effort::new(2))), 50),
                            ),
                            // Equal clock: the overlay's side wins.
                            (
                                "urgency",
                                write(ModeledValue::Urgency(Some(Urgency::High)), 90),
                            ),
                            // New: the overlay's key lands.
                            (
                                "scheduled",
                                write(ModeledValue::Scheduled(Some(ts(200))), 150),
                            ),
                        ]),
                    )]
                    .into(),
                ),
                ..Default::default()
            },
        );
        let merged = union(&base, &overlay);
        let task = &merged.sections.tasks.as_ref().unwrap()["55"];
        assert_eq!(
            task.fields["effort"].value,
            ModeledValue::Effort(Some(Effort::new(1))),
            "the newer embedded clock wins"
        );
        assert_eq!(
            task.fields["urgency"].value,
            ModeledValue::Urgency(Some(Urgency::High)),
            "an equal clock resolves to the overlay"
        );
        assert_eq!(
            task.fields["scheduled"].value,
            ModeledValue::Scheduled(Some(ts(200)))
        );
    }

    #[test]
    fn union_preserves_unknown_content_from_the_base_and_extends_additively() {
        let mut base = document(2, AppDataSections::default());
        base.unknown.insert("env".to_owned(), serde_json::json!(1));
        base.sections
            .unknown
            .insert("proofs".to_owned(), serde_json::json!({ "hash": "abc" }));
        base.sections.tasks = Some(
            [(
                "55".to_owned(),
                TaskEntry {
                    fields: BTreeMap::new(),
                    unmodeled: [("keep".to_owned(), serde_json::json!("me"))].into(),
                },
            )]
            .into(),
        );
        let mut overlay = document(2, AppDataSections::default());
        overlay
            .unknown
            .insert("fresh".to_owned(), serde_json::json!(2));
        overlay.sections.unknown.insert(
            "settings".to_owned(),
            serde_json::json!({ "theme": "dark" }),
        );
        overlay.sections.tasks = Some(
            [(
                "55".to_owned(),
                TaskEntry {
                    fields: [(
                        "effort".to_owned(),
                        write(ModeledValue::Effort(Some(Effort::new(5))), 100),
                    )]
                    .into(),
                    unmodeled: BTreeMap::new(),
                },
            )]
            .into(),
        );
        let merged = union(&base, &overlay);
        assert_eq!(merged.unknown["env"], serde_json::json!(1));
        assert_eq!(merged.unknown["fresh"], serde_json::json!(2));
        assert_eq!(
            merged.sections.unknown["proofs"],
            serde_json::json!({"hash": "abc"})
        );
        assert_eq!(
            merged.sections.unknown["settings"],
            serde_json::json!({"theme": "dark"})
        );
        let task = &merged.sections.tasks.as_ref().unwrap()["55"];
        assert_eq!(task.unmodeled["keep"], "me");
        assert_eq!(
            task.fields["effort"].value,
            ModeledValue::Effort(Some(Effort::new(5)))
        );
    }

    #[test]
    fn union_of_a_document_with_itself_is_the_document() {
        let doc = sample_document();
        assert_eq!(union(&doc, &doc), doc);
    }

    #[test]
    fn union_tombstones_compete_by_the_same_clock() {
        let rules = |clock: i64| RulesEntry {
            rules: [(
                "r".to_owned(),
                RuleEntry::Tombstone {
                    clock: ts(clock),
                    by: None,
                },
            )]
            .into(),
            unknown: BTreeMap::new(),
        };
        let mut base = document(2, AppDataSections::default());
        base.sections.reminder_rules = Some([("55".to_owned(), rules(100))].into());
        let mut overlay = document(2, AppDataSections::default());
        overlay.sections.reminder_rules = Some([("55".to_owned(), rules(200))].into());
        let merged = union(&base, &overlay);
        assert_eq!(
            merged.sections.reminder_rules.as_ref().unwrap()["55"].rules["r"]
                .clock()
                .unwrap(),
            ts(200),
            "the newer tombstone wins"
        );
    }

    #[test]
    fn prune_removes_exactly_the_absent_cards_entries() {
        let mut doc = sample_document();
        // A tombstoned rules entry for an absent card must be pruned too.
        doc.sections.reminder_rules.as_mut().unwrap().insert(
            "77".to_owned(),
            RulesEntry {
                rules: [(
                    "r".to_owned(),
                    RuleEntry::Tombstone {
                        clock: ts(1),
                        by: None,
                    },
                )]
                .into(),
                unknown: BTreeMap::new(),
            },
        );
        let present: std::collections::BTreeSet<String> =
            ["55"].into_iter().map(str::to_owned).collect();
        let pruned = prune_absent_cards(&doc, &present);
        let tasks = pruned.sections.tasks.as_ref().unwrap();
        assert_eq!(tasks.len(), 1);
        assert!(tasks.contains_key("55"));
        let rules = pruned.sections.reminder_rules.as_ref().unwrap();
        assert_eq!(rules.len(), 1);
        assert!(rules.contains_key("55"));
        // Unknown sections survive untouched.
        assert_eq!(pruned.sections.unknown, doc.sections.unknown);
        // An empty present set prunes both sections away entirely.
        let empty = prune_absent_cards(&doc, &std::collections::BTreeSet::new());
        assert!(empty.sections.tasks.is_none());
        assert!(empty.sections.reminder_rules.is_none());
    }

    /// Manifest assertion 3, layer 1 (D12, reworded per D-V2): the
    /// modeled key set equals the manifest's `AppData` rows plus
    /// `scheduled` — structurally checked against `TASK_FIELDS`, with
    /// `scheduled` verified to remain a manifest row outside the `AppData`
    /// family (the lane decision lives in the sync/pipeline layers).
    #[test]
    fn modeled_keys_are_the_appdata_rows_plus_scheduled() {
        let appdata_rows: Vec<&str> = TASK_FIELDS
            .iter()
            .filter(|(_, _, family)| matches!(family, crate::entities::FieldFamily::AppData))
            .map(|(name, _, _)| *name)
            .collect();
        let mut expected = appdata_rows.clone();
        expected.push("scheduled");
        let mut sorted_expected = expected.clone();
        sorted_expected.sort_unstable();
        let mut sorted_modeled = MODELED_TASK_FIELD_KEYS.to_vec();
        sorted_modeled.sort_unstable();
        assert_eq!(sorted_modeled, sorted_expected);
        assert_eq!(MODELED_TASK_FIELD_KEYS.len(), 6);

        // `scheduled` is a manifest Native row (the dynamic lane) and
        // never an AppData row.
        assert!(
            TASK_FIELDS
                .iter()
                .any(|(name, _, family)| *name == "scheduled"
                    && matches!(family, crate::entities::FieldFamily::Native))
        );
        assert!(!appdata_rows.contains(&"scheduled"));
        // Every modeled key dispatches (the codec's single source).
        for key in MODELED_TASK_FIELD_KEYS {
            let probe = serde_json::json!({ "v": null, "clock": "2026-01-01T00:00:00Z" });
            assert!(
                decode_field_write(key, &probe).is_some(),
                "{key} must decode its v:null envelope"
            );
        }
        // A non-modeled key never dispatches.
        assert!(decode_field_write("task-kind", &serde_json::json!({ "v": 1 })).is_none());
    }

    // ---- chunk 4: overlay builders, tombstone derivation, lane GC ----

    use crate::entities::{Task, TaskClocks};
    use crate::ids::{ReminderRuleId, TaskId};

    fn task_fixture() -> Task {
        let t = |secs: i64| Utc.timestamp_opt(secs, 0).unwrap();
        Task {
            id: TaskId::from(uuid::Uuid::from_u128(1)),
            remote: None,
            title: "task".into(),
            description: String::new(),
            duedate: None,
            scheduled: Some(t(9_000)),
            done: None,
            stack: crate::ids::StackId::from(uuid::Uuid::from_u128(2)),
            order: 0,
            label_memberships: std::collections::BTreeMap::default(),
            effort: Some(Effort::new(900)),
            importance: Some(Importance::High),
            urgency: None,
            recur: None,
            kind: crate::entities::TaskKind::Standard,

            archived: false,
            deleted: false,
            clocks: TaskClocks {
                title: t(1_000),
                description: t(1_000),
                duedate: t(1_000),
                scheduled: t(1_100),
                done: t(1_000),
                position: t(1_000),
                effort: t(1_200),
                importance: t(1_300),
                urgency: t(1_400),
                recur: t(1_400),
                kind: t(1_400),

                archived: t(1_000),
                deleted: t(1_000),
            },
            authors: crate::entities::TaskAuthors::default(),
            remote_seen: None,
            remote_baseline: None,
        }
    }

    #[test]
    fn task_entry_builds_full_send_writes_from_values_and_clocks() {
        let mut task = task_fixture();
        let entry = task_entry_for(&task, false);
        assert_eq!(entry.fields.len(), 6);
        assert_eq!(
            entry.fields["effort"],
            TaskFieldWrite {
                value: ModeledValue::Effort(Some(Effort::new(900))),
                clock: task.clocks.effort,

                by: None,
            }
        );
        assert_eq!(
            entry.fields["scheduled"].value,
            ModeledValue::Scheduled(Some(task.scheduled.unwrap()))
        );
        // Native ownership: the scheduled key is not emitted; recur and
        // kind (no native lanes) still are.
        let native = task_entry_for(&task, true);
        assert_eq!(native.fields.len(), 5);
        assert!(!native.fields.contains_key("scheduled"));
        assert!(native.fields.contains_key("recur"));
        assert!(native.fields.contains_key("kind"));
        // A dateless-of-rule task emits the typed clear.
        assert_eq!(
            native.fields["recur"],
            TaskFieldWrite {
                value: ModeledValue::Recur(None),
                clock: task.clocks.recur,

                by: None,
            }
        );
        // A task holding a rule emits the rule verbatim under its clock.
        let rule = crate::recur::RecurRule {
            series: crate::ids::SeriesId::from_uuid(uuid::Uuid::from_u128(7)),
            anchor: chrono::NaiveDate::from_ymd_opt(2026, 3, 1).unwrap(),
            pattern: crate::recur::RecurPattern::Daily {
                every: crate::recur::EveryDays::new(2).unwrap(),
            },
            bound: crate::recur::RecurBound::None,
            continued_from: None,
            paused_until: None,
        };
        task.recur = Some(rule.clone());
        task.clocks.recur = ts(2_000);
        let entry = task_entry_for(&task, false);
        assert_eq!(
            entry.fields["recur"],
            TaskFieldWrite {
                value: ModeledValue::Recur(Some(rule)),
                clock: ts(2_000),

                by: None,
            }
        );
    }

    // Property 8's row half: the emitted recur write survives a full
    // serde round-trip through the row envelope losslessly.

    // ---- the break-rules section grammar (#588 D-4) -------------------

    // P3 (the modeled row): the row round-trips through the wire shape
    // byte-for-value.
    #[test]
    fn modeled_break_rule_row_roundtrips() {
        let row = break_rule_entry();
        let json = serde_json::to_value(&row).unwrap();
        assert_eq!(json["label"], "time for a break");
        assert_eq!(json["threshold_seconds"], 1_500);
        assert_eq!(json["insistence"], "alarm");
        assert_eq!(json["created"], json["clock"], "v1 stamps creation");
        let back: BreakRuleEntry = serde_json::from_value(json).unwrap();
        assert_eq!(back, row);
    }

    #[test]
    fn deleted_row_decodes_tombstone_and_bad_clock_demotes() {
        let good: BreakRuleEntry =
            serde_json::from_value(serde_json::json!({ "deleted": true, "clock": ts(9) })).unwrap();
        assert_eq!(
            good,
            BreakRuleEntry::Tombstone {
                clock: ts(9),
                by: None
            }
        );
        let bad: BreakRuleEntry =
            serde_json::from_value(serde_json::json!({ "deleted": true, "clock": "x" })).unwrap();
        assert!(matches!(bad, BreakRuleEntry::Verbatim(_)));
    }

    #[test]
    fn missing_insistence_defaults_normal() {
        let json = serde_json::json!({
            "label": "stretch",
            "threshold_seconds": 60,
            "created": ts(1),
            "clock": ts(2),
        });
        let row: BreakRuleEntry = serde_json::from_value(json).unwrap();
        assert!(matches!(
            row,
            BreakRuleEntry::Rule {
                insistence: Insistence::Normal,
                ..
            }
        ));
    }

    #[test]
    fn unknown_insistence_demotes_the_row() {
        let json = serde_json::json!({
            "label": "stretch",
            "threshold_seconds": 60,
            "created": ts(1),
            "clock": ts(2),
            "insistence": "shout",
        });
        let row: BreakRuleEntry = serde_json::from_value(json).unwrap();
        assert!(matches!(row, BreakRuleEntry::Verbatim(_)));
    }

    #[test]
    fn zero_or_negative_threshold_demotes_the_row() {
        for secs in [0_i64, -30] {
            let json = serde_json::json!({
                "label": "stretch",
                "threshold_seconds": secs,
                "created": ts(1),
                "clock": ts(2),
                "insistence": "normal",
            });
            let row: BreakRuleEntry = serde_json::from_value(json).unwrap();
            assert!(
                matches!(row, BreakRuleEntry::Verbatim(_)),
                "threshold {secs} is domain-inconstructible and demotes"
            );
        }
    }

    #[test]
    fn non_object_row_demotes_verbatim() {
        let row: BreakRuleEntry = serde_json::from_value(serde_json::json!([1, 2, 3])).unwrap();
        assert!(matches!(row, BreakRuleEntry::Verbatim(_)));
    }

    // P4 (D-2/D-5, the emission filter): only the flushing board's rows
    // emit, every key present, `clock` is the creation stamp.
    #[test]
    fn break_rules_for_scopes_to_the_board_and_emits_every_key() {
        let mut rules = BTreeMap::new();
        let mine = break_rule_row(1, 700);
        let foreign = break_rule_row(2, 701);
        rules.insert(mine.id, mine.clone());
        rules.insert(foreign.id, foreign.clone());
        let rows = break_rules_for(
            &rules,
            crate::ids::BoardId::from(uuid::Uuid::from_u128(700)),
            None,
        );
        assert_eq!(rows.len(), 1, "foreign-board rows never emit");
        let BreakRuleEntry::Rule { clock, .. } = &rows[&mine.id.as_uuid().to_string()] else {
            panic!("the live row decodes modeled");
        };
        assert_eq!(*clock, ts(100), "v1's write clock is the creation stamp");
        let wire = serde_json::to_string(&rows).unwrap();
        assert!(
            !wire.contains(&foreign.id.as_uuid().to_string()),
            "the foreign rule's identity never reaches the document: {wire}"
        );
    }

    // P5 (D-7, the tombstone derivation): base-present live rows
    // missing locally tombstone at the given stamp; existing tombstones
    // and verbatim rows pass through untouched; locally-live rows and
    // verbatim rows never appear in the derivation output.
    #[test]
    fn derive_break_rule_tombstones_pins() {
        let local = break_rule_row(1, 700);
        let mut rules = BTreeMap::new();
        rules.insert(local.id, local.clone());
        let mut base = BTreeMap::new();
        base.insert(local.id.as_uuid().to_string(), break_rule_entry());
        let deleted = crate::ids::BreakRuleId::from(uuid::Uuid::from_u128(2));
        base.insert(
            deleted.as_uuid().to_string(),
            BreakRuleEntry::Tombstone {
                clock: ts(50),
                by: None,
            },
        );
        base.insert(
            "strange".to_owned(),
            BreakRuleEntry::Verbatim(serde_json::json!({ "mystery": true })),
        );
        let derived = derive_break_rule_tombstones(&base, &rules, ts(200), None);
        // The derivation returns only *newly derived* tombstones; the
        // existing base tombstone passes through because the flush
        // inserts the derivation into the base entry, never re-stamping.
        assert_eq!(derived.len(), 0, "nothing is missing locally");
        let mut rules_without = rules.clone();
        rules_without.remove(&local.id);
        let derived = derive_break_rule_tombstones(&base, &rules_without, ts(200), None);
        assert_eq!(
            derived.get(local.id.as_uuid().to_string().as_str()),
            Some(&BreakRuleEntry::Tombstone {
                clock: ts(200),
                by: None
            }),
            "base-present live rows missing locally tombstone at the given stamp"
        );
        assert!(
            !derived.contains_key(&deleted.as_uuid().to_string()),
            "existing tombstones are never re-stamped"
        );
        assert!(!derived.contains_key("strange"), "verbatim never ordered");
        // The truncated-view guard, positive face: a narrowed base only
        // tombstones keys it actually carries.
        let narrowed = BTreeMap::from([(
            crate::ids::BreakRuleId::from(uuid::Uuid::from_u128(3))
                .as_uuid()
                .to_string(),
            break_rule_entry(),
        )]);
        let derived = derive_break_rule_tombstones(&narrowed, &rules, ts(200), None);
        assert_eq!(derived.len(), 1);
        assert!(
            derived.contains_key(
                crate::ids::BreakRuleId::from(uuid::Uuid::from_u128(3))
                    .as_uuid()
                    .to_string()
                    .as_str()
            ),
            "base-present rows missing locally tombstone at the given stamp"
        );
    }

    // P7 (D-6, the relay leg): an old peer without the modeled field
    // relays the whole `break-rules` section verbatim through
    // decode → union → encode.
    #[test]
    fn break_rules_section_survives_an_old_peer_relay() {
        let payload = serde_json::json!({ "fancy": true });
        let sections = AppDataSections {
            break_rules: Some(
                [(
                    "018f0000-0000-7000-8000-000000000009".to_owned(),
                    serde_json::from_value::<BreakRuleEntry>(serde_json::json!({
                        "uuid-of-a-future-version": payload
                    }))
                    .unwrap(),
                )]
                .into_iter()
                .collect(),
            ),
            ..Default::default()
        };
        let base = document(2, sections);
        let overlay = document(2, AppDataSections::default());
        let merged = union(&base, &overlay);
        let row = &merged.sections.break_rules.as_ref().unwrap()
            [&"018f0000-0000-7000-8000-000000000009".to_owned()];
        let BreakRuleEntry::Verbatim(inner) = row else {
            panic!("the unmodelable row relays verbatim");
        };
        assert_eq!(
            inner,
            &serde_json::json!({ "uuid-of-a-future-version": payload })
        );
    }

    proptest! {
        #![proptest_config(crate::test_support::proptest_config(256))]
        #[test]
        fn recur_rows_round_trip_through_serde_losslessly(
            rule in crate::test_support::recur_rule_strategy(),
        ) {
            let mut task = task_fixture();
            task.recur = Some(rule.clone());
            task.clocks.recur = ts(3_000);
            let entry = task_entry_for(&task, false);
            let encoded = serde_json::to_value(&entry.fields["recur"]).unwrap();
            let decoded = decode_field_write("recur", &encoded)
                .expect("a well-formed recur write always decodes");
            assert_eq!(decoded.value, ModeledValue::Recur(Some(rule)));
            assert_eq!(decoded.clock, ts(3_000));
        }
    }

    // Property 8's whole-chain half: rule → local task → emitted overlay
    // row → serde wire form → the document parser's key dispatch → the
    // sidecar fold — lossless for generated rules.
    proptest! {
        #![proptest_config(crate::test_support::proptest_config(256))]
        #[test]
        fn recur_survives_the_whole_emit_encode_decode_fold_chain(
            rule in crate::test_support::recur_rule_strategy(),
        ) {
            let mut task = task_fixture();
            task.recur = Some(rule.clone());
            task.clocks.recur = ts(4_000);
            // Emit: the overlay builder's full-send write.
            let entry = task_entry_for(&task, false);
            // Encode: the wire shape the flush uploads.
            let encoded = serde_json::to_value(&entry).unwrap();
            // Decode: the document parser's per-key dispatch on the row.
            let fields = encoded["fields"].as_object().expect("row object");
            let decoded = decode_field_write("recur", &fields["recur"])
                .expect("a well-formed recur write always decodes");
            // Fold: doc clock == local clock (ties to the document — the
            // read result must land).
            let mut merged = task.clone();
            let folded_entry = TaskEntry {
                fields: [(
                    "recur".to_owned(),
                    decoded,
                )]
                .into(),
                unmodeled: BTreeMap::new(),
            };
            crate::merge::fold_task_app_data(
                &mut merged,
                &task,
                &folded_entry,
                crate::pipeline::PendingTaskIntent::default(),
                false,
            );
            assert_eq!(merged.recur, Some(rule));
            assert_eq!(merged.clocks.recur, ts(4_000));
        }
    }

    #[test]
    fn rules_entry_builds_live_entries_keyed_by_uuid() {
        let task = task_fixture();
        let mut rules = std::collections::BTreeMap::new();
        let rule = crate::reminders::ReminderRule {
            insistence: Insistence::Normal,
            id: ReminderRuleId::from(uuid::Uuid::from_u128(10)),
            task: task.id,
            kind: crate::reminders::ReminderKind::DueOffset(
                crate::reminders::ReminderOffset::at_due(),
            ),
            created_at: task.clocks.title,
            updated_at: task.clocks.effort,
        };
        let foreign = crate::reminders::ReminderRule {
            id: ReminderRuleId::from(uuid::Uuid::from_u128(11)),
            task: TaskId::from(uuid::Uuid::from_u128(99)),
            ..rule.clone()
        };
        rules.insert(rule.id, rule.clone());
        rules.insert(foreign.id, foreign.clone());
        let entry = rules_entry_for(&rules, task.id, None);
        assert_eq!(entry.rules.len(), 1, "only the task's own rules");
        assert_eq!(
            entry.rules[&rule.id.as_uuid().to_string()],
            RuleEntry::Rule {
                insistence: Insistence::Normal,
                kind: rule.kind,
                created: rule.created_at,
                clock: rule.updated_at,

                by: None,
            }
        );
    }

    #[test]
    fn a_due_offset_rule_serializes_the_legacy_shape_verbatim() {
        // Byte-shape pin: a DueOffset rule emits no kind key at all —
        // documents carrying only due-offset rules stay identical for
        // old-reading clients.
        let task = task_fixture();
        let rule = crate::reminders::ReminderRule {
            insistence: Insistence::Normal,
            id: crate::ids::ReminderRuleId::from(uuid::Uuid::from_u128(21)),
            task: task.id,
            kind: crate::reminders::ReminderKind::DueOffset(ReminderOffset::at_due()),
            created_at: ts(100),
            updated_at: ts(120),
        };
        let rule_id = rule.id;
        let entry = rules_entry_for(&BTreeMap::from([(rule_id, rule)]), task.id, None);
        let value = serde_json::to_value(&entry.rules[&rule_id.as_uuid().to_string()]).unwrap();
        let map = value.as_object().expect("a rule entry is a JSON map");
        assert!(map.contains_key("offset"), "the legacy offset key");
        assert!(
            !map.contains_key("at"),
            "no kind key rides the legacy shape"
        );
        assert!(
            !map.contains_key("kind"),
            "no kind key rides the legacy shape"
        );
    }

    #[test]
    fn an_at_rule_entry_roundtrips_and_an_unmodelable_at_demotes() {
        let task = task_fixture();
        let rule = crate::reminders::ReminderRule {
            insistence: Insistence::Alarm,
            id: crate::ids::ReminderRuleId::from(uuid::Uuid::from_u128(22)),
            task: task.id,
            kind: crate::reminders::ReminderKind::At(
                crate::reminders::AtInstant::whole_second(ts(1_700_000_000)).unwrap(),
            ),
            created_at: ts(100),
            updated_at: ts(120),
        };
        let rule_id = rule.id;
        let entry = rules_entry_for(&BTreeMap::from([(rule_id, rule.clone())]), task.id, None);
        // Serialize → parse → same rule (the wire round-trip).
        let value = serde_json::to_value(&entry).unwrap();
        let back: RulesEntry = serde_json::from_value(value).unwrap();
        assert_eq!(
            back.rules[&rule_id.as_uuid().to_string()],
            RuleEntry::Rule {
                kind: rule.kind,
                created: rule.created_at,
                clock: rule.updated_at,
                insistence: rule.insistence,

                by: None,
            },
            "the At entry round-trips kind-exact"
        );
        // A present-but-unmodelable `at` value demotes the whole entry
        // to Verbatim (never a silent downgrade).
        let raw = serde_json::json!({
            "rules": { "r": { "at": "not-a-timestamp", "created": "2026-01-01T00:00:00Z",
                              "clock": "2026-01-01T00:01:00Z", "insistence": "normal" } }
        });
        let parsed: RulesEntry = serde_json::from_value(raw).unwrap();
        assert!(
            matches!(parsed.rules["r"], RuleEntry::Verbatim(_)),
            "an unmodelable at demotes to Verbatim"
        );
    }

    #[test]
    fn tombstone_derivation_tombstones_exactly_the_locally_absent_live_rules() {
        let clock = ts(5_000);
        let kept = ReminderRuleId::from(uuid::Uuid::from_u128(16));
        let mut local = std::collections::BTreeMap::new();
        local.insert(
            kept,
            crate::reminders::ReminderRule {
                insistence: Insistence::Normal,
                id: kept,
                task: TaskId::from(uuid::Uuid::from_u128(1)),
                kind: crate::reminders::ReminderKind::DueOffset(
                    crate::reminders::ReminderOffset::at_due(),
                ),
                created_at: ts(1),
                updated_at: ts(2),
            },
        );
        let base = RulesEntry {
            rules: [
                (
                    "00000000-0000-0000-0000-00000000000a",
                    RuleEntry::Rule {
                        insistence: Insistence::Normal,
                        kind: crate::reminders::ReminderKind::DueOffset(
                            crate::reminders::ReminderOffset::at_due(),
                        ),
                        created: ts(1),
                        clock: ts(1),

                        by: None,
                    },
                ),
                // An existing tombstone passes through untouched.
                (
                    "00000000-0000-0000-0000-00000000000b",
                    RuleEntry::Tombstone {
                        clock: ts(3),
                        by: None,
                    },
                ),
                // Verbatim is never ordered against.
                (
                    "00000000-0000-0000-0000-00000000000c",
                    RuleEntry::Verbatim(serde_json::json!(1)),
                ),
            ]
            .into_iter()
            .map(|(k, e)| (k.to_owned(), e))
            .collect(),
            unknown: BTreeMap::new(),
        };
        let derived = derive_rule_tombstones(&base, &local, clock, None);
        assert_eq!(derived.len(), 1);
        assert_eq!(
            derived["00000000-0000-0000-0000-00000000000a"],
            RuleEntry::Tombstone { clock, by: None }
        );
    }

    #[test]
    fn lane_gc_drops_only_the_modeled_scheduled_keys() {
        let mut doc = sample_document();
        // An unmodeled key named "scheduled" inside `unmodeled` must survive.
        doc.sections
            .tasks
            .as_mut()
            .unwrap()
            .get_mut("55")
            .unwrap()
            .unmodeled
            .insert("scheduled".to_owned(), serde_json::json!("strange"));
        let gc = lane_gc_scheduled(&doc, true);
        for entry in gc.sections.tasks.as_ref().unwrap().values() {
            assert!(!entry.fields.contains_key("scheduled"));
        }
        assert_eq!(
            gc.sections.tasks.as_ref().unwrap()["55"].unmodeled["scheduled"],
            serde_json::json!("strange"),
            "unmodeled content is never touched"
        );
        // Everything else is identical.
        let mut without = doc.clone();
        without
            .sections
            .tasks
            .as_mut()
            .unwrap()
            .values_mut()
            .for_each(|e| {
                e.fields.remove("scheduled");
            });
        assert_eq!(gc, without);
        // drop == false is the identity.
        assert_eq!(lane_gc_scheduled(&doc, false), doc);
    }

    fn sample_document() -> AppDataDocument {
        let sections = AppDataSections {
            tasks: Some(
                [
                    (
                        "55".to_owned(),
                        entry(&[
                            (
                                "effort",
                                write(ModeledValue::Effort(Some(Effort::new(900))), 100),
                            ),
                            (
                                "scheduled",
                                write(ModeledValue::Scheduled(Some(ts(200))), 110),
                            ),
                        ]),
                    ),
                    (
                        "66".to_owned(),
                        entry(&[("urgency", write(ModeledValue::Urgency(None), 90))]),
                    ),
                ]
                .into(),
            ),
            reminder_rules: Some(
                [(
                    "55".to_owned(),
                    RulesEntry {
                        rules: [(
                            "018f0000-0000-7000-8000-000000000001".to_owned(),
                            RuleEntry::Rule {
                                insistence: Insistence::Normal,
                                kind: crate::reminders::ReminderKind::DueOffset(
                                    ReminderOffset::before(chrono::TimeDelta::seconds(-900))
                                        .unwrap(),
                                ),
                                created: ts(100),
                                clock: ts(120),

                                by: None,
                            },
                        )]
                        .into(),
                        unknown: BTreeMap::new(),
                    },
                )]
                .into(),
            ),
            ..Default::default()
        };
        let mut sections = sections;
        sections
            .unknown
            .insert("proofs".to_owned(), serde_json::json!({"hash": "abc"}));
        sections.dismissals = Some(
            [(
                "55".to_owned(),
                DismissalsEntry {
                    dismissals: [(
                        dismissal_wire_key(
                            crate::ids::ReminderRuleId::from(uuid::Uuid::from_u128(
                                0x018f_0000_0000_7000_8000_0000_0000_0001,
                            )),
                            ts(100),
                        ),
                        DismissalEntry::Dismissed {
                            dismissed_at: ts(120),

                            by: None,
                        },
                    )]
                    .into(),
                    unknown: BTreeMap::new(),
                },
            )]
            .into(),
        );
        document(2, sections)
    }

    // ---- chunk B: the dismissals section (design D-1/D-4/D-7/D-9) ----

    use crate::ids::AdhocAlarmId;
    use crate::reminders::{ReminderDismissal, ReminderRef};

    fn dismissal(reminder: ReminderRef, occurrence: i64, clock: i64) -> ReminderDismissal {
        ReminderDismissal {
            reminder,
            occurrence: ts(occurrence),
            dismissed_at: ts(clock),
        }
    }

    #[test]
    fn dismissal_rows_round_trip_and_unknown_shapes_stay_verbatim() {
        // The modeled row decodes typed; a future shape, a
        // `default:`-keyed row, and an unknown key beside `dismissals`
        // relay value-verbatim (the reserved-names discipline).
        let raw = serde_json::json!({
            "dismissals": {
                "018f0000-0000-7000-8000-000000000001|2026-04-01T08:00:00+00:00":
                    { "dismissed_at": "2026-04-01T09:00:00Z" },
                "future-shape-key": { "withdrawn": true },
                "default:00000000-0000-0000-0000-000000000009|2026-04-01T08:00:00+00:00":
                    { "dismissed_at": "2026-04-01T09:00:00Z" }
            },
            "peer-extension": 1
        });
        let entry: DismissalsEntry = serde_json::from_value(raw.clone()).unwrap();
        assert!(matches!(
            entry.dismissals["018f0000-0000-7000-8000-000000000001|2026-04-01T08:00:00+00:00"],
            DismissalEntry::Dismissed { .. }
        ));
        assert!(matches!(
            entry.dismissals["future-shape-key"],
            DismissalEntry::Verbatim(_)
        ));
        // The `default:`-keyed row rides the same entry envelope (its
        // value decodes fine); what keeps it off the wire-local state is
        // the fold — its key never parses as a rule identity.
        assert!(
            parse_dismissal_wire_key(
                "default:00000000-0000-0000-0000-000000000009|2026-04-01T08:00:00+00:00"
            )
            .is_none()
        );
        assert_eq!(entry.unknown["peer-extension"], 1);
        // Decode→encode is value-verbatim on everything unmodeled.
        let back = serde_json::to_value(&entry).unwrap();
        assert_eq!(
            back["dismissals"]["future-shape-key"],
            raw["dismissals"]["future-shape-key"]
        );
        assert_eq!(
            back["dismissals"]["default:00000000-0000-0000-0000-000000000009|2026-04-01T08:00:00+00:00"],
            raw["dismissals"]["default:00000000-0000-0000-0000-000000000009|2026-04-01T08:00:00+00:00"]
        );
        assert_eq!(back["peer-extension"], 1);
    }

    #[test]
    fn the_wire_key_is_injective_and_round_trips() {
        let rule = crate::ids::ReminderRuleId::from(uuid::Uuid::from_u128(0xabc));
        let occurrence = ts(1_234_567);
        let key = dismissal_wire_key(rule, occurrence);
        assert_eq!(key.matches('|').count(), 1);
        let (parsed_rule, parsed_occurrence) = parse_dismissal_wire_key(&key).unwrap();
        assert_eq!(parsed_rule, rule);
        assert_eq!(parsed_occurrence, occurrence);
        // The fold-ignore keys never parse.
        assert!(parse_dismissal_wire_key("default:abc|2026-04-01T08:00:00+00:00").is_none());
        assert!(parse_dismissal_wire_key("alarm:abc|2026-04-01T08:00:00+00:00").is_none());
        assert!(parse_dismissal_wire_key("garbage").is_none());
    }

    #[test]
    fn dismissal_emission_is_rule_ref_only_and_task_scoped() {
        let task = crate::ids::TaskId::from(uuid::Uuid::from_u128(1));
        let other = crate::ids::TaskId::from(uuid::Uuid::from_u128(2));
        let own_rule = crate::ids::ReminderRuleId::from(uuid::Uuid::from_u128(10));
        let foreign_rule = crate::ids::ReminderRuleId::from(uuid::Uuid::from_u128(11));
        let orphan_rule = crate::ids::ReminderRuleId::from(uuid::Uuid::from_u128(12));
        let mk_rule = |id, task| crate::reminders::ReminderRule {
            insistence: Insistence::Normal,
            id,
            task,
            kind: crate::reminders::ReminderKind::DueOffset(
                crate::reminders::ReminderOffset::at_due(),
            ),
            created_at: ts(1),
            updated_at: ts(2),
        };
        let mut rules = BTreeMap::new();
        rules.insert(own_rule, mk_rule(own_rule, task));
        rules.insert(foreign_rule, mk_rule(foreign_rule, other));
        let mut dismissals = std::collections::BTreeSet::new();
        dismissals.insert(dismissal(ReminderRef::Rule(own_rule), 100, 110));
        // Another task's rule: never this card's row.
        dismissals.insert(dismissal(ReminderRef::Rule(foreign_rule), 100, 111));
        // Orphan (rule deleted locally): filtered by the same scoping.
        dismissals.insert(dismissal(ReminderRef::Rule(orphan_rule), 100, 112));
        // The device-local arms: unrepresentable on the wire, ever.
        dismissals.insert(dismissal(ReminderRef::Default(task), 100, 113));
        dismissals.insert(dismissal(
            ReminderRef::Adhoc(AdhocAlarmId::from(uuid::Uuid::from_u128(20))),
            100,
            114,
        ));
        let entry = dismissals_entry_for(&dismissals, &rules, task, None);
        assert_eq!(entry.dismissals.len(), 1);
        assert!(
            entry
                .dismissals
                .contains_key(&dismissal_wire_key(own_rule, ts(100)))
        );
        assert_eq!(
            entry.dismissals[&dismissal_wire_key(own_rule, ts(100))],
            DismissalEntry::Dismissed {
                dismissed_at: ts(110),
                by: None,
            }
        );
        assert_eq!(entry.unknown, BTreeMap::new());
    }

    #[test]
    fn union_dismissals_picks_newer_clocks_ties_to_the_overlay_and_keeps_verbatim() {
        let key = dismissal_wire_key(
            crate::ids::ReminderRuleId::from(uuid::Uuid::from_u128(1)),
            ts(100),
        );
        let mk = |clock: i64| DismissalsEntry {
            dismissals: [(
                key.clone(),
                DismissalEntry::Dismissed {
                    dismissed_at: ts(clock),

                    by: None,
                },
            )]
            .into(),
            unknown: BTreeMap::new(),
        };
        let mut base = mk(100);
        base.dismissals.insert(
            "verbatim-key".to_owned(),
            DismissalEntry::Verbatim(serde_json::json!({"future": true})),
        );
        let mut overlay = mk(50);
        overlay.dismissals.insert(
            "fresh-key".to_owned(),
            DismissalEntry::Dismissed {
                dismissed_at: ts(1),

                by: None,
            },
        );
        union_dismissals_entry(&mut base, &overlay);
        assert_eq!(
            base.dismissals[&key].clock(),
            Some(ts(100)),
            "the newer embedded clock wins"
        );
        assert_eq!(base.dismissals.len(), 3, "verbatim + fresh are additive");
        assert!(matches!(
            base.dismissals["verbatim-key"],
            DismissalEntry::Verbatim(_)
        ));
        // An equal clock resolves to the overlay — and a Verbatim slot is
        // never displaced by a clocked entry.
        let mut base = mk(100);
        base.dismissals.insert(
            "v".to_owned(),
            DismissalEntry::Verbatim(serde_json::json!(7)),
        );
        let mut overlay = mk(100);
        overlay.dismissals.insert(
            "v".to_owned(),
            DismissalEntry::Dismissed {
                dismissed_at: ts(1),

                by: None,
            },
        );
        union_dismissals_entry(&mut base, &overlay);
        assert!(matches!(base.dismissals["v"], DismissalEntry::Verbatim(_)));
    }

    #[test]
    fn prune_removes_exactly_the_absent_cards_dismissal_entries() {
        let mut doc = sample_document();
        doc.sections
            .dismissals
            .as_mut()
            .unwrap()
            .insert("77".to_owned(), DismissalsEntry::default());
        let present: std::collections::BTreeSet<String> =
            ["55"].into_iter().map(str::to_owned).collect();
        let pruned = prune_absent_cards(&doc, &present);
        let dismissals = pruned.sections.dismissals.as_ref().unwrap();
        assert_eq!(dismissals.len(), 1);
        assert!(dismissals.contains_key("55"));
        let empty = prune_absent_cards(&doc, &std::collections::BTreeSet::new());
        assert!(empty.sections.dismissals.is_none());
    }

    // ---- DR-5/2: the board-level time-entries section (D-T10/D-T11) --

    fn entry_row(task: Option<&str>, clock: i64) -> TimeEntryRecord {
        TimeEntryRecord::Entry {
            task: task.map(str::to_owned),
            title: "write chapter 3".to_owned(),
            start: ts(100),
            end: Some(ts(200)),
            created: ts(90),
            clock: ts(clock),
            kind: crate::tracking::TimeEntryKind::Work,

            by: None,
        }
    }

    fn open_row(task: Option<&str>, clock: i64) -> TimeEntryRecord {
        TimeEntryRecord::Entry {
            task: task.map(str::to_owned),
            title: "running".to_owned(),
            start: ts(100),
            end: None,
            created: ts(90),
            clock: ts(clock),
            kind: crate::tracking::TimeEntryKind::Work,

            by: None,
        }
    }

    // ---- The kind fold on the wire row (D-2) --------------------------

    #[test]
    fn old_wire_row_without_kind_decodes_entry_with_work() {
        let raw = serde_json::json!({
            "task": "412", "title": "old row", "start": ts(100),
            "end": ts(200), "created": ts(90), "clock": ts(100),
        });
        let decoded = TimeEntryRecord::deserialize_verbatim(raw);
        assert_eq!(
            decoded,
            TimeEntryRecord::Entry {
                task: Some("412".to_owned()),
                title: "old row".to_owned(),
                start: ts(100),
                end: Some(ts(200)),
                created: ts(90),
                clock: ts(100),
                kind: crate::tracking::TimeEntryKind::Work,

                by: None,
            },
            "absent kind folds default-off"
        );
        // A null kind (a hostile writer) decodes the same way.
        let null_kind = serde_json::json!({
            "task": "412", "title": "old row", "start": ts(100),
            "end": ts(200), "created": ts(90), "clock": ts(100),
            "kind": Value::Null,
        });
        assert_eq!(TimeEntryRecord::deserialize_verbatim(null_kind), decoded);
    }

    #[test]
    fn respite_wire_row_roundtrips() {
        let mut row = entry_row(Some("412"), 100);
        if let TimeEntryRecord::Entry { kind, .. } = &mut row {
            *kind = crate::tracking::TimeEntryKind::Respite;
        }
        let json = serde_json::to_value(&row).unwrap();
        assert_eq!(json["kind"], serde_json::json!("respite"));
        let back = TimeEntryRecord::deserialize_verbatim(json);
        assert_eq!(back, row);
        assert_eq!(
            serde_json::to_value(entry_row(Some("412"), 100)).unwrap()["kind"],
            serde_json::json!("work")
        );
    }

    #[test]
    fn unmodeled_kind_string_demotes_the_whole_row_to_verbatim() {
        let raw = serde_json::json!({
            "task": "412", "title": "future", "start": ts(100),
            "end": ts(200), "created": ts(90), "clock": ts(100),
            "kind": "sabbatical",
        });
        assert!(matches!(
            TimeEntryRecord::deserialize_verbatim(raw),
            TimeEntryRecord::Verbatim(_)
        ));
    }

    #[test]
    fn overlay_emits_the_kind_key() {
        use crate::ids::{RemoteBoardId, TimeEntryId};
        use crate::tracking::{TimeEntry, TimeEntryKind};
        let task = bound_task(false);
        let tasks = std::collections::BTreeMap::from([(task.id, task.clone())]);
        let mut entry = TimeEntry {
            id: TimeEntryId::from(uuid::Uuid::from_u128(9)),
            task: task.id,
            task_title: "t".to_owned(),
            start: ts(100),
            end: None,
            created_at: ts(90),
            updated_at: ts(110),
            kind: TimeEntryKind::Work,
        };
        let entries = std::collections::BTreeMap::from([(entry.id, entry.clone())]);
        let emitted = time_entries_for(
            &entries,
            &tasks,
            RemoteBoardId::deck(2),
            ts(1_000_000),
            None,
        );
        assert_eq!(
            serde_json::to_value(emitted.values().next().unwrap()).unwrap()["kind"],
            serde_json::json!("work")
        );
        entry.kind = TimeEntryKind::Respite;
        let entries = std::collections::BTreeMap::from([(entry.id, entry)]);
        let emitted = time_entries_for(
            &entries,
            &tasks,
            RemoteBoardId::deck(2),
            ts(1_000_000),
            None,
        );
        assert_eq!(
            serde_json::to_value(emitted.values().next().unwrap()).unwrap()["kind"],
            serde_json::json!("respite")
        );
    }

    #[test]
    fn time_entry_rows_round_trip_and_unknown_shapes_stay_verbatim() {
        let raw = serde_json::json!({
            "018f0000-0000-7000-8000-000000000001":
                { "task": "412", "title": "Write chapter 3",
                  "start": "2026-04-01T08:00:00Z", "end": "2026-04-01T09:00:00Z",
                  "created": "2026-04-01T07:59:00Z", "clock": "2026-04-01T09:00:00Z" },
            "018f0000-0000-7000-8000-000000000002":
                { "task": null, "title": "local-only",
                  "start": "2026-04-01T08:00:00Z", "end": null,
                  "created": "2026-04-01T07:59:00Z", "clock": "2026-04-01T09:00:00Z" },
            "018f0000-0000-7000-8000-000000000003":
                { "deleted": true, "clock": "2026-04-01T10:00:00Z" },
            // The demotions — each lands Verbatim with value equality.
            "no-title": { "task": "412", "start": "2026-04-01T08:00:00Z",
                          "created": "2026-04-01T07:59:00Z", "clock": "2026-04-01T09:00:00Z" },
            "bad-clock": { "task": "412", "title": "t", "start": "2026-04-01T08:00:00Z",
                           "created": "2026-04-01T07:59:00Z", "clock": "not-a-time" },
            "bad-end": { "task": "412", "title": "t", "start": "2026-04-01T08:00:00Z",
                         "end": "yesterday-ish", "created": "2026-04-01T07:59:00Z",
                         "clock": "2026-04-01T09:00:00Z" },
            "non-object-row": 7,
            "deleted-bad-clock": { "deleted": true, "clock": "not-a-time" },
            "task-not-a-string": { "task": 412, "title": "t", "start": "2026-04-01T08:00:00Z",
                                   "created": "2026-04-01T07:59:00Z",
                                   "clock": "2026-04-01T09:00:00Z" }
        });
        let section: BTreeMap<String, TimeEntryRecord> =
            serde_json::from_value(raw.clone()).unwrap();
        assert_eq!(
            section["018f0000-0000-7000-8000-000000000001"],
            TimeEntryRecord::Entry {
                task: Some("412".to_owned()),
                title: "Write chapter 3".to_owned(),
                start: "2026-04-01T08:00:00Z".parse().unwrap(),
                end: Some("2026-04-01T09:00:00Z".parse().unwrap()),
                created: "2026-04-01T07:59:00Z".parse().unwrap(),
                clock: "2026-04-01T09:00:00Z".parse().unwrap(),
                kind: crate::tracking::TimeEntryKind::Work,
                by: None,
            },
            "the full entry decodes typed"
        );
        assert!(
            matches!(
                section["018f0000-0000-7000-8000-000000000002"],
                TimeEntryRecord::Entry {
                    task: None,
                    end: None,
                    ..
                }
            ),
            "the null-task open row decodes tolerantly"
        );
        assert!(matches!(
            section["018f0000-0000-7000-8000-000000000003"],
            TimeEntryRecord::Tombstone { .. }
        ));
        for key in [
            "no-title",
            "bad-clock",
            "bad-end",
            "non-object-row",
            "deleted-bad-clock",
            "task-not-a-string",
        ] {
            assert!(
                matches!(section[key], TimeEntryRecord::Verbatim(_)),
                "{key}"
            );
            // Decode→encode is value-verbatim on everything unmodeled.
            assert_eq!(
                serde_json::to_value(&section[key]).unwrap(),
                raw[key],
                "{key} relays verbatim"
            );
        }
    }

    #[test]
    fn the_time_entries_section_serializes_exactly_the_modeled_keys() {
        let entry = serde_json::to_value(entry_row(Some("412"), 100)).unwrap();
        let mut keys: Vec<_> = entry.as_object().unwrap().keys().cloned().collect();
        keys.sort();
        // `kind` is the additive-tail contract extension (D-2).
        assert_eq!(
            keys,
            vec!["clock", "created", "end", "kind", "start", "task", "title"]
        );
        let open = serde_json::to_value(open_row(None, 100)).unwrap();
        assert_eq!(open["end"], Value::Null);
        assert_eq!(open["task"], Value::Null);
        let tombstone = serde_json::to_value(TimeEntryRecord::Tombstone {
            clock: ts(100),
            by: None,
        })
        .unwrap();
        let mut keys: Vec<_> = tombstone.as_object().unwrap().keys().cloned().collect();
        keys.sort();
        assert_eq!(keys, vec!["clock", "deleted"]);
        assert_eq!(tombstone["deleted"], true);
    }

    #[test]
    fn the_time_entries_section_rides_the_document_envelope() {
        let doc = document(
            2,
            AppDataSections {
                time_entries: Some(
                    [("k".to_owned(), entry_row(Some("412"), 100))]
                        .into_iter()
                        .collect(),
                ),
                ..Default::default()
            },
        );
        let json = serde_json::to_value(&doc).unwrap();
        assert!(json["sections"]["time-entries"]["k"].is_object());
        let back: AppDataDocument = serde_json::from_value(json).unwrap();
        assert_eq!(back, doc);
        // A non-object body for the known key fails the document exactly
        // like every existing known section (the strict boundary, D-5).
        let garbage: Result<AppDataDocument, _> = serde_json::from_value(serde_json::json!({
            "format": 1, "board": 2,
            "sections": { "time-entries": "not-a-map" }
        }));
        assert!(garbage.is_err());
    }

    #[test]
    fn union_time_entries_picks_newer_clocks_ties_to_the_overlay_and_keeps_verbatim() {
        let mut base = BTreeMap::new();
        base.insert("newer-base".to_owned(), entry_row(Some("412"), 200));
        base.insert("tie".to_owned(), entry_row(Some("412"), 100));
        base.insert(
            "verbatim".to_owned(),
            TimeEntryRecord::Verbatim(serde_json::json!({ "future": true })),
        );
        base.insert("base-only".to_owned(), entry_row(Some("413"), 50));
        let mut overlay = BTreeMap::new();
        overlay.insert("newer-base".to_owned(), entry_row(Some("412"), 100));
        overlay.insert("tie".to_owned(), open_row(Some("412"), 100));
        overlay.insert(
            "verbatim".to_owned(),
            TimeEntryRecord::Tombstone {
                clock: ts(999),
                by: None,
            },
        );
        overlay.insert("fresh".to_owned(), entry_row(Some("414"), 300));
        union_time_entries(&mut base, &overlay);
        assert_eq!(
            base["newer-base"].clock(),
            Some(ts(200)),
            "the newer embedded clock wins"
        );
        assert_eq!(
            base["tie"],
            open_row(Some("412"), 100),
            "an equal clock resolves to the overlay (D-7, the deliberate divergence)"
        );
        assert!(
            matches!(base["verbatim"], TimeEntryRecord::Verbatim(_)),
            "a verbatim slot is never displaced by a clocked row"
        );
        assert!(base.contains_key("base-only"), "base-only rows survive");
        assert_eq!(base["fresh"], entry_row(Some("414"), 300));
        // All four (Some/None) section arms ride the document union.
        let doc = |entries: Option<BTreeMap<String, TimeEntryRecord>>| {
            document(
                2,
                AppDataSections {
                    time_entries: entries,
                    ..Default::default()
                },
            )
        };
        let some: BTreeMap<String, TimeEntryRecord> =
            [("k".to_owned(), entry_row(Some("1"), 1))].into();
        assert!(
            union(&doc(None), &doc(Some(some.clone())))
                .sections
                .time_entries
                .is_some()
        );
        assert!(
            union(&doc(Some(some.clone())), &doc(None))
                .sections
                .time_entries
                .is_some()
        );
        assert!(
            union(&doc(None), &doc(None))
                .sections
                .time_entries
                .is_none()
        );
        assert_eq!(
            union(&doc(Some(some.clone())), &doc(Some(some)))
                .sections
                .time_entries,
            Some([("k".to_owned(), entry_row(Some("1"), 1))].into())
        );
    }

    #[test]
    fn union_time_entry_tombstones_compete_by_the_same_clock() {
        let mut base = BTreeMap::new();
        base.insert("e".to_owned(), entry_row(Some("412"), 100));
        let mut overlay = BTreeMap::new();
        overlay.insert(
            "e".to_owned(),
            TimeEntryRecord::Tombstone {
                clock: ts(200),
                by: None,
            },
        );
        union_time_entries(&mut base, &overlay);
        assert!(
            matches!(base["e"], TimeEntryRecord::Tombstone { .. }),
            "a newer tombstone beats the live row"
        );
        let mut base = BTreeMap::new();
        base.insert(
            "e".to_owned(),
            TimeEntryRecord::Tombstone {
                clock: ts(100),
                by: None,
            },
        );
        let mut overlay = BTreeMap::new();
        overlay.insert("e".to_owned(), entry_row(Some("412"), 300));
        union_time_entries(&mut base, &overlay);
        assert_eq!(
            base["e"],
            entry_row(Some("412"), 300),
            "a later-dated live row beats the older tombstone"
        );
    }

    // ---- DR-5/2: the overlay builder + the truncated-view guard -----

    use crate::ids::{RemoteCardId, RemoteCardRef, RemoteStackId, TimeEntryId};

    /// A bound copy of `task_fixture` on board 2 (the `document()` board),
    /// card 412, optionally soft-deleted.
    fn bound_task(deleted: bool) -> Task {
        let mut task = task_fixture();
        task.remote = Some(RemoteCardRef {
            board: RemoteBoardId::deck(2),
            stack: RemoteStackId::deck(7),
            card: RemoteCardId::deck(412),
        });
        task.deleted = deleted;
        task
    }

    fn tracked(seed: u128, start: i64, open: bool, task: TaskId) -> crate::tracking::TimeEntry {
        crate::tracking::TimeEntry {
            id: TimeEntryId::from(uuid::Uuid::from_u128(seed)),
            task,
            task_title: "task".to_owned(),
            start: ts(start),
            end: open.then_some(ts(start + 1_000)),
            created_at: ts(start - 10),
            updated_at: ts(start + 10),
            kind: crate::tracking::TimeEntryKind::Work,
        }
    }

    fn base_entry(task: &str, start: i64) -> TimeEntryRecord {
        TimeEntryRecord::Entry {
            task: Some(task.to_owned()),
            title: "remote title".to_owned(),
            start: ts(start),
            end: None,
            created: ts(start - 10),
            clock: ts(start + 10),
            kind: crate::tracking::TimeEntryKind::Work,

            by: None,
        }
    }

    #[test]
    fn time_entries_emission_is_card_bound_only_and_window_filtered() {
        let now = ts(10_000_000);
        let mut bound = bound_task(false);
        bound.id = TaskId::from(uuid::Uuid::from_u128(101));
        let deleted_but_bound = {
            let mut task = task_fixture();
            task.id = TaskId::from(uuid::Uuid::from_u128(102));
            task.remote = Some(RemoteCardRef {
                board: RemoteBoardId::deck(2),
                stack: RemoteStackId::deck(7),
                card: RemoteCardId::deck(413),
            });
            task.deleted = true;
            task
        };
        let mut unbound = task_fixture();
        unbound.id = TaskId::from(uuid::Uuid::from_u128(103));
        let foreign = {
            let mut task = bound_task(false);
            task.id = TaskId::from(uuid::Uuid::from_u128(104));
            task.remote = Some(RemoteCardRef {
                board: RemoteBoardId::deck(9),
                stack: RemoteStackId::deck(7),
                card: RemoteCardId::deck(414),
            });
            task
        };
        let mut tasks = BTreeMap::new();
        tasks.insert(bound.id, bound.clone());
        tasks.insert(deleted_but_bound.id, deleted_but_bound.clone());
        tasks.insert(unbound.id, unbound.clone());
        tasks.insert(foreign.id, foreign.clone());

        let fresh = tracked(1, 9_000_000, true, bound.id);
        let boundary_start = now.timestamp()
            - chrono::TimeDelta::try_days(TRACKING_SYNC_RETENTION_DAYS)
                .unwrap()
                .num_seconds();
        let on_boundary = tracked(2, boundary_start, false, bound.id);
        let aged_secs = now.timestamp()
            - chrono::TimeDelta::try_days(TRACKING_SYNC_RETENTION_DAYS)
                .unwrap()
                .num_seconds()
            - 100;
        let aged = tracked(3, aged_secs, false, bound.id);
        let deleted_task_entry = tracked(4, 9_000_000, false, deleted_but_bound.id);
        let unbound_entry = tracked(5, 9_000_000, false, unbound.id);
        let foreign_entry = tracked(6, 9_000_000, false, foreign.id);
        let dangling = tracked(
            7,
            9_000_000,
            false,
            TaskId::from(uuid::Uuid::from_u128(999)),
        );
        let mut entries = BTreeMap::new();
        for e in [
            &fresh,
            &on_boundary,
            &aged,
            &deleted_task_entry,
            &unbound_entry,
            &foreign_entry,
            &dangling,
        ] {
            entries.insert(e.id, e.clone());
        }
        let emitted = time_entries_for(&entries, &tasks, RemoteBoardId::deck(2), now, None);
        assert_eq!(emitted.len(), 3, "fresh + boundary + deleted-task-bound");
        assert!(emitted.contains_key(&fresh.id.as_uuid().to_string()));
        assert!(
            emitted.contains_key(&on_boundary.id.as_uuid().to_string()),
            "the boundary instant itself is inside (inclusive)"
        );
        assert!(
            emitted.contains_key(&deleted_task_entry.id.as_uuid().to_string()),
            "soft-deleted-but-bound keeps flowing (post-deletion ops roam)"
        );
        assert!(!emitted.contains_key(&aged.id.as_uuid().to_string()));
        assert!(!emitted.contains_key(&unbound_entry.id.as_uuid().to_string()));
        assert!(!emitted.contains_key(&foreign_entry.id.as_uuid().to_string()));
        assert!(!emitted.contains_key(&dangling.id.as_uuid().to_string()));
        // The emitted shape: decimal card id, title snapshot, clock = updated_at.
        assert_eq!(
            emitted[&fresh.id.as_uuid().to_string()],
            TimeEntryRecord::Entry {
                task: Some("412".to_owned()),
                title: "task".to_owned(),
                start: fresh.start,
                end: fresh.end,
                created: fresh.created_at,
                clock: fresh.updated_at,
                kind: crate::tracking::TimeEntryKind::Work,
                by: None,
            }
        );
    }

    #[test]
    fn tombstone_derivation_tombstones_exactly_the_locally_absent_live_entries() {
        let now = ts(10_000_000);
        let clock = ts(10_000_001);
        let aged_secs = now.timestamp()
            - chrono::TimeDelta::try_days(TRACKING_SYNC_RETENTION_DAYS)
                .unwrap()
                .num_seconds()
            - 100;
        let resolvable: std::collections::BTreeSet<String> =
            ["412"].into_iter().map(str::to_owned).collect();
        let local_id = TimeEntryId::from(uuid::Uuid::from_u128(1));
        let mut local = BTreeMap::new();
        local.insert(
            local_id,
            tracked(1, 9_000_000, true, TaskId::from(uuid::Uuid::from_u128(1))),
        );
        let base = BTreeMap::from([
            // Locally absent, resolvable, in-window: tombstoned.
            (
                "00000000-0000-0000-0000-000000000002".to_owned(),
                base_entry("412", 9_000_000),
            ),
            // Locally present: never tombstoned.
            (local_id.as_uuid().to_string(), base_entry("412", 9_000_001)),
            // An existing tombstone passes through untouched.
            (
                "00000000-0000-0000-0000-000000000003".to_owned(),
                TimeEntryRecord::Tombstone {
                    clock: ts(500),
                    by: None,
                },
            ),
            // Verbatim is never ordered against.
            (
                "00000000-0000-0000-0000-000000000004".to_owned(),
                TimeEntryRecord::Verbatim(serde_json::json!(1)),
            ),
            // Out-of-window base row: stripped by GC, not tombstoned.
            (
                "00000000-0000-0000-0000-000000000005".to_owned(),
                base_entry("412", aged_secs),
            ),
            // Unresolvable card: ages out instead.
            (
                "00000000-0000-0000-0000-000000000006".to_owned(),
                base_entry("999", 9_000_002),
            ),
            // Unparseable key: not an entry identity.
            ("not-a-uuid".to_owned(), base_entry("412", 9_000_003)),
        ]);
        let derived = derive_time_entry_tombstones(&base, &local, &resolvable, now, clock, None);
        assert_eq!(derived.len(), 1);
        assert_eq!(
            derived["00000000-0000-0000-0000-000000000002"],
            TimeEntryRecord::Tombstone { clock, by: None }
        );
    }

    #[test]
    fn lane_gc_drops_only_aged_time_entries() {
        let now = ts(10_000_000);
        let aged_secs = now.timestamp()
            - chrono::TimeDelta::try_days(TRACKING_SYNC_RETENTION_DAYS)
                .unwrap()
                .num_seconds()
            - 1;
        let doc = document(
            2,
            AppDataSections {
                time_entries: Some(
                    [
                        ("fresh-live".to_owned(), entry_row(Some("412"), 9_000_000)),
                        ("aged-live".to_owned(), base_entry("412", aged_secs)),
                        (
                            "fresh-tombstone".to_owned(),
                            TimeEntryRecord::Tombstone {
                                clock: ts(9_000_001),

                                by: None,
                            },
                        ),
                        (
                            "aged-tombstone".to_owned(),
                            TimeEntryRecord::Tombstone {
                                clock: ts(aged_secs),

                                by: None,
                            },
                        ),
                        (
                            "verbatim".to_owned(),
                            TimeEntryRecord::Verbatim(serde_json::json!({ "future": 1 })),
                        ),
                    ]
                    .into_iter()
                    .collect(),
                ),
                ..Default::default()
            },
        );
        let gc = lane_gc_old_time_entries(&doc, now);
        let kept = gc.sections.time_entries.as_ref().unwrap();
        assert_eq!(kept.len(), 3, "fresh live + fresh tombstone + verbatim");
        assert!(kept.contains_key("fresh-live"));
        assert!(kept.contains_key("fresh-tombstone"));
        assert!(kept.contains_key("verbatim"));
        // The boundary instant itself survives (inclusive).
        let boundary = lane_gc_old_time_entries(
            &document(
                2,
                AppDataSections {
                    time_entries: Some(
                        [(
                            "b".to_owned(),
                            base_entry(
                                "412",
                                now.timestamp()
                                    - chrono::TimeDelta::try_days(TRACKING_SYNC_RETENTION_DAYS)
                                        .unwrap()
                                        .num_seconds(),
                            ),
                        )]
                        .into_iter()
                        .collect(),
                    ),
                    ..Default::default()
                },
            ),
            now,
        );
        assert_eq!(boundary.sections.time_entries.as_ref().unwrap().len(), 1);
        // An emptied section becomes None; other sections are untouched.
        let mut empties = document(
            2,
            AppDataSections {
                time_entries: Some(
                    [("aged".to_owned(), base_entry("412", aged_secs))]
                        .into_iter()
                        .collect(),
                ),
                ..Default::default()
            },
        );
        empties.sections.tasks = Some([("55".to_owned(), entry(&[]))].into_iter().collect());
        let gc = lane_gc_old_time_entries(&empties, now);
        assert!(gc.sections.time_entries.is_none());
        assert!(gc.sections.tasks.is_some(), "other sections untouched");
    }

    #[test]
    fn the_overlay_never_re_emits_what_the_gc_strips() {
        let now = ts(10_000_000);
        let aged_secs = now.timestamp()
            - chrono::TimeDelta::try_days(TRACKING_SYNC_RETENTION_DAYS)
                .unwrap()
                .num_seconds()
            - 1;
        let bound = bound_task(false);
        let mut tasks = BTreeMap::new();
        tasks.insert(bound.id, bound.clone());
        let aged = tracked(3, aged_secs, false, bound.id);
        let fresh = tracked(4, 9_000_000, false, bound.id);
        let mut entries = BTreeMap::new();
        entries.insert(aged.id, aged.clone());
        entries.insert(fresh.id, fresh.clone());
        // Builder and GC over the same now agree: no aged row in either.
        let overlay = time_entries_for(&entries, &tasks, RemoteBoardId::deck(2), now, None);
        assert_eq!(overlay.len(), 1);
        assert!(overlay.contains_key(&fresh.id.as_uuid().to_string()));
        let base_doc = document(
            2,
            AppDataSections {
                time_entries: Some(
                    [(aged.id.as_uuid().to_string(), base_entry("412", aged_secs))]
                        .into_iter()
                        .collect(),
                ),
                ..Default::default()
            },
        );
        let gc = lane_gc_old_time_entries(&base_doc, now);
        assert!(
            gc.sections.time_entries.is_none(),
            "the GC strips exactly what the builder refuses to emit"
        );
        // A stale device's aged base row unions with the fresh overlay…
        let stale = document(
            2,
            AppDataSections {
                time_entries: Some(
                    [(aged.id.as_uuid().to_string(), base_entry("412", aged_secs))]
                        .into_iter()
                        .collect(),
                ),
                ..Default::default()
            },
        );
        let overlay_doc = document(
            2,
            AppDataSections {
                time_entries: Some(overlay),
                ..Default::default()
            },
        );
        let merged = union(&stale, &overlay_doc);
        let gc = lane_gc_old_time_entries(&merged, now);
        assert!(
            !gc.sections
                .time_entries
                .as_ref()
                .unwrap()
                .contains_key(&aged.id.as_uuid().to_string()),
            "…and the union cannot resurrect it: the next flush re-ages it"
        );
    }

    // ---- #588: the dismissals-wire widening ---------------------------

    #[test]
    fn wire_key_grammar_pins() {
        let rule = crate::ids::ReminderRuleId::from(uuid::Uuid::from_u128(1));
        let break_rule = crate::ids::BreakRuleId::from(uuid::Uuid::from_u128(2));
        let occurrence = ts(1_000);
        // The bare grammar rejects the break form (the old-peer
        // fail-closed mechanism, pinned).
        let break_key = break_dismissal_wire_key(break_rule, occurrence);
        assert!(
            parse_dismissal_wire_key(&break_key).is_none(),
            "an old peer's strict bare-UUID parse fails on the break: prefix"
        );
        // The break grammar accepts exactly its own emitted keys.
        assert_eq!(
            parse_break_dismissal_wire_key(&break_key),
            Some((break_rule, occurrence)),
            "round-trip"
        );
        for bad in [
            format!("{}|{}", rule.as_uuid(), occurrence.to_rfc3339()),
            format!(
                "default:{}|{}",
                uuid::Uuid::from_u128(3),
                occurrence.to_rfc3339()
            ),
            format!(
                "alarm:{}|{}",
                uuid::Uuid::from_u128(4),
                occurrence.to_rfc3339()
            ),
            "break:garbage|2026-04-01T08:00:00+00:00".to_owned(),
            format!("break:{}", break_rule.as_uuid()),
            "future-shape".to_owned(),
        ] {
            assert!(
                parse_break_dismissal_wire_key(&bad).is_none(),
                "the break grammar rejects {bad:?}"
            );
        }
    }

    // P8 (D-10, the reserved-entry emission filter): Break-ref rows
    // only, orphan/foreign-board rules filtered, other families never
    // leave their lanes.
    #[test]
    fn break_dismissals_entry_for_filters_orphans_and_foreign_boards() {
        let board = crate::ids::BoardId::from(uuid::Uuid::from_u128(700));
        let on_board = break_rule_row(1, 700);
        let foreign = break_rule_row(2, 701);
        let mut break_rules = BTreeMap::new();
        break_rules.insert(on_board.id, on_board);
        break_rules.insert(foreign.id, foreign.clone());
        let orphan = crate::ids::BreakRuleId::from(uuid::Uuid::from_u128(3));
        let task = crate::ids::TaskId::from(uuid::Uuid::from_u128(4));
        let alarm = crate::ids::AdhocAlarmId::from(uuid::Uuid::from_u128(5));
        let mut dismissals = std::collections::BTreeSet::new();
        for (reminder, occ) in [
            (
                crate::reminders::ReminderRef::Break(crate::ids::BreakRuleId::from(
                    uuid::Uuid::from_u128(1),
                )),
                100_i64,
            ),
            (crate::reminders::ReminderRef::Break(foreign.id), 101),
            (crate::reminders::ReminderRef::Break(orphan), 102),
            (crate::reminders::ReminderRef::Default(task), 103),
            (crate::reminders::ReminderRef::Adhoc(alarm), 104),
        ] {
            dismissals.insert(crate::reminders::ReminderDismissal {
                reminder,
                occurrence: ts(occ),
                dismissed_at: ts(occ + 1),
            });
        }
        let entry = break_dismissals_entry_for(&dismissals, &break_rules, board, None);
        assert_eq!(
            entry.dismissals.len(),
            1,
            "only the on-board rule's row emits"
        );
        assert_eq!(entry.unknown, BTreeMap::new());
        let wire = serde_json::to_string(&entry).unwrap();
        assert!(
            !wire.contains(&foreign.id.as_uuid().to_string()),
            "foreign-board rows never emit: {wire}"
        );
        assert!(
            !wire.contains(&orphan.as_uuid().to_string()),
            "orphan rows never emit: {wire}"
        );
        assert!(!wire.contains(&task.as_uuid().to_string()), "{wire}");
        assert!(!wire.contains(&alarm.as_uuid().to_string()), "{wire}");
        assert!(!wire.contains("default:"), "{wire}");
        assert!(!wire.contains("alarm:"), "{wire}");
    }

    // P9 (D-12, the prune): the reserved entry survives a prune whose
    // present-card set excludes it; the break-rules section has no
    // prune arm at all.
    #[test]
    fn reserved_break_entry_survives_the_prune() {
        let sections = AppDataSections {
            dismissals: Some(
                [
                    ("55".to_owned(), dismissals_card_entry()),
                    (
                        BREAK_DISMISSALS_ENTRY_KEY.to_owned(),
                        serde_json::from_value(serde_json::json!({
                            "dismissals": {
                                "break:018f0000-0000-7000-8000-000000000001|2026-04-01T08:00:00+00:00":
                                    { "dismissed_at": "2026-04-01T08:00:01+00:00" }
                            }
                        }))
                        .unwrap(),
                    ),
                ]
                .into_iter()
                .collect(),
            ),
            break_rules: Some(
                [(
                    "018f0000-0000-7000-8000-000000000002".to_owned(),
                    serde_json::from_value::<BreakRuleEntry>(serde_json::json!({
                        "label": "stretch",
                        "threshold_seconds": 60,
                        "created": "2026-04-01T08:00:00+00:00",
                        "clock": "2026-04-01T08:00:00+00:00",
                        "insistence": "normal"
                    }))
                    .unwrap(),
                )]
                .into_iter()
                .collect(),
            ),
            ..Default::default()
        };
        let doc = document(2, sections);
        let pruned = prune_absent_cards(&doc, &["99".to_owned()].into_iter().collect());
        let dismissals = pruned.sections.dismissals.as_ref().unwrap();
        assert!(
            !dismissals.contains_key("55"),
            "the absent card's entry is stripped"
        );
        assert!(
            dismissals.contains_key(BREAK_DISMISSALS_ENTRY_KEY),
            "the reserved entry survives regardless of the present-card set"
        );
        assert!(
            pruned.sections.break_rules.is_some(),
            "the board-level break-rules section has no prune arm"
        );
    }

    fn dismissals_card_entry() -> DismissalsEntry {
        serde_json::from_value(serde_json::json!({
            "dismissals": {
                "018f0000-0000-7000-8000-000000000009|2026-04-01T08:00:00+00:00":
                    { "dismissed_at": "2026-04-01T08:00:01+00:00" }
            }
        }))
        .unwrap()
    }

    // ---- proptest: verbatim preservation is the load-bearing property --

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(128))]

        #[test]
        fn documents_round_trip_through_serde(
            board in 0_u64..1_000,
            effort in proptest::option::of(0_u64..1_000_000),
            urgency in proptest::option::of("[a-z_]+"),
            scheduled in proptest::option::of(0_i64..4_000_000_000),
        ) {
            let mut sections = AppDataSections::default();
            let mut fields = BTreeMap::new();
            fields.insert(
                "effort".to_owned(),
                write(ModeledValue::Effort(effort.map(Effort::new)), 100),
            );
            if let Some(Ok(level)) =
                urgency.map(|name| serde_json::from_value::<Urgency>(Value::String(name)))
            {
                fields.insert(
                    "urgency".to_owned(),
                    write(ModeledValue::Urgency(Some(level)), 110),
                );
            }
            fields.insert(
                "scheduled".to_owned(),
                write(ModeledValue::Scheduled(scheduled.map(ts)), 120),
            );
            sections.tasks = Some([("55".to_owned(), TaskEntry { fields, unmodeled: BTreeMap::new() })].into());
            let doc = document(board, sections);
            let json = serde_json::to_string(&doc).unwrap();
            let back: AppDataDocument = serde_json::from_str(&json).unwrap();
            prop_assert_eq!(back, doc);
        }

        #[test]
        fn unknown_content_is_value_verbatim_across_decode_encode(
            envelope_key in "[a-z-]{1,12}",
            section_key in "[a-z-]{1,12}",
            entry_key in "[a-z-]{1,12}",
            payload in "[a-zA-Z0-9 ]{0,40}",
        ) {
            let raw = serde_json::json!({
                "format": 1,
                "board": 7,
                envelope_key.clone(): { "deep": [payload.clone(), 3, null] },
                "sections": {
                    "tasks": { "55": {
                        "fields": { entry_key.clone(): { "strange": payload.clone() } },
                    } },
                    section_key.clone(): { "x": payload.clone() },
                },
            });
            let doc: AppDataDocument = serde_json::from_value(raw.clone()).unwrap();
            let back = serde_json::to_value(&doc).unwrap();
            prop_assert_eq!(&back[envelope_key.as_str()], &raw[envelope_key.as_str()]);
            prop_assert_eq!(&back["sections"][section_key.as_str()], &raw["sections"][section_key.as_str()]);
            prop_assert_eq!(
                &back["sections"]["tasks"]["55"]["fields"][entry_key.as_str()],
                &raw["sections"]["tasks"]["55"]["fields"][entry_key.as_str()]
            );
        }

        #[test]
        #[test]
        fn tombstone_derivation_never_invents_keys(
            present in proptest::bool::ANY,
            clock in 0_i64..4_000_000_000,
        ) {
            let id = "00000000-0000-0000-0000-00000000000a";
            let base = RulesEntry {
                rules: [(id.to_owned(), RuleEntry::Rule {
                    insistence: Insistence::Normal,
                    kind: crate::reminders::ReminderKind::DueOffset(crate::reminders::ReminderOffset::at_due()),
                    created: ts(1),
                    clock: ts(2),

                by: None,})].into(),
                unknown: BTreeMap::new(),
            };
            let mut local = BTreeMap::new();
            if present {
                local.insert(
                    ReminderRuleId::from(uuid::Uuid::from_u128(0xa)),
                    crate::reminders::ReminderRule {
                        insistence: Insistence::Normal,
                        id: ReminderRuleId::from(uuid::Uuid::from_u128(0xa)),
                        task: TaskId::from(uuid::Uuid::from_u128(1)),
                        kind: crate::reminders::ReminderKind::DueOffset(crate::reminders::ReminderOffset::at_due()),
                        created_at: ts(1),
                        updated_at: ts(2),
                    },
                );
            }
            let derived = derive_rule_tombstones(&base, &local, ts(clock), None);
            if present {
                prop_assert!(derived.is_empty());
            } else {
                prop_assert_eq!(derived.len(), 1);
                prop_assert!(derived.contains_key(id));
            }
        }

        #[test]
        fn lane_gc_touched_only_modeled_scheduled_keys(
            effort in proptest::option::of(0_u64..1_000),
            drop in proptest::bool::ANY,
        ) {
            let mut task = task_fixture();
            task.clocks.effort = ts(7);
            task.effort = effort.map(Effort::new);
            let t = task;
            let doc = document(
                2,
                AppDataSections {
                    tasks: Some([("55".to_owned(), task_entry_for(&t, false))].into()),
                    ..Default::default()
                },
            );
            let gc = lane_gc_scheduled(&doc, drop);
            if drop {
                prop_assert!(!gc.sections.tasks.as_ref().unwrap()["55"].fields.contains_key("scheduled"));
                prop_assert_eq!(
                    gc.sections.tasks.as_ref().unwrap()["55"].fields.get("effort"),
                    doc.sections.tasks.as_ref().unwrap()["55"].fields.get("effort")
                );
            } else {
                prop_assert_eq!(gc, doc);
            }
        }

        #[test]
        fn gc_keeps_present_cards_and_unknown_sections(
            absent in "[0-9]{3}",
            payload in "[a-zA-Z0-9]{0,16}",
        ) {
            let mut sections = AppDataSections {
                tasks: Some(
                    [
                        ("55".to_owned(), entry(&[])),
                        (absent.clone(), entry(&[])),
                    ]
                    .into(),
                ),
                ..Default::default()
            };
            sections
                .unknown
                .insert("proofs".to_owned(), serde_json::json!({ "p": payload }));
            let doc = document(2, sections);
            let present: std::collections::BTreeSet<String> = ["55"].into_iter().map(str::to_owned).collect();
            let pruned = prune_absent_cards(&doc, &present);
            prop_assert!(!pruned.sections.tasks.as_ref().unwrap().contains_key(&absent));
            prop_assert_eq!(pruned.sections.unknown["proofs"].clone(), serde_json::json!({ "p": payload }));
        }

        // P6 (D-6, the union): newer clock wins, equal clocks resolve to
        // the overlay, idempotent re-union (the dismissal_union_lww_laws
        // shape over tombstone rows).
        #[test]
        fn break_rules_union_lww_laws(
            base_clock in 0_i64..4_000_000_000,
            overlay_clock in 0_i64..4_000_000_000,
        ) {
            let key = crate::ids::BreakRuleId::from(uuid::Uuid::from_u128(1))
                .as_uuid()
                .to_string();
            let mk = |clock: i64| {
                BTreeMap::from([(
                    key.clone(),
                    BreakRuleEntry::Tombstone { clock: ts(clock), by: None },
                )])
            };
            let mut base = mk(base_clock);
            let overlay = mk(overlay_clock);
            union_break_rules(&mut base, &overlay);
            prop_assert_eq!(
                base[&key].clock(),
                Some(ts(base_clock.max(overlay_clock))),
                "the newer embedded clock wins"
            );
            let once = base.clone();
            union_break_rules(&mut base, &overlay);
            prop_assert_eq!(base, once, "idempotent");
        }

        // P1 (design D-7): the dismissals section rides the document
        // union's embedded-clock LWW — idempotent, and the winner's
        // clock is never older than either side's.
        #[test]
        fn dismissal_union_lww_laws(
            base_clock in 0_i64..4_000_000_000,
            overlay_clock in 0_i64..4_000_000_000,
        ) {
            let key = dismissal_wire_key(
                crate::ids::ReminderRuleId::from(uuid::Uuid::from_u128(1)),
                ts(100),
            );
            let mk = |clock: i64| DismissalsEntry {
                dismissals: [(
                    key.clone(),
                    DismissalEntry::Dismissed { dismissed_at: ts(clock), by: None },
                )]
                .into(),
                unknown: BTreeMap::new(),
            };
            let mut base = mk(base_clock);
            let overlay = mk(overlay_clock);
            union_dismissals_entry(&mut base, &overlay);
            prop_assert_eq!(
                base.dismissals[&key].clock(),
                Some(ts(base_clock.max(overlay_clock))),
                "the newer embedded clock wins"
            );
            // Idempotence: re-unioning the overlay changes nothing.
            let once = base.clone();
            union_dismissals_entry(&mut base, &overlay);
            prop_assert_eq!(base, once);
        }

        // P2 (design D-4, the fail-closed pin, extended in place for
        // #588 D-15). The constraint: the dismissals wire never carries
        // a *device-local* family's identity, while the roaming Break
        // family emits only under its own `break:` grammar inside the
        // reserved board-level entry. Three typed legs over the same
        // generated set:
        //
        // - **Card leg** (unchanged assertions): the card builder's
        //   wire content contains no TaskId, no AdhocAlarmId, no
        //   BreakRuleId, no default:/alarm:/break: key form — the
        //   fail-closed contract, byte-identical.
        // - **Reserved leg** (the carve-out): the reserved-entry
        //   builder emits `break:<uuid>|…` keys for exactly the
        //   generated Break-ref rows whose rule exists on the board,
        //   and never carries TaskId/AdhocAlarmId text or a
        //   default:/alarm: prefix. Family membership decides; no
        //   string allowlist. Flip-proof: temporarily emit a
        //   Default-ref row from the reserved builder (one test-local
        //   match arm) and the conjunction/negative assertions fail.
        // - **Conjunction leg**: card entries ∪ reserved entry never
        //   carries a device-local family's identity.
        #[test]
        fn no_local_ids_reach_the_dismissals_wire(
            task_seed in 0_u128..1_000_000,
            alarm_seed in 0_u128..1_000_000,
            rule_seed in 0_u128..1_000_000,
            break_seed in 0_u128..1_000_000,
            occurrence in 0_i64..4_000_000_000,
            with_default_row in proptest::bool::ANY,
            with_adhoc_row in proptest::bool::ANY,
            with_rule_row in proptest::bool::ANY,
            with_break_row in proptest::bool::ANY,
        ) {
            let task = crate::ids::TaskId::from(uuid::Uuid::from_u128(task_seed));
            let alarm = AdhocAlarmId::from(uuid::Uuid::from_u128(alarm_seed));
            let rule = crate::ids::ReminderRuleId::from(uuid::Uuid::from_u128(rule_seed));
            let mut rules = BTreeMap::new();
            rules.insert(
                rule,
                crate::reminders::ReminderRule {
                    insistence: Insistence::Normal,
                    id: rule,
                    task,
                    kind: crate::reminders::ReminderKind::DueOffset(crate::reminders::ReminderOffset::at_due()),
                    created_at: ts(1),
                    updated_at: ts(2),
                },
            );
            let mut dismissals = std::collections::BTreeSet::new();
            if with_rule_row {
                dismissals.insert(dismissal(ReminderRef::Rule(rule), occurrence, occurrence + 1));
            }
            if with_default_row {
                dismissals.insert(dismissal(ReminderRef::Default(task), occurrence, occurrence + 2));
            }
            if with_adhoc_row {
                dismissals.insert(dismissal(ReminderRef::Adhoc(alarm), occurrence, occurrence + 3));
            }
            let break_rule = crate::ids::BreakRuleId::from(uuid::Uuid::from_u128(break_seed));
            let board = crate::ids::BoardId::from(uuid::Uuid::from_u128(break_seed ^ 0xdead_beef));
            let mut break_rules = BTreeMap::new();
            break_rules.insert(
                break_rule,
                crate::reminders::BreakRule {
                    id: break_rule,
                    board,
                    label: "time for a break".to_owned(),
                    threshold: crate::tracking::WorkThreshold::try_new(
                        chrono::TimeDelta::try_seconds(1_500).unwrap(),
                    )
                    .unwrap(),
                    insistence: Insistence::Normal,
                    created_at: ts(1),
                },
            );
            if with_break_row {
                dismissals.insert(dismissal(
                    ReminderRef::Break(break_rule),
                    occurrence,
                    occurrence + 4,
                ));
            }
            let entry = dismissals_entry_for(&dismissals, &rules, task, None);
            let wire = serde_json::to_string(&entry).unwrap();
            prop_assert!(
                !wire.contains(&task.as_uuid().to_string()),
                "the local TaskId reached the wire: {wire}"
            );
            prop_assert!(
                !wire.contains(&alarm.as_uuid().to_string()),
                "the local AdhocAlarmId reached the wire: {wire}"
            );
            prop_assert!(
                !wire.contains(&break_rule.as_uuid().to_string()),
                "the local BreakRuleId reached the card wire: {wire}"
            );
            prop_assert!(!wire.contains("default:"), "{wire}");
            prop_assert!(!wire.contains("alarm:"), "{wire}");
            prop_assert!(!wire.contains("break:"), "{wire}");

            // The reserved leg (the carve-out): typed positive for the
            // Break family, typed negative for the device-local ones.
            let reserved = break_dismissals_entry_for(&dismissals, &break_rules, board, None);
            let reserved_wire = serde_json::to_string(&reserved).unwrap();
            prop_assert_eq!(
                reserved_wire.contains(&format!("break:{}|", break_rule.as_uuid())),
                with_break_row,
                "exactly the on-board Break-ref rows emit under the break: grammar"
            );
            prop_assert!(
                !reserved_wire.contains(&task.as_uuid().to_string()),
                "the local TaskId reached the reserved entry: {reserved_wire}"
            );
            prop_assert!(
                !reserved_wire.contains(&alarm.as_uuid().to_string()),
                "the local AdhocAlarmId reached the reserved entry: {reserved_wire}"
            );
            prop_assert!(!reserved_wire.contains("default:"), "{reserved_wire}");
            prop_assert!(!reserved_wire.contains("alarm:"), "{reserved_wire}");

            // The conjunction leg: the whole serialized dismissals
            // section (card entries ∪ reserved entry) never carries a
            // device-local family's identity.
            let section_wire = format!("{wire}{reserved_wire}");
            prop_assert!(
                !section_wire.contains(&task.as_uuid().to_string()),
                "the local TaskId reached the section wire"
            );
            prop_assert!(
                !section_wire.contains(&alarm.as_uuid().to_string()),
                "the local AdhocAlarmId reached the section wire"
            );
            prop_assert!(!section_wire.contains("default:"));
            prop_assert!(!section_wire.contains("alarm:"));
        }

        // The pre-dismissals relay (agenda §4(f)/M6 extended to the
        // third section): `dismissals`-section content this version
        // does not model — verbatim rows, unknown keys — survives
        // decode→encode value-verbatim.
        #[test]
        fn unmodeled_dismissals_content_is_value_verbatim_across_decode_encode(
            row_key in "[a-z:-]{1,20}",
            peer_key in "[a-z-]{1,12}",
            payload in "[a-zA-Z0-9 ]{0,40}",
        ) {
            let raw = serde_json::json!({
                "format": 1,
                "board": 7,
                "sections": {
                    "dismissals": {
                        "55": {
                            "dismissals": {
                                row_key.clone(): { "strange": payload.clone() }
                            },
                            peer_key.clone(): payload.clone(),
                        }
                    },
                },
            });
            let doc: AppDataDocument = serde_json::from_value(raw.clone()).unwrap();
            let back = serde_json::to_value(&doc).unwrap();
            prop_assert_eq!(
                &back["sections"]["dismissals"]["55"]["dismissals"][row_key.as_str()],
                &raw["sections"]["dismissals"]["55"]["dismissals"][row_key.as_str()]
            );
            prop_assert_eq!(
                &back["sections"]["dismissals"]["55"][peer_key.as_str()],
                &raw["sections"]["dismissals"]["55"][peer_key.as_str()]
            );
        }

        // P6 (DR-5/2, D-T13): the time-entries lane GC changes exactly
        // that section — fresh rows and verbatim survive, aged rows of
        // both arms go, every other section is untouched.
        #[test]
        fn gc_touched_only_time_entries(
            rows in proptest::collection::vec(
                (0_u128..8, proptest::option::of(0_u64..1_000), 0_i64..4_000_000_000, proptest::bool::ANY),
                0..8,
            ),
            now in 4_000_000_000_i64..4_100_000_000,
        ) {
            let mut sections = AppDataSections::default();
            let mut time_entries = BTreeMap::new();
            for (seed, card, start, tombstone) in rows {
                let key = uuid::Uuid::from_u128(seed).to_string();
                let row = if tombstone {
                    TimeEntryRecord::Tombstone { clock: ts(start), by: None }
                } else {
                    base_entry(&card.map(|c| c.to_string()).unwrap_or_default(), start)
                };
                time_entries.insert(key, row);
            }
            sections.time_entries = Some(time_entries);
            sections.tasks = Some([("55".to_owned(), entry(&[]))].into_iter().collect());
            let doc = document(2, sections);
            let gc = lane_gc_old_time_entries(&doc, ts(now));
            prop_assert_eq!(gc.sections.tasks, doc.sections.tasks, "tasks untouched");
            prop_assert_eq!(gc.sections.unknown, doc.sections.unknown);
            let window_start = ts(now) - chrono::TimeDelta::try_days(TRACKING_SYNC_RETENTION_DAYS).unwrap();
            if let (Some(before), Some(after)) = (&doc.sections.time_entries, &gc.sections.time_entries) {
                for (key, row) in after {
                    let instant = match row {
                        TimeEntryRecord::Entry { start, .. } => Some(*start),
                        TimeEntryRecord::Tombstone { clock, .. } => Some(*clock),
                        TimeEntryRecord::Verbatim(_) => None,
                    };
                    prop_assert!(instant.is_none_or(|t| t >= window_start), "survivors are in-window");
                    prop_assert_eq!(row, &before[key], "survivors are unchanged");
                }
            }
        }

        // P3 (DR-5/2, D-T11): the time-entries section rides the document
        // union's embedded-clock LWW — idempotent, and the winner's clock
        // is never older than either side's; an open→closed transition
        // rides as a newer write.
        #[test]
        fn time_entry_union_lww_laws(
            base_clock in 0_i64..4_000_000_000,
            overlay_clock in 0_i64..4_000_000_000,
        ) {
            let mut base = BTreeMap::new();
            base.insert("e".to_owned(), entry_row(Some("412"), base_clock));
            let mut overlay = BTreeMap::new();
            overlay.insert("e".to_owned(), open_row(Some("412"), overlay_clock));
            union_time_entries(&mut base, &overlay);
            prop_assert_eq!(
                base["e"].clock(),
                Some(ts(base_clock.max(overlay_clock))),
                "the newer embedded clock wins"
            );
            // Idempotence: re-unioning the overlay changes nothing.
            let once = base.clone();
            union_time_entries(&mut base, &overlay);
            prop_assert_eq!(&base, &once);
            // Convergence up to the tie rule's overlay bias: the standard
            // three-document identity the dismissal laws use.
            let union_maps = |a: &BTreeMap<String, TimeEntryRecord>,
                              b: &BTreeMap<String, TimeEntryRecord>| {
                let mut merged = a.clone();
                union_time_entries(&mut merged, b);
                merged
            };
            let ab = union_maps(&base, &overlay);
            let ba = union_maps(&overlay, &base);
            let converged = union_maps(&ab, &ba);
            prop_assert_eq!(&converged, &ab);
            let converged = union_maps(&ba, &ab);
            prop_assert_eq!(&converged, &ab);
        }

        // P4 (DR-5/2, the D-T12 pin): for arbitrary base maps and
        // resolvable sets, every derived tombstone names a key that
        // decodes as a live base row, parses as a UUID, lies within the
        // window, resolves, and is locally absent — a truncated view can
        // never invent a deletion.
        #[test]
        fn tombstone_derivation_never_deletes_from_a_truncated_view(
            entries in proptest::collection::vec(
                (0_u128..8, proptest::option::of(0_u64..1_000), 0_i64..4_000_000_000, proptest::bool::ANY),
                0..8,
            ),
            resolvable_cards in proptest::collection::vec("[0-9]{1,4}", 0..4),
            now in 4_000_000_000_i64..4_100_000_000,
            clock in 0_i64..4_000_000_000,
        ) {
            let resolvable: std::collections::BTreeSet<String> =
                resolvable_cards.into_iter().collect();
            let mut base = BTreeMap::new();
            let mut local = BTreeMap::new();
            for (seed, card, start, locally_present) in entries {
                let key = uuid::Uuid::from_u128(seed).to_string();
                let row = base_entry(&card.map(|c| c.to_string()).unwrap_or_default(), start);
                base.insert(key, row);
                if locally_present && let Some(task_seed) = card {
                    let id = TimeEntryId::from(uuid::Uuid::from_u128(seed));
                    let task = TaskId::from(uuid::Uuid::from_u128(u128::from(task_seed)));
                    local.insert(id, tracked(seed, start, true, task));
                }
            }
            let derived =
                derive_time_entry_tombstones(&base, &local, &resolvable, ts(now), ts(clock), None);
            for (key, row) in &derived {
                prop_assert!(matches!(row, TimeEntryRecord::Tombstone { .. }), "only tombstones derive");
                let Some(parsed) = uuid::Uuid::parse_str(key).ok() else {
                    prop_assert!(false, "derived keys parse as UUIDs");
                    return Err(TestCaseError::Fail("unparseable key".into()));
                };
                let TimeEntryRecord::Entry { start, task: Some(task), .. } = &base[key] else {
                    prop_assert!(false, "derived keys name live base rows");
                    return Err(TestCaseError::Fail("non-live base row".into()));
                };
                prop_assert!(*start >= ts(now) - chrono::TimeDelta::try_days(TRACKING_SYNC_RETENTION_DAYS).unwrap(), "within the window");
                prop_assert!(resolvable.contains(task), "the card resolves");
                prop_assert!(!local.contains_key(&TimeEntryId::from(parsed)), "locally absent");
            }
        }

        // P5 (DR-5/2, the fail-closed emission pin): every emitted key
        // parses as a UUID and every emitted `task` is a decimal string
        // naming a `board`-bound card — no local or unbound identity
        // reaches the time-entries wire.
        #[test]
        fn no_local_or_unbound_ids_reach_the_time_entries_wire(
            entries in proptest::collection::vec(
                (0_u128..16, 0_u128..16, proptest::option::of(0_u64..1_000), 0_i64..4_000_000_000),
                0..8,
            ),
            target_board in 0_u64..1_000,
            now in 4_000_000_000_i64..4_100_000_000,
        ) {
            let board = RemoteBoardId::deck(target_board);
            let mut tasks = BTreeMap::new();
            let mut entries_map = BTreeMap::new();
            for (entry_seed, task_seed, card, start) in &entries {
                let task = TaskId::from(uuid::Uuid::from_u128(*task_seed));
                tasks.entry(task).or_insert_with(|| {
                    let mut t = task_fixture();
                    t.id = task;
                    t.remote = card.map(|c| RemoteCardRef {
                        board,
                        stack: RemoteStackId::deck(7),
                        card: RemoteCardId::deck(c),
                    });
                    t
                });
                let entry = tracked(*entry_seed, *start, true, task);
                entries_map.insert(entry.id, entry);
            }
            let emitted = time_entries_for(&entries_map, &tasks, board, ts(now), None);
            for (key, row) in &emitted {
                prop_assert!(uuid::Uuid::parse_str(key).is_ok(), "keys are UUIDs");
                let TimeEntryRecord::Entry { task: Some(task), .. } = row else {
                    return Err(TestCaseError::Fail("emitted rows carry a card".into()));
                };
                let Ok(card_num) = task.parse::<u64>() else {
                    prop_assert!(false, "the card id is decimal");
                    return Err(TestCaseError::Fail("non-decimal card".into()));
                };
                prop_assert!(
                    tasks.values().any(|t| t.remote.is_some_and(|r| r.board == board && r.card.as_deck() == Some(card_num))),
                    "the card names a board-bound task"
                );
            }
        }
    }
}

#[cfg(test)]
mod habit_section_tests {
    use super::*;
    use crate::habit::{
        Habit, HabitClocks, HabitLogEntry, HabitLogKey, HabitTarget, NonZeroCount, WindowStart,
    };
    use crate::ids::{BoardId, HabitId};
    use proptest::prelude::*;
    use std::collections::BTreeSet;

    fn ts(secs: i64) -> DateTime<Utc> {
        chrono::TimeZone::timestamp_opt(&Utc, secs, 0).unwrap()
    }

    fn habit(raw: u128, board: u128, deleted: bool, stamp: i64) -> Habit {
        Habit {
            id: HabitId::from(uuid::Uuid::from_u128(raw)),
            board: BoardId::from(uuid::Uuid::from_u128(board)),
            title: format!("habit {raw}"),
            target: HabitTarget::PerDay(NonZeroCount::new(3).unwrap()),
            order: 0,
            anchor_time: None,
            clocks: HabitClocks::all(ts(stamp)),
            continued_from: None,
            deleted,
        }
    }

    fn live_row(stamp: i64) -> HabitRecord {
        HabitRecord::Live {
            title: "water".to_owned(),
            target: HabitTarget::PerDay(NonZeroCount::new(3).unwrap()),
            order: 0,
            anchor_time: None,
            clocks: HabitClocks::all(ts(stamp)),
            continued_from: None,

            by: None,
        }
    }

    fn key(habit_raw: u128, date: chrono::NaiveDate) -> HabitLogKey {
        HabitLogKey {
            habit: HabitId::from(uuid::Uuid::from_u128(habit_raw)),
            window: WindowStart::of_period(crate::habit::PeriodKind::Day, date),
        }
    }

    fn date(y: i32, m: u32, d: u32) -> chrono::NaiveDate {
        chrono::NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn document(board: u64, sections: AppDataSections) -> AppDataDocument {
        AppDataDocument {
            format: APPDATA_FORMAT,
            board: RemoteBoardId::deck(board),
            sections,
            unknown: BTreeMap::new(),
        }
    }

    // ---- T-1: section serde ------------------------------------------------

    #[test]
    fn the_habit_sections_ride_the_envelope_under_their_modeled_keys() {
        let sections = AppDataSections {
            habits: Some(
                [(uuid::Uuid::from_u128(1).to_string(), live_row(100))]
                    .into_iter()
                    .collect(),
            ),
            habit_log: Some(
                [(
                    key(1, date(2026, 10, 28)).wire_form(),
                    HabitLogRecord::Entry {
                        count: 2,
                        clock: ts(100),

                        by: None,
                    },
                )]
                .into_iter()
                .collect(),
            ),
            ..AppDataSections::default()
        };
        let json = serde_json::to_string(&document(7, sections)).unwrap();
        assert!(json.contains(r#""habits""#), "the definitions key is exact");
        assert!(json.contains(r#""habit-log""#), "the log key is exact");
        let back: AppDataDocument = serde_json::from_str(&json).unwrap();
        assert_eq!(back.sections.habits.as_ref().unwrap().len(), 1);
        assert_eq!(back.sections.habit_log.as_ref().unwrap().len(), 1);
    }

    #[test]
    fn a_malformed_live_body_demotes_to_verbatim_and_never_fails_the_document() {
        let raw = format!(
            r#"{{"format":1,"board":7,"sections":{{"habits":{{"{}":{{"title":42}}}}}}}}"#,
            uuid::Uuid::from_u128(1)
        );
        let doc: AppDataDocument = serde_json::from_str(&raw).expect("the document never fails");
        let row = &doc.sections.habits.as_ref().unwrap()[&uuid::Uuid::from_u128(1).to_string()];
        assert!(matches!(row, HabitRecord::Verbatim(_)));
    }

    #[test]
    fn a_future_deleted_log_row_decodes_verbatim_the_leniency_boundary() {
        // D-H5: no command deletes a log row, so no tombstone arm exists;
        // a future delete shape must relay, never fail.
        let row: HabitLogRecord =
            serde_json::from_str(r#"{"deleted": true, "clock": "2026-10-28T00:00:00Z"}"#).unwrap();
        assert!(matches!(row, HabitLogRecord::Verbatim(_)));
    }

    #[test]
    fn board_level_habit_keys_are_invisible_to_prune_absent_cards() {
        // R-9's structural pin: pruning against an EMPTY present set
        // empties every card-keyed section but loses nothing from the
        // two board-level ones — a future arm that starts pruning board
        // keys fails here loudly.
        let habit_uuid = uuid::Uuid::from_u128(1).to_string();
        let sections = AppDataSections {
            tasks: Some(
                [("55".to_owned(), TaskEntry::default())]
                    .into_iter()
                    .collect(),
            ),
            habits: Some([(habit_uuid.clone(), live_row(100))].into_iter().collect()),
            habit_log: Some(
                [(
                    key(1, date(2026, 10, 28)).wire_form(),
                    HabitLogRecord::Entry {
                        count: 2,
                        clock: ts(100),

                        by: None,
                    },
                )]
                .into_iter()
                .collect(),
            ),
            ..AppDataSections::default()
        };
        let pruned = prune_absent_cards(&document(7, sections), &BTreeSet::new());
        assert!(pruned.sections.tasks.is_none(), "card-keyed sections empty");
        assert_eq!(
            pruned.sections.habits.as_ref().unwrap().get(&habit_uuid),
            Some(&live_row(100)),
            "habit definitions outlive everything"
        );
        assert_eq!(pruned.sections.habit_log.as_ref().unwrap().len(), 1);
    }

    // ---- T-2: union (P-1) ---------------------------------------------------

    #[test]
    fn union_habits_newer_whole_row_wins_and_ties_resolve_to_the_overlay() {
        let uuid = uuid::Uuid::from_u128(1).to_string();
        let base: BTreeMap<String, HabitRecord> =
            [(uuid.clone(), live_row(100))].into_iter().collect();
        let mut overlay = BTreeMap::new();
        let mut older = live_row(100);
        if let HabitRecord::Live { title, clocks, .. } = &mut older {
            *title = "renamed".to_owned();
            clocks.title = ts(200); // effective clock 200 > 100
            let _ = clocks;
        }
        overlay.insert(uuid.clone(), older);
        let mut merged = base.clone();
        union_habits(&mut merged, &overlay);
        assert!(
            matches!(&merged[&uuid], HabitRecord::Live { title, .. } if title == "renamed"),
            "the newer effective clock wins whole-row"
        );
        // Exact tie: overlay wins (the D-T11 flush-side ruling).
        let mut overlay_tie = BTreeMap::new();
        let mut tied = live_row(100);
        if let HabitRecord::Live { title, .. } = &mut tied {
            *title = "tied".to_owned();
            // clocks all at 100: the effective clock ties the base's.
        }
        overlay_tie.insert(uuid.clone(), tied);
        let mut merged_tie = base.clone();
        union_habits(&mut merged_tie, &overlay_tie);
        assert!(
            matches!(&merged_tie[&uuid], HabitRecord::Live { title, .. } if title == "tied"),
            "ties resolve to the overlay"
        );
    }

    #[test]
    fn union_habit_log_is_row_lww_with_overlay_ties() {
        let k = key(1, date(2026, 10, 28)).wire_form();
        let base: BTreeMap<String, HabitLogRecord> = [(
            k.clone(),
            HabitLogRecord::Entry {
                count: 3,
                clock: ts(200),

                by: None,
            },
        )]
        .into_iter()
        .collect();
        let mut overlay = BTreeMap::new();
        overlay.insert(
            k.clone(),
            HabitLogRecord::Entry {
                count: 5,
                clock: ts(100),

                by: None,
            },
        );
        let mut merged = base.clone();
        union_habit_log(&mut merged, &overlay);
        assert!(
            matches!(&merged[&k], HabitLogRecord::Entry { count: 3, .. }),
            "the strictly-newer base row stands"
        );
        let mut overlay_newer = BTreeMap::new();
        overlay_newer.insert(
            k.clone(),
            HabitLogRecord::Entry {
                count: 7,
                clock: ts(300),

                by: None,
            },
        );
        union_habit_log(&mut merged, &overlay_newer);
        assert!(
            matches!(&merged[&k], HabitLogRecord::Entry { count: 7, .. }),
            "the newer overlay row wins"
        );
    }

    // ---- T-3: tombstone races (P-2) ------------------------------------------

    #[test]
    fn tombstone_and_live_rows_compete_by_the_effective_clock_both_directions() {
        let uuid = uuid::Uuid::from_u128(1).to_string();
        // A delete after an edit wins.
        let base = [(uuid.clone(), live_row(100))].into_iter().collect();
        let mut overlay = BTreeMap::new();
        overlay.insert(
            uuid.clone(),
            HabitRecord::Tombstone {
                clock: ts(300),
                by: None,
            },
        );
        let mut merged = base;
        union_habits(&mut merged, &overlay);
        assert!(matches!(&merged[&uuid], HabitRecord::Tombstone { .. }));

        // A later-dated live row resurrects over an older tombstone.
        let base = [(
            uuid.clone(),
            HabitRecord::Tombstone {
                clock: ts(100),
                by: None,
            },
        )]
        .into_iter()
        .collect();
        let mut overlay = BTreeMap::new();
        overlay.insert(uuid.clone(), live_row(200));
        let mut merged = base;
        union_habits(&mut merged, &overlay);
        assert!(matches!(&merged[&uuid], HabitRecord::Live { .. }));

        // An effective-clock tie between tombstone and live: overlay wins.
        let base = [(uuid.clone(), live_row(100))].into_iter().collect();
        let mut overlay = BTreeMap::new();
        overlay.insert(
            uuid.clone(),
            HabitRecord::Tombstone {
                clock: ts(100),
                by: None,
            },
        );
        let mut merged = base;
        union_habits(&mut merged, &overlay);
        assert!(matches!(&merged[&uuid], HabitRecord::Tombstone { .. }));
    }

    // ---- T-4: verbatim relay (P-4) -------------------------------------------

    #[test]
    fn verbatim_rows_survive_union_and_a_full_serde_round_trip_value_identically() {
        let uuid = uuid::Uuid::from_u128(1).to_string();
        let k = key(1, date(2026, 10, 28)).wire_form();
        let verbatim_habit = serde_json::json!({ "exotic": [1, 2, { "shape": true }] });
        let verbatim_log = serde_json::json!({ "future": {"deleted": true, "extra": 9} });
        let base = document(
            7,
            AppDataSections {
                habits: Some(
                    [(uuid.clone(), HabitRecord::Verbatim(verbatim_habit.clone()))]
                        .into_iter()
                        .collect(),
                ),
                habit_log: Some(
                    [(k.clone(), HabitLogRecord::Verbatim(verbatim_log.clone()))]
                        .into_iter()
                        .collect(),
                ),
                ..AppDataSections::default()
            },
        );
        // Overlay carries clocked rows for the same keys — the verbatim
        // slots stay untouched (never ordered against), unknown keys
        // extend additively.
        let overlay = document(
            7,
            AppDataSections {
                habits: Some([(uuid.clone(), live_row(900))].into_iter().collect()),
                habit_log: Some(
                    [(
                        key(2, date(2026, 10, 29)).wire_form(),
                        HabitLogRecord::Entry {
                            count: 1,
                            clock: ts(900),

                            by: None,
                        },
                    )]
                    .into_iter()
                    .collect(),
                ),
                ..AppDataSections::default()
            },
        );
        let merged = union(&base, &overlay);
        assert_eq!(
            merged.sections.habits.as_ref().unwrap().get(&uuid),
            Some(&HabitRecord::Verbatim(verbatim_habit)),
        );
        assert_eq!(merged.sections.habits.as_ref().unwrap().len(), 1);
        assert_eq!(
            merged.sections.habit_log.as_ref().unwrap().get(&k),
            Some(&HabitLogRecord::Verbatim(verbatim_log)),
        );
        assert_eq!(merged.sections.habit_log.as_ref().unwrap().len(), 2);
        let json = serde_json::to_string(&merged).unwrap();
        let back: AppDataDocument = serde_json::from_str(&json).unwrap();
        assert_eq!(back, merged, "the relay is value-identical through serde");
    }

    // ---- T-5b: habits_for ------------------------------------------------------

    #[test]
    fn habits_for_emits_live_and_tombstoned_definitions_of_one_board_only() {
        let mine = habit(1, 800, false, 100);
        let gone = habit(2, 800, true, 200);
        let foreign = habit(3, 900, false, 300);
        let board = BoardId::from(uuid::Uuid::from_u128(800));
        let emitted = habits_for(
            &[
                (mine.id, mine.clone()),
                (gone.id, gone.clone()),
                (foreign.id, foreign),
            ]
            .into_iter()
            .collect(),
            board,
            None,
        );
        assert_eq!(
            emitted.len(),
            2,
            "definitions carry no retention; foreign never emits"
        );
        assert_eq!(
            emitted.get(&mine.id.as_uuid().to_string()),
            Some(&HabitRecord::Live {
                title: mine.title.clone(),
                target: mine.target,
                order: mine.order,
                anchor_time: mine.anchor_time,
                clocks: mine.clocks,
                continued_from: mine.continued_from,

                by: None,
            })
        );
        assert_eq!(
            emitted.get(&gone.id.as_uuid().to_string()),
            Some(&HabitRecord::Tombstone {
                clock: ts(200),
                by: None
            })
        );
    }

    // ---- T-5: retention agreement (P-4) ----------------------------------------

    fn in_window(date: chrono::NaiveDate, today: chrono::NaiveDate) -> bool {
        date >= today
            .checked_sub_days(chrono::Days::new(
                u64::try_from(HABIT_LOG_SYNC_RETENTION_DAYS).unwrap(),
            ))
            .unwrap_or(chrono::NaiveDate::MIN)
    }

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(64))]

        #[test]
        fn the_retention_window_is_one_window_across_emission_union_and_gc(
            (habit, offsets, today, age_base) in crate::test_support::habit_strategy()
                .prop_flat_map(|habit| (
                    Just(habit),
                    proptest::collection::vec(-400_i64..=10, 0..12),
                    crate::test_support::naive_date_strategy(),
                    -400_i64..=10,
                )),
        ) {
            let board = habit.board;
            let raw = u128::from_be_bytes(habit.id.as_uuid().into_bytes());
            let mut log = BTreeMap::new();
            for (i, offset) in offsets.iter().enumerate() {
                let d = today
                    .checked_add_signed(chrono::TimeDelta::try_days(*offset).unwrap())
                    .unwrap();
                log.insert(
                    key(raw, d),
                    HabitLogEntry { count: u32::try_from(i + 1).unwrap(), clock: ts(100) },
                );
            }
            let emitted = habit_log_for(&[(habit.id, habit.clone())].into(), &log, board, today, None);
            let expected: Vec<String> = log
                .iter()
                .filter(|(k, _)| in_window(k.window.as_date(), today))
                .map(|(k, _)| k.wire_form())
                .collect();
            let got: Vec<String> = emitted.keys().cloned().collect();
            prop_assert_eq!(got.len(), expected.len(), "emission is exactly the in-window set");
            for k in &expected {
                prop_assert!(emitted.contains_key(k), "every in-window row emits");
            }
            // Aged base rows union in but the lane GC strips them; the
            // converged document is exactly the in-window set.
            let aged_d = today
                .checked_add_signed(chrono::TimeDelta::try_days(age_base).unwrap())
                .unwrap();
            let aged_key = key(raw, aged_d);
            let mut base_log = emitted.clone();
            base_log.insert(
                aged_key.wire_form(),
                HabitLogRecord::Entry { count: 9, clock: ts(50), by: None },
            );
            let base = document(
                7,
                AppDataSections {
                    habit_log: (!base_log.is_empty()).then_some(base_log),
                    ..AppDataSections::default()
                },
            );
            let overlay = document(
                7,
                AppDataSections {
                    habit_log: (!emitted.is_empty()).then_some(emitted.clone()),
                    ..AppDataSections::default()
                },
            );
            let merged = lane_gc_old_habit_log(&union(&base, &overlay), today);
            let converged = merged.sections.habit_log.unwrap_or_default();
            for k in converged.keys() {
                let parsed = HabitLogKey::parse_wire_form(k);
                prop_assert!(
                    matches!(&converged[k], HabitLogRecord::Verbatim(_))
                        || parsed.is_some_and(|p| in_window(p.window.as_date(), today)),
                    "no aged row survives the converged document"
                );
            }
            // The asymmetry (D-H14) is structural: every function in the
            // chain takes the local maps by reference and returns fresh
            // documents — no wire GC can reach local truth. Pinned by the
            // borrow checker, not by a runtime assertion.
        }
    }
}

#[cfg(test)]
mod author_by_tests {
    use super::*;
    use crate::ids::AuthorId;
    use chrono::TimeZone;

    fn author() -> AuthorId {
        AuthorId::from_uuid(uuid::Uuid::from_u128(0xBEE_F00))
    }

    fn ts(secs: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(secs, 0).unwrap()
    }

    /// T-SIDE-1: a row with `by` serializes the author and decodes it
    /// back; a `by`-absent row decodes `None` (old documents decode
    /// unchanged); an unparseable `by` demotes the row to `Verbatim`
    /// (never a silent de-authoring).
    #[test]
    fn by_serde_roundtrips_absent_decodes_none_and_garbage_fails_closed() {
        let a = author();
        let rule = RuleEntry::Rule {
            kind: crate::reminders::ReminderKind::At(
                crate::reminders::AtInstant::whole_second(ts(1_000)).unwrap(),
            ),
            created: ts(900),
            clock: ts(1_000),
            insistence: crate::reminders::Insistence::Normal,
            by: Some(a),
        };
        let raw = serde_json::to_value(&rule).unwrap();
        let object = raw.as_object().unwrap();
        assert_eq!(
            object.get("by").and_then(|b| b.as_str()),
            Some(a.to_string().as_str()),
            "the author rides the display form"
        );
        let back = RuleEntry::deserialize_verbatim(raw);
        assert_eq!(back, rule);

        // Absent `by`: the pre-F1 shape decodes authorless.
        let old = RuleEntry::deserialize_verbatim(serde_json::json!({
            "at": ts(1_000).to_rfc3339(),
            "created": ts(900).to_rfc3339(),
            "clock": ts(1_000).to_rfc3339(),
            "insistence": "normal",
        }));
        let RuleEntry::Rule { by: None, .. } = old else {
            panic!("old shape decodes as a Rule row");
        };

        // Present but unparseable: the whole row demotes to Verbatim.
        let garbage = RuleEntry::deserialize_verbatim(serde_json::json!({
            "at": ts(1_000).to_rfc3339(),
            "created": ts(900).to_rfc3339(),
            "clock": ts(1_000).to_rfc3339(),
            "insistence": "normal",
            "by": "not-a-uuid",
        }));
        assert!(
            matches!(garbage, RuleEntry::Verbatim(_)),
            "an unparsable author demotes the row"
        );

        // Tombstones carry the same grammar.
        let tomb = RuleEntry::Tombstone {
            clock: ts(2_000),
            by: Some(a),
        };
        let raw = serde_json::to_value(&tomb).unwrap();
        assert!(raw.as_object().unwrap().contains_key("by"));
        let back = RuleEntry::deserialize_verbatim(raw);
        assert_eq!(back, tomb);

        // The task field write's `by` leg roundtrips through the
        // keyed decode.
        let write = TaskFieldWrite {
            value: ModeledValue::Scheduled(Some(ts(3_000))),
            clock: ts(2_500),
            by: Some(a),
        };
        let raw = serde_json::to_value(&write).unwrap();
        let back = decode_field_write("scheduled", &raw).unwrap();
        assert_eq!(back, write);
        let absent = decode_field_write(
            "scheduled",
            &serde_json::json!({ "v": null, "clock": ts(2_500).to_rfc3339() }),
        )
        .unwrap();
        assert_eq!(absent.by, None);
    }

    /// T-SIDE-2 (the old-reader relay simulation): a pre-F1 reader has
    /// no `by` field — it decodes the authored row (unknown key
    /// ignored), re-serializes it `by`-less, and re-publishes. The
    /// relayed row must decode as a live authorless row (never
    /// verbatim), the fold then compares it against its authored peer
    /// by the clock alone, and the re-published form stays byte-stable
    /// across further relay generations.
    #[test]
    fn old_reader_relay_converges_authorless() {
        let a = author();
        let authored = serde_json::json!({
            "count": 3,
            "clock": ts(1_000).to_rfc3339(),
            "by": a.to_string(),
        });
        // The pre-F1 reader: decode (unknown key ignored), re-serialize.
        // Its type has no `by` field, so its re-emit cannot carry one —
        // the key is stripped here, exactly as the old serializer does.
        let row: HabitLogRecord = serde_json::from_value(authored).unwrap();
        let mut old_reader_reemit = serde_json::to_value(&row).unwrap();
        old_reader_reemit
            .as_object_mut()
            .expect("an entry row is an object")
            .remove("by");
        assert!(
            old_reader_reemit.get("by").is_none(),
            "a pre-F1 reader carries no `by` knowledge"
        );
        // The relay's `by`-less publication, decoded back by us (the
        // mixed fleet's tolerant reader).
        let relayed: HabitLogRecord = serde_json::from_value(old_reader_reemit.clone()).unwrap();
        let HabitLogRecord::Entry {
            count,
            clock,
            by: None,
        } = relayed
        else {
            panic!("the by-less relay decodes as a live authorless row");
        };
        assert_eq!(count, 3);
        assert_eq!(clock, ts(1_000));
        // The relay is generation-stable: our re-emit of the relayed row
        // is byte-identical to the old reader's (both omit the key).
        assert_eq!(serde_json::to_value(&relayed).unwrap(), old_reader_reemit);

        // Fold: the relayed (authorless) leg competes against the
        // authored row — at equal clocks `None < Some` gives the
        // authored row the win; the `None`-vs-`None` tie keeps the
        // pinned strictly-newer direction.
        let mut state = crate::habit::HabitState::default();
        let key = crate::habit::HabitLogKey::parse_wire_form(
            "0193a1b2-c3d4-7cc9-8a2e-5f1e2d3c4b5a|2026-01-01",
        )
        .unwrap();
        let relayed_rows = [(
            key.wire_form(),
            HabitLogRecord::Entry {
                count: 3,
                clock: ts(1_000),
                by: None,
            },
        )]
        .into_iter()
        .collect();
        crate::merge::fold_habit_log(&mut state, &relayed_rows, None);
        assert_eq!(state.log.get(&key).map(|e| e.count), Some(3));
        let authored_rows = [(
            key.wire_form(),
            HabitLogRecord::Entry {
                count: 4,
                clock: ts(1_000),
                by: Some(a),
            },
        )]
        .into_iter()
        .collect();
        crate::merge::fold_habit_log(&mut state, &authored_rows, None);
        assert_eq!(
            state.log.get(&key).map(|e| e.count),
            Some(4),
            "the authored equal-clock row outranks the relayed authorless one"
        );
    }
}
