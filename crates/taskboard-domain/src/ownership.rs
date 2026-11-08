// SPDX-License-Identifier: MIT OR Apache-2.0
//! The board→connector ownership map (D-BF5): the *derived* view of the
//! persisted board bindings, plus the typed second-claim law.
//!
//! The assignment's only write-side is the state engine's binding rows
//! (`Board.remote`) — config never assigns boards. This module folds
//! those rows into an explicit map so the enforcement points (the
//! engine's `BindBoard` guard, report attribution, bootstrap attach)
//! read one total structure instead of re-deriving "who owns this
//! board" by ad-hoc scans. It can never drift from state because it is
//! state folded.

use std::collections::BTreeMap;

use crate::ids::{BoardId, ConnectorId};
use crate::state::AppState;

/// Every bound board's owning connector, keyed by local board id.
/// Unbound boards are absent — ownership is the binding, and an absent
/// entry is the typed "not mine" answer.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BoardOwnershipMap(BTreeMap<BoardId, ConnectorId>);

impl BoardOwnershipMap {
    /// Folds the map from a board table: every board carrying a remote
    /// binding maps to its binding's connector.
    #[must_use]
    pub fn from_boards(boards: &BTreeMap<BoardId, crate::entities::Board>) -> Self {
        Self(
            boards
                .values()
                .filter_map(|board| {
                    board
                        .remote
                        .as_ref()
                        .map(|binding| (board.id, binding.connector.clone()))
                })
                .collect(),
        )
    }

    /// Folds the map from an [`AppState`] (the live engine view; the
    /// persisted [`crate::persistence::PersistedState`] fold goes
    /// through [`Self::from_boards`]).
    #[must_use]
    pub fn from_state(app: &AppState) -> Self {
        Self::from_boards(&app.boards)
    }

    /// The board's owning connector; `None` when the board is unbound —
    /// the structural "not mine" answer (D-BF5: never a nearest match).
    #[must_use]
    pub fn owner_of(&self, board: &BoardId) -> Option<&ConnectorId> {
        self.0.get(board)
    }

    /// Every binding as `(board, owning connector)`, in board-id order —
    /// the attach-time walk's input (bootstrap checks each connector
    /// against the configured set; D-BF5's loud orphan refusal).
    pub fn bindings(&self) -> impl Iterator<Item = (BoardId, &ConnectorId)> {
        self.0.iter().map(|(board, connector)| (*board, connector))
    }
}

/// The live board bound to this connector's remote board id, if any.
/// The second-claim check's read side (D-BF5): a `Some` holder is the
/// typed refusal's payload; the pairing is tag-aware (the enum variants
/// never compare equal across tags) and connector-scoped (the same
/// numeric id under another connector is a different remote).
#[must_use]
pub fn holder_of(
    app: &AppState,
    connector: &ConnectorId,
    remote: &crate::ids::RemoteBoardId,
) -> Option<BoardId> {
    app.boards.values().find_map(|board| {
        board
            .remote
            .as_ref()
            .filter(|binding| binding.connector == *connector && binding.board == *remote)
            .map(|_| board.id)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::{Board, BoardAuthors, BoardClocks, Color};
    use crate::ids::{ConnectorId, RemoteBoardId};
    use crate::state::AppState;

    fn board_with(id: BoardId, connector: &str, remote: RemoteBoardId) -> Board {
        Board {
            id,
            remote: Some(crate::ids::RemoteBoardBinding {
                connector: ConnectorId::from(connector),
                board: remote,
            }),
            title: "b".into(),
            color: Color::new("3366cc"),
            archived: false,
            deleted: false,
            clocks: BoardClocks {
                title: crate::entities::BoardClocks::epoch(),
                color: crate::entities::BoardClocks::epoch(),
                archived: crate::entities::BoardClocks::epoch(),
                deleted: crate::entities::BoardClocks::epoch(),
            },
            authors: BoardAuthors::default(),
            remote_seen: None,
        }
    }

    fn unbound(id: BoardId) -> Board {
        let mut board = board_with(id, "never", RemoteBoardId::deck(0));
        board.remote = None;
        board
    }

    /// T5 — the pure fold: bound boards map to their binding's
    /// connector, unbound boards are absent.
    #[test]
    fn from_state_folds_bindings_and_skips_unbound_boards() {
        let deck_board = BoardId::from_uuid(uuid::Uuid::from_u128(1));
        let files_board = BoardId::from_uuid(uuid::Uuid::from_u128(2));
        let loose_board = BoardId::from_uuid(uuid::Uuid::from_u128(3));
        let mut app = AppState::default();
        app.boards.insert(
            deck_board,
            board_with(deck_board, "nextcloud", RemoteBoardId::deck(42)),
        );
        app.boards.insert(
            files_board,
            board_with(
                files_board,
                "files",
                RemoteBoardId::files(uuid::Uuid::from_u128(7)),
            ),
        );
        app.boards.insert(loose_board, unbound(loose_board));

        let map = BoardOwnershipMap::from_state(&app);
        assert_eq!(
            map.owner_of(&deck_board).map(ConnectorId::as_str),
            Some("nextcloud")
        );
        assert_eq!(
            map.owner_of(&files_board).map(ConnectorId::as_str),
            Some("files")
        );
        assert_eq!(map.owner_of(&loose_board), None);
        // The fold is a pure function of state: an empty app folds to
        // the empty map.
        assert_eq!(
            BoardOwnershipMap::from_state(&AppState::default()),
            BoardOwnershipMap::default()
        );
        // The binding walk enumerates every bound board exactly once.
        assert_eq!(
            map.bindings().collect::<Vec<_>>(),
            vec![
                (deck_board, &ConnectorId::from("nextcloud")),
                (files_board, &ConnectorId::from("files")),
            ]
        );
    }

    /// The holder lookup is tag-aware: the same numeric payload under a
    /// different tag is a different remote and has no holder.
    #[test]
    fn holder_of_is_tag_aware() {
        let holder = BoardId::from_uuid(uuid::Uuid::from_u128(1));
        let mut app = AppState::default();
        app.boards
            .insert(holder, board_with(holder, "nc", RemoteBoardId::deck(42)));
        assert_eq!(
            holder_of(&app, &ConnectorId::from("nc"), &RemoteBoardId::deck(42)),
            Some(holder)
        );
        assert_eq!(
            holder_of(&app, &ConnectorId::from("nc"), &RemoteBoardId::deck(43)),
            None
        );
        // A UUID payload never equals a Deck payload.
        assert_eq!(
            holder_of(
                &app,
                &ConnectorId::from("nc"),
                &RemoteBoardId::files(uuid::Uuid::from_u128(42))
            ),
            None
        );
        assert_eq!(
            holder_of(&app, &ConnectorId::from("other"), &RemoteBoardId::deck(42)),
            None
        );
    }
}
