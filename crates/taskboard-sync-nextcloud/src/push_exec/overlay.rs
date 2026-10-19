// SPDX-License-Identifier: MIT OR Apache-2.0
//! Binding-overlay math for the push executor (stratum 1).

use std::collections::HashMap;

use taskboard_domain::{RemoteBoardId, RemoteCardRef, RemoteIndex, RemoteLabelId, RemoteStackId};

/// Local id → remote id bindings for one cycle, extendable by create
/// echoes. The *inverse* of the domain's read-side [`RemoteIndex`].
#[derive(Debug, Clone)]
pub struct BindingOverlay {
    pub(super) board: RemoteBoardId,
    pub(super) stack_by_local: HashMap<taskboard_domain::StackId, RemoteStackId>,
    pub(super) task_by_local:
        HashMap<taskboard_domain::TaskId, (RemoteStackId, taskboard_domain::RemoteCardId)>,
    pub(super) label_by_local: HashMap<taskboard_domain::LabelId, RemoteLabelId>,
}

impl BindingOverlay {
    /// Seeds the overlay from the read state's bindings, scoped to the
    /// cycle's board: refs naming a *different* remote board are invisible
    /// here — `card_of` stamps the cycle's board id, and an unfiltered
    /// overlay would mis-tag another board's refs at N ≥ 2 boards.
    #[must_use]
    pub fn from_index(board: RemoteBoardId, index: &RemoteIndex) -> Self {
        Self {
            board,
            stack_by_local: index
                .stack_by_ref
                .iter()
                .filter(|(r, _)| r.board == board)
                .map(|(r, local)| (*local, r.stack))
                .collect(),
            task_by_local: index
                .task_by_ref
                .iter()
                .filter(|(r, _)| r.board == board)
                .map(|(r, local)| (*local, (r.stack, r.card)))
                .collect(),
            label_by_local: index
                .label_by_ref
                .iter()
                .filter(|(r, _)| r.board == board)
                .map(|(r, local)| (*local, r.label))
                .collect(),
        }
    }

    /// The board every push is addressed to (the cycle's pull target).
    #[must_use]
    pub const fn board(&self) -> RemoteBoardId {
        self.board
    }

    pub(super) fn card_of(&self, task: taskboard_domain::TaskId) -> Option<RemoteCardRef> {
        self.task_by_local
            .get(&task)
            .map(|(stack, card)| RemoteCardRef {
                board: self.board,
                stack: *stack,
                card: *card,
            })
    }
}
