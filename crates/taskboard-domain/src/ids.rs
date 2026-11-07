// SPDX-License-Identifier: MIT OR Apache-2.0
//! Typed identifiers for local (offline-first) and remote (Deck) entities.
//!
//! Local identity is `UUIDv7` generated client-side before any remote sync
//! exists, so entities created offline are stable from birth. Remote Deck
//! ids are `u64` and only unique *per board* for stacks/cards/labels; the
//! newtypes keep board/stack/card/label numbers from being swapped, and the
//! composite ref structs carry the board context where it is required.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use uuid::Uuid;

macro_rules! local_id {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        pub struct $name(Uuid);

        impl $name {
            /// Wraps an existing UUID (tests, adoption of stored ids).
            #[must_use]
            pub const fn from_uuid(id: Uuid) -> Self {
                Self(id)
            }

            /// The underlying UUID.
            #[must_use]
            pub const fn as_uuid(self) -> Uuid {
                self.0
            }
        }

        impl From<Uuid> for $name {
            fn from(id: Uuid) -> Self {
                Self(id)
            }
        }
    };
}

local_id!(
    /// Local identity of a board (`UUIDv7`, generated client-side).
    BoardId
);
local_id!(
    /// Local identity of a stack (`UUIDv7`, generated client-side).
    StackId
);
local_id!(
    /// Local identity of a task (`UUIDv7`, generated client-side).
    TaskId
);
local_id!(
    /// Local identity of a label (`UUIDv7`, generated client-side).
    LabelId
);
local_id!(
    /// Local identity of a reminder rule (`UUIDv7`, generated client-side).
    ReminderRuleId
);
local_id!(
    /// Local identity of a time entry (`UUIDv7`, generated client-side).
    /// Roaming family: the entry UUID is the wire key of the synced
    /// `time-entries` section (ADR 0017).
    TimeEntryId
);
local_id!(
    /// Local identity of a break rule (`UUIDv7`, generated client-side).
    /// Roaming family (#588, D-1): the rule UUID is the wire key of the
    /// synced board-level `break-rules` section, and break dismissals
    /// ride the dismissals section's reserved board-level entry under
    /// `break:`-prefixed keys — its own wire-key grammar, never the
    /// `ReminderRuleId` family's bare-UUID dismissal keys (that strict
    /// parse *is* the old-peer fail-closed mechanism).
    BreakRuleId
);
local_id!(
    /// Local identity of a standalone adhoc alarm (`UUIDv7`, generated
    /// client-side; device-local — never synced).
    AdhocAlarmId
);
local_id!(
    /// Local identity of a recurring series (`UUIDv7`, minted
    /// client-side; stable across the whole occurrence chain).
    SeriesId
);
local_id!(
    /// Local identity of a habit tracker (`UUIDv7`, generated
    /// client-side; the entity lands with the habit chunk — the id
    /// family exists for the `RecurRule.continued_from` seam).
    HabitId
);
local_id!(
    /// Local identity of a journal entry (`UUIDv7`, generated
    /// client-side; time-ordered so the undo line's `created_at`/`id`
    /// tie-break stays stable across devices).
    JournalId
);
local_id!(
    /// Local identity of a reflection entry (`UUIDv7`, generated
    /// client-side). Roaming family — the entry UUID is the future
    /// import-fold's wire key (device-local in v0.2; the *family* is
    /// the roaming-readiness, DR-7 D-J5).
    ReflectionId
);

macro_rules! remote_id {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        pub struct $name(pub u64);

        impl $name {
            /// The raw Deck id.
            #[must_use]
            pub const fn get(self) -> u64 {
                self.0
            }
        }
    };
}

remote_id!(
    /// Remote Deck board id (`u64`).
    RemoteBoardId
);
remote_id!(
    /// Remote Deck stack id (`u64`, unique per board).
    RemoteStackId
);
remote_id!(
    /// Remote Deck card id (`u64`, unique per board).
    RemoteCardId
);
remote_id!(
    /// Remote Deck label id (`u64`, unique per board).
    RemoteLabelId
);

/// Config-minted connector name (the `[[sync.connectors]]` key; the
/// legacy scalar backend synthesizes `"nextcloud"`) — a human-meaningful
/// name, not a UUID.
///
/// # Minting vs. leniency
///
/// Input-adjacent construction (config load, settings forms, raw config
/// re-reads) must go through [`ConnectorId::from_kebab`], which enforces
/// the kebab predicate. The total `From<&str>`/`From<String>` impls are
/// **deliberate leniency, deserialization-only in scope**: serde
/// payloads of already-validated persisted rows (the storage D-3 decode,
/// where a persisted tag naming an unconfigured connector decodes as-is
/// and the owning actor simply never finds the board), the UI adapter's
/// trusted re-mint of a name the app handed over moments earlier, and
/// test fixtures. They are not an input-adjacent mint; production code
/// outside those scopes must not use them.
///
/// Uniqueness across the configured connector set is a set-level
/// property and stays with `AppConfig::validate` in `taskboard-app`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ConnectorId(String);

/// A `[[sync.connectors]]` name that failed the kebab predicate;
/// carries the offending name verbatim.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("connector name invalid")]
pub struct InvalidConnectorName {
    /// The name that did not pass.
    pub name: String,
}

impl ConnectorId {
    /// The kebab predicate (`[a-z0-9-]`, non-empty, no leading or
    /// trailing `-`, no `--`).
    #[must_use]
    pub fn is_kebab_name(name: &str) -> bool {
        !name.is_empty()
            && name
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            && !name.starts_with('-')
            && !name.ends_with('-')
            && !name.contains("--")
    }

    /// The mint: the only construction path for a `ConnectorId` that
    /// enters from user-controlled text (config load, settings forms,
    /// raw config re-reads).
    ///
    /// # Errors
    ///
    /// [`InvalidConnectorName`] with the offending name verbatim when
    /// the kebab predicate fails.
    pub fn from_kebab(name: &str) -> Result<Self, InvalidConnectorName> {
        if Self::is_kebab_name(name) {
            Ok(Self(name.to_owned()))
        } else {
            Err(InvalidConnectorName {
                name: name.to_owned(),
            })
        }
    }

    /// The connector name as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The lenient decode/fixture constructor; see the type doc for the
/// scoped ruling (deserialization and fixtures only — never
/// input-adjacent).
impl From<&str> for ConnectorId {
    fn from(name: &str) -> Self {
        Self(name.to_owned())
    }
}

/// See the `From<&str>` impl and the type doc.
impl From<String> for ConnectorId {
    fn from(name: String) -> Self {
        Self(name)
    }
}

impl std::fmt::Display for ConnectorId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// One device's author identity: who last wrote a field. Same UUID as
/// the substrate's `device.json` `device_id` — the app bootstrap
/// resolves the identity once at boot and hands `Some(AuthorId)` to
/// the planner; headless contexts (tests, identity-less boot windows)
/// stamp `None`, and the fold's `None < Some` author leg keeps those
/// rows honest rather than corrupt.
///
/// # Minting vs. leniency
///
/// The minting sites are the app bootstrap's device-identity handoff
/// (via [`AuthorId::from_uuid`] off the resolved `DeviceId`) and test
/// fixtures — nothing else constructs one. Wire/storage decode goes
/// through [`AuthorId::parse`], which is strict: an unparsable author
/// value fails closed at the boundary (a silently de-authored row
/// would change its fold rank against the author-carrying peer).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AuthorId(Uuid);

impl AuthorId {
    /// Strict UUID parsing — the decode path for wire, storage, and
    /// document forms. `None` on any non-UUID input.
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        Uuid::parse_str(raw).ok().map(Self)
    }

    /// The underlying UUID.
    #[must_use]
    pub const fn as_uuid(self) -> Uuid {
        self.0
    }

    /// The trusted-internal constructor from an already-validated
    /// UUID (the bootstrap's resolved `DeviceId`, test fixtures).
    /// Never a text-input path — text goes through [`AuthorId::parse`].
    #[must_use]
    pub const fn from_uuid(id: Uuid) -> Self {
        Self(id)
    }
}

impl fmt::Display for AuthorId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // `Uuid`'s `Display` is already the lowercase hyphenated form.
        self.0.fmt(f)
    }
}

impl Serialize for AuthorId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for AuthorId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Self::parse(&raw).ok_or_else(|| serde::de::Error::custom("an author id must be a UUID"))
    }
}

/// A board's remote binding: the remote board *and* the connector that
/// owns it. The connector tag is config-minted (DF-3): one name from
/// config through the registry, the module, and the storage row.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct RemoteBoardBinding {
    /// The connector that owns this binding (its actor syncs this board).
    pub connector: ConnectorId,
    /// The remote board id within that connector's namespace.
    pub board: RemoteBoardId,
}

/// Location of a card on the remote: Deck card ids are only unique per
/// board, so the full ref carries board and stack context.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord, Hash)]
pub struct RemoteCardRef {
    /// Owning board.
    pub board: RemoteBoardId,
    /// Owning stack.
    pub stack: RemoteStackId,
    /// The card itself.
    pub card: RemoteCardId,
}

/// Location of a stack on the remote (board context included).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord, Hash)]
pub struct RemoteStackRef {
    /// Owning board.
    pub board: RemoteBoardId,
    /// The stack itself.
    pub stack: RemoteStackId,
}

/// Location of a label on the remote (board context included).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord, Hash)]
pub struct RemoteLabelRef {
    /// Owning board.
    pub board: RemoteBoardId,
    /// The label itself.
    pub label: RemoteLabelId,
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(256))]

        #[test]
        fn local_ids_roundtrip_through_uuid(raw in any::<u128>()) {
            let raw = uuid::Uuid::from_u128(raw);
            let task = TaskId::from(raw);
            prop_assert_eq!(task.as_uuid(), raw);
            prop_assert_eq!(task, TaskId::from_uuid(raw));
            // Every id kind shares the macro-generated accessors.
            let board = BoardId::from_uuid(raw);
            prop_assert_eq!(board.as_uuid(), raw);
            prop_assert_eq!(board, BoardId::from(raw));
            let stack = StackId::from_uuid(raw);
            prop_assert_eq!(stack.as_uuid(), raw);
            prop_assert_eq!(stack, StackId::from(raw));
            let label = LabelId::from_uuid(raw);
            prop_assert_eq!(label.as_uuid(), raw);
            prop_assert_eq!(label, LabelId::from(raw));
            let rule = ReminderRuleId::from_uuid(raw);
            prop_assert_eq!(rule.as_uuid(), raw);
            prop_assert_eq!(rule, ReminderRuleId::from(raw));
            let alarm = AdhocAlarmId::from_uuid(raw);
            prop_assert_eq!(alarm.as_uuid(), raw);
            prop_assert_eq!(alarm, AdhocAlarmId::from(raw));
            let break_rule = BreakRuleId::from_uuid(raw);
            prop_assert_eq!(break_rule.as_uuid(), raw);
            prop_assert_eq!(break_rule, BreakRuleId::from(raw));
            let entry = TimeEntryId::from_uuid(raw);
            prop_assert_eq!(entry.as_uuid(), raw);
            prop_assert_eq!(entry, TimeEntryId::from(raw));
            let series = SeriesId::from_uuid(raw);
            prop_assert_eq!(series.as_uuid(), raw);
            prop_assert_eq!(series, SeriesId::from(raw));
            let habit = HabitId::from_uuid(raw);
            prop_assert_eq!(habit.as_uuid(), raw);
            prop_assert_eq!(habit, HabitId::from(raw));
        }

        #[test]
        fn remote_ids_roundtrip_through_u64(raw in any::<u64>()) {
            prop_assert_eq!(RemoteCardId(raw).get(), raw);
            prop_assert_eq!(RemoteStackId(raw).get(), raw);
            prop_assert_eq!(RemoteLabelId(raw).get(), raw);
            prop_assert_eq!(RemoteBoardId(raw).get(), raw);
        }

        #[test]
        fn bindings_roundtrip_through_serde(
            name in "[a-z][a-z0-9-]{0,31}",
            b in any::<u64>(),
        ) {
            let binding = RemoteBoardBinding {
                connector: ConnectorId::from(name.as_str()),
                board: RemoteBoardId(b),
            };
            let json = serde_json::to_string(&binding).unwrap();
            let back: RemoteBoardBinding = serde_json::from_str(&json).unwrap();
            prop_assert_eq!(back, binding);
        }

        #[test]
        fn refs_roundtrip_through_serde(
            b in any::<u64>(),
            s in any::<u64>(),
            c in any::<u64>(),
            l in any::<u64>(),
        ) {
            let card = RemoteCardRef {
                board: RemoteBoardId(b),
                stack: RemoteStackId(s),
                card: RemoteCardId(c),
            };
            let stack = RemoteStackRef { board: RemoteBoardId(b), stack: RemoteStackId(s) };
            let label = RemoteLabelRef { board: RemoteBoardId(b), label: RemoteLabelId(l) };
            let card2: RemoteCardRef =
                serde_json::from_str(&serde_json::to_string(&card).unwrap()).unwrap();
            prop_assert_eq!(card2, card);
            let stack2: RemoteStackRef =
                serde_json::from_str(&serde_json::to_string(&stack).unwrap()).unwrap();
            prop_assert_eq!(stack2, stack);
            let label2: RemoteLabelRef =
                serde_json::from_str(&serde_json::to_string(&label).unwrap()).unwrap();
            prop_assert_eq!(label2, label);
        }

        #[test]
        fn predicate_and_mint_agree_everywhere(s in ".*") {
            prop_assert_eq!(ConnectorId::is_kebab_name(&s), ConnectorId::from_kebab(&s).is_ok());
        }
    }

    #[test]
    fn kebab_accept_table() {
        for ok in ["a", "0", "nextcloud", "nc-2", "a-1-b", "abc123"] {
            assert!(ConnectorId::is_kebab_name(ok), "accept {ok:?}");
            assert!(ConnectorId::from_kebab(ok).is_ok(), "accept {ok:?}");
        }
    }

    #[test]
    fn kebab_reject_table() {
        for bad in [
            "", "-", "--", "-a", "a-", "a--b", "A", "aB", "a_b", "a b", "ä", "a.b", "/",
        ] {
            assert!(!ConnectorId::is_kebab_name(bad), "reject {bad:?}");
            let err = ConnectorId::from_kebab(bad).expect_err("reject");
            assert_eq!(err.name, bad, "offending name carried verbatim");
        }
    }

    #[test]
    fn minted_name_round_trips_identity() {
        let minted = ConnectorId::from_kebab("nextcloud").unwrap();
        assert_eq!(minted.as_str(), "nextcloud");
        assert_eq!(minted.to_string(), "nextcloud");
    }

    #[test]
    fn total_from_documents_the_lenient_decode_path() {
        // Deliberate leniency pin: the total `From` exists for serde
        // decode of persisted rows and test fixtures, not for
        // input-adjacent minting. It constructs a non-kebab value
        // as-is; policy, not endorsement.
        let lenient = ConnectorId::from("NOT_KEBAB");
        assert_eq!(lenient.as_str(), "NOT_KEBAB");
    }

    #[test]
    fn author_id_parses_strictly_and_displays_lowercase_hyphenated() {
        let text = "0193a1b2-c3d4-7cc9-8a2e-5f1e2d3c4b5a";
        let author = AuthorId::parse(text).expect("parses");
        assert_eq!(author.to_string(), text);
        assert_eq!(author.as_uuid(), uuid::Uuid::parse_str(text).expect("uuid"));
        assert!(AuthorId::parse("nope").is_none());
        assert!(AuthorId::parse("").is_none());
    }

    #[test]
    fn author_id_serde_roundtrips_over_the_display_form_and_rejects_non_uuids() {
        let text = "0193a1b2-c3d4-7cc9-8a2e-5f1e2d3c4b5a";
        let author = AuthorId::parse(text).expect("parses");
        let raw = serde_json::to_value(author).expect("serializes");
        assert_eq!(raw, serde_json::json!(text));
        assert_eq!(
            serde_json::from_value::<AuthorId>(raw).expect("decodes"),
            author
        );
        assert!(serde_json::from_value::<AuthorId>(serde_json::json!("nope")).is_err());
        assert!(serde_json::from_value::<AuthorId>(serde_json::json!(42)).is_err());
    }

    #[test]
    fn author_id_orders_totally_so_the_duel_leg_is_total() {
        let a = AuthorId::parse("0193a1b2-c3d4-7cc9-8a2e-5f1e2d3c4b5a").expect("parses");
        let b = AuthorId::parse("0193a1b2-c3d4-7cc9-8a2e-5f1e2d3c4b5b").expect("parses");
        assert_ne!(a.cmp(&b), std::cmp::Ordering::Equal);
        // The fold's pinned forward choice, restated: `None < Some`.
        assert!(Option::<AuthorId>::None < Some(a));
        assert!(Some(b) > Some(a));
    }
}
