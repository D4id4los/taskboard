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

/// The codec's modeled task-field keys — its single dispatch source and
/// the subject of the manifest lockstep assertion: [`crate::entities::
/// TASK_FIELDS`]' `AppData` rows plus `scheduled` (the Native row whose
/// sync lane is dynamic per D-V2; the lane decision never lives here).
pub const MODELED_TASK_FIELD_KEYS: &[&str] =
    &["effort", "importance", "urgency", "recur", "scheduled"];

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
}

impl Serialize for TaskFieldWrite {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let value = encode_modeled(&self.value);
        serde_json::json!({ "v": value, "clock": self.clock }).serialize(serializer)
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
    Some(TaskFieldWrite { value, clock })
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
    /// "insistence": "normal"|"alarm" }`. The insistence key is
    /// additive: a pre-insistence document omits it and folds
    /// `Normal`; an unknown rung demotes the whole entry to
    /// [`RuleEntry::Verbatim`] (never a silent downgrade).
    Rule {
        offset: ReminderOffset,
        created: DateTime<Utc>,
        clock: DateTime<Utc>,
        insistence: Insistence,
    },
    /// `{ "deleted": true, "clock": rfc3339 }` — rule *membership* needs
    /// explicit tombstones (absence from a possibly-truncated document
    /// must never delete locally).
    Tombstone { clock: DateTime<Utc> },
    /// A shape this version does not model: preserved verbatim.
    Verbatim(Value),
}

impl RuleEntry {
    /// The entry's write clock; `None` for verbatim content (which the
    /// union therefore never orders against a clocked entry).
    #[must_use]
    pub fn clock(&self) -> Option<DateTime<Utc>> {
        match self {
            Self::Rule { clock, .. } | Self::Tombstone { clock } => Some(*clock),
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
            if let Ok(clock) = serde_json::from_value(map["clock"].clone()) {
                return Self::Tombstone { clock };
            }
            return Self::Verbatim(raw);
        }
        if let (Ok(offset), Ok(created), Ok(clock), Ok(insistence)) = (
            decode_offset(map.get("offset")),
            serde_json::from_value(map.get("created").cloned().unwrap_or(Value::Null)),
            serde_json::from_value(map.get("clock").cloned().unwrap_or(Value::Null)),
            // Default-on-missing (the additive-section precedent): an
            // old document folds `Normal`. A *present* but unmodelable
            // value is not defaulted — it falls through to `Verbatim`.
            decode_insistence_default_on_missing(map.get("insistence")),
        ) {
            return Self::Rule {
                offset,
                created,
                clock,
                insistence,
            };
        }
        Self::Verbatim(raw)
    }
}

impl Serialize for RuleEntry {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Rule {
                offset,
                created,
                clock,
                insistence,
            } => serde_json::json!({
                "offset": offset.as_duration().num_seconds(),
                "created": created,
                "clock": clock,
                "insistence": insistence.text_form(),
            })
            .serialize(serializer),
            Self::Tombstone { clock } => {
                serde_json::json!({ "deleted": true, "clock": clock }).serialize(serializer)
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

/// One card's synced-dismissal entry (the `dismissals` section, design
/// D-1/D-9) — the [`RulesEntry`] shape discipline over a tombstone-free
/// payload: dismissals are permanent for their occurrence, so there are
/// no deletion tombstones to model.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DismissalsEntry {
    /// Keyed by [`dismissal_wire_key`] — `"<rule uuid>|<rfc3339
    /// occurrence>"` (JSON map keys must be strings; the
    /// `ReminderSnoozeKey` string-encoding precedent). The rule UUID's
    /// document presence *is* its remote identity (D-S9); no local
    /// `TaskId`/`AdhocAlarmId` text form is ever representable here.
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
    Dismissed { dismissed_at: DateTime<Utc> },
    /// A shape this version does not model: preserved verbatim.
    Verbatim(Value),
}

impl DismissalEntry {
    /// The entry's write clock; `None` for verbatim content (which the
    /// union therefore never orders against a clocked entry).
    #[must_use]
    pub fn clock(&self) -> Option<DateTime<Utc>> {
        match self {
            Self::Dismissed { dismissed_at } => Some(*dismissed_at),
            Self::Verbatim(_) => None,
        }
    }

    /// Decodes one dismissal row; an unmodelable shape stays `Verbatim`
    /// with the original value (never an error).
    fn deserialize_verbatim(raw: Value) -> Self {
        let Value::Object(map) = &raw else {
            return Self::Verbatim(raw);
        };
        if let Ok(dismissed_at) =
            serde_json::from_value(map.get("dismissed_at").cloned().unwrap_or(Value::Null))
        {
            return Self::Dismissed { dismissed_at };
        }
        Self::Verbatim(raw)
    }
}

impl Serialize for DismissalEntry {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Dismissed { dismissed_at } => {
                serde_json::json!({ "dismissed_at": dismissed_at }).serialize(serializer)
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
    /// "end": rfc3339|null, "created": rfc3339, "clock": rfc3339 }`.
    /// `task: None` decodes tolerantly (a peer row without the key) but
    /// is never emitted by this version — an unbound-task entry has no
    /// identity another device could resolve (D-T10).
    Entry {
        task: Option<String>,
        title: String,
        start: DateTime<Utc>,
        end: Option<DateTime<Utc>>,
        created: DateTime<Utc>,
        clock: DateTime<Utc>,
    },
    /// `{ "deleted": true, "clock": rfc3339 }` — a global delete (D-T12:
    /// deliberate deletes roam; retention aging never does).
    Tombstone { clock: DateTime<Utc> },
    /// A shape this version does not model: preserved verbatim.
    Verbatim(Value),
}

impl TimeEntryRecord {
    /// The row's write clock; `None` for verbatim content (which the
    /// union therefore never orders against a clocked row).
    #[must_use]
    pub fn clock(&self) -> Option<DateTime<Utc>> {
        match self {
            Self::Entry { clock, .. } | Self::Tombstone { clock } => Some(*clock),
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
            if let Ok(clock) = serde_json::from_value(map["clock"].clone()) {
                return Self::Tombstone { clock };
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
        if let (Some(task), Some(end), Ok(title), Ok(start), Ok(created), Ok(clock)) = (
            task,
            end,
            serde_json::from_value(map.get("title").cloned().unwrap_or(Value::Null)),
            serde_json::from_value(map.get("start").cloned().unwrap_or(Value::Null)),
            serde_json::from_value(map.get("created").cloned().unwrap_or(Value::Null)),
            serde_json::from_value(map.get("clock").cloned().unwrap_or(Value::Null)),
        ) {
            return Self::Entry {
                task,
                title,
                start,
                end,
                created,
                clock,
            };
        }
        Self::Verbatim(raw)
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
            } => serde_json::json!({
                "task": task,
                "title": title,
                "start": start,
                "end": end,
                "created": created,
                "clock": clock,
            })
            .serialize(serializer),
            Self::Tombstone { clock } => {
                serde_json::json!({ "deleted": true, "clock": clock }).serialize(serializer)
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
/// not model (a `default:`/`alarm:`-prefixed row a hand-edited document
/// might carry, a malformed pair, …) — the fold ignores it, the codec
/// relays it verbatim.
pub(crate) fn parse_dismissal_wire_key(
    key: &str,
) -> Option<(crate::ids::ReminderRuleId, DateTime<Utc>)> {
    let (uuid_part, occurrence_part) = key.split_once('|')?;
    let uuid = uuid::Uuid::parse_str(uuid_part).ok()?;
    let occurrence = DateTime::parse_from_rfc3339(occurrence_part).ok()?;
    Some((
        crate::ids::ReminderRuleId::from(uuid),
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

/// The flush GC (D-S2): removes the entries — both known sections,
/// tombstones included — whose decimal card key is absent from
/// `present_cards` (the cycle's complete snapshot's card set). Entries
/// for present cards and unknown sections are never touched.
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
        let kept = keep(dismissals, present_cards);
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
        },
    );
    let importance = ModeledValue::Importance(task.importance);
    fields.insert(
        "importance".to_owned(),
        TaskFieldWrite {
            value: importance,
            clock: task.clocks.importance,
        },
    );
    let urgency = ModeledValue::Urgency(task.urgency);
    fields.insert(
        "urgency".to_owned(),
        TaskFieldWrite {
            value: urgency,
            clock: task.clocks.urgency,
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
        },
    );
    if !native_scheduled {
        let scheduled = ModeledValue::Scheduled(task.scheduled);
        fields.insert(
            "scheduled".to_owned(),
            TaskFieldWrite {
                value: scheduled,
                clock: task.clocks.scheduled,
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
) -> RulesEntry {
    let mapped: BTreeMap<String, RuleEntry> = rules
        .values()
        .filter(|rule| rule.task == task)
        .map(|rule| {
            (
                rule.id.as_uuid().to_string(),
                RuleEntry::Rule {
                    offset: rule.offset,
                    created: rule.created_at,
                    clock: rule.updated_at,
                    insistence: rule.insistence,
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
/// **Fail-closed emission filter (design D-4).** The state-side
/// `ReminderDismissal` is representable for all [`ReminderRef`] kinds
/// (local Default/Adhoc dismissals must clear in-app), so the filter —
/// not the type — is the boundary: `Default(TaskId)` and
/// `Adhoc(AdhocAlarmId)` rows are skipped here and can never reach the
/// wire, because the wire key shape admits a rule UUID only. The rule's
/// task scoping mirrors [`rules_entry_for`] (`rule.task == task`), and
/// the proptest pin (`no_local_ids_reach_the_dismissals_wire`) is the
/// tripwire against a future widening. Orphan rows (rule deleted
/// locally) are filtered by the same scoping; their already-uploaded
/// rows persist in the base document via the union's additivity.
#[must_use]
pub fn dismissals_entry_for(
    dismissals: &std::collections::BTreeSet<crate::reminders::ReminderDismissal>,
    rules: &BTreeMap<crate::ids::ReminderRuleId, crate::reminders::ReminderRule>,
    task: crate::ids::TaskId,
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
                    },
                ))
            }
            crate::reminders::ReminderRef::Default(_) | crate::reminders::ReminderRef::Adhoc(_) => {
                None
            }
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
) -> BTreeMap<String, RuleEntry> {
    base.rules
        .iter()
        .filter_map(|(key, entry)| {
            let RuleEntry::Rule { .. } = entry else {
                return None; // tombstones pass through, verbatim never ordered
            };
            let uuid = uuid::Uuid::parse_str(key).ok()?;
            let id = crate::ids::ReminderRuleId::from(uuid);
            (!local.contains_key(&id)).then(|| (key.clone(), RuleEntry::Tombstone { clock }))
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
) -> BTreeMap<String, TimeEntryRecord> {
    let window_start = time_entry_window_start(now);
    entries
        .iter()
        .filter_map(|(id, entry)| {
            if entry.start < window_start {
                return None; // aged: never re-emitted (D-T13)
            }
            let task = tasks.get(&entry.task)?;
            let binding = task.remote.as_ref()?;
            if binding.board != board {
                return None; // unbound or another board's document
            }
            Some((
                id.as_uuid().to_string(),
                TimeEntryRecord::Entry {
                    task: Some(binding.card.get().to_string()),
                    title: entry.task_title.clone(),
                    start: entry.start,
                    end: entry.end,
                    created: entry.created_at,
                    clock: entry.updated_at,
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
                TimeEntryRecord::Tombstone { clock } => *clock >= window_start,
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
            (!local.contains_key(&id)).then(|| (key.clone(), TimeEntryRecord::Tombstone { clock }))
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
        }
    }

    fn document(board: u64, sections: AppDataSections) -> AppDataDocument {
        AppDataDocument {
            format: APPDATA_FORMAT,
            board: RemoteBoardId(board),
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
                                offset: ReminderOffset::before(chrono::TimeDelta::seconds(-900))
                                    .unwrap(),
                                created: ts(100),
                                clock: ts(120),
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
                offset: crate::reminders::ReminderOffset::before(
                    chrono::TimeDelta::try_minutes(-15).unwrap()
                )
                .unwrap(),
                created: "2026-01-01T00:00:00Z".parse().unwrap(),
                clock: "2026-01-01T00:01:00Z".parse().unwrap(),
                insistence: crate::reminders::Insistence::Normal,
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
                offset: crate::reminders::ReminderOffset::before(
                    chrono::TimeDelta::try_minutes(-15).unwrap()
                )
                .unwrap(),
                created: "2026-01-01T00:00:00Z".parse().unwrap(),
                clock: "2026-01-01T00:01:00Z".parse().unwrap(),
                insistence: crate::reminders::Insistence::Alarm,
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
            rules: [("r".to_owned(), RuleEntry::Tombstone { clock: ts(clock) })].into(),
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
                rules: [("r".to_owned(), RuleEntry::Tombstone { clock: ts(1) })].into(),
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
        assert_eq!(MODELED_TASK_FIELD_KEYS.len(), 5);

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

                archived: t(1_000),
                deleted: t(1_000),
            },
            remote_seen: None,
            remote_baseline: None,
        }
    }

    #[test]
    fn task_entry_builds_full_send_writes_from_values_and_clocks() {
        let mut task = task_fixture();
        let entry = task_entry_for(&task, false);
        assert_eq!(entry.fields.len(), 5);
        assert_eq!(
            entry.fields["effort"],
            TaskFieldWrite {
                value: ModeledValue::Effort(Some(Effort::new(900))),
                clock: task.clocks.effort,
            }
        );
        assert_eq!(
            entry.fields["scheduled"].value,
            ModeledValue::Scheduled(Some(task.scheduled.unwrap()))
        );
        // Native ownership: the scheduled key is not emitted; recur (no
        // native lane) still is.
        let native = task_entry_for(&task, true);
        assert_eq!(native.fields.len(), 4);
        assert!(!native.fields.contains_key("scheduled"));
        assert!(native.fields.contains_key("recur"));
        // A dateless-of-rule task emits the typed clear.
        assert_eq!(
            native.fields["recur"],
            TaskFieldWrite {
                value: ModeledValue::Recur(None),
                clock: task.clocks.recur,
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
        };
        task.recur = Some(rule.clone());
        task.clocks.recur = ts(2_000);
        let entry = task_entry_for(&task, false);
        assert_eq!(
            entry.fields["recur"],
            TaskFieldWrite {
                value: ModeledValue::Recur(Some(rule)),
                clock: ts(2_000),
            }
        );
    }

    // Property 8's row half: the emitted recur write survives a full
    // serde round-trip through the row envelope losslessly.
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

    #[test]
    fn rules_entry_builds_live_entries_keyed_by_uuid() {
        let task = task_fixture();
        let mut rules = std::collections::BTreeMap::new();
        let rule = crate::reminders::ReminderRule {
            insistence: Insistence::Normal,
            id: ReminderRuleId::from(uuid::Uuid::from_u128(10)),
            task: task.id,
            offset: crate::reminders::ReminderOffset::at_due(),
            created_at: task.clocks.title,
            updated_at: task.clocks.effort,
        };
        let foreign = crate::reminders::ReminderRule {
            id: ReminderRuleId::from(uuid::Uuid::from_u128(11)),
            task: TaskId::from(uuid::Uuid::from_u128(99)),
            ..rule.clone()
        };
        rules.insert(rule.id, rule.clone());
        rules.insert(foreign.id, foreign);
        let entry = rules_entry_for(&rules, task.id);
        assert_eq!(entry.rules.len(), 1, "only the task's own rules");
        assert_eq!(
            entry.rules[&rule.id.as_uuid().to_string()],
            RuleEntry::Rule {
                insistence: Insistence::Normal,
                offset: rule.offset,
                created: rule.created_at,
                clock: rule.updated_at,
            }
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
                offset: crate::reminders::ReminderOffset::at_due(),
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
                        offset: crate::reminders::ReminderOffset::at_due(),
                        created: ts(1),
                        clock: ts(1),
                    },
                ),
                // An existing tombstone passes through untouched.
                (
                    "00000000-0000-0000-0000-00000000000b",
                    RuleEntry::Tombstone { clock: ts(3) },
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
        let derived = derive_rule_tombstones(&base, &local, clock);
        assert_eq!(derived.len(), 1);
        assert_eq!(
            derived["00000000-0000-0000-0000-00000000000a"],
            RuleEntry::Tombstone { clock }
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
                                offset: ReminderOffset::before(chrono::TimeDelta::seconds(-900))
                                    .unwrap(),
                                created: ts(100),
                                clock: ts(120),
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
            offset: crate::reminders::ReminderOffset::at_due(),
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
        let entry = dismissals_entry_for(&dismissals, &rules, task);
        assert_eq!(entry.dismissals.len(), 1);
        assert!(
            entry
                .dismissals
                .contains_key(&dismissal_wire_key(own_rule, ts(100)))
        );
        assert_eq!(
            entry.dismissals[&dismissal_wire_key(own_rule, ts(100))],
            DismissalEntry::Dismissed {
                dismissed_at: ts(110)
            }
        );
        assert!(entry.unknown.is_empty());
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
        }
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
        assert_eq!(
            keys,
            vec!["clock", "created", "end", "start", "task", "title"]
        );
        let open = serde_json::to_value(open_row(None, 100)).unwrap();
        assert_eq!(open["end"], Value::Null);
        assert_eq!(open["task"], Value::Null);
        let tombstone =
            serde_json::to_value(TimeEntryRecord::Tombstone { clock: ts(100) }).unwrap();
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
            TimeEntryRecord::Tombstone { clock: ts(999) },
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
            TimeEntryRecord::Tombstone { clock: ts(200) },
        );
        union_time_entries(&mut base, &overlay);
        assert!(
            matches!(base["e"], TimeEntryRecord::Tombstone { .. }),
            "a newer tombstone beats the live row"
        );
        let mut base = BTreeMap::new();
        base.insert(
            "e".to_owned(),
            TimeEntryRecord::Tombstone { clock: ts(100) },
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
            board: RemoteBoardId(2),
            stack: RemoteStackId(7),
            card: RemoteCardId(412),
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
                board: RemoteBoardId(2),
                stack: RemoteStackId(7),
                card: RemoteCardId(413),
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
                board: RemoteBoardId(9),
                stack: RemoteStackId(7),
                card: RemoteCardId(414),
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
        let emitted = time_entries_for(&entries, &tasks, RemoteBoardId(2), now);
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
                TimeEntryRecord::Tombstone { clock: ts(500) },
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
        let derived = derive_time_entry_tombstones(&base, &local, &resolvable, now, clock);
        assert_eq!(derived.len(), 1);
        assert_eq!(
            derived["00000000-0000-0000-0000-000000000002"],
            TimeEntryRecord::Tombstone { clock }
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
                            },
                        ),
                        (
                            "aged-tombstone".to_owned(),
                            TimeEntryRecord::Tombstone {
                                clock: ts(aged_secs),
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
        let overlay = time_entries_for(&entries, &tasks, RemoteBoardId(2), now);
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
                    offset: crate::reminders::ReminderOffset::at_due(),
                    created: ts(1),
                    clock: ts(2),
                })].into(),
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
                        offset: crate::reminders::ReminderOffset::at_due(),
                        created_at: ts(1),
                        updated_at: ts(2),
                    },
                );
            }
            let derived = derive_rule_tombstones(&base, &local, ts(clock));
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
                    DismissalEntry::Dismissed { dismissed_at: ts(clock) },
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

        // P2 (design D-4, the fail-closed pin): for an arbitrary local
        // dismissal set — Default- and Adhoc-ref rows included — no
        // local TaskId or AdhocAlarmId identity and no default:/alarm:
        // key form can reach the serialized dismissals wire content.
        #[test]
        fn no_local_ids_reach_the_dismissals_wire(
            task_seed in 0_u128..1_000_000,
            alarm_seed in 0_u128..1_000_000,
            rule_seed in 0_u128..1_000_000,
            occurrence in 0_i64..4_000_000_000,
            with_default_row in proptest::bool::ANY,
            with_adhoc_row in proptest::bool::ANY,
            with_rule_row in proptest::bool::ANY,
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
                    offset: crate::reminders::ReminderOffset::at_due(),
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
            let entry = dismissals_entry_for(&dismissals, &rules, task);
            let wire = serde_json::to_string(&entry).unwrap();
            prop_assert!(
                !wire.contains(&task.as_uuid().to_string()),
                "the local TaskId reached the wire: {wire}"
            );
            prop_assert!(
                !wire.contains(&alarm.as_uuid().to_string()),
                "the local AdhocAlarmId reached the wire: {wire}"
            );
            prop_assert!(!wire.contains("default:"), "{wire}");
            prop_assert!(!wire.contains("alarm:"), "{wire}");
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
                    TimeEntryRecord::Tombstone { clock: ts(start) }
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
                        TimeEntryRecord::Tombstone { clock } => Some(*clock),
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
                derive_time_entry_tombstones(&base, &local, &resolvable, ts(now), ts(clock));
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
            let board = RemoteBoardId(target_board);
            let mut tasks = BTreeMap::new();
            let mut entries_map = BTreeMap::new();
            for (entry_seed, task_seed, card, start) in &entries {
                let task = TaskId::from(uuid::Uuid::from_u128(*task_seed));
                tasks.entry(task).or_insert_with(|| {
                    let mut t = task_fixture();
                    t.id = task;
                    t.remote = card.map(|c| RemoteCardRef {
                        board,
                        stack: RemoteStackId(7),
                        card: RemoteCardId(c),
                    });
                    t
                });
                let entry = tracked(*entry_seed, *start, true, task);
                entries_map.insert(entry.id, entry);
            }
            let emitted = time_entries_for(&entries_map, &tasks, board, ts(now));
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
                    tasks.values().any(|t| t.remote.is_some_and(|r| r.board == board && r.card.get() == card_num)),
                    "the card names a board-bound task"
                );
            }
        }
    }
}
