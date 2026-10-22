// SPDX-License-Identifier: MIT OR Apache-2.0
//! Canonical entities: [`Board`], [`Stack`], [`Task`], [`Label`].
//!
//! Conflict model: each editable field of an entity carries a
//! `DateTime<Utc>` write clock (per-field last-writer-wins). Local edits
//! stamp the touched fields only; adopting a remote version stamps every
//! field with the remote entity-level `lastModified`. Position
//! (`stack` + `order`) shares one clock — a move is a single user intent.
//! Tombstones are `deleted: bool` plus the `deleted` field clock.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt::Write as _;

use crate::ids::{
    BoardId, LabelId, RemoteBoardBinding, RemoteCardRef, RemoteLabelRef, RemoteStackRef, StackId,
    TaskId,
};

/// Lenient color value: construction passes anything through and `Display`
/// round-trips. Validation is the sync adapter's job (`DeckColor`); the
/// domain only needs the value to survive storage and echo adoption.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Color(String);

impl Color {
    /// Wraps any wire string (lenient by design).
    #[must_use]
    pub fn new(raw: impl Into<String>) -> Self {
        Self(raw.into())
    }

    /// The raw string form.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Color {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Estimated effort in whole seconds; `None` on a [`Task`] = unestimated.
/// Unsigned by construction (negative effort is unrepresentable); the
/// storage codec rejects values beyond `i64::MAX` at its boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Effort(u64);

impl Effort {
    /// Wraps a whole-seconds value.
    #[must_use]
    pub const fn new(seconds: u64) -> Self {
        Self(seconds)
    }

    /// Whole seconds.
    #[must_use]
    pub const fn seconds(self) -> u64 {
        self.0
    }

    /// Parses the canonical text form: `<n><s|m|h>` components
    /// concatenated, or a string of bare digits = minutes (`30`, `1h30m`,
    /// `45s` are valid; `1h30` — a unit-less component after a
    /// unit-carrying one — is not). No signs, floats, whitespace, or
    /// empty input; overflow at any step is `None` (never a saturation —
    /// a wrapped estimate would be a lie).
    #[must_use]
    pub fn parse_text_form(raw: &str) -> Option<Self> {
        if raw.is_empty() {
            return None;
        }
        let mut rest = raw;
        let mut total: u64 = 0;
        let mut used_unit = false;
        while !rest.is_empty() {
            let digits_end = rest
                .find(|c: char| !c.is_ascii_digit())
                .unwrap_or(rest.len());
            let (digits, tail) = rest.split_at(digits_end);
            if digits.is_empty() {
                return None;
            }
            let value: u64 = digits.parse().ok()?;
            match tail.as_bytes().first() {
                // Bare digits: minutes, only when no unit has been seen
                // anywhere (`1h30` is unambiguously invalid).
                None => {
                    if used_unit {
                        return None;
                    }
                    total = total.checked_add(value.checked_mul(60)?)?;
                    rest = "";
                }
                Some(b's') => {
                    total = total.checked_add(value)?;
                    used_unit = true;
                    rest = &tail[1..];
                }
                Some(b'm') => {
                    total = total.checked_add(value.checked_mul(60)?)?;
                    used_unit = true;
                    rest = &tail[1..];
                }
                Some(b'h') => {
                    total = total.checked_add(value.checked_mul(3600)?)?;
                    used_unit = true;
                    rest = &tail[1..];
                }
                Some(_) => return None,
            }
        }
        Some(Self(total))
    }

    /// The canonical text form: descending `h`/`m`/`s`, zero components
    /// omitted, all-zero renders `0s`.
    #[must_use]
    pub fn text_form(self) -> String {
        let hours = self.0 / 3600;
        let minutes = (self.0 % 3600) / 60;
        let seconds = self.0 % 60;
        let mut out = String::new();
        if hours > 0 {
            let _ = write!(out, "{hours}h");
        }
        if minutes > 0 {
            let _ = write!(out, "{minutes}m");
        }
        if seconds > 0 {
            let _ = write!(out, "{seconds}s");
        }
        if out.is_empty() {
            return "0s".to_owned();
        }
        out
    }
}

/// The two level enums' shared text vocabulary: index = declaration
/// (severity) order. One vocabulary for CLI parse, CLI render, JSON
/// row, and UI labels; the enums stay distinct types (no shared trait).
const LEVEL_TEXT_FORMS: [&str; 5] = ["very-low", "low", "normal", "high", "very-high"];

/// The vocabulary index of a canonical level name.
fn parse_level_text_form(raw: &str) -> Option<usize> {
    LEVEL_TEXT_FORMS.iter().position(|form| *form == raw)
}

/// The canonical level name at a declaration-order index.
const fn level_text_form(index: usize) -> &'static str {
    LEVEL_TEXT_FORMS[index]
}

/// Importance level for later decision support; `None` on a [`Task`] =
/// unset (a task outside the matrix must not read as "Normal").
/// Declaration order = severity order: the derived `Ord` is the sorting
/// contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Importance {
    VeryLow,
    Low,
    Normal,
    High,
    VeryHigh,
}

impl Importance {
    /// Declaration order, mirroring the shared level vocabulary's index.
    const LEVELS: [Self; 5] = [
        Self::VeryLow,
        Self::Low,
        Self::Normal,
        Self::High,
        Self::VeryHigh,
    ];

    /// Parses the canonical hyphenated text form (`"very-low"` …
    /// `"very-high"`).
    #[must_use]
    pub fn parse_text_form(raw: &str) -> Option<Self> {
        parse_level_text_form(raw).map(|index| Self::LEVELS[index])
    }

    /// The canonical hyphenated text form (the serde name with `-` for
    /// `_`).
    #[must_use]
    pub const fn text_form(self) -> &'static str {
        level_text_form(self as usize)
    }
}

/// Urgency level for later decision support; `None` on a [`Task`] =
/// unset. A distinct type from [`Importance`] on purpose:
/// cross-assignment is a type error and the matrix view grows them
/// independently. Declaration order = severity order (derived `Ord`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Urgency {
    VeryLow,
    Low,
    Normal,
    High,
    VeryHigh,
}

impl Urgency {
    /// Declaration order, mirroring the shared level vocabulary's index.
    const LEVELS: [Self; 5] = [
        Self::VeryLow,
        Self::Low,
        Self::Normal,
        Self::High,
        Self::VeryHigh,
    ];

    /// Parses the canonical hyphenated text form (`"very-low"` …
    /// `"very-high"`).
    #[must_use]
    pub fn parse_text_form(raw: &str) -> Option<Self> {
        parse_level_text_form(raw).map(|index| Self::LEVELS[index])
    }

    /// The canonical hyphenated text form (the serde name with `-` for
    /// `_`).
    #[must_use]
    pub const fn text_form(self) -> &'static str {
        level_text_form(self as usize)
    }
}

/// Which sync lane the Deck connector carries a task field on (#244).
/// The lane is *the connector's* mapping, not an intrinsic property of
/// the field — the local data model stays one construct.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldFamily {
    /// Carried on the Deck card itself (entity-stamp LWW).
    Native,
    /// Carried on the app-data sidecar document (embedded-clock fold).
    AppData,
}

/// The mergeable task-field manifest: the single source the lockstep
/// assertions pin every structure against (#244). One row per
/// [`TaskClocks`] slot; `.1` names the [`Task`] members the clock guards
/// (composites name both); `.2` is the Deck connector's lane.
pub const TASK_FIELDS: &[(&str, &[&str], FieldFamily)] = &[
    ("title", &["title"], FieldFamily::Native),
    ("description", &["description"], FieldFamily::Native),
    ("duedate", &["duedate"], FieldFamily::Native),
    ("done", &["done"], FieldFamily::Native),
    ("position", &["stack", "order"], FieldFamily::Native),
    ("labels", &["labels"], FieldFamily::Native),
    ("archived", &["archived"], FieldFamily::Native),
    ("deleted", &["deleted"], FieldFamily::Native),
    ("scheduled", &["scheduled"], FieldFamily::Native),
    ("effort", &["effort"], FieldFamily::AppData),
    ("importance", &["importance"], FieldFamily::AppData),
    ("urgency", &["urgency"], FieldFamily::AppData),
];

/// Per-field write timestamps of a [`Task`] (decision: per-field LWW).
///
/// Local edits touch only the edited fields; remote adoption stamps every
/// clock with the remote `lastModified`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskClocks {
    /// Write time of `title`.
    pub title: DateTime<Utc>,
    /// Write time of `description`.
    pub description: DateTime<Utc>,
    /// Write time of `duedate`.
    pub duedate: DateTime<Utc>,
    /// Write time of `scheduled`.
    pub scheduled: DateTime<Utc>,
    /// Write time of `done`.
    pub done: DateTime<Utc>,
    /// Write time of the composite position (`stack` + `order`).
    pub position: DateTime<Utc>,
    /// Write time of the whole label set.
    pub labels: DateTime<Utc>,
    /// Write time of `effort`.
    pub effort: DateTime<Utc>,
    /// Write time of `importance`.
    pub importance: DateTime<Utc>,
    /// Write time of `urgency`.
    pub urgency: DateTime<Utc>,
    /// Write time of `archived`.
    pub archived: DateTime<Utc>,
    /// Write time of the tombstone (`deleted`).
    pub deleted: DateTime<Utc>,
}

/// Per-field write timestamps of a [`Stack`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct StackClocks {
    /// Write time of `title`.
    pub title: DateTime<Utc>,
    /// Write time of `order`.
    pub order: DateTime<Utc>,
    /// Write time of the tombstone (`deleted`).
    pub deleted: DateTime<Utc>,
}

/// Per-field write timestamps of a [`Label`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LabelClocks {
    /// Write time of `title`.
    pub title: DateTime<Utc>,
    /// Write time of `color`.
    pub color: DateTime<Utc>,
    /// Write time of the tombstone (`deleted`).
    pub deleted: DateTime<Utc>,
}

/// Per-field write timestamps of a [`Board`] (per-field LWW, ADR 0004).
///
/// Local verbs stamp only their touched field; remote adoption stamps
/// every clock with the remote `lastModified`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoardClocks {
    /// Write time of `title`.
    pub title: DateTime<Utc>,
    /// Write time of `color`.
    pub color: DateTime<Utc>,
    /// Write time of `archived`.
    pub archived: DateTime<Utc>,
    /// Write time of the tombstone (`deleted`).
    pub deleted: DateTime<Utc>,
}

impl BoardClocks {
    /// The epoch clock: "never locally written" — every remote stamp
    /// wins, so pre-clock rows keep the remote-wins behavior bit-for-bit.
    #[must_use]
    pub fn epoch() -> DateTime<Utc> {
        use chrono::TimeZone as _;
        chrono::Utc.timestamp_opt(-62_135_596_800, 0).unwrap()
    }
}

/// A board. Title, color, and the archived flag are locally editable
/// (`boards rename`/`color`/`archive`/`unarchive`); deletion is the
/// `boards delete` tombstone ceremony.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Board {
    /// Local identity.
    pub id: BoardId,
    /// Remote binding: the remote board *and* the connector that owns it
    /// (config-minted tag, DF-3); `None` until bound to a Deck board.
    pub remote: Option<RemoteBoardBinding>,
    /// Board title.
    pub title: String,
    /// Board color (lenient).
    pub color: Color,
    /// Archived flag.
    pub archived: bool,
    /// Tombstone flag (boards have real remote delete timestamps).
    pub deleted: bool,
    /// Per-field write clocks.
    pub clocks: BoardClocks,
    /// Remote `lastModified` at last adoption; `None` = never synced.
    pub remote_seen: Option<DateTime<Utc>>,
}

/// A column within a board.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stack {
    /// Local identity.
    pub id: StackId,
    /// Remote binding; `None` until the pending create is pushed.
    pub remote: Option<RemoteStackRef>,
    /// Owning board (local id).
    pub board: BoardId,
    /// Stack title.
    pub title: String,
    /// Sort position within the board.
    pub order: i64,
    /// Archived flag.
    pub archived: bool,
    /// Tombstone flag.
    pub deleted: bool,
    /// Per-field write clocks.
    pub clocks: StackClocks,
    /// Remote `lastModified` at last adoption; `None` = never synced.
    pub remote_seen: Option<DateTime<Utc>>,
}

/// A task card — the central entity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Task {
    /// Local identity.
    pub id: TaskId,
    /// Remote binding; `None` until the pending create is pushed.
    pub remote: Option<RemoteCardRef>,
    /// Task title.
    pub title: String,
    /// Task description (Markdown, as Deck stores it).
    pub description: String,
    /// Due date; `None` = none.
    pub duedate: Option<DateTime<Utc>>,
    /// Scheduled start; `None` = unscheduled. The "when do I intend to
    /// begin" half of the deadline split (`duedate` is "when it must be
    /// finished").
    pub scheduled: Option<DateTime<Utc>>,
    /// Completion timestamp; `None` = open.
    pub done: Option<DateTime<Utc>>,
    /// Owning stack — half of the composite position field.
    pub stack: StackId,
    /// Sort order within the stack — half of the composite position field.
    pub order: i64,
    /// Attached labels (whole-set LWW as one mergeable field).
    pub labels: std::collections::BTreeSet<LabelId>,
    /// Estimated effort; `None` = unestimated.
    pub effort: Option<Effort>,
    /// Importance; `None` = unset.
    pub importance: Option<Importance>,
    /// Urgency; `None` = unset.
    pub urgency: Option<Urgency>,
    /// Archived flag.
    pub archived: bool,
    /// Tombstone flag; `clocks.deleted` is the tombstone's write time.
    pub deleted: bool,
    /// Per-field write clocks.
    pub clocks: TaskClocks,
    /// Remote `lastModified` at last adoption; `None` = never synced.
    /// Fast-path guard for the pull rules (unchanged remote ⇒ keep local).
    pub remote_seen: Option<DateTime<Utc>>,
}

impl Task {
    /// `true` while the task is not tombstoned.
    #[must_use]
    pub const fn is_live(&self) -> bool {
        !self.deleted
    }

    /// `true` while the task is not completed.
    #[must_use]
    pub const fn is_open(&self) -> bool {
        self.done.is_none()
    }
}

/// A label defined on a board.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Label {
    /// Local identity.
    pub id: LabelId,
    /// Remote binding; `None` until the pending create is pushed.
    pub remote: Option<RemoteLabelRef>,
    /// Owning board (local id).
    pub board: BoardId,
    /// Label title.
    pub title: String,
    /// Label color (lenient).
    pub color: Color,
    /// Tombstone flag.
    pub deleted: bool,
    /// Per-field write clocks.
    pub clocks: LabelClocks,
    /// Remote `lastModified` at last adoption; `None` = never synced.
    pub remote_seen: Option<DateTime<Utc>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support;
    use proptest::prelude::*;

    #[test]
    fn epoch_clock_roundtrips_through_rfc3339() {
        let text = BoardClocks::epoch().to_rfc3339();
        let back: DateTime<Utc> = text.parse().expect("parse");
        assert_eq!(
            back,
            BoardClocks::epoch(),
            "storage decode must accept the epoch"
        );
    }

    #[test]
    fn effort_roundtrips_through_serde_as_a_bare_integer() {
        let effort = Effort::new(900);
        let json = serde_json::to_value(effort).unwrap();
        assert_eq!(json, serde_json::json!(900), "transparent integer encoding");
        let back: Effort = serde_json::from_value(json).unwrap();
        assert_eq!(back, effort);
        assert_eq!(back.seconds(), 900);
    }

    #[test]
    fn level_enums_serialize_their_serde_unit_names() {
        let importance_cases = [
            (Importance::VeryLow, "very_low"),
            (Importance::Low, "low"),
            (Importance::Normal, "normal"),
            (Importance::High, "high"),
            (Importance::VeryHigh, "very_high"),
        ];
        let urgency_cases = [
            (Urgency::VeryLow, "very_low"),
            (Urgency::Low, "low"),
            (Urgency::Normal, "normal"),
            (Urgency::High, "high"),
            (Urgency::VeryHigh, "very_high"),
        ];
        for (level, name) in importance_cases {
            let json = serde_json::to_value(level).unwrap();
            assert_eq!(json, serde_json::json!(name));
            assert_eq!(serde_json::from_value::<Importance>(json).unwrap(), level);
        }
        for (level, name) in urgency_cases {
            let json = serde_json::to_value(level).unwrap();
            assert_eq!(json, serde_json::json!(name));
            assert_eq!(serde_json::from_value::<Urgency>(json).unwrap(), level);
        }
    }

    #[test]
    fn sorted_all_variants_equals_declaration_order() {
        let importance = [
            Importance::VeryLow,
            Importance::Low,
            Importance::Normal,
            Importance::High,
            Importance::VeryHigh,
        ];
        let urgency = [
            Urgency::VeryLow,
            Urgency::Low,
            Urgency::Normal,
            Urgency::High,
            Urgency::VeryHigh,
        ];
        let mut sorted = importance;
        sorted.sort();
        assert_eq!(sorted, importance, "derived Ord = declaration order");
        let mut sorted = urgency;
        sorted.sort();
        assert_eq!(sorted, urgency, "derived Ord = declaration order");
    }

    #[test]
    fn effort_parse_grammar_table() {
        let cases = [
            ("30", 30 * 60),  // bare digits are minutes
            ("45s", 45),      // seconds
            ("2h", 2 * 3600), // hours
            ("1h30m", 5400),  // components compose
            ("1h30m15s", 5415),
            ("0", 0),
            ("0s", 0),
        ];
        for (raw, seconds) in cases {
            assert_eq!(
                Effort::parse_text_form(raw),
                Some(Effort::new(seconds)),
                "{raw} must parse"
            );
        }
        for bad in [
            "1h30",                          // unit-less after a unit-carrying component
            "-30m",                          // no signs
            "1.5h",                          // no floats
            "1 h30m",                        // no whitespace
            "h30m",                          // no unit-less leading component
            "10min",                         // units are exactly s|m|h
            "",                              // no empty input
            "none",                          // the clear sentinel is the CLI wrapper's, not ours
            "9999999999999999999999999999h", // component overflow
        ] {
            assert_eq!(Effort::parse_text_form(bad), None, "{bad} must be rejected");
        }
    }

    #[test]
    fn effort_total_overflow_is_none() {
        // A total that overflows u64 while accumulating valid components:
        // the max whole hours plus enough seconds to pass the ceiling.
        let hours = u64::MAX / 3600;
        let room = u64::MAX - hours * 3600;
        let raw = format!("{hours}h{}s", 3600 - room);
        assert_eq!(Effort::parse_text_form(&raw), None);
        // ...while the same hours alone is representable.
        assert_eq!(
            Effort::parse_text_form(&format!("{hours}h")),
            Some(Effort::new(hours * 3600)),
        );
    }

    #[test]
    fn effort_text_form_canonical_table() {
        let cases = [
            (5400u64, "1h30m"),
            (3600, "1h"),
            (59, "59s"),
            (60, "1m"),
            (0, "0s"),
            (30 * 60, "30m"),
        ];
        for (seconds, form) in cases {
            assert_eq!(Effort::new(seconds).text_form(), form, "{seconds}s");
        }
    }

    #[test]
    fn effort_levels_reject_none_text_form() {
        // The `none` clear sentinel never enters the domain grammar, for
        // effort or the levels.
        assert_eq!(Effort::parse_text_form("none"), None);
        assert_eq!(Importance::parse_text_form("none"), None);
        assert_eq!(Urgency::parse_text_form("none"), None);
    }

    #[test]
    fn level_text_forms_roundtrip_all_values() {
        let importance = [
            (Importance::VeryLow, "very-low"),
            (Importance::Low, "low"),
            (Importance::Normal, "normal"),
            (Importance::High, "high"),
            (Importance::VeryHigh, "very-high"),
        ];
        for (level, form) in importance {
            assert_eq!(level.text_form(), form);
            assert_eq!(Importance::parse_text_form(form), Some(level));
        }
        let urgency = [
            (Urgency::VeryLow, "very-low"),
            (Urgency::Low, "low"),
            (Urgency::Normal, "normal"),
            (Urgency::High, "high"),
            (Urgency::VeryHigh, "very-high"),
        ];
        for (level, form) in urgency {
            assert_eq!(level.text_form(), form);
            assert_eq!(Urgency::parse_text_form(form), Some(level));
        }
        for bad in ["very_low", "medium", "NONE", ""] {
            assert_eq!(Importance::parse_text_form(bad), None, "{bad}");
            assert_eq!(Urgency::parse_text_form(bad), None, "{bad}");
        }
        // The types are distinct on purpose: a cross parse is `None` by
        // construction, but the spellings are one shared vocabulary.
        assert_eq!(
            Importance::parse_text_form(Urgency::High.text_form()),
            Some(Importance::High),
        );
    }

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(256))]

        #[test]
        fn entities_roundtrip_through_serde(task in test_support::task_strategy()) {
            let json = serde_json::to_string(&task).unwrap();
            let back: Task = serde_json::from_str(&json).unwrap();
            prop_assert_eq!(back, task);
        }

        #[test]
        fn color_roundtrips(raw in ".{0,64}") {
            let color = Color::new(raw.clone());
            prop_assert_eq!(color.as_str(), raw.as_str());
            prop_assert_eq!(color.to_string(), raw);
        }

        #[test]
        fn task_helpers_match_flags(task in test_support::task_strategy()) {
            prop_assert_eq!(task.is_live(), !task.deleted);
            prop_assert_eq!(task.is_open(), task.done.is_none());
        }

        #[test]
        fn effort_parse_is_the_inverse_of_text_form(
            seconds in 0u64..=(u64::MAX - 4000),
        ) {
            let effort = Effort::new(seconds);
            let form = effort.text_form();
            prop_assert_eq!(
                Effort::parse_text_form(&form),
                Some(effort),
                "parse(text_form(e)) == Some(e)"
            );
        }
    }
}
