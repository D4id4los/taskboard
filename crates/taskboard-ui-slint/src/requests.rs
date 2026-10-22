// SPDX-License-Identifier: AGPL-3.0-only

//! The UI → app request lane (round-1 D21): typed requests the UI
//! sends through its dedicated mpsc for app-side processing that is
//! neither a state command nor a system event. The adapter is the only
//! sender (`try_send`, warn-and-drop); the app crate drains the
//! receiver. A channel payload — domain newtypes are fine, no serde.

use taskboard_domain::{BoardId, StackId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiRequest {
    /// The full per-board stack-width snapshot (round-1 D21):
    /// idempotent under re-send — the app forwarder replaces that
    /// board's entry wholesale.
    StackWidthsChanged {
        board: BoardId,
        widths: Vec<(StackId, u32)>,
    },
}
