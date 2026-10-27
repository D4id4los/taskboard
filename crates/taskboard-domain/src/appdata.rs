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
use crate::reminders::ReminderOffset;

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
    /// `{ "offset": <seconds ≤ 0>, "created": rfc3339, "clock": rfc3339 }`.
    Rule {
        offset: ReminderOffset,
        created: DateTime<Utc>,
        clock: DateTime<Utc>,
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
        if let (Ok(offset), Ok(created), Ok(clock)) = (
            decode_offset(map.get("offset")),
            serde_json::from_value(map.get("created").cloned().unwrap_or(Value::Null)),
            serde_json::from_value(map.get("clock").cloned().unwrap_or(Value::Null)),
        ) {
            return Self::Rule {
                offset,
                created,
                clock,
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
            } => serde_json::json!({
                "offset": offset.as_duration().num_seconds(),
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
/// truth (chunk 4's D11 overlay assembly): the three always-app keys as
/// `{v, clock}` full-send writes out of the task's values + `TaskClocks`
/// slots, and `scheduled` only while `!native_scheduled` (the D-V2
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::TASK_FIELDS;
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
            labels: std::collections::BTreeSet::default(),
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
                labels: t(1_000),
                effort: t(1_200),
                importance: t(1_300),
                urgency: t(1_400),
                recur: t(1_400),

                archived: t(1_000),
                deleted: t(1_000),
            },
            remote_seen: None,
        }
    }

    #[test]
    fn task_entry_builds_full_send_writes_from_values_and_clocks() {
        let task = task_fixture();
        let entry = task_entry_for(&task, false);
        assert_eq!(entry.fields.len(), 4);
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
        // Native ownership: the scheduled key is not emitted.
        let native = task_entry_for(&task, true);
        assert_eq!(native.fields.len(), 3);
        assert!(!native.fields.contains_key("scheduled"));
    }

    #[test]
    fn rules_entry_builds_live_entries_keyed_by_uuid() {
        let task = task_fixture();
        let mut rules = std::collections::BTreeMap::new();
        let rule = crate::reminders::ReminderRule {
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
    }
}
