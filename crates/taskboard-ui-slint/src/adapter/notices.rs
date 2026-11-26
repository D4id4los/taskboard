// SPDX-License-Identifier: AGPL-3.0-only

use slint::{ComponentHandle as _, SharedString};

use super::Adapter;
use super::{Notice, paint_files_sync_chip};
use crate::AppWindow;

impl Adapter {
    /// The drain-side batch form of
    /// [`Self::record_system_event`].
    pub(crate) fn record_system_events(&mut self, events: &[taskboard_domain::SystemEvent]) {
        for event in events {
            self.record_system_event(event);
        }
    }

    /// The notice's cadence-tick half: expires the line past its
    /// horizon and pushes both status-bar properties. Runs every tick
    /// (cheap: two property writes), so expiry needs no timer of its
    /// own.
    pub(crate) fn push_notice(&mut self, window: &AppWindow) {
        let now = chrono::Utc::now();
        if !crate::presentation::notice_visible(now, self.notice.as_ref().map(|n| n.until)) {
            self.notice = None;
        }
        let (text, visible) = match &self.notice {
            Some(notice) => (notice.text.as_str(), true),
            None => ("", false),
        };
        window.set_notice_text(text.into());
        window.set_notice_visible(visible);
    }

    /// The two signal lanes' push half (the small-gated journeys plan
    /// W-5/W-7): the `ConfigReload` lane's typed state and the
    /// degraded-boot badge's flag, one write each per pass. Both lanes
    /// are display-only by construction (R-3/R-5): no command, no
    /// callback, no dismiss.
    pub(crate) fn push_signal_lanes(&mut self, window: &AppWindow) {
        let settings = window.global::<crate::SettingsModel>();
        settings.set_reload_lane_state(match self.reload_lane {
            crate::presentation::ConfigReloadPresentation::Quiet => {
                crate::ConfigReloadLaneState::None
            }
            crate::presentation::ConfigReloadPresentation::Applied => {
                crate::ConfigReloadLaneState::Applied
            }
            crate::presentation::ConfigReloadPresentation::KeptLastKnownGood => {
                crate::ConfigReloadLaneState::Kept
            }
        });
        window.set_journal_degraded_visible(self.journal_degraded);
    }

    /// Records one system-event episode (the system drain's
    /// pre-refresh hook, D8): connector liveness and the resurrect
    /// notice are the recorded verdicts today.
    pub(crate) fn record_system_event(&mut self, event: &taskboard_domain::SystemEvent) {
        use taskboard_domain::SystemEvent as E;
        match event {
            E::ConnectorLost(id) => {
                self.settings.liveness.insert(id.to_string(), true);
            }
            E::ConnectorRestored(id) => {
                self.settings.liveness.insert(id.to_string(), false);
            }
            E::ConfigReload(outcome) => {
                // The read-only Settings lane's fold (W-5, R-3): the
                // outcome surfaces, nothing dispatches.
                self.reload_lane = crate::presentation::fold_config_reload(outcome);
            }
            E::JournalDegraded { .. } => {
                // The degraded-boot badge (W-7, R-5): display-only and
                // persistent — set once, never cleared in-session; the
                // next healthy boot's fresh session replaces it.
                self.journal_degraded = true;
            }
            E::TaskResurrected(id) => {
                // The title resolves once, at event time, against the
                // current snapshot — the resurrected task is live and
                // unbound by the time the cycle settles.
                let title = self
                    .state
                    .load()
                    .tasks
                    .get(id)
                    .map(|t| t.title.clone())
                    .unwrap_or_default();
                self.notice = Some(Notice {
                    text: crate::presentation::resurrect_notice_phrase(&title),
                    until: chrono::Utc::now() + crate::presentation::RESURRECT_NOTICE_HORIZON,
                });
            }
            _ => {}
        }
    }

    /// Polls every parked reply once (`try_recv` — never a blocking
    /// recv, F18). A closed receiver (the forwarder died) counts as an
    /// outcome too: the pending state clears either way (R-reply).
    /// A `KeymapUpdate` swaps the keymap live and refreshes; a failed
    /// scale reverts the theme value; persistence is never undone by
    /// any reply.
    /// The delivery chip's drain (round-3 chunk 7, D31): the
    /// non-blocking watch poll beside the two broadcasts — a changed
    /// health snapshot pushes the chip properties and reports the
    /// change so the caller marks the pass dirty (the settings-reply
    /// drain's shape). Healthy maps to an absent chip; degraded maps
    /// to the visible one via [`crate::presentation::delivery_chip`].
    /// Returns whether the snapshot changed.
    pub(crate) fn drain_delivery_health(&mut self, window: &AppWindow) -> bool {
        if !self.delivery_health.has_changed().unwrap_or(false) {
            return false;
        }
        let health = self.delivery_health.borrow_and_update().clone();
        let chip = crate::presentation::delivery_chip(&health);
        window.set_delivery_chip_visible(chip.is_some());
        window.set_delivery_chip(SharedString::from(chip.unwrap_or_default()));
        true
    }

    /// The files-sync chip's drain (DR-8 chunk 4, D-FH5): the same
    /// non-blocking watch poll as the delivery chip's. `None` (the
    /// substrate off) leaves the chip at its hidden default — no boot
    /// with the actor off ever shows it (the golden-stability pin).
    /// Returns whether the snapshot changed.
    pub(crate) fn drain_files_sync(&mut self, window: &AppWindow) -> bool {
        let Some(rx) = self.files_sync.as_mut() else {
            return false;
        };
        if !rx.has_changed().unwrap_or(false) {
            return false;
        }
        let chip = rx.borrow_and_update().clone();
        // The per-board states map (US-8, D-8): the settings rows'
        // adoption phase rides the same watch.
        self.settings.files_board_states = chip.board_states.clone();
        paint_files_sync_chip(window, &chip);
        true
    }
}
