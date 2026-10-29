// SPDX-License-Identifier: AGPL-3.0-only

//! The delivery-health test hand-over (round-3 chunk 7, D31).

use taskboard_ui_slint::presentation::DeliveryHealth;

/// A detached delivery-health receiver (its sender dropped): the
/// hand-over the mount tests that don't exercise the health lane use.
#[must_use]
pub fn detached_health() -> tokio::sync::watch::Receiver<DeliveryHealth> {
    let (_tx, rx) = tokio::sync::watch::channel(DeliveryHealth::default());
    rx
}
