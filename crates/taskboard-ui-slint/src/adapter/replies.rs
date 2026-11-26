// SPDX-License-Identifier: AGPL-3.0-only

use std::collections::BTreeMap;

use slint::{ComponentHandle as _, SharedString};

use taskboard_domain::{BoardId, RemoteBoardId};

use tokio::sync::oneshot;

use super::Adapter;
use crate::AppWindow;
use crate::actions::ActionId;
use crate::adapter::seams::status_panel_data;
use crate::menu::{self, MenuEntry};
use crate::requests::{self, KeymapUpdate, SettingsError, UiRequest};
use crate::router::Mode;
use crate::view_state::FlowOutcome;

/// One in-flight settings operation: the reply half parked until the
/// drain polls it (the repaint timer's job, D25b — never a blocking
/// recv on the UI thread).
#[derive(Debug)]
pub(in crate::adapter) enum PendingReply {
    /// `ListRemoteBoards`' rows, keyed by the listed connector (round-5
    /// chunk 8): the reply refreshes that connector's mapping rows.
    Remotes(
        String,
        oneshot::Receiver<Result<Vec<requests::RemoteBoardRow>, SettingsError>>,
    ),
    /// The list-carrying replies (`SaveConnection`/`RemoveConnection`/
    /// `AttachConnector`/`CreateLocalBoard`/`DetachConnector`) — the
    /// full pair (r2 D32).
    List(oneshot::Receiver<Result<requests::ConnectionsSnapshot, SettingsError>>),
    /// `SetKeymapPreset`'s rebuilt keymap.
    Keymap(oneshot::Receiver<Result<KeymapUpdate, SettingsError>>),
    /// `SetScale`, with the scale to revert to on an `Err` reply.
    Scale(oneshot::Receiver<Result<(), SettingsError>>, f64),
    /// One of the settings editor's general commits: the reply
    /// carries the fresh file-layer [`SettingsSnapshot`].
    Snapshot(oneshot::Receiver<Result<crate::requests::SettingsSnapshot, SettingsError>>),
}

/// Which operation is in flight (the form's busy marker; one at a
/// time — the buttons disable while pending).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::adapter) enum PendingKind {
    Save,
    Remove,
    /// `ListRemoteBoards` (round-5 chunk 8): the Deck row's marker.
    ListRemote,
    /// `AttachConnector` (r2 D33, explicit since round-5 chunk 8): the
    /// remote row's marker.
    Attach,
    /// `CreateLocalBoard` (round-5 chunk 8): the remote row's marker.
    CreateBind,
    /// `DetachConnector` (r2 D33): the remote row's marker.
    Detach,
    /// `PublishBoard` (round-6 D-6.9, #699): the Deck row's marker.
    Publish,
    /// `MoveBoardToFiles` (round-6 D-6.8 phase 2, #722): the Files
    /// row's marker.
    Move,
    /// `StopBoardFilesSync` (round-7 D-7.3, D9): the roster row's
    /// marker (`busy_board` names the row). Attribution only.
    StopFilesSync,
    /// `PublishBoardToFiles` (round-7 D-7.3, D9): the roster row's
    /// marker (`busy_board` names the row). Attribution only.
    PublishFiles,
    Preset,
    Scale,
    /// One of the settings editor's commits (the GUI settings editor
    /// chunk), named by the lane field it edits — text commits and
    /// choice picks alike (the kind is attribution only; no busy
    /// marker matches it).
    General(crate::requests::Field),
}

/// One rendered connections item's address (round-5 chunk 8): what a
/// callback's item index resolves to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::adapter) enum ItemAddr {
    /// The card header; Remove addresses the card's first connector
    /// (the app removes the whole identity).
    Account {
        connector: String,
    },
    /// The Files toggle row's address (round-6 D-6.6): the card's
    /// first Nextcloud connector — the re-anchor target when the
    /// toggle picks On on a non-anchor card.
    Files {
        connector: String,
    },
    /// The files roster's per-row action seam (round-7 D-7.3, W-3/D1
    /// — round-6's title-only visibility row grew the roster grammar,
    /// overturning round-6 D-6's "disconnect is a non-goal" ruling;
    /// authority: the round-7 plan §2 D-7.3). `connector` is the
    /// card's files connector — the busy marker's target and the
    /// lane-on fact's re-derivation source at the trust boundary.
    FilesBoard {
        board: BoardId,
        connector: String,
    },
    Deck {
        connector: String,
    },
    Remote {
        connector: String,
        remote: RemoteBoardId,
        bound: Option<BoardId>,
    },
}
impl AutoListBackoff {
    /// Records one more failure (the re-arm, AC-2).
    pub(in crate::adapter) fn record_failure(&mut self, now: tokio::time::Instant) {
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
        self.last_failure = now;
    }

    /// The suppression window: `min(30s * 2^(n-1), 10min)` — the
    /// shift caps at 5 so the doubling saturates at the 10-minute
    /// ceiling instead of overflowing.
    pub(in crate::adapter) fn window(&self) -> std::time::Duration {
        let doubling = 1u32 << self.consecutive_failures.saturating_sub(1).min(5);
        std::time::Duration::from_secs(30)
            .saturating_mul(doubling)
            .min(std::time::Duration::from_secs(600))
    }

    /// Whether the mount auto-list stays suppressed right now.
    fn active(&self, now: tokio::time::Instant) -> bool {
        now.saturating_duration_since(self.last_failure) < self.window()
    }
}

/// The suppression predicate over the backoff map (the mount
/// auto-list's one gate; the seam the request-count tests resolve).
pub(in crate::adapter) fn auto_list_suppressed(
    backoff: &BTreeMap<String, AutoListBackoff>,
    connector: &str,
    now: tokio::time::Instant,
) -> bool {
    backoff
        .get(connector)
        .is_some_and(|entry| entry.active(now))
}
/// The settings surface's adapter half (round-1 chunk 3, D7/D8/D9):
/// the form mirrors, the send seams, the reply drain, and the
/// liveness overrides. Every reply path is non-blocking — the drain
/// polls `try_recv` under the repaint timer, and a dead lane
/// degrades to a cleared pending state (R-reply).
/// The settings view-model: the adapter-side state the settings view
/// renders from. The draft is the `settings_form` mirror (the buffer
/// precedent); the password clears on send — write-only by
/// construction (D26b).
/// The auto-list backoff's typed state (US-3, D-4): consecutive
/// failures and the last failure's instant. The window doubles per
/// consecutive failure (`min(30s * 2^(n-1), 10min)`); the clock is
/// tokio's (pause/advance-testable).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AutoListBackoff {
    pub(in crate::adapter) consecutive_failures: u32,
    pub(in crate::adapter) last_failure: tokio::time::Instant,
}

impl Adapter {
    /// The mount auto-list (round-6 D-6.7): one `ListRemoteBoards`
    /// per connector without a landed listing, fired only from
    /// `settings_view_mounted` — deferred to the first settings
    /// visit, never at boot. Guards against double-firing: a landed
    /// listing (`remotes`), an in-flight auto listing
    /// (`auto_listings`), and the connector's manual refresh riding
    /// the pending machine. A full/closed request lane degrades to a
    /// log line (the `send` choke-point precedent); the map slot is
    /// what keeps the guard, so a failed `try_send` inserts nothing.
    pub(in crate::adapter) fn fire_auto_listings(&mut self) {
        for row in &self.settings.rows {
            let connector = row.name.clone();
            // The backoff gate (US-3 AC-1, D-4): inside the window no
            // request fires and the card renders the failed truth
            // with the retry posture.
            if auto_list_suppressed(
                &self.settings.auto_list_backoff,
                &connector,
                tokio::time::Instant::now(),
            ) {
                continue;
            }
            if self.settings.remotes.contains_key(&connector)
                || self.settings.auto_listings.contains_key(&connector)
                || (self.settings.pending == Some(PendingKind::ListRemote)
                    && self.settings.busy_target == connector)
            {
                continue;
            }
            let (reply, reply_rx) = oneshot::channel();
            let request = UiRequest::ListRemoteBoards {
                connector: connector.clone(),
                reply,
            };
            if self.requests.try_send(request).is_ok() {
                self.settings.auto_listings.insert(connector, reply_rx);
            } else {
                tracing::warn!(
                    "request channel full or closed; the settings auto-list was dropped"
                );
            }
        }
    }

    // ------------------------------------------------------------------
    // -- the export dialog (the J2 export plan W-1..W-5) ---------------
    // ------------------------------------------------------------------

    /// The root page's mint (the kiosk gate, R-2): the export
    /// dialog's action-menu entry point is ABSENT on kiosk — the
    /// matrix ruling the plan §2 records (no kiosk affordance), not
    /// a greyed row (D-4's disabled-with-caption doctrine is the
    /// dialog's own posture, not the entry points').
    pub(in crate::adapter) fn mint_root_entries(
        &self,
        states: &std::collections::BTreeMap<ActionId, bool>,
    ) -> Vec<MenuEntry> {
        let mut page = menu::root_page(&self.keymap, &self.menu_chain, states);
        if self.kiosk {
            // The desktop-only rows render absent on kiosk (the
            // ExportDialogOpen precedent; UF2 R-2 adds the picker).
            page.retain(|entry| {
                entry.action != Some(ActionId::ExportDialogOpen)
                    && entry.action != Some(ActionId::AppUndoBranches)
            });
        }
        page
    }

    /// The close's outcomes (the interpretation arm's body, shared
    /// with the seam): the VM and the pending reply drop (US-2/S2's
    /// escape law — nothing dispatches, the app-side run completes
    /// or refuses independently), the insert mode unwinds.
    pub(in crate::adapter) fn export_dialog_close_outcomes(&mut self) -> Vec<FlowOutcome> {
        if self.export_vm.take().is_none() {
            return Vec::new();
        }
        self.export_reply = None;
        // The preview/copy lanes' escape law (the D2 preview plan
        // D-7): the app-side generation completes or refuses
        // independently — the drain consumes the reply, the stash
        // clears with the dialog.
        self.export_preview_reply = None;
        self.export_preview_settled = None;
        self.export_copy_reply = None;
        self.export_copy_settled = None;
        if self.shell.mode == Mode::Insert {
            vec![FlowOutcome::SetMode(Mode::Normal), FlowOutcome::RefreshNow]
        } else {
            vec![FlowOutcome::RefreshNow]
        }
    }

    /// The pickers' mirror (W-3): the typed index contract mapped at
    /// the trust boundary — a stale index is absorbed (the silent
    /// law). The family pick clears the outcome (a new family's run
    /// is a new fact).
    pub(crate) fn export_spec_picked(&mut self, control: i32, index: i32) {
        use crate::export_dialog::{FAMILY_PICKED, FORMAT_PICKED, LAYOUT_PICKED};
        use crate::requests::{ExportFamily, ExportFileFormat, PdfLayoutChoice};
        let Some(vm) = self.export_vm.as_mut() else {
            return;
        };
        match control {
            FAMILY_PICKED => {
                vm.family = match index {
                    0 => ExportFamily::Tracking,
                    1 => ExportFamily::Pdf,
                    _ => return,
                };
                vm.outcome = None;
                // The D-5 family-gating fold: leaving the csv/json leg
                // invalidates the pane and the copy fold (the stale
                // family's facts never render).
                if vm.family != crate::requests::ExportFamily::Tracking {
                    vm.preview_shown = false;
                    vm.preview_text.clear();
                    vm.preview_truncated = false;
                    vm.preview_settled_refusal = None;
                    vm.copy_settled = false;
                    vm.copy_refusal = None;
                }
            }
            FORMAT_PICKED => {
                vm.format = match index {
                    0 => ExportFileFormat::Csv,
                    1 => ExportFileFormat::Json,
                    _ => return,
                };
            }
            LAYOUT_PICKED => {
                vm.layout = match index {
                    0 => PdfLayoutChoice::Fold,
                    1 => PdfLayoutChoice::A4,
                    2 => PdfLayoutChoice::A5,
                    3 => PdfLayoutChoice::A6,
                    _ => return,
                };
            }
            _ => {}
        }
    }

    /// The fields' mirror (W-3): verbatim text into the buffer (the
    /// one truth); validity folds at the push pass.
    pub(crate) fn export_field_edited(&mut self, field: i32, text: &str) {
        use crate::export_dialog::{
            FIELD_BOARD, FIELD_DESTINATION, FIELD_SINCE, FIELD_TASK, FIELD_UNTIL,
        };
        let Some(vm) = self.export_vm.as_mut() else {
            return;
        };
        match field {
            FIELD_SINCE => vm.since = text.into(),
            FIELD_UNTIL => vm.until = text.into(),
            FIELD_TASK => vm.task = text.into(),
            FIELD_BOARD => vm.board = text.into(),
            FIELD_DESTINATION => vm.destination = text.into(),
            _ => {}
        }
    }

    /// The open-after checkbox's mirror (the pdf leg's D-2 control;
    /// the widget gates the dispatch on the feature flag).
    pub(crate) fn export_open_after_toggled(&mut self) {
        let Some(vm) = self.export_vm.as_mut() else {
            return;
        };
        vm.open_after = !vm.open_after;
    }

    /// The Run dispatch (W-4, D-5): the trust boundary re-checks the
    /// gate (never trusts the widget's enabled fold), mints the spec
    /// verbatim from the buffer, and sends over the request lane
    /// with the oneshot reply half. Busy posture on the send; a
    /// full/closed lane is the typed Failed outcome (the F-3
    /// warn-and-drop convention would orphan the wait).
    pub(crate) fn export_run_clicked(&mut self, window: &AppWindow) {
        let Some(vm) = self.export_vm.as_mut() else {
            return;
        };
        if !vm.run_enabled(&self.export_defaults) {
            return;
        }
        let spec = vm.mint_spec();
        let (reply, reply_rx) = oneshot::channel();
        let request = crate::requests::UiRequest::ExportRun { spec, reply };
        if self.requests.try_send(request).is_err() {
            // The lane died or is full: a typed failure, never a
            // silent absorb (the busy posture must not strand).
            vm.busy = false;
            vm.outcome = Some(crate::requests::ExportRunOutcome::Refused(
                crate::requests::ExportRefusal::LaneDropped,
            ));
            self.finish_mutation(window);
            return;
        }
        vm.busy = true;
        vm.outcome = None;
        self.export_reply = Some(reply_rx);
        self.finish_mutation(window);
    }

    /// The reply poll (W-5, D-5): the non-blocking poll the other
    /// drains run — a settled reply lifts the busy posture and
    /// STASHES the outcome for the display fold; the caller then
    /// invokes the window's `export-outcome-shown` callback OUTSIDE
    /// the adapter borrow (the re-entrancy law — the fold handler
    /// re-enters). A closed lane counts as settled (the sender died —
    /// the app half dropped the reply). Returns whether the callback
    /// must fire.
    pub(crate) fn poll_export_run(&mut self) -> bool {
        let Some(rx) = self.export_reply.as_mut() else {
            return false;
        };
        let settled = match rx.try_recv() {
            Ok(outcome) => Some(outcome),
            Err(oneshot::error::TryRecvError::Empty) => return false,
            Err(oneshot::error::TryRecvError::Closed) => {
                tracing::warn!("the export run reply lane died; the slot clears");
                Some(crate::requests::ExportRunOutcome::Refused(
                    crate::requests::ExportRefusal::LaneDropped,
                ))
            }
        };
        self.export_reply = None;
        if let Some(vm) = self.export_vm.as_mut() {
            vm.busy = false;
        }
        self.export_settled = settled;
        true
    }

    /// The W-5 display fold (the `export-outcome-shown` handler): the
    /// stashed settled outcome lands in the dialog's VM and the push
    /// re-renders the outcome area. No settled stash = absorbed (a
    /// stray dispatch).
    pub(crate) fn export_outcome_fold(&mut self, window: &AppWindow) {
        let Some(outcome) = self.export_settled.take() else {
            return;
        };
        if let Some(vm) = self.export_vm.as_mut() {
            vm.outcome = Some(outcome);
        }
        self.finish_mutation(window);
    }

    /// The dialog's pushed model (the recur-editor contract: the
    /// adapter computes it per pass, the widget never holds state).
    pub(in crate::adapter) fn export_dialog_model(&self) -> crate::ExportDialogModel {
        use crate::requests::ExportRunOutcome as Outcome;
        let vm = self
            .export_vm
            .as_ref()
            .expect("the model mints only while the dialog holds");
        let family = match vm.family {
            crate::requests::ExportFamily::Tracking => 0,
            crate::requests::ExportFamily::Pdf => 1,
        };
        let format = match vm.format {
            crate::requests::ExportFileFormat::Csv => 0,
            crate::requests::ExportFileFormat::Json => 1,
        };
        let layout = match vm.layout {
            crate::requests::PdfLayoutChoice::Fold => 0,
            crate::requests::PdfLayoutChoice::A4 => 1,
            crate::requests::PdfLayoutChoice::A5 => 2,
            crate::requests::PdfLayoutChoice::A6 => 3,
        };
        let (outcome_kind, outcome_path, refusal) = match &vm.outcome {
            None => (crate::export_dialog::OUTCOME_NONE, String::new(), -1),
            Some(Outcome::Written { path }) => (
                crate::export_dialog::OUTCOME_WRITTEN,
                path.display().to_string(),
                -1,
            ),
            Some(Outcome::Refused(kind)) => (
                crate::export_dialog::OUTCOME_REFUSED,
                String::new(),
                crate::export_dialog::ExportVm::refusal_index(*kind),
            ),
        };
        crate::ExportDialogModel {
            family,
            format,
            layout,
            since: SharedString::from(vm.since.clone()),
            since_validity: crate::export_dialog::ExportVm::date_validity(&vm.since),
            until: SharedString::from(vm.until.clone()),
            until_validity: crate::export_dialog::ExportVm::date_validity(&vm.until),
            task: SharedString::from(vm.task.clone()),
            board: SharedString::from(vm.board.clone()),
            destination: SharedString::from(vm.destination.clone()),
            // The config default home as the placeholder (D-2/D-3):
            // the engine resolves the dated file name at run time.
            destination_placeholder: SharedString::from(match vm.family {
                crate::requests::ExportFamily::Tracking => self.export_defaults.export_dir.clone(),
                crate::requests::ExportFamily::Pdf => self.export_defaults.pdf_dir.clone(),
            }),
            open_after: vm.open_after,
            pdf_available: self.export_defaults.pdf_available,
            busy: vm.busy,
            run_enabled: vm.run_enabled(&self.export_defaults),
            outcome_kind,
            outcome_path: SharedString::from(outcome_path),
            refusal,
            // The preview/clipboard lane's folds (the D2 preview plan
            // W-3/W-4): the pane's facts verbatim, the per-affordance
            // busy flags, and the D-5 family-gating flag.
            preview_enabled: vm.family == crate::requests::ExportFamily::Tracking,
            preview_busy: vm.preview_busy,
            copy_busy: vm.copy_busy,
            preview_settled: vm.preview_shown || vm.preview_settled_refusal.is_some(),
            preview_shown: vm.preview_shown,
            preview_text: SharedString::from(vm.preview_text.clone()),
            preview_truncated: vm.preview_truncated,
            preview_refusal: vm
                .preview_settled_refusal
                .map_or(-1, crate::requests::refusal_index),
            copy_settled: vm.copy_settled,
            copy_bytes: vm.copy_bytes,
            copy_refusal: vm.copy_refusal.map_or(-1, crate::requests::refusal_index),
        }
    }

    /// The auto-listings' drain (round-6 D-6.7): the same
    /// non-blocking poll the other drains run, called beside
    /// [`Self::drain_setting_replies`] in the same repaint pass. A
    /// settled reply fills `remotes` and nothing else — never
    /// `pending`, `busy_target`, or the outcome banner (a background
    /// listing is silent by construction; the banner belongs to
    /// user-initiated ops). A failed reply clears the slot and leaves
    /// the row unlisted, so the Deck row's refresh hint stays. A
    /// closed lane counts as settled (the sender died); the slot
    /// clears so a later mount may retry.
    pub(crate) fn drain_auto_listings(&mut self, window: &AppWindow) -> bool {
        let mut settled: Vec<(String, Result<Vec<requests::RemoteBoardRow>, SettingsError>)> =
            Vec::new();
        for (connector, rx) in &mut self.settings.auto_listings {
            match rx.try_recv() {
                Ok(outcome) => settled.push((connector.clone(), outcome)),
                Err(oneshot::error::TryRecvError::Empty) => {}
                Err(oneshot::error::TryRecvError::Closed) => {
                    tracing::warn!("the settings auto-list reply lane died; the slot clears");
                    settled.push((connector.clone(), Err(SettingsError::VerifyFailed)));
                }
            }
        }
        let mut dirty = false;
        for (connector, outcome) in settled {
            self.settings.auto_listings.remove(&connector);
            match outcome {
                Ok(rows) => {
                    // A successful listing clears the backoff (AC-3).
                    self.settings.auto_list_backoff.remove(&connector);
                    self.settings.remotes.insert(connector, rows);
                }
                Err(err) => {
                    // The failure re-arms the backoff, one more
                    // consecutive failure than before (AC-2).
                    let now = tokio::time::Instant::now();
                    let entry = self
                        .settings
                        .auto_list_backoff
                        .entry(connector.clone())
                        .or_insert(AutoListBackoff {
                            consecutive_failures: 0,
                            last_failure: now,
                        });
                    entry.record_failure(now);
                    tracing::info!(variant = ?err, "the settings auto-list failed");
                }
            }
            dirty = true;
        }
        if dirty {
            // The model is a derive input outside the state Arc —
            // forced. `refresh_forced` ends in `push_settings`.
            self.refresh_forced(window);
        }
        dirty
    }

    /// Dispatches one `UiRequest::StatusQuery` (OP3 W-1, D-3): the
    /// reply half parks in [`StatusPanelVm`]; the slint-side busy
    /// posture holds until the reply folds or the channel drops (the
    /// typed failure — W-3). A full/closed lane degrades to the
    /// failure posture, never a wedge.
    pub(in crate::adapter) fn dispatch_status_query(&mut self, window: &AppWindow) {
        let (reply, reply_rx) = oneshot::channel();
        if self
            .requests
            .try_send(crate::requests::UiRequest::StatusQuery { reply })
            .is_ok()
        {
            self.status_panel.pending = Some(reply_rx);
            window.global::<crate::StatusPanelFacts>().set_busy(true);
        } else {
            tracing::warn!("request channel full or closed; status query dropped");
            window.invoke_status_failure_shown();
        }
    }

    /// The status reply drain (OP3 W-2/W-3): the same non-blocking
    /// poll the repaint cadence runs — a settled reply folds into the
    /// panel's typed facts; a dropped reply renders the failure
    /// posture.
    pub(crate) fn drain_status_reply(&mut self, window: &AppWindow) {
        let Some(rx) = self.status_panel.pending.as_mut() else {
            return;
        };
        match rx.try_recv() {
            Ok(snapshot) => {
                self.status_panel.pending = None;
                window.invoke_status_fields_folded(status_panel_data(&snapshot));
            }
            Err(oneshot::error::TryRecvError::Empty) => {}
            Err(oneshot::error::TryRecvError::Closed) => {
                self.status_panel.pending = None;
                window.invoke_status_failure_shown();
            }
        }
    }

    // -- the status panel's remediation lane (the D2 command plan
    //    W-1..W-4/W-6) ---------------------------------------------

    /// The remediation rows' arm moments (W-1/W-2/W-3): arm-only —
    /// nothing dispatches; the armed strip mounts. The permission
    /// fold (`kiosk`) is the adapter-side typed-permission gate (the
    /// trap-a law: a forbidden posture absorbs the arm — the rows are
    /// disabled there anyway; the gate never trusts the widget).
    pub(crate) fn status_remediation_arm(
        &mut self,
        window: &AppWindow,
        action: crate::requests::StatusRemediationAction,
    ) {
        if self.kiosk {
            tracing::info!(?action, "the remediation arm refused: kiosk posture");
            return;
        }
        {
            let facts = window.global::<crate::StatusPanelFacts>();
            facts.set_remediation_armed(match action {
                crate::requests::StatusRemediationAction::KillHolder => 0,
                crate::requests::StatusRemediationAction::ForceUnlock => 1,
                crate::requests::StatusRemediationAction::RestartDaemon => 2,
            });
        }
        self.status_panel.armed = Some(action);
    }

    /// The armed strip's decision pair (W-4): `0` = Confirm (the
    /// armed verb's exact dispatch — one `StatusRemediate` with the
    /// oneshot reply; busy posture on the strip), `1` = Cancel
    /// (nothing dispatched, the strip unmounts). A stale strip
    /// (disarmed under a foreign settle) absorbs the confirm.
    pub(crate) fn status_remediation_decision(&mut self, window: &AppWindow, decision: i32) {
        if decision != 0 {
            // Cancel: disarm, nothing dispatched.
            self.status_panel.armed = None;
            window
                .global::<crate::StatusPanelFacts>()
                .set_remediation_armed(-1);
            return;
        }
        let Some(action) = self.status_panel.armed else {
            return;
        };
        let (reply, reply_rx) = oneshot::channel();
        if self
            .requests
            .try_send(crate::requests::UiRequest::StatusRemediate { action, reply })
            .is_err()
        {
            // The lane died or is full: the typed infrastructure
            // refusal folds (the busy posture must not strand).
            self.remediation_settled = Some(crate::requests::StatusRemediationOutcome::Refused(
                crate::requests::StatusRemediationRefusal::IpcUnavailable,
            ));
            window.invoke_status_remediation_outcome_shown();
            return;
        }
        self.status_panel.remediation_pending = Some(reply_rx);
        window
            .global::<crate::StatusPanelFacts>()
            .set_remediation_busy(true);
    }

    /// The remediation reply poll (W-6): the same non-blocking poll
    /// the other drains run. A settled reply stashes for the fold and
    /// returns true (the callback fires OUTSIDE the adapter borrow);
    /// a closed lane counts as settled (the typed infrastructure
    /// refusal).
    pub(crate) fn poll_status_remediation(&mut self) -> bool {
        let Some(rx) = self.status_panel.remediation_pending.as_mut() else {
            return false;
        };
        let settled = match rx.try_recv() {
            Ok(outcome) => Some(outcome),
            Err(oneshot::error::TryRecvError::Empty) => return false,
            Err(oneshot::error::TryRecvError::Closed) => {
                tracing::warn!("the remediation reply lane died; the slot clears");
                Some(crate::requests::StatusRemediationOutcome::Refused(
                    crate::requests::StatusRemediationRefusal::IpcUnavailable,
                ))
            }
        };
        self.status_panel.remediation_pending = None;
        self.status_panel.armed = None;
        self.remediation_settled = settled;
        true
    }

    /// The W-6 display fold (the `status_remediation_outcome_shown`
    /// handler): the stashed outcome lands in the panel's facts, the
    /// busy posture clears (the callback body), and the post-fact
    /// refresh re-queries (D-3: the outcome posture is never stale —
    /// one `StatusQuery` per executed remediation or refusal). No
    /// stash = absorbed (a stray dispatch).
    pub(crate) fn status_remediation_outcome_fold(&mut self, window: &AppWindow) {
        let Some(outcome) = self.remediation_settled.take() else {
            return;
        };
        {
            let facts = window.global::<crate::StatusPanelFacts>();
            let (kind, refusal) = match &outcome {
                crate::requests::StatusRemediationOutcome::HolderSignalled => (1, -1),
                crate::requests::StatusRemediationOutcome::LockReleased => (2, -1),
                crate::requests::StatusRemediationOutcome::DaemonRestarted { .. } => (3, -1),
                crate::requests::StatusRemediationOutcome::Refused(kind) => {
                    // The canonical refusal index (the facts' doc
                    // table — one order, no drift).
                    let index = match kind {
                        crate::requests::StatusRemediationRefusal::HolderNotAlive => 0,
                        crate::requests::StatusRemediationRefusal::HolderAlive => 1,
                        crate::requests::StatusRemediationRefusal::SelfPid => 2,
                        crate::requests::StatusRemediationRefusal::SpawnFailed => 3,
                        crate::requests::StatusRemediationRefusal::IpcUnavailable => 4,
                    };
                    (4, index)
                }
            };
            facts.set_remediation_outcome_kind(kind);
            facts.set_remediation_refusal(refusal);
            facts.set_remediation_busy(false);
        }
        // The post-fact refresh (D-3): one re-query rides the settle.
        self.dispatch_status_query(window);
    }

    // -- the export dialog's preview/clipboard lane (the D2 preview
    //    plan W-1..W-4) -------------------------------------------

    /// The Preview dispatch (W-1, D-4): the trust boundary re-checks
    /// the gate (never trusts the widget), mints the spec verbatim
    /// from the buffer, and sends `ExportPreview` — never `ExportRun`
    /// (no file is written; the written-path fact stays Run's
    /// exclusively). Per-affordance busy on the send; a full/closed
    /// lane folds the typed `LaneDropped` refusal.
    pub(crate) fn export_preview_clicked(&mut self, window: &AppWindow) {
        let Some(vm) = self.export_vm.as_mut() else {
            return;
        };
        if vm.busy || !vm.run_enabled(&self.export_defaults) {
            return;
        }
        let spec = vm.mint_spec();
        let (reply, reply_rx) = oneshot::channel();
        let request = crate::requests::UiRequest::ExportPreview { spec, reply };
        if self.requests.try_send(request).is_err() {
            vm.preview_settled_refusal = Some(crate::requests::ExportRefusal::LaneDropped);
            self.finish_mutation(window);
            return;
        }
        vm.preview_busy = true;
        vm.preview_shown = false;
        vm.preview_settled_refusal = None;
        self.export_preview_reply = Some(reply_rx);
        self.finish_mutation(window);
    }

    /// The Copy dispatch (W-2, D-3): the same boundary law as the
    /// preview — the payload REGENERATES app-side; the pane's text is
    /// never read.
    pub(crate) fn export_copy_clicked(&mut self, window: &AppWindow) {
        let Some(vm) = self.export_vm.as_mut() else {
            return;
        };
        if vm.busy || !vm.run_enabled(&self.export_defaults) {
            return;
        }
        let spec = vm.mint_spec();
        let (reply, reply_rx) = oneshot::channel();
        let request = crate::requests::UiRequest::ExportClipboardCopy { spec, reply };
        if self.requests.try_send(request).is_err() {
            vm.copy_settled = true;
            vm.copy_refusal = Some(crate::requests::ExportRefusal::LaneDropped);
            self.finish_mutation(window);
            return;
        }
        vm.copy_busy = true;
        vm.copy_settled = false;
        vm.copy_refusal = None;
        self.export_copy_reply = Some(reply_rx);
        self.finish_mutation(window);
    }

    /// The preview reply poll (W-3): the non-blocking poll; settled
    /// replies stash for the `export_preview_shown` fold. A closed
    /// lane counts as settled (the typed `LaneDropped`).
    pub(crate) fn poll_export_preview(&mut self) -> bool {
        let Some(rx) = self.export_preview_reply.as_mut() else {
            return false;
        };
        let settled = match rx.try_recv() {
            Ok(outcome) => Some(outcome),
            Err(oneshot::error::TryRecvError::Empty) => return false,
            Err(oneshot::error::TryRecvError::Closed) => {
                Some(crate::requests::ExportPreviewOutcome::Refused(
                    crate::requests::ExportRefusal::LaneDropped,
                ))
            }
        };
        self.export_preview_reply = None;
        if let Some(vm) = self.export_vm.as_mut() {
            vm.preview_busy = false;
        }
        self.export_preview_settled = settled;
        true
    }

    /// The copy reply poll (the D-3 twin).
    pub(crate) fn poll_export_copy(&mut self) -> bool {
        let Some(rx) = self.export_copy_reply.as_mut() else {
            return false;
        };
        let settled = match rx.try_recv() {
            Ok(outcome) => Some(outcome),
            Err(oneshot::error::TryRecvError::Empty) => return false,
            Err(oneshot::error::TryRecvError::Closed) => {
                Some(crate::requests::ExportClipboardOutcome::Refused(
                    crate::requests::ExportRefusal::LaneDropped,
                ))
            }
        };
        self.export_copy_reply = None;
        if let Some(vm) = self.export_vm.as_mut() {
            vm.copy_busy = false;
        }
        self.export_copy_settled = settled;
        true
    }

    /// The W-3 preview fold (the `export_preview_shown` handler): the
    /// stashed outcome lands in the dialog's VM (payload + the typed
    /// cap fact, or the refusal kind). The dialog stays mounted with
    /// its filter state intact on any refusal.
    pub(crate) fn export_preview_fold(&mut self, window: &AppWindow) {
        let Some(outcome) = self.export_preview_settled.take() else {
            return;
        };
        if let Some(vm) = self.export_vm.as_mut() {
            vm.preview_busy = false;
            match outcome {
                crate::requests::ExportPreviewOutcome::Payload {
                    text, truncated, ..
                } => {
                    vm.preview_settled_refusal = None;
                    vm.preview_shown = true;
                    vm.preview_text = text;
                    vm.preview_truncated = truncated;
                }
                crate::requests::ExportPreviewOutcome::Refused(kind) => {
                    vm.preview_settled_refusal = Some(kind);
                    vm.preview_shown = false;
                }
            }
        }
        self.finish_mutation(window);
    }

    /// The W-3's copy twin fold: the copied-byte fact or the refusal
    /// kind lands in the VM.
    pub(crate) fn export_copy_fold(&mut self, window: &AppWindow) {
        let Some(outcome) = self.export_copy_settled.take() else {
            return;
        };
        if let Some(vm) = self.export_vm.as_mut() {
            vm.copy_busy = false;
            vm.copy_settled = true;
            match outcome {
                crate::requests::ExportClipboardOutcome::Copied { bytes } => {
                    vm.copy_refusal = None;
                    vm.copy_bytes = i32::try_from(bytes).unwrap_or(i32::MAX);
                }
                crate::requests::ExportClipboardOutcome::Refused(kind) => {
                    vm.copy_refusal = Some(kind);
                }
            }
        }
        self.finish_mutation(window);
    }

    /// The D-5 family-gating fold (the `export_preview_gated`
    /// handler): the family pick leaving the csv/json leg invalidates
    /// the preview pane and the copy fold (the gating is a fold flag,
    /// never hidden lanes; the pane never shows a stale family's
    /// payload). Same family = absorbed.
    pub(crate) fn export_preview_gated_fold(&mut self, window: &AppWindow) {
        let Some(vm) = self.export_vm.as_mut() else {
            return;
        };
        if vm.family != crate::requests::ExportFamily::Tracking {
            vm.preview_shown = false;
            vm.preview_text.clear();
            vm.preview_truncated = false;
            vm.preview_settled_refusal = None;
            vm.preview_settled_refusal = None;
            vm.copy_settled = false;
            vm.copy_refusal = None;
            self.finish_mutation(window);
        }
    }

    /// The board-reconcile preview's drain (OP1 US-3, D-4): the same
    /// non-blocking poll the other drains run, called beside them in
    /// the repaint pass. A settled reply routes into the armed flow's
    /// `ReconcilePreviewState` (Ready — the letter's counts land, or
    /// Failed — the typed dead-lane fact disarms); the receiver slot
    /// clears either way. A pending-but-flowless slot (the flow died
    /// under a settle) clears silently — the reply is discarded.
    pub(crate) fn drain_reconcile_preview(&mut self, window: &AppWindow) {
        // The flowless slot (the armed flow died under a settle or a
        // confirm): the pending reply is discarded — the flowless-slot
        // rule, checked before any poll.
        if !matches!(
            self.view_state.flow(),
            crate::view_state::Flow::ConfirmBoardReconcile { .. }
        ) {
            self.reconcile_preview = None;
            return;
        }
        let Some((board, rx)) = self.reconcile_preview.as_mut() else {
            return;
        };
        let settled = match rx.try_recv() {
            Ok(Some(counts)) => Some(Ok(counts)),
            Err(tokio::sync::oneshot::error::TryRecvError::Empty) => None,
            // A reply with no counts (the state read failed) is the
            // same not-derivable disarm as a dead lane — an unlettered
            // confirm is not informed consent (D-4's staging law).
            Ok(None) | Err(tokio::sync::oneshot::error::TryRecvError::Closed) => Some(Err(())),
        };
        let Some(outcome) = settled else {
            return;
        };
        let board = *board;
        self.reconcile_preview = None;
        let outcomes = self
            .view_state
            .settle_board_reconcile_preview(board, outcome);
        self.apply_outcomes(window, &outcomes);
    }
}

#[cfg(test)]
mod auto_list_backoff_tests {
    use super::*;
    use std::time::Duration;

    /// US-3 AC-1 — inside the window the mount auto-list is
    /// suppressed; past it the request fires again (typed fold over
    /// the backoff map, the request-count seam's gate).
    #[test]
    fn us3_ac1_backoff_suppresses_the_auto_list() {
        let now = tokio::time::Instant::now();
        let mut backoff = BTreeMap::new();
        backoff.insert(
            "deck".to_owned(),
            AutoListBackoff {
                consecutive_failures: 1,
                last_failure: now,
            },
        );
        assert!(auto_list_suppressed(
            &backoff,
            "deck",
            now + Duration::from_secs(10)
        ));
        assert!(
            !auto_list_suppressed(&backoff, "deck", now + Duration::from_secs(31)),
            "past the first-failure window the listing fires again"
        );
        assert!(
            !auto_list_suppressed(&backoff, "other", now),
            "an unbacked connector never suppresses"
        );
    }

    /// The window doubling: n failures widen the window up to the
    /// 10-minute ceiling (deterministic, no wall clock).
    #[test]
    fn the_window_doubles_and_caps() {
        let entry = |n: u32| AutoListBackoff {
            consecutive_failures: n,
            last_failure: tokio::time::Instant::now(),
        };
        assert_eq!(entry(1).window(), Duration::from_secs(30));
        assert_eq!(entry(2).window(), Duration::from_secs(60));
        assert_eq!(entry(3).window(), Duration::from_secs(120));
        assert_eq!(entry(6).window(), Duration::from_secs(600));
        assert_eq!(entry(50).window(), Duration::from_secs(600), "caps");
    }

    /// US-3 AC-2/AC-3 — the re-arm and the clear: a failure adds one
    /// consecutive failure; the map's remove (the refresh override
    /// and the success settle) resets the posture.
    #[test]
    fn us3_ac3_failure_re_arms_and_success_clears() {
        let now = tokio::time::Instant::now();
        let mut entry = AutoListBackoff {
            consecutive_failures: 0,
            last_failure: now,
        };
        entry.record_failure(now);
        assert_eq!(entry.consecutive_failures, 1);
        entry.record_failure(now);
        assert_eq!(entry.consecutive_failures, 2, "re-arms on each failure");
        let mut map = BTreeMap::new();
        map.insert("deck".to_owned(), entry);
        map.remove("deck");
        assert!(!auto_list_suppressed(&map, "deck", now), "clear resets");
    }
}
