// SPDX-License-Identifier: MIT OR Apache-2.0
//! The materialized-entity payload schema (backbone §7.2, plan §3):
//! the one uniform wire shape every board entity rides — in chunk
//! entries and in snapshot state alike. An entity is a *materialized
//! state*: fields + clocks + authors per field, **never raw op
//! references** (a late joiner cannot resolve them — carrying one here
//! would be a design finding), never `remote` bindings or
//! `remote_seen` (connector-local bookkeeping; files-board remote
//! identities are the domain UUIDs, D-BF4 — there is nothing to
//! rebind).
//!
//! Decode posture per field — the `UserRecord` author rule applied at
//! field granularity:
//! - `{ "value", "clock", "by"? }`: `by` absent or null decodes as
//!   `None` (tolerant); a *present but unparseable* `by` or a
//!   non-RFC-3339 `clock` demotes the field to the unknown relay —
//!   **fail-closed** (silently de-authoring would change the fold
//!   rank); the value heals via the owner's next snapshot.
//! - Unknown field *names* decode as unknown relays: carried,
//!   republished by snapshots, never folded, never an error.
//! - Unknown entity `kind`s, non-object entities, and entities lacking
//!   `kind`/`id` decode as whole-entity verbatim relays — carried and
//!   republished so a newer peer's entity kind survives an older
//!   reader, never folded.
//! - A malformed membership record demotes the whole entity to the
//!   verbatim relay (documented leniency: rank honesty over partial
//!   modeling; the value heals via the owner's next snapshot).
//!
//! The per-kind field-slot lists are pinned as data
//! ([`BOARD_FIELD_SLOTS`] etc.) — the anti-drift tripwire against the
//! domain's clock families: an emitted entity's slot-name set is
//! asserted equal to the documented list, so a clock-family change
//! that outpaces the payload fails a test, not a peer device.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use taskboard_domain::entities::{
    Board, BoardAuthors, BoardClocks, Label, LabelAuthors, LabelClocks, MembershipState, Stack,
    StackAuthors, StackClocks, Task, TaskAuthors, TaskClocks,
};
use taskboard_domain::ids::{AuthorId, BoardId, LabelId, StackId};
use uuid::Uuid;

/// The modeled entity kinds. A wire kind outside this set relays as a
/// whole-entity verbatim, never an error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EntityKind {
    Board,
    Stack,
    Task,
    Label,
}

/// `board`'s pinned field slots (`BoardClocks` slot-for-slot).
pub const BOARD_FIELD_SLOTS: &[&str] = &["title", "color", "archived", "deleted"];

/// `stack`'s pinned field slots (`StackClocks` slot-for-slot). The
/// non-clocked residual `archived` rides `order`'s clock/author
/// (exactly the reconcile merge's pairing) — its value is the
/// composite `{"order", "archived"}` object.
pub const STACK_FIELD_SLOTS: &[&str] = &["title", "order", "deleted"];

/// `task`'s pinned field slots (`TaskClocks` slot-for-slot).
pub const TASK_FIELD_SLOTS: &[&str] = &[
    "title",
    "description",
    "duedate",
    "scheduled",
    "done",
    "position",
    "effort",
    "importance",
    "urgency",
    "recur",
    "kind",
    "archived",
    "deleted",
];

/// `label`'s pinned field slots (`LabelClocks` slot-for-slot).
pub const LABEL_FIELD_SLOTS: &[&str] = &["title", "color", "deleted"];

/// The slot list of a kind (the pin, as data).
#[must_use]
pub const fn field_slots(kind: EntityKind) -> &'static [&'static str] {
    match kind {
        EntityKind::Board => BOARD_FIELD_SLOTS,
        EntityKind::Stack => STACK_FIELD_SLOTS,
        EntityKind::Task => TASK_FIELD_SLOTS,
        EntityKind::Label => LABEL_FIELD_SLOTS,
    }
}

/// One field's write: the value, the clock that wrote it, and the
/// author who wrote it (`None` = an authorless lane; the fold's
/// `None < Some` leg keeps those rows honest).
#[derive(Debug, Clone, PartialEq)]
pub struct FieldValue {
    pub value: Value,
    pub clock: DateTime<Utc>,
    pub by: Option<AuthorId>,
}

/// One decoded slot of an entity's `fields` map: a modeled
/// clock+authored value, or an unknown relay (a foreign field name, or
/// a modeled name whose envelope failed the fail-closed grammar).
#[derive(Debug, Clone, PartialEq)]
pub enum Field {
    Value(FieldValue),
    /// The verbatim floor: carried, republished by snapshots, never
    /// folded, never errors the entity.
    Unknown(Value),
}

/// Per-membership OR-set wire record (#129's `MembershipState`
/// vocabulary). The domain's membership state carries no author slot —
/// v1 emission writes `by: None`; the slot is the forward seam.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MembershipRecord {
    /// `true` = the membership is live (`Added`), `false` = the
    /// remembered removal tombstone (`Removed`).
    pub added: bool,
    pub clock: DateTime<Utc>,
    pub by: Option<AuthorId>,
}

/// A materialized entity state. `unknown` carries unknown entity-level
/// wire keys verbatim (the relay rule — republished by snapshots).
#[derive(Debug, Clone, PartialEq)]
pub struct MaterializedEntity {
    pub kind: EntityKind,
    /// Plain UUID: unknown-but-valid ids relay; only `to_task`/`to_*`
    /// re-type it into the domain's id families.
    pub id: Uuid,
    pub fields: BTreeMap<String, Field>,
    /// Task only (absent = no memberships ever referenced).
    pub memberships: Option<BTreeMap<String, MembershipRecord>>,
    pub unknown: BTreeMap<String, Value>,
}

/// One entry of a chunk's / snapshot's entity list: a modeled entity,
/// or a whole-entity verbatim relay (the entry-list arm for shapes
/// that cannot relay meaningfully as modeled entities — never folded).
#[derive(Debug, Clone, PartialEq)]
pub enum EntityEntry {
    Modeled(MaterializedEntity),
    Verbatim(Value),
}

impl MaterializedEntity {
    /// Encodes a [`Task`] as a materialized entity. Strips `remote`,
    /// `remote_seen`, and `remote_baseline` at encode time (never raw
    /// op references, never connector bookkeeping).
    ///
    /// # Panics
    /// Never: every slot value serializes (the expects are
    /// total-function pins over plain domain data).
    #[must_use]
    pub fn from_task(task: &Task) -> Self {
        let (clocks, authors) = (&task.clocks, &task.authors);
        let fields = slot_fields([
            (
                "title",
                json(task.title.clone()),
                clocks.title,
                authors.title,
            ),
            (
                "description",
                json(task.description.clone()),
                clocks.description,
                authors.description,
            ),
            (
                "duedate",
                json(task.duedate),
                clocks.duedate,
                authors.duedate,
            ),
            (
                "scheduled",
                json(task.scheduled),
                clocks.scheduled,
                authors.scheduled,
            ),
            ("done", json(task.done), clocks.done, authors.done),
            (
                "position",
                json(PositionWire {
                    stack: task.stack.as_uuid(),
                    order: task.order,
                }),
                clocks.position,
                authors.position,
            ),
            ("effort", json(task.effort), clocks.effort, authors.effort),
            (
                "importance",
                json(task.importance),
                clocks.importance,
                authors.importance,
            ),
            (
                "urgency",
                json(task.urgency),
                clocks.urgency,
                authors.urgency,
            ),
            (
                "recur",
                json(task.recur.clone()),
                clocks.recur,
                authors.recur,
            ),
            ("kind", json(task.kind), clocks.kind, authors.kind),
            (
                "archived",
                json(task.archived),
                clocks.archived,
                authors.archived,
            ),
            (
                "deleted",
                json(task.deleted),
                clocks.deleted,
                authors.deleted,
            ),
        ]);
        Self {
            kind: EntityKind::Task,
            id: task.id.as_uuid(),
            memberships: membership_records(&task.label_memberships),
            fields,
            unknown: BTreeMap::new(),
        }
    }

    /// Encodes a [`Stack`]. The non-clocked residual `archived` rides
    /// `order`'s clock/author: its wire value is the composite
    /// `{"order", "archived"}` object.
    ///
    /// # Panics
    /// Never: every slot value serializes.
    #[must_use]
    pub fn from_stack(stack: &Stack) -> Self {
        let (clocks, authors) = (&stack.clocks, &stack.authors);
        let fields = slot_fields([
            (
                "title",
                json(stack.title.clone()),
                clocks.title,
                authors.title,
            ),
            (
                "order",
                json(OrderWire {
                    order: stack.order,
                    archived: stack.archived,
                }),
                clocks.order,
                authors.order,
            ),
            (
                "deleted",
                json(stack.deleted),
                clocks.deleted,
                authors.deleted,
            ),
        ]);
        Self {
            kind: EntityKind::Stack,
            id: stack.id.as_uuid(),
            fields,
            memberships: None,
            unknown: BTreeMap::new(),
        }
    }

    /// Encodes a [`Label`].
    ///
    /// # Panics
    /// Never: every slot value serializes.
    #[must_use]
    pub fn from_label(label: &Label) -> Self {
        let fields = slot_fields([
            (
                "title",
                json(label.title.clone()),
                label.clocks.title,
                label.authors.title,
            ),
            (
                "color",
                json(label.color.clone()),
                label.clocks.color,
                label.authors.color,
            ),
            (
                "deleted",
                json(label.deleted),
                label.clocks.deleted,
                label.authors.deleted,
            ),
        ]);
        Self {
            kind: EntityKind::Label,
            id: label.id.as_uuid(),
            fields,
            memberships: None,
            unknown: BTreeMap::new(),
        }
    }

    /// Encodes a [`Board`].
    ///
    /// # Panics
    /// Never: every slot value serializes.
    #[must_use]
    pub fn from_board(board: &Board) -> Self {
        let fields = slot_fields([
            (
                "title",
                json(board.title.clone()),
                board.clocks.title,
                board.authors.title,
            ),
            (
                "color",
                json(board.color.clone()),
                board.clocks.color,
                board.authors.color,
            ),
            (
                "archived",
                json(board.archived),
                board.clocks.archived,
                board.authors.archived,
            ),
            (
                "deleted",
                json(board.deleted),
                board.clocks.deleted,
                board.authors.deleted,
            ),
        ]);
        Self {
            kind: EntityKind::Board,
            id: board.id.as_uuid(),
            fields,
            memberships: None,
            unknown: BTreeMap::new(),
        }
    }

    /// The entity's raw wire form (used by the whole-entity relay
    /// tests and the fold's content leg).
    ///
    /// # Panics
    /// Never: an in-memory entity always serializes (total-function pin).
    #[must_use]
    pub fn to_wire_value(&self) -> Value {
        serde_json::to_value(self).expect("a modeled entity always serializes")
    }

    /// Decodes back into a [`Task`] (`None` for foreign kinds or
    /// entities whose modeled slots failed the strict value decode —
    /// shape drift heals via the owner's next snapshot, never a
    /// panic). `remote`/`remote_seen`/`remote_baseline` decode as
    /// `None`: the payload carries none of them.
    #[must_use]
    pub fn to_task(&self) -> Option<Task> {
        if self.kind != EntityKind::Task {
            return None;
        }
        let slot = |name: &str| decode_slot(&self.fields, name).cloned();
        let position: PositionWire = serde_json::from_value(slot("position")?).ok()?;
        Some(Task {
            id: taskboard_domain::ids::TaskId::from_uuid(self.id),
            remote: None,
            title: serde_json::from_value(slot("title")?).ok()?,
            description: serde_json::from_value(slot("description")?).ok()?,
            duedate: serde_json::from_value(slot("duedate")?).ok()?,
            scheduled: serde_json::from_value(slot("scheduled")?).ok()?,
            done: serde_json::from_value(slot("done")?).ok()?,
            stack: StackId::from_uuid(position.stack),
            order: position.order,
            label_memberships: self.decode_memberships(),
            effort: serde_json::from_value(slot("effort")?).ok()?,
            importance: serde_json::from_value(slot("importance")?).ok()?,
            urgency: serde_json::from_value(slot("urgency")?).ok()?,
            recur: serde_json::from_value(slot("recur")?).ok()?,
            kind: serde_json::from_value(slot("kind")?).ok()?,
            archived: serde_json::from_value(slot("archived")?).ok()?,
            deleted: serde_json::from_value(slot("deleted")?).ok()?,
            clocks: TaskClocks {
                title: slot_clock(&self.fields, "title")?,
                description: slot_clock(&self.fields, "description")?,
                duedate: slot_clock(&self.fields, "duedate")?,
                scheduled: slot_clock(&self.fields, "scheduled")?,
                done: slot_clock(&self.fields, "done")?,
                position: slot_clock(&self.fields, "position")?,
                effort: slot_clock(&self.fields, "effort")?,
                importance: slot_clock(&self.fields, "importance")?,
                urgency: slot_clock(&self.fields, "urgency")?,
                recur: slot_clock(&self.fields, "recur")?,
                kind: slot_clock(&self.fields, "kind")?,
                archived: slot_clock(&self.fields, "archived")?,
                deleted: slot_clock(&self.fields, "deleted")?,
            },
            authors: TaskAuthors {
                title: slot_author(&self.fields, "title"),
                description: slot_author(&self.fields, "description"),
                duedate: slot_author(&self.fields, "duedate"),
                scheduled: slot_author(&self.fields, "scheduled"),
                done: slot_author(&self.fields, "done"),
                position: slot_author(&self.fields, "position"),
                effort: slot_author(&self.fields, "effort"),
                importance: slot_author(&self.fields, "importance"),
                urgency: slot_author(&self.fields, "urgency"),
                recur: slot_author(&self.fields, "recur"),
                kind: slot_author(&self.fields, "kind"),
                archived: slot_author(&self.fields, "archived"),
                deleted: slot_author(&self.fields, "deleted"),
            },
            remote_seen: None,
            remote_baseline: None,
        })
    }

    /// Decodes back into a [`Stack`] owned by `board` (the payload
    /// carries no board context — the subtree path owns it).
    #[must_use]
    pub fn to_stack(&self, board: BoardId) -> Option<Stack> {
        if self.kind != EntityKind::Stack {
            return None;
        }
        let slot = |name: &str| decode_slot(&self.fields, name).cloned();
        let order: OrderWire = serde_json::from_value(slot("order")?).ok()?;
        Some(Stack {
            id: taskboard_domain::ids::StackId::from_uuid(self.id),
            remote: None,
            board,
            title: serde_json::from_value(slot("title")?).ok()?,
            order: order.order,
            archived: order.archived,
            deleted: serde_json::from_value(slot("deleted")?).ok()?,
            clocks: StackClocks {
                title: slot_clock(&self.fields, "title")?,
                order: slot_clock(&self.fields, "order")?,
                deleted: slot_clock(&self.fields, "deleted")?,
            },
            authors: StackAuthors {
                title: slot_author(&self.fields, "title"),
                order: slot_author(&self.fields, "order"),
                deleted: slot_author(&self.fields, "deleted"),
            },
            remote_seen: None,
        })
    }

    /// Decodes back into a [`Label`] owned by `board`.
    #[must_use]
    pub fn to_label(&self, board: BoardId) -> Option<Label> {
        if self.kind != EntityKind::Label {
            return None;
        }
        let slot = |name: &str| decode_slot(&self.fields, name).cloned();
        Some(Label {
            id: taskboard_domain::ids::LabelId::from_uuid(self.id),
            remote: None,
            board,
            title: serde_json::from_value(slot("title")?).ok()?,
            color: serde_json::from_value(slot("color")?).ok()?,
            deleted: serde_json::from_value(slot("deleted")?).ok()?,
            clocks: LabelClocks {
                title: slot_clock(&self.fields, "title")?,
                color: slot_clock(&self.fields, "color")?,
                deleted: slot_clock(&self.fields, "deleted")?,
            },
            authors: LabelAuthors {
                title: slot_author(&self.fields, "title"),
                color: slot_author(&self.fields, "color"),
                deleted: slot_author(&self.fields, "deleted"),
            },
            remote_seen: None,
        })
    }

    /// Decodes back into a [`Board`].
    #[must_use]
    pub fn to_board(&self) -> Option<Board> {
        if self.kind != EntityKind::Board {
            return None;
        }
        let slot = |name: &str| decode_slot(&self.fields, name).cloned();
        Some(Board {
            id: BoardId::from_uuid(self.id),
            remote: None,
            title: serde_json::from_value(slot("title")?).ok()?,
            color: serde_json::from_value(slot("color")?).ok()?,
            archived: serde_json::from_value(slot("archived")?).ok()?,
            deleted: serde_json::from_value(slot("deleted")?).ok()?,
            clocks: BoardClocks {
                title: slot_clock(&self.fields, "title")?,
                color: slot_clock(&self.fields, "color")?,
                archived: slot_clock(&self.fields, "archived")?,
                deleted: slot_clock(&self.fields, "deleted")?,
            },
            authors: BoardAuthors {
                title: slot_author(&self.fields, "title"),
                color: slot_author(&self.fields, "color"),
                archived: slot_author(&self.fields, "archived"),
                deleted: slot_author(&self.fields, "deleted"),
            },
            remote_seen: None,
        })
    }

    /// The membership map back into the domain's OR-set map.
    /// Non-UUID keys relay in the payload but cannot map to the
    /// domain — they are skipped (still republished by snapshots).
    fn decode_memberships(&self) -> BTreeMap<LabelId, MembershipState> {
        let mut out = BTreeMap::new();
        for (key, record) in self.memberships.iter().flat_map(|m| m.iter()) {
            let Ok(id) = Uuid::parse_str(key) else {
                continue;
            };
            out.insert(
                LabelId::from_uuid(id),
                if record.added {
                    MembershipState::Added { at: record.clock }
                } else {
                    MembershipState::Removed { at: record.clock }
                },
            );
        }
        out
    }
}

/// Builds a slot map from `(name, value, clock, author)` tuples —
/// the one emission shape every `from_*` encoder rides.
fn slot_fields(
    slots: impl IntoIterator<Item = (&'static str, Value, DateTime<Utc>, Option<AuthorId>)>,
) -> BTreeMap<String, Field> {
    slots
        .into_iter()
        .map(|(name, value, clock, by)| {
            (
                name.to_owned(),
                Field::Value(FieldValue { value, clock, by }),
            )
        })
        .collect()
}

/// The domain OR-set map as wire membership records. The domain's
/// membership state carries no author slot: `None` is the v1 emission
/// and the fold's authorless floor.
fn membership_records(
    memberships: &BTreeMap<LabelId, MembershipState>,
) -> Option<BTreeMap<String, MembershipRecord>> {
    if memberships.is_empty() {
        return None;
    }
    Some(
        memberships
            .iter()
            .map(|(id, state)| {
                (
                    id.as_uuid().to_string(),
                    MembershipRecord {
                        added: matches!(state, MembershipState::Added { .. }),
                        clock: state.at(),
                        by: None,
                    },
                )
            })
            .collect(),
    )
}

fn json<T: Serialize>(value: T) -> Value {
    serde_json::to_value(value).expect("a domain slot value always serializes")
}

/// The task `position` slot's composite wire value (one field: one
/// clock, one author — the domain's composite-position rule).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct PositionWire {
    stack: Uuid,
    order: i64,
}

/// The stack `order` slot's composite wire value: the residual
/// `archived` flag rides the order clock/author (the reconcile
/// merge's pairing).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct OrderWire {
    order: i64,
    archived: bool,
}

fn decode_slot<'a>(fields: &'a BTreeMap<String, Field>, name: &str) -> Option<&'a Value> {
    match fields.get(name)? {
        Field::Value(fv) => Some(&fv.value),
        Field::Unknown(_) => None,
    }
}

fn slot_clock(fields: &BTreeMap<String, Field>, name: &str) -> Option<DateTime<Utc>> {
    match fields.get(name)? {
        Field::Value(fv) => Some(fv.clock),
        Field::Unknown(_) => None,
    }
}

fn slot_author(fields: &BTreeMap<String, Field>, name: &str) -> Option<AuthorId> {
    match fields.get(name)? {
        Field::Value(fv) => fv.by,
        Field::Unknown(_) => None,
    }
}

impl Serialize for Field {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            // Emission writes `by` only when `Some` (v1-clean).
            Field::Value(fv) => {
                let mut map = serde_json::Map::new();
                map.insert("value".to_owned(), fv.value.clone());
                map.insert(
                    "clock".to_owned(),
                    serde_json::to_value(fv.clock).map_err(serde::ser::Error::custom)?,
                );
                if let Some(by) = fv.by {
                    map.insert(
                        "by".to_owned(),
                        serde_json::to_value(by).map_err(serde::ser::Error::custom)?,
                    );
                }
                Value::Object(map).serialize(serializer)
            }
            Field::Unknown(value) => value.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for Field {
    /// Total over any wire shape: everything that fails the modeled
    /// grammar (including the fail-closed author demotion) lands in
    /// [`Field::Unknown`], never an error, never a panic.
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = Value::deserialize(deserializer)?;
        let Some(map) = raw.as_object() else {
            return Ok(Self::Unknown(raw));
        };
        let (Some(value), Some(clock)) = (map.get("value"), map.get("clock")) else {
            return Ok(Self::Unknown(raw));
        };
        let Ok(clock) = serde_json::from_value::<DateTime<Utc>>(clock.clone()) else {
            return Ok(Self::Unknown(raw));
        };
        // A present-but-unparseable `by` demotes the whole field:
        // fail-closed (silently de-authoring would change the fold rank).
        let value = value.clone();
        match map.get("by") {
            None | Some(Value::Null) => Ok(Self::Value(FieldValue {
                value,
                clock,
                by: None,
            })),
            Some(raw_by) => match serde_json::from_value::<AuthorId>(raw_by.clone()) {
                Ok(by) => Ok(Self::Value(FieldValue {
                    value,
                    clock,
                    by: Some(by),
                })),
                Err(_) => Ok(Self::Unknown(raw)),
            },
        }
    }
}

impl Serialize for MaterializedEntity {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serde_json::Map::new();
        map.insert(
            "kind".to_owned(),
            serde_json::to_value(self.kind).map_err(serde::ser::Error::custom)?,
        );
        map.insert(
            "id".to_owned(),
            serde_json::to_value(self.id).map_err(serde::ser::Error::custom)?,
        );
        map.insert(
            "fields".to_owned(),
            serde_json::to_value(&self.fields).map_err(serde::ser::Error::custom)?,
        );
        if let Some(memberships) = &self.memberships {
            map.insert(
                "memberships".to_owned(),
                serde_json::to_value(memberships).map_err(serde::ser::Error::custom)?,
            );
        }
        for (key, value) in &self.unknown {
            map.insert(key.clone(), value.clone());
        }
        Value::Object(map).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for MaterializedEntity {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = Value::deserialize(deserializer)?;
        let map = raw
            .as_object()
            .ok_or_else(|| serde::de::Error::custom("an entity must be an object"))?;
        let kind: EntityKind = serde_json::from_value(
            map.get("kind")
                .ok_or_else(|| serde::de::Error::missing_field("kind"))?
                .clone(),
        )
        .map_err(|_| serde::de::Error::custom("an unknown entity kind"))?;
        let id: Uuid = serde_json::from_value(
            map.get("id")
                .ok_or_else(|| serde::de::Error::missing_field("id"))?
                .clone(),
        )
        .map_err(|_| serde::de::Error::custom("an entity id must be a UUID"))?;
        let mut fields = BTreeMap::new();
        if let Some(raw_fields) = map.get("fields").and_then(Value::as_object) {
            for (name, value) in raw_fields {
                // `Field`'s decode is total over any wire shape, so the
                // inner call cannot fail.
                fields.insert(
                    name.clone(),
                    Field::deserialize(value.clone()).expect("Field decode is total"),
                );
            }
        }
        let memberships = match map.get("memberships") {
            None | Some(Value::Null) => None,
            Some(Value::Object(raw_memberships)) => {
                let mut decoded = BTreeMap::new();
                for (key, value) in raw_memberships {
                    let record: MembershipWire = serde_json::from_value(value.clone())
                        .map_err(|_| serde::de::Error::custom("a malformed membership record"))?;
                    decoded.insert(key.clone(), record.into_record());
                }
                Some(decoded)
            }
            Some(_) => {
                return Err(serde::de::Error::custom("a malformed membership record"));
            }
        };
        let mut unknown = BTreeMap::new();
        for (key, value) in map {
            if !matches!(key.as_str(), "kind" | "id" | "fields" | "memberships") {
                unknown.insert(key.clone(), value.clone());
            }
        }
        Ok(Self {
            kind,
            id,
            fields,
            memberships,
            unknown,
        })
    }
}

/// The membership record's wire grammar (`{ "added", "clock", "by"? }`).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
struct MembershipWire {
    added: bool,
    clock: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    by: Option<AuthorId>,
}

impl MembershipWire {
    const fn into_record(self) -> MembershipRecord {
        MembershipRecord {
            added: self.added,
            clock: self.clock,
            by: self.by,
        }
    }
}

impl Serialize for MembershipRecord {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        MembershipWire {
            added: self.added,
            clock: self.clock,
            by: self.by,
        }
        .serialize(serializer)
    }
}

impl Serialize for EntityEntry {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Modeled(entity) => entity.serialize(serializer),
            Self::Verbatim(value) => value.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for EntityEntry {
    /// Lenient per §3.2: garbage entity shapes relay as
    /// [`EntityEntry::Verbatim`], never kill the list.
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = Value::deserialize(deserializer)?;
        Ok(
            if let Ok(entity) = MaterializedEntity::deserialize(raw.clone()) {
                Self::Modeled(entity)
            } else {
                Self::Verbatim(raw)
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use serde_json::json;
    use taskboard_domain::entities::{Effort, Importance, MembershipState, TaskKind};
    use taskboard_domain::ids::{AuthorId, LabelId, StackId, TaskId};
    use taskboard_domain::test_support::{proptest_config, task_strategy};

    fn test_clock() -> chrono::DateTime<chrono::Utc> {
        "2026-11-08T09:15:31Z".parse().expect("rfc3339")
    }

    fn author() -> AuthorId {
        AuthorId::parse("0193a1b2-c3d4-7cc9-8a2e-5f1e2d3c4b5a").expect("parses")
    }

    fn task_fixture(id: u128, title: &str, clock: DateTime<Utc>, author: Option<AuthorId>) -> Task {
        let t = clock;
        Task {
            id: TaskId::from_uuid(Uuid::from_u128(id)),
            remote: None,
            title: title.to_owned(),
            description: String::new(),
            duedate: None,
            scheduled: None,
            done: None,
            stack: StackId::from_uuid(Uuid::from_u128(id + 1)),
            order: 3,
            label_memberships: BTreeMap::from([(
                LabelId::from_uuid(Uuid::from_u128(id + 2)),
                MembershipState::Added { at: t },
            )]),
            effort: Some(Effort::new(900)),
            importance: Some(Importance::High),
            urgency: None,
            recur: None,
            kind: TaskKind::Standard,
            archived: false,
            deleted: false,
            clocks: TaskClocks {
                title: t,
                description: t,
                duedate: t,
                scheduled: t,
                done: t,
                position: t,
                effort: t,
                importance: t,
                urgency: t,
                recur: t,
                kind: t,
                archived: t,
                deleted: t,
            },
            authors: TaskAuthors {
                title: author,
                ..TaskAuthors::default()
            },
            remote_seen: None,
            remote_baseline: None,
        }
    }

    #[test]
    fn the_encoded_field_slots_match_the_pinned_tables() {
        // The anti-drift tripwire: every emitted slot-name set equals
        // the documented list for its kind.
        let t = test_clock();
        let task = task_fixture(1, "ship", t, None);
        let encoded = MaterializedEntity::from_task(&task).to_wire_value();
        let names: Vec<&str> = encoded["fields"]
            .as_object()
            .expect("object")
            .keys()
            .map(String::as_str)
            .collect();
        // The wire map is sorted; the pinned tables compare as sets.
        let mut sorted = TASK_FIELD_SLOTS.to_vec();
        sorted.sort_unstable();
        assert_eq!(names, sorted);
        let stack = Stack {
            id: taskboard_domain::ids::StackId::from_uuid(Uuid::from_u128(9)),
            remote: None,
            board: taskboard_domain::ids::BoardId::from_uuid(Uuid::from_u128(8)),
            title: "todo".into(),
            order: 1,
            archived: false,
            deleted: false,
            clocks: StackClocks {
                title: t,
                order: t,
                deleted: t,
            },
            authors: StackAuthors::default(),
            remote_seen: None,
        };
        let encoded = MaterializedEntity::from_stack(&stack).to_wire_value();
        let names: Vec<&str> = encoded["fields"]
            .as_object()
            .expect("object")
            .keys()
            .map(String::as_str)
            .collect();
        let mut sorted = STACK_FIELD_SLOTS.to_vec();
        sorted.sort_unstable();
        assert_eq!(names, sorted);
        let label = Label {
            id: taskboard_domain::ids::LabelId::from_uuid(Uuid::from_u128(10)),
            remote: None,
            board: taskboard_domain::ids::BoardId::from_uuid(Uuid::from_u128(8)),
            title: "urgent".into(),
            color: taskboard_domain::entities::Color::new("#ff0000"),
            deleted: false,
            clocks: LabelClocks {
                title: t,
                color: t,
                deleted: t,
            },
            authors: LabelAuthors::default(),
            remote_seen: None,
        };
        let encoded = MaterializedEntity::from_label(&label).to_wire_value();
        let names: Vec<&str> = encoded["fields"]
            .as_object()
            .expect("object")
            .keys()
            .map(String::as_str)
            .collect();
        let mut sorted = LABEL_FIELD_SLOTS.to_vec();
        sorted.sort_unstable();
        assert_eq!(names, sorted);
        let board = Board {
            id: taskboard_domain::ids::BoardId::from_uuid(Uuid::from_u128(8)),
            remote: None,
            title: "home".into(),
            color: taskboard_domain::entities::Color::new("blue"),
            archived: false,
            deleted: false,
            clocks: BoardClocks {
                title: t,
                color: t,
                archived: t,
                deleted: t,
            },
            authors: BoardAuthors::default(),
            remote_seen: None,
        };
        let encoded = MaterializedEntity::from_board(&board).to_wire_value();
        let names: Vec<&str> = encoded["fields"]
            .as_object()
            .expect("object")
            .keys()
            .map(String::as_str)
            .collect();
        let mut sorted = BOARD_FIELD_SLOTS.to_vec();
        sorted.sort_unstable();
        assert_eq!(names, sorted);
    }

    /// The v1-clean emission pins: `by` present iff `Some`, the
    /// composite position is one field, and no connector bookkeeping
    /// key ever rides the payload.
    #[test]
    fn emission_is_v1_clean() {
        let author = author();
        let task = task_fixture(1, "ship", test_clock(), Some(author));
        let wire = MaterializedEntity::from_task(&task).to_wire_value();
        assert_eq!(wire["kind"], json!("task"));
        assert_eq!(wire["id"], json!(Uuid::from_u128(1).to_string()));
        let position = &wire["fields"]["position"];
        assert_eq!(
            position["value"]["stack"],
            json!(task.stack.as_uuid().to_string())
        );
        assert_eq!(position["value"]["order"], json!(3));
        let title = &wire["fields"]["title"];
        assert_eq!(title["by"], json!(author.to_string()));
        assert_eq!(title["value"], json!("ship"));
        let unauthored = task_fixture(2, "quiet", test_clock(), None);
        let wire = MaterializedEntity::from_task(&unauthored).to_wire_value();
        assert!(
            wire["fields"]["title"].get("by").is_none(),
            "`by` is written only when Some"
        );
        for banned in ["remote", "remote_seen", "remote_baseline", "ops"] {
            assert!(wire.get(banned).is_none(), "{banned} never rides a payload");
        }
    }

    #[test]
    fn a_task_roundtrips_through_the_payload() {
        let author = author();
        let original = task_fixture(1, "ship", test_clock(), Some(author));
        let decoded = MaterializedEntity::from_task(&original)
            .to_task()
            .expect("decodes");
        assert_eq!(decoded, original);
    }

    #[test]
    fn memberships_roundtrip_and_removed_states_survive() {
        let t = test_clock();
        let mut task = task_fixture(1, "ship", t, None);
        task.label_memberships.insert(
            LabelId::from_uuid(Uuid::from_u128(99)),
            MembershipState::Removed { at: t },
        );
        let decoded = MaterializedEntity::from_task(&task)
            .to_task()
            .expect("decodes");
        assert_eq!(decoded.label_memberships, task.label_memberships);
        let wire = MaterializedEntity::from_task(&task).to_wire_value();
        let removed = &wire["memberships"][&Uuid::from_u128(99).to_string()];
        assert_eq!(removed["added"], json!(false));
    }

    #[test]
    fn an_absent_membership_map_is_none_never_empty() {
        let t = test_clock();
        let mut task = task_fixture(1, "ship", t, None);
        task.label_memberships.clear();
        let entity = MaterializedEntity::from_task(&task);
        assert!(entity.memberships.is_none());
        let wire = entity.to_wire_value();
        assert!(
            wire.get("memberships").is_none(),
            "absent = never referenced"
        );
    }

    /// The fail-closed author rule: a present-but-unparseable `by`
    /// demotes the *field* to the unknown relay — the rest of the
    /// entity stays modeled, and the demoted field never folds.
    #[test]
    fn an_unparseable_by_demotes_the_field_fail_closed() {
        let raw = json!({
            "kind": "task",
            "id": Uuid::from_u128(1).to_string(),
            "fields": {
                "title": { "value": "ship", "clock": "2026-11-08T09:15:31Z", "by": "not-a-uuid" },
                "deleted": { "value": false, "clock": "2026-11-08T09:15:31Z" }
            }
        });
        let entity: MaterializedEntity = serde_json::from_value(raw).expect("decodes");
        assert!(matches!(entity.fields["title"], Field::Unknown(_)));
        assert!(matches!(entity.fields["deleted"], Field::Value(_)));
        assert!(
            entity.to_task().is_none(),
            "a demoted modeled slot blocks the entity decode"
        );
    }

    #[test]
    fn a_non_rfc3339_clock_demotes_the_field_fail_closed() {
        let raw = json!({
            "kind": "task",
            "id": Uuid::from_u128(1).to_string(),
            "fields": {
                "title": { "value": "ship", "clock": "yesterday-ish", "by": null }
            }
        });
        let entity: MaterializedEntity = serde_json::from_value(raw).expect("decodes");
        assert!(matches!(entity.fields["title"], Field::Unknown(_)));
    }

    #[test]
    fn unknown_field_names_relay_never_fold() {
        let raw = json!({
            "kind": "task",
            "id": Uuid::from_u128(1).to_string(),
            "fields": {
                "title": { "value": "x", "clock": "2026-11-08T09:15:31Z" },
                "quantum_bias": { "spin": 1 }
            }
        });
        let entity: MaterializedEntity = serde_json::from_value(raw).expect("decodes");
        assert!(matches!(entity.fields["quantum_bias"], Field::Unknown(_)));
        // And the relay republishes verbatim.
        let wire = entity.to_wire_value();
        assert_eq!(wire["fields"]["quantum_bias"], json!({ "spin": 1 }));
    }

    #[test]
    fn unknown_entity_level_keys_relay() {
        let raw = json!({
            "kind": "task",
            "id": Uuid::from_u128(1).to_string(),
            "fields": {},
            "future_key": { "deep": [1, 2] }
        });
        let entity: MaterializedEntity = serde_json::from_value(raw).expect("decodes");
        assert_eq!(entity.unknown["future_key"], json!({ "deep": [1, 2] }));
        assert_eq!(
            entity.to_wire_value()["future_key"],
            json!({ "deep": [1, 2] })
        );
    }

    #[test]
    fn unrelayable_entity_shapes_become_verbatim_entries() {
        let entries: Vec<EntityEntry> = serde_json::from_value(json!([
            "just a string",
            42,
            { "no": "kind" },
            { "kind": "quantum_entangled_card", "id": Uuid::from_u128(2).to_string() },
            { "kind": "task", "id": "not-a-uuid" },
            {
                "kind": "task",
                "id": Uuid::from_u128(3).to_string(),
                "fields": { "title": { "value": "ok", "clock": "2026-11-08T09:15:31Z" } }
            }
        ]))
        .expect("the whole list always decodes");
        assert_eq!(entries.len(), 6, "garbage relays, never kills the chunk");
        for entry in &entries[..5] {
            assert!(matches!(entry, EntityEntry::Verbatim(_)));
        }
        let EntityEntry::Modeled(modeled) = &entries[5] else {
            panic!("the well-formed entity models");
        };
        assert_eq!(modeled.kind, EntityKind::Task);
    }

    #[test]
    fn a_malformed_membership_demotes_the_whole_entity_fail_closed() {
        let raw = json!({
            "kind": "task",
            "id": Uuid::from_u128(1).to_string(),
            "fields": { "title": { "value": "x", "clock": "2026-11-08T09:15:31Z" } },
            "memberships": { "key": { "added": "definitely" } }
        });
        let entry: EntityEntry = serde_json::from_value(raw).expect("the entry relays");
        assert!(
            matches!(entry, EntityEntry::Verbatim(_)),
            "a malformed membership rides the whole-entity relay"
        );
    }

    proptest! {
        #![proptest_config(proptest_config(256))]

        /// P7 (codec roundtrip leg): every generated task survives
        /// encode → decode as itself — remote bookkeeping stripped
        /// symmetrically on both sides.
        #[test]
        fn generated_tasks_roundtrip_through_the_payload(mut original in task_strategy()) {
            original.remote = None;
            original.remote_seen = None;
            original.remote_baseline = None;
            let decoded = MaterializedEntity::from_task(&original)
                .to_task()
                .expect("decodes");
            prop_assert_eq!(decoded, original);
        }

        /// P8 (fuzz never-panic leg, field granularity): arbitrary JSON
        /// through the entity decode always yields a typed outcome.
        #[test]
        fn arbitrary_entity_values_never_panic(raw in crate::test_support::json_value()) {
            let entry: EntityEntry = serde_json::from_value(raw).expect("the entry relays");
            let _ = matches!(entry, EntityEntry::Modeled(_));
        }

        /// Arbitrary `by`/`clock` junk through the field grammar always
        /// decodes to some typed `Field`.
        #[test]
        fn arbitrary_field_values_never_panic(
            value in crate::test_support::json_value(),
            by in crate::test_support::json_value(),
            raw_clock in "[a-zA-Z0-9:+T-]{0,40}",
        ) {
            let raw = json!({ "value": value, "clock": raw_clock, "by": by });
            let field: Field = serde_json::from_value(raw).expect("Field decode is total");
            let _ = matches!(field, Field::Value(_) | Field::Unknown(_));
        }
    }
}
