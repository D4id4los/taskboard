// SPDX-License-Identifier: MIT OR Apache-2.0
//! Typed identifiers for local (offline-first) and remote (Deck) entities.
//!
//! Local identity is `UUIDv7` generated client-side before any remote sync
//! exists, so entities created offline are stable from birth. Remote Deck
//! ids are `u64` and only unique *per board* for stacks/cards/labels; the
//! newtypes keep board/stack/card/label numbers from being swapped, and the
//! composite ref structs carry the board context where it is required.

use serde::{Deserialize, Serialize};
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
/// name, not a UUID. Kebab-case and uniqueness are validated at config
/// load; this constructor cannot fail, so `From` stays total.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ConnectorId(String);

impl ConnectorId {
    /// The connector name as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for ConnectorId {
    fn from(name: &str) -> Self {
        Self(name.to_owned())
    }
}

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
        }

        #[test]
        fn remote_ids_roundtrip_through_u64(raw in any::<u64>()) {
            prop_assert_eq!(RemoteCardId(raw).get(), raw);
            prop_assert_eq!(RemoteStackId(raw).get(), raw);
            prop_assert_eq!(RemoteLabelId(raw).get(), raw);
            prop_assert_eq!(RemoteBoardId(raw).get(), raw);
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
    }
}
