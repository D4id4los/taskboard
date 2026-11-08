// SPDX-License-Identifier: MIT OR Apache-2.0
//! The Deck→files move's wire payloads (D-BF11): the `moved_from`
//! provenance on a board's identity, and the create-once commit record
//! `boards/<board>/move.json` that carries the Deck-to-UUID mapping.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use taskboard_domain::board_move::BoardAdoption;
use taskboard_domain::{RemoteCardId, RemoteLabelId, RemoteStackId};
use uuid::Uuid;

use crate::ids::DeviceId;

/// The record format this build reads and writes.
pub const MOVE_RECORD_FORMAT: u32 = 1;

/// Which Deck board a subtree replaces. Deck ids are per-server, so the
/// server origin is part of the identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MovedFrom {
    pub deck_server: String,
    pub deck_board: u64,
}

/// A Deck server URL in the comparable form `moved_from` stores.
#[must_use]
pub fn deck_origin(server_url: &str) -> String {
    server_url.trim_end_matches('/').to_ascii_lowercase()
}

/// Why a commit record cannot be used; it is never adopted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum MoveRecordError {
    #[error("the move record is malformed")]
    Malformed,
    #[error("the move record has a newer format")]
    NewerFormat { format: u32 },
}

/// The commit record: its existence closes the undo window and licenses
/// adoption by other devices. Written once, because snapshots get
/// replaced and a long-offline device still needs the mapping.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MoveRecord {
    pub format_version: u32,
    #[serde(flatten)]
    pub moved_from: MovedFrom,
    pub moved_by: DeviceId,
    pub committed_at: chrono::DateTime<chrono::Utc>,
    pub stacks: BTreeMap<u64, Uuid>,
    pub cards: BTreeMap<u64, Uuid>,
    pub labels: BTreeMap<u64, Uuid>,
}

impl MoveRecord {
    /// Decodes a stored record at the trust boundary.
    ///
    /// # Errors
    ///
    /// [`MoveRecordError`] for undecodable or too-new records.
    pub fn decode(bytes: &[u8]) -> Result<Self, MoveRecordError> {
        let record: Self = serde_json::from_slice(bytes).map_err(|_| MoveRecordError::Malformed)?;
        if record.format_version > MOVE_RECORD_FORMAT {
            return Err(MoveRecordError::NewerFormat {
                format: record.format_version,
            });
        }
        Ok(record)
    }

    /// The domain's view of this record for the board at `files_board`.
    #[must_use]
    pub fn adoption(&self, files_board: Uuid) -> BoardAdoption {
        BoardAdoption {
            files_board,
            deck_board: self.moved_from.deck_board,
            stacks: self
                .stacks
                .iter()
                .map(|(id, target)| (RemoteStackId::deck(*id), *target))
                .collect(),
            cards: self
                .cards
                .iter()
                .map(|(id, target)| (RemoteCardId::deck(*id), *target))
                .collect(),
            labels: self
                .labels
                .iter()
                .map(|(id, target)| (RemoteLabelId::deck(*id), *target))
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record() -> MoveRecord {
        MoveRecord {
            format_version: MOVE_RECORD_FORMAT,
            moved_from: MovedFrom {
                deck_server: "https://cloud.example".into(),
                deck_board: 42,
            },
            moved_by: DeviceId::mint(),
            committed_at: chrono::DateTime::UNIX_EPOCH,
            stacks: BTreeMap::from([(7, Uuid::from_u128(1))]),
            cards: BTreeMap::from([(70, Uuid::from_u128(2))]),
            labels: BTreeMap::from([(9, Uuid::from_u128(3))]),
        }
    }

    #[test]
    fn a_record_round_trips_and_maps_to_the_domain() {
        let original = record();
        let bytes = serde_json::to_vec(&original).expect("serializes");
        let decoded = MoveRecord::decode(&bytes).expect("decodes");
        assert_eq!(decoded, original);
        let adoption = decoded.adoption(Uuid::from_u128(9));
        assert_eq!(adoption.deck_board, 42);
        assert_eq!(
            adoption.cards.get(&RemoteCardId::deck(70)),
            Some(&Uuid::from_u128(2))
        );
    }

    #[test]
    fn garbage_and_future_records_are_typed_refusals() {
        assert_eq!(MoveRecord::decode(b"{}"), Err(MoveRecordError::Malformed));
        let mut future = record();
        future.format_version = MOVE_RECORD_FORMAT + 1;
        let bytes = serde_json::to_vec(&future).expect("serializes");
        assert_eq!(
            MoveRecord::decode(&bytes),
            Err(MoveRecordError::NewerFormat {
                format: MOVE_RECORD_FORMAT + 1
            })
        );
    }

    #[test]
    fn origins_compare_regardless_of_case_and_trailing_slash() {
        assert_eq!(
            deck_origin("https://Cloud.Example/"),
            "https://cloud.example"
        );
    }
}
