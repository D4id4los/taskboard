// SPDX-License-Identifier: AGPL-3.0-only

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle as _, ModelRc, SharedString};

use taskboard_domain::{AppState, BoardId, RemoteBoardId};

use tokio::sync::oneshot;

use taskboard_domain::ConnectorId;

use super::Adapter;
use super::replies::{AutoListBackoff, ItemAddr, PendingKind, PendingReply};
use super::settings_vm::{
    GeneralField, SettingsOutcome, SettingsVm, map_form_field, map_form_kind, preset_index,
    request_scale_anchor,
};
use crate::AppWindow;
use crate::adapter::install_seam;
use crate::requests::{self, ConnectionProfile, KeymapUpdate, Secret, SettingsError, UiRequest};
use crate::settings_form::{self, ConnectionDraft};

impl Adapter {
    #[allow(clippy::too_many_lines)] // one arm per reply variant, kept flat
    pub(crate) fn drain_setting_replies(&mut self, window: &AppWindow) {
        let mut i = 0;
        let mut drained = false;
        while i < self.settings.replies.len() {
            // The settled reply's outcome, computed borrow-local and
            // routed through the one choke point below (r2 D28).
            let mut landed: Option<(SettingsOutcome, Option<SettingsError>)> = None;
            let settled = match &mut self.settings.replies[i] {
                PendingReply::Remotes(connector, rx) => match rx.try_recv() {
                    Ok(Ok(rows)) => {
                        // A successful manual listing clears the
                        // backoff too (US-3 AC-3 — the same posture
                        // as the auto lane's success).
                        self.settings.auto_list_backoff.remove(connector.as_str());
                        self.settings.remotes.insert(connector.clone(), rows);
                        landed = Some((SettingsOutcome::Ok, None));
                        true
                    }
                    Ok(Err(err)) => {
                        // The manual refresh's failure re-arms the
                        // backoff (US-3 AC-2 — the override cleared
                        // the entry before the send).
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
                        landed = Some((SettingsVm::outcome_of(&err), Some(err.clone())));
                        true
                    }
                    Err(oneshot::error::TryRecvError::Empty) => false,
                    Err(oneshot::error::TryRecvError::Closed) => {
                        landed = Some((SettingsOutcome::Failed, None));
                        true
                    }
                },
                PendingReply::List(rx) => match rx.try_recv() {
                    Ok(Ok(snapshot)) => {
                        // The connector rows re-render from the reply
                        // (r2 D32). The bindings do NOT read the
                        // reply's boards half (r4 D39 §2.3): the
                        // remote rows and the pickers' options are
                        // re-derived from the engine state on every
                        // push pass, so they cannot go stale by
                        // construction. A removed connector's listing
                        // leaves with it.
                        self.settings.rows = snapshot.connectors;
                        let rows = &self.settings.rows;
                        self.settings
                            .remotes
                            .retain(|name, _| rows.iter().any(|row| &row.name == name));
                        landed = Some((SettingsOutcome::Ok, None));
                        true
                    }
                    Ok(Err(err)) => {
                        landed = Some((SettingsVm::outcome_of(&err), Some(err.clone())));
                        true
                    }
                    Err(oneshot::error::TryRecvError::Empty) => false,
                    Err(oneshot::error::TryRecvError::Closed) => {
                        landed = Some((SettingsOutcome::Failed, None));
                        true
                    }
                },
                PendingReply::Keymap(rx) => match rx.try_recv() {
                    Ok(Ok(update)) => {
                        self.apply_keymap_update(window, update);
                        landed = Some((SettingsOutcome::Ok, None));
                        true
                    }
                    Ok(Err(err)) => {
                        landed = Some((SettingsVm::outcome_of(&err), Some(err.clone())));
                        true
                    }
                    Err(oneshot::error::TryRecvError::Empty) => false,
                    Err(oneshot::error::TryRecvError::Closed) => {
                        landed = Some((SettingsOutcome::Failed, None));
                        true
                    }
                },
                PendingReply::Snapshot(rx) => match rx.try_recv() {
                    Ok(Ok(snapshot)) => {
                        // The reply echoes the fresh file values (r2
                        // D32's rows-rerender rule, generalized). The
                        // env set is the hand-over's standing state —
                        // replies carry it empty by contract; the VM
                        // preserves its set, never overwrites.
                        let env = self.settings.snapshot.env_overridden.clone();
                        self.settings.snapshot = snapshot;
                        self.settings.snapshot.env_overridden = env;
                        landed = Some((SettingsOutcome::Ok, None));
                        true
                    }
                    Ok(Err(err)) => {
                        landed = Some((SettingsVm::outcome_of(&err), Some(err.clone())));
                        true
                    }
                    Err(oneshot::error::TryRecvError::Empty) => false,
                    Err(oneshot::error::TryRecvError::Closed) => {
                        landed = Some((SettingsOutcome::Failed, None));
                        true
                    }
                },
                PendingReply::Scale(rx, previous) => match rx.try_recv() {
                    Ok(Ok(())) => {
                        landed = Some((SettingsOutcome::Ok, None));
                        true
                    }
                    Ok(Err(err)) => {
                        landed = Some((SettingsVm::outcome_of(&err), Some(err.clone())));
                        // The live-apply revert (D9): the theme value
                        // goes back to the previous setting, and the
                        // scroll re-anchors on the scale row (r3
                        // D20c) — the content height returns and the
                        // row stays where it was.
                        request_scale_anchor(window);
                        #[allow(clippy::cast_possible_truncation)]
                        window
                            .global::<crate::Theme>()
                            .set_base_scale(*previous as f32);
                        true
                    }
                    Err(oneshot::error::TryRecvError::Empty) => false,
                    Err(oneshot::error::TryRecvError::Closed) => {
                        landed = Some((SettingsOutcome::Failed, None));
                        true
                    }
                },
            };
            if settled {
                let _ = self.settings.replies.remove(i);
                self.settings.pending = None;
                self.settings.busy_target.clear();
                self.settings.busy_remote = None;
                self.settings.busy_board = None;
                if let Some((outcome, detail)) = landed {
                    self.set_settings_outcome(window, outcome, detail);
                }
                drained = true;
            } else {
                i += 1;
            }
        }
        if drained {
            // The keymap swap / reply settle are derive inputs outside
            // the state Arc — forced. `refresh_forced` ends in
            // `push_settings`, so no separate push here.
            self.refresh_forced(window);
        }
    }

    /// The single outcome choke point (r2 D28): every assignment of
    /// the settings outcome routes through here — assign outcome +
    /// detail, drive the auto-clear timer (a pending op stops it; a
    /// terminal outcome re-arms it), then push. No outcome write may
    /// bypass it.
    fn set_settings_outcome(
        &mut self,
        window: &AppWindow,
        outcome: SettingsOutcome,
        detail: Option<SettingsError>,
    ) {
        self.settings.outcome = outcome;
        self.settings.detail = detail;
        if outcome == SettingsOutcome::Busy {
            // A pending op must never auto-clear mid-flight.
            self.status_timer.stop();
        } else {
            // Every new terminal outcome re-arms the clear.
            self.status_timer.start(
                slint::TimerMode::SingleShot,
                std::time::Duration::from_millis(crate::settings_status::STATUS_VISIBLE_MS),
                {
                    let fire = Rc::clone(&self.status_fire);
                    move || fire()
                },
            );
        }
        self.push_settings(window);
    }

    /// The rotation flip (K13): tick the state machine, reset the
    /// `FocusScrollHint` global (the stale-hint guard — a
    /// board-column hint surviving into a Today flip can never seek;
    /// the nonce bump is a guarded no-op seek, `row < 0` returns),
    /// apply the flip's outcomes (never a Command — the state
    /// machine's unit sweep pins that), and return the full dwell for
    /// the re-arm.
    pub(in crate::adapter) fn rotation_tick(&mut self, window: &AppWindow) -> std::time::Duration {
        let Some(rotation) = self.rotation.as_mut() else {
            return std::time::Duration::ZERO;
        };
        let mode = rotation.tick();
        let dwell = rotation.dwell();
        let state = self.state.load();
        let outcomes = self.view_state.enter_kiosk_mode(&state, mode);
        // The kiosk flip swaps the mounted view outside `apply_effect`
        // entirely (r4 chunk 4, D14's enumerate-once table row) — an
        // explicit dismiss; cheap totality even though a passive kiosk
        // display rarely hovers.
        Self::dismiss_tooltip(window);
        let scroll = window.global::<crate::FocusScrollHint>();
        scroll.set_target_a(-1);
        scroll.set_target_b(-1);
        scroll.set_nonce(scroll.get_nonce() + 1);
        self.apply_outcomes(window, &outcomes);
        dwell
    }

    /// The dwell re-arm: every flip re-arms the FULL dwell — no
    /// carry-over, no interaction pause (D-V3: the display is
    /// passive; rotation wins over operator navigation).
    pub(in crate::adapter) fn rearm_rotation(&mut self, dwell: std::time::Duration) {
        let fire = Rc::clone(&self.rotation_fire);
        self.rotation_timer
            .start(slint::TimerMode::SingleShot, dwell, move || fire());
    }

    /// The auto-clear fire (r2 D28): a stale fire during a new Busy
    /// is a no-op; otherwise the status returns to hidden (and the
    /// next refresh cannot resurrect it — the VM clears, not the
    /// property).
    pub(in crate::adapter) fn settings_status_expired(&mut self, window: &AppWindow) {
        self.status_timer.stop();
        if self.settings.pending.is_some() {
            return;
        }
        self.settings.outcome = SettingsOutcome::None;
        self.settings.detail = None;
        self.push_settings(window);
    }

    /// The live keymap swap (D9): the new map drives resolution, menu
    /// hints, and tooltips from the next refresh on; the findings
    /// counter updates the status read. The replied preset is the
    /// selection echo's source (r2 D27) — it lands on the model with
    /// the next `push_settings`, never optimistically at click time.
    fn apply_keymap_update(&mut self, window: &AppWindow, update: KeymapUpdate) {
        self.keymap = update.keymap;
        self.preset = update.preset;
        self.settings.preset = update.preset;
        window
            .global::<crate::UiStatus>()
            .set_startup_keymap_findings(i32::try_from(update.findings).unwrap_or(i32::MAX));
    }

    /// The validity flavour pass (round-3 D19): each field's `FieldStatus`
    /// recomputed from the form ladder — the same source of truth the
    /// Save gate reads — plus the per-field error string, which is the
    /// status line's own sentence for the same (field, kind) pair (zero
    /// new presentation code). The password's validity is computed from
    /// the draft, which clears on send: a successful save never leaves a
    /// stale red border behind. The scale field's flavour comes from its
    /// staged draft only: with no draft the field merely echoes the live
    /// scale, which is valid by construction — a permanent success border
    /// would be noise, so that state is neutral. The scale echo is
    /// draft-when-present, live-scale-otherwise (round-3 D20a, finding
    /// 23): written on every pass so the field can never drift from the
    /// applied value.
    fn push_settings_validity(global: &crate::SettingsModel, vm: &SettingsVm, live_scale: f64) {
        fn apply(
            global: &crate::SettingsModel,
            field: settings_form::Field,
            status: settings_form::FieldStatus,
        ) {
            let (validity, error) = match status {
                settings_form::FieldStatus::Neutral => (crate::FieldValidity::Neutral, ""),
                settings_form::FieldStatus::Valid => (crate::FieldValidity::Valid, ""),
                settings_form::FieldStatus::Invalid(kind) => (
                    crate::FieldValidity::Invalid,
                    crate::settings_status::invalid_text(
                        map_form_field(field),
                        map_form_kind(kind),
                    ),
                ),
            };
            match field {
                settings_form::Field::ServerUrl => {
                    global.set_validity_server(validity);
                    global.set_error_server(error.into());
                }
                settings_form::Field::Username => {
                    global.set_validity_username(validity);
                    global.set_error_username(error.into());
                }
                settings_form::Field::Password => {
                    global.set_validity_password(validity);
                    global.set_error_password(error.into());
                }
            }
        }
        for field in [
            settings_form::Field::ServerUrl,
            settings_form::Field::Username,
            settings_form::Field::Password,
        ] {
            apply(global, field, settings_form::field_status(field, &vm.draft));
        }
        let scale_status = vm.scale_draft.as_deref().map_or(
            settings_form::FieldStatus::Neutral,
            settings_form::scale_status,
        );
        let (scale_validity, scale_error) = match scale_status {
            settings_form::FieldStatus::Neutral => (crate::FieldValidity::Neutral, ""),
            settings_form::FieldStatus::Valid => (crate::FieldValidity::Valid, ""),
            settings_form::FieldStatus::Invalid(kind) => (
                crate::FieldValidity::Invalid,
                crate::settings_status::invalid_text(
                    crate::requests::Field::Scale,
                    map_form_kind(kind),
                ),
            ),
        };
        global.set_validity_scale(scale_validity);
        global.set_error_scale(scale_error.into());
        global.set_scale_echo(
            vm.scale_draft
                .clone()
                .unwrap_or_else(|| settings_form::format_scale(live_scale))
                .into(),
        );
    }

    /// The account cards' walk-ordered projection (round-5 chunk 8,
    /// D-8.1): per card the header, the Files toggle (the default
    /// connector's identity only — the D-SF10 contract), and per
    /// connector a Deck row plus its remote rows (the last listing's
    /// universe; bindings resolve from the live state). Returns the
    /// rendered items and their address table, index-aligned.
    #[allow(clippy::too_many_lines)] // one walk-ordered fold, one arm per item kind
    fn connection_items_model(
        state: &AppState,
        vm: &SettingsVm,
    ) -> (Vec<crate::ConnectionItem>, Vec<ItemAddr>) {
        use crate::presentation::ConnectorStatus as S;
        let row_status = |status: S| match status {
            S::Idle => crate::SettingsRowStatus::Idle,
            S::Syncing => crate::SettingsRowStatus::Syncing,
            S::Offline => crate::SettingsRowStatus::Offline,
            S::Failed => crate::SettingsRowStatus::Failed,
            S::Down => crate::SettingsRowStatus::Down,
        };
        let pending = vm.pending;
        let blank = crate::ConnectionItem {
            kind: crate::ConnectionItemKind::Account,
            title: SharedString::new(),
            detail: SharedString::new(),
            show_title: false,
            status: crate::SettingsRowStatus::Idle,
            busy: false,
            bound: false,
            listed: false,
            files_on: false,
            pending_phase: crate::models::publish_card_phase(None),
            restore_offerable: false,
            adoption_phase: crate::models::files_board_adoption(
                crate::presentation::FilesBoardAdoption::None,
            ),
            files_enabled_pinned: false,
            files_anchor_pinned: false,
            files_restart: false,
            backend_word: SharedString::new(),
            badge_text: SharedString::new(),
            badge_tone: crate::SettingsBadgeTone::Neutral,
            action_label: SharedString::new(),
            action_enabled: false,
            archived: false,
            armed: false,
        };
        // The backend word per connector (round 6, D-6.8 phase 1): the
        // adapter owns the mapping, the UI stays dumb. Standalone
        // connectors drop the "Deck" prefix — the row shows the
        // connector's own name.
        let backend_word = |name: &str| {
            vm.rows
                .iter()
                .find(|row| row.name == name)
                .map(|row| match row.backend {
                    crate::requests::ConnectorBackend::Nextcloud => SharedString::from("Deck"),
                    crate::requests::ConnectorBackend::Standalone => SharedString::new(),
                })
                .unwrap_or_default()
        };
        let mut items = Vec::new();
        let mut addrs = Vec::new();
        for card in crate::presentation::account_cards(&vm.rows) {
            let status = crate::presentation::account_status(state, &card, |name| {
                vm.liveness.get(name).copied().unwrap_or(false)
            });
            items.push(crate::ConnectionItem {
                kind: crate::ConnectionItemKind::Account,
                title: format!("{} @ {}", card.username, card.server_url).into(),
                status: row_status(status),
                busy: pending == Some(PendingKind::Remove)
                    && card.connectors.contains(&vm.busy_target),
                ..blank.clone()
            });
            addrs.push(ItemAddr::Account {
                connector: card.connectors[0].clone(),
            });
            // The Files row rides every Nextcloud account card
            // (round-6 D-6.6): the one global flag shown as per-card
            // truth — `files_on` = enabled ∧ this card holds the
            // resolved anchor, so exactly one card renders On. The
            // re-anchor target is the card's first Nextcloud
            // connector (deterministic walk order). Standalone cards
            // carry no Files row.
            let is_nextcloud = |name: &str| {
                vm.rows
                    .iter()
                    .find(|row| row.name == name)
                    .is_some_and(|row| row.backend == crate::requests::ConnectorBackend::Nextcloud)
            };
            if let Some(files_connector) = card
                .connectors
                .iter()
                .find(|name| is_nextcloud(name))
                .cloned()
            {
                let files_on = vm.snapshot.files_sync_enabled
                    && vm.snapshot.files_sync_anchor.as_deref() == Some(files_connector.as_str());
                items.push(crate::ConnectionItem {
                    kind: crate::ConnectionItemKind::Files,
                    files_on,
                    // The move ceremony rides the Files row (round-6
                    // D-6.8 phase 2, D-9): one BusyDot, no fake
                    // progress — the reply is the next truth.
                    busy: pending == Some(PendingKind::Move) && vm.busy_target == files_connector,
                    files_enabled_pinned: crate::presentation::files_enabled_pinned(
                        &vm.snapshot.env_overridden,
                    ),
                    files_anchor_pinned: crate::presentation::files_anchor_pinned(
                        &vm.snapshot.env_overridden,
                    ),
                    files_restart: vm.snapshot.applies.get("files_sync.enabled").is_some_and(
                        |semantics| semantics.apply == crate::requests::SectionApply::OnRestart,
                    ),
                    ..blank.clone()
                });
                addrs.push(ItemAddr::Files {
                    connector: files_connector.clone(),
                });
                // The per-account roster (round-7 D-7.3, W-3/D11): the
                // Files section's body — every live board once per
                // Nextcloud card, channel-tagged, sorted, one action
                // per row, all composed Rust-side by the fold. Binding
                // truth re-derived from the engine state every pass;
                // rendering is independent of the lane switch
                // (toggling Files off freezes the sync, the binding
                // persists — hiding the rows would fake the world).
                for row in
                    crate::presentation::files_board_roster(state, files_on, &files_connector)
                {
                    let busy = matches!(
                        pending,
                        Some(PendingKind::StopFilesSync | PendingKind::PublishFiles)
                    ) && vm.busy_board == Some(row.board);
                    items.push(crate::ConnectionItem {
                        kind: crate::ConnectionItemKind::FilesBoard,
                        title: row.title.clone().into(),
                        busy,
                        restore_offerable: row.restore_offerable,
                        adoption_phase: {
                            // The adoption phase (US-8, D-8): keyed by
                            // the files identity (the board's own
                            // domain uuid); boards absent from the map
                            // render the settled truth.
                            let files_uuid = state
                                .boards
                                .get(&row.board)
                                .and_then(|b| b.remote.as_ref())
                                .and_then(|binding| binding.board.as_files());
                            let adoption = files_uuid
                                .and_then(|uuid| vm.files_board_states.get(&uuid))
                                .map_or(crate::presentation::FilesBoardAdoption::None, |st| {
                                    st.adoption
                                });
                            crate::models::files_board_adoption(adoption)
                        },
                        badge_text: crate::presentation::files_board_badge_label(row.badge).into(),
                        badge_tone: crate::presentation::files_board_badge_tone(row.badge),
                        action_label: row
                            .action
                            .map(crate::presentation::files_board_action_label)
                            .unwrap_or_default()
                            .into(),
                        action_enabled: row.action_enabled,
                        archived: row.archived,
                        ..blank.clone()
                    });
                    addrs.push(ItemAddr::FilesBoard {
                        board: row.board,
                        connector: files_connector.clone(),
                    });
                }
            }
            let several = card.connectors.len() > 1;
            for connector in &card.connectors {
                let listing = vm.remotes.get(connector);
                items.push(crate::ConnectionItem {
                    kind: crate::ConnectionItemKind::Deck,
                    title: connector.clone().into(),
                    backend_word: backend_word(connector),
                    show_title: several,
                    busy: matches!(
                        pending,
                        Some(
                            PendingKind::ListRemote
                                | PendingKind::Attach
                                | PendingKind::CreateBind
                                | PendingKind::Detach
                                | PendingKind::Publish
                        )
                    ) && vm.busy_target == *connector,
                    // The ceremony's card-local phase (US-5, D-6):
                    // display-only, the addressed connector only.
                    pending_phase: {
                        let publishing =
                            pending == Some(PendingKind::Publish) && vm.busy_target == *connector;
                        let adopting = pending == Some(PendingKind::PublishFiles)
                            && vm.busy_target == *connector;
                        crate::models::publish_card_phase(crate::presentation::publish_card_phase(
                            (publishing || adopting).then_some((publishing, adopting)),
                        ))
                    },
                    listed: listing.is_some(),
                    ..blank.clone()
                });
                addrs.push(ItemAddr::Deck {
                    connector: connector.clone(),
                });
                // The listing's rows (round-5 chunk 8), then the
                // bindings-without-listing rows (round-6 D-6.7): a
                // live binding on this connector whose remote is not
                // among the landed listing still renders — fully
                // actionable — titled by the local board (the remote
                // title is unknown until a listing enriches it).
                let listed_remotes: Vec<_> = listing
                    .into_iter()
                    .flatten()
                    .map(|remote| remote.remote_id)
                    .collect();
                let live_bindings: Vec<_> = state
                    .boards
                    .values()
                    .filter(|board| {
                        !board.deleted
                            && !board.archived
                            && board
                                .remote
                                .as_ref()
                                .is_some_and(|binding| binding.connector.as_str() == connector)
                    })
                    .collect();
                let unlisted_bindings: Vec<_> = live_bindings
                    .iter()
                    .filter(|board| {
                        board
                            .remote
                            .as_ref()
                            .is_some_and(|binding| !listed_remotes.contains(&binding.board))
                    })
                    .collect();
                for remote in listing.into_iter().flatten() {
                    let bound = live_bindings
                        .iter()
                        .find(|board| {
                            board
                                .remote
                                .as_ref()
                                .is_some_and(|binding| binding.board == remote.remote_id)
                        })
                        .copied();
                    items.push(crate::ConnectionItem {
                        kind: crate::ConnectionItemKind::Remote,
                        title: remote.title.clone().into(),
                        detail: bound.map(|b| b.title.clone()).unwrap_or_default().into(),
                        bound: bound.is_some(),
                        busy: matches!(
                            pending,
                            Some(
                                PendingKind::Attach | PendingKind::CreateBind | PendingKind::Detach
                            )
                        ) && vm.busy_target == *connector
                            && vm.busy_remote == Some(remote.remote_id),
                        ..blank.clone()
                    });
                    addrs.push(ItemAddr::Remote {
                        connector: connector.clone(),
                        remote: remote.remote_id,
                        bound: bound.map(|b| b.id),
                    });
                }
                // The bindings whose remote is absent from the listing
                // (round-6 D-6.7): synthesized Remote rows, `bound` by
                // construction, titled by the local board, detail
                // empty (the remote title is unknown pre-listing).
                // The address is the real one, so detach/bind-pick
                // resolve unchanged — the row is fully actionable
                // before any listing exists.
                for board in &unlisted_bindings {
                    let binding = board
                        .remote
                        .as_ref()
                        .expect("unlisted_bindings filtered on a binding");
                    items.push(crate::ConnectionItem {
                        kind: crate::ConnectionItemKind::Remote,
                        title: board.title.clone().into(),
                        bound: true,
                        ..blank.clone()
                    });
                    addrs.push(ItemAddr::Remote {
                        connector: connector.clone(),
                        remote: binding.board,
                        bound: Some(board.id),
                    });
                }
            }
        }
        (items, addrs)
    }

    /// Pushes the settings view-model onto the window global (the
    /// refresh pass's settings half; rows fold liveness + the bound
    /// boards' phases, D8). The row lists ride their persistent
    /// handles through the positional diff (r2 D9).
    #[allow(clippy::too_many_lines)] // the per-pass push, one block per surface fact
    pub(crate) fn push_settings(&mut self, window: &AppWindow) {
        use crate::SettingsRowStatus as Row;
        let state = self.state.load();
        // The busy machine (r2 D34): the flag timeline is the model's;
        // the connector rows mark the Remove ceremony.
        let connector_busy = matches!(self.settings.pending, Some(PendingKind::Remove));
        // The pickers' options (r4 D39 §2.3's rule, kept): the unbound
        // live boards, re-derived from the engine state EVERY pass —
        // the mount-refresh by construction.
        let choices: Vec<requests::BoardInfo> = crate::presentation::boards_of(&state)
            .into_iter()
            .filter(|board| board.bound_connector.is_none())
            .collect();
        let options: Vec<crate::DropdownRow> = choices
            .iter()
            .map(|board| crate::DropdownRow {
                label: board.name.clone().into(),
                indicator: crate::DropdownIndicator::LinkOff,
                tone: crate::DropdownTone::Neutral,
                selected: false,
                busy: false,
            })
            .collect();
        self.settings.local_choices = choices.iter().map(|board| board.id).collect();
        let (mut items, addrs) = Self::connection_items_model(&state, &self.settings);
        self.settings.items = addrs;
        // The armed strip's pass rules (UX R-2 D-4): the busy lane
        // disarms (the outcome render for that lane — the disable
        // grammar prevents re-entry, the disarm keeps the grammar
        // honest), a vanished subject disarms (the stale-`ItemAddr`
        // dead-strip rule), and the armed flag mirrors per row.
        if self.settings.pending.is_some()
            || self
                .settings
                .armed_item
                .as_ref()
                .is_none_or(|armed| !self.settings.items.contains(armed))
        {
            self.settings.armed_item = None;
        }
        for (item, addr) in items.iter_mut().zip(&self.settings.items) {
            item.armed = self.settings.armed_item.as_ref() == Some(addr);
        }
        let rows: Vec<crate::ConnectionRow> = self
            .settings
            .rows
            .iter()
            .map(|row| {
                let id = ConnectorId::from(row.name.clone());
                let down = self.settings.liveness.get(row.name.as_str()).copied();
                let status =
                    crate::presentation::connector_row_status(&state, &id, down.unwrap_or(false));
                crate::ConnectionRow {
                    name: row.name.clone().into(),
                    server: row.server_url.clone().into(),
                    username: row.username.clone().into(),
                    busy: connector_busy && self.settings.busy_target == row.name,
                    status: match status {
                        crate::presentation::ConnectorStatus::Idle => Row::Idle,
                        crate::presentation::ConnectorStatus::Syncing => Row::Syncing,
                        crate::presentation::ConnectorStatus::Offline => Row::Offline,
                        crate::presentation::ConnectorStatus::Failed => Row::Failed,
                        crate::presentation::ConnectorStatus::Down => Row::Down,
                    },
                }
            })
            .collect();
        let global = window.global::<crate::SettingsModel>();
        self.models.push_settings_rows(&global, &rows);
        self.models.push_board_options(&global, &options);
        self.models.push_connection_items(&global, &items);
        // The page-usable guard (US-5 AC-1, D-6): the publish family
        // keeps the page's controls enabled — the addressed card's
        // phase badge carries the ceremony; the attach/remove family
        // keeps today's wholesale posture.
        global.set_busy(
            self.settings.pending.is_some_and(|kind| {
                !matches!(kind, PendingKind::Publish | PendingKind::PublishFiles)
            }),
        );
        global.set_busy_target(self.settings.busy_target.clone().into());
        global.set_draft_server(self.settings.draft.server_url.clone().into());
        global.set_draft_username(self.settings.draft.username.clone().into());
        global.set_save_enabled(self.settings.save_enabled());
        let live_scale = f64::from(window.global::<crate::Theme>().get_base_scale());
        Self::push_settings_validity(&global, &self.settings, live_scale);
        // The timers `when` field's validity flavour (r4 D27): the
        // same per-pass push, over the typed draft — the timer
        // grammar's own tz seam (`&chrono::Local`, the parse's
        // convention).
        let (when_validity, when_hint) = match crate::presentation::timer_when_validity(
            &self.timer_when_draft,
            self.day_ctx().now,
            &chrono::Local,
        ) {
            None => (crate::FieldValidity::Neutral, ""),
            Some(true) => (crate::FieldValidity::Valid, ""),
            Some(false) => (
                crate::FieldValidity::Invalid,
                crate::presentation::TIMER_SYNTAX_HINT,
            ),
        };
        window.set_timer_when_validity(when_validity);
        window.set_timer_when_error_hint(SharedString::from(when_hint));
        // The break-rule form's facts (T-14, #797): the form's open
        // flag and the threshold field's validity flavour (the
        // `when` field's convention — parse at the trust boundary
        // with the `WorkThreshold` mint's text grammar).
        window.set_break_create_open(self.break_rule_form_open);
        let (break_validity, break_hint) = if self.break_rule_threshold_draft.is_empty() {
            (crate::FieldValidity::Neutral, "")
        } else {
            match taskboard_domain::WorkThreshold::parse_text_form(&self.break_rule_threshold_draft)
            {
                Ok(_) => (crate::FieldValidity::Valid, ""),
                Err(_) => (
                    crate::FieldValidity::Invalid,
                    crate::presentation::BREAK_THRESHOLD_HINT,
                ),
            }
        };
        window.set_break_threshold_validity(break_validity);
        window.set_break_threshold_error_hint(SharedString::from(break_hint));
        // The journal ledger's editor facts (T-15, #798): open flag,
        // the staged echoes, the ratings' validity flavours (the
        // `Rating` mint's 1..=5; empty is neutral — the legal clear),
        // the curated flag chips' rendered membership, and the armed
        // strip's flag.
        window.set_reflection_editor_open(self.reflection_active.is_some());
        window.set_reflection_editor_text_echo(SharedString::from(
            self.reflection_text_draft.clone(),
        ));
        window.set_reflection_editor_mood_echo(SharedString::from(
            self.reflection_mood_draft.clone(),
        ));
        window.set_reflection_editor_energy_echo(SharedString::from(
            self.reflection_energy_draft.clone(),
        ));
        let rating_validity = |draft: &str| {
            if draft.is_empty() {
                crate::FieldValidity::Neutral
            } else if taskboard_domain::reflection::Rating::try_new(
                draft.parse::<u8>().unwrap_or(0),
            )
            .is_ok()
            {
                crate::FieldValidity::Valid
            } else {
                crate::FieldValidity::Invalid
            }
        };
        window.set_reflection_editor_mood_validity(rating_validity(&self.reflection_mood_draft));
        window
            .set_reflection_editor_energy_validity(rating_validity(&self.reflection_energy_draft));
        // The walk's rating drafts (#1431): the same grammar mirrored
        // onto the walk overlay's Prompts fields — the seeds ride the
        // drafts (blank is the neutral unrated), the flavours use the
        // `Rating` mint.
        let walk_rating_drafts = self
            .view_state
            .walk()
            .map_or((String::new(), String::new()), |walk| {
                (walk.mood_text.clone(), walk.energy_text.clone())
            });
        window.set_walk_mood_seed(SharedString::from(walk_rating_drafts.0.clone()));
        window.set_walk_energy_seed(SharedString::from(walk_rating_drafts.1.clone()));
        window.set_walk_mood_validity(rating_validity(&walk_rating_drafts.0));
        window.set_walk_energy_validity(rating_validity(&walk_rating_drafts.1));
        window.set_reflection_remove_armed(self.reflection_armed.is_some());
        let chips: Vec<crate::ReflectionFlagChip> = crate::presentation::HEALTH_FLAGS
            .iter()
            .map(|flag| {
                let current = self
                    .reflection_active
                    .and_then(|id| {
                        self.state
                            .load()
                            .reflection
                            .entries
                            .get(&id)
                            .map(|entry| entry.attributes.flags.contains(flag))
                    })
                    .unwrap_or(false);
                let member = if current {
                    !self.reflection_flags_remove.contains(flag)
                } else {
                    self.reflection_flags_add.contains(flag)
                };
                crate::ReflectionFlagChip {
                    label: SharedString::from(flag.text_form()),
                    member,
                }
            })
            .collect();
        window.set_reflection_editor_flags_model(ModelRc::new(slint::VecModel::from(chips)));
        global.set_preset_index(preset_index(self.settings.preset));
        global.set_status_text(SharedString::from(crate::settings_status::status_text(
            self.settings.outcome,
            self.settings.detail.as_ref(),
        )));
        global.set_last_outcome(match self.settings.outcome {
            SettingsOutcome::None => crate::SettingsOutcome::None,
            SettingsOutcome::Ok => crate::SettingsOutcome::Ok,
            SettingsOutcome::Busy => crate::SettingsOutcome::Busy,
            SettingsOutcome::Rejected => crate::SettingsOutcome::Rejected,
            SettingsOutcome::Unavailable => crate::SettingsOutcome::Unavailable,
            SettingsOutcome::Failed => crate::SettingsOutcome::Failed,
        });
        // The T-16 typed detail fact: the refusal's EnvLocked class
        // renders structurally (the tests' typed projection of the
        // detail variant — the sentence text is never asserted).
        global.set_last_error_env_locked(matches!(
            self.settings.detail,
            Some(SettingsError::EnvLocked { .. })
        ));
        // The T-38 typed detail fact (the env-locked fact's twin):
        // the busy lane's RequestPending class renders structurally —
        // the tests' typed projection of the detail variant.
        global.set_last_error_request_pending(matches!(
            self.settings.detail,
            Some(SettingsError::RequestPending)
        ));
        self.push_general_settings(&global);
    }

    /// The general editor's push half (the GUI settings editor): per
    /// field the echo (draft-when-present, committed-otherwise — the
    /// scale echo's grammar), the validity flavour (the same mirror
    /// predicate the commit seam refuses on), and the app-stated
    /// env-lock and apply facts; the choice echoes ride the snapshot;
    /// the read-only facts list re-renders per pass (the board
    /// options' re-derive precedent).
    #[allow(clippy::too_many_lines)] // per-field push, one arm per field
    fn push_general_settings(&mut self, global: &crate::SettingsModel) {
        let vm = &self.settings;
        for (gf, draft) in [
            (GeneralField::DayStart, &vm.day_start_draft),
            (GeneralField::TickSecs, &vm.tick_draft),
            (GeneralField::ReminderBefore, &vm.before_draft),
            (GeneralField::PollInterval, &vm.poll_draft),
            (GeneralField::BackoffInitial, &vm.backoff_initial_draft),
            (GeneralField::BackoffMax, &vm.backoff_max_draft),
            (GeneralField::DueSoonDays, &vm.due_soon_draft),
            (GeneralField::FocusWorkMins, &vm.focus_work_draft),
            (GeneralField::FocusBreakMins, &vm.focus_break_draft),
            (GeneralField::FocusLongBreakMins, &vm.focus_long_break_draft),
            (GeneralField::FocusLongBreakEvery, &vm.focus_every_draft),
            (GeneralField::FilesPollSecs, &vm.files_poll_secs_draft),
        ] {
            let predicate: fn(&str) -> Option<settings_form::InvalidKind> = match gf {
                GeneralField::DayStart => settings_form::day_start_invalid_kind,
                GeneralField::TickSecs
                | GeneralField::PollInterval
                | GeneralField::BackoffInitial
                | GeneralField::BackoffMax
                | GeneralField::FilesPollSecs => settings_form::secs_invalid_kind,
                GeneralField::ReminderBefore => settings_form::before_invalid_kind,
                GeneralField::DueSoonDays => settings_form::due_soon_invalid_kind,
                GeneralField::FocusWorkMins
                | GeneralField::FocusBreakMins
                | GeneralField::FocusLongBreakMins => settings_form::focus_mins_invalid_kind,
                GeneralField::FocusLongBreakEvery => settings_form::focus_every_invalid_kind,
            };
            let status = draft
                .as_deref()
                .map_or(settings_form::FieldStatus::Neutral, |text| {
                    settings_form::general_status(text, predicate)
                });
            let (validity, error) = match status {
                settings_form::FieldStatus::Neutral => (crate::FieldValidity::Neutral, ""),
                settings_form::FieldStatus::Valid => (crate::FieldValidity::Valid, ""),
                settings_form::FieldStatus::Invalid(kind) => (
                    crate::FieldValidity::Invalid,
                    crate::settings_status::invalid_text(gf.lane_field(), map_form_kind(kind)),
                ),
            };
            let echo: SharedString = draft
                .clone()
                .unwrap_or_else(|| gf.echo_of(&vm.snapshot))
                .into();
            let env_locked = vm.snapshot.env_overridden.contains(gf.key());
            let on_restart = vm
                .snapshot
                .applies
                .get(gf.key())
                .is_some_and(|a| a.apply == crate::requests::SectionApply::OnRestart);
            match gf {
                GeneralField::DayStart => {
                    global.set_day_start_echo(echo);
                    global.set_validity_day_start(validity);
                    global.set_error_day_start(error.into());
                    global.set_day_start_env_locked(env_locked);
                    global.set_day_start_on_restart(on_restart);
                }
                GeneralField::TickSecs => {
                    global.set_tick_echo(echo);
                    global.set_validity_tick(validity);
                    global.set_error_tick(error.into());
                    global.set_tick_env_locked(env_locked);
                    global.set_tick_on_restart(on_restart);
                }
                GeneralField::ReminderBefore => {
                    global.set_before_echo(echo);
                    global.set_validity_before(validity);
                    global.set_error_before(error.into());
                    global.set_before_env_locked(env_locked);
                    global.set_before_on_restart(on_restart);
                }
                GeneralField::PollInterval => {
                    global.set_poll_echo(echo);
                    global.set_validity_poll(validity);
                    global.set_error_poll(error.into());
                    global.set_poll_env_locked(env_locked);
                    global.set_poll_on_restart(on_restart);
                }
                GeneralField::BackoffInitial => {
                    global.set_backoff_initial_echo(echo);
                    global.set_validity_backoff_initial(validity);
                    global.set_error_backoff_initial(error.into());
                    global.set_backoff_initial_env_locked(env_locked);
                    global.set_backoff_initial_on_restart(on_restart);
                }
                GeneralField::BackoffMax => {
                    global.set_backoff_max_echo(echo);
                    global.set_validity_backoff_max(validity);
                    global.set_error_backoff_max(error.into());
                    global.set_backoff_max_env_locked(env_locked);
                    global.set_backoff_max_on_restart(on_restart);
                }
                GeneralField::DueSoonDays => {
                    global.set_due_soon_echo(echo);
                    global.set_validity_due_soon(validity);
                    global.set_error_due_soon(error.into());
                    global.set_due_soon_env_locked(env_locked);
                    global.set_due_soon_on_restart(on_restart);
                }
                GeneralField::FocusWorkMins => {
                    global.set_focus_work_echo(echo);
                    global.set_validity_focus_work(validity);
                    global.set_error_focus_work(error.into());
                    global.set_focus_work_env_locked(env_locked);
                    global.set_focus_work_on_restart(on_restart);
                }
                GeneralField::FocusBreakMins => {
                    global.set_focus_break_echo(echo);
                    global.set_validity_focus_break(validity);
                    global.set_error_focus_break(error.into());
                    global.set_focus_break_env_locked(env_locked);
                    global.set_focus_break_on_restart(on_restart);
                }
                GeneralField::FocusLongBreakMins => {
                    global.set_focus_long_break_echo(echo);
                    global.set_validity_focus_long_break(validity);
                    global.set_error_focus_long_break(error.into());
                    global.set_focus_long_break_env_locked(env_locked);
                    global.set_focus_long_break_on_restart(on_restart);
                }
                GeneralField::FocusLongBreakEvery => {
                    global.set_focus_every_echo(echo);
                    global.set_validity_focus_every(validity);
                    global.set_error_focus_every(error.into());
                    global.set_focus_every_env_locked(env_locked);
                    global.set_focus_every_on_restart(on_restart);
                }
                GeneralField::FilesPollSecs => {
                    global.set_files_poll_echo(echo);
                    global.set_validity_files_poll(validity);
                    global.set_error_files_poll(error.into());
                    global.set_files_poll_env_locked(env_locked);
                    // The registry's verdict (R-4): poll_secs applies
                    // live through the whole-snapshot lane; the same
                    // app-stated fact every live row renders.
                    global.set_files_poll_on_restart(on_restart);
                }
            }
        }
        // The choice echoes and facts.
        global.set_reminders_enabled_index(i32::from(!vm.snapshot.reminders_enabled));
        let enabled_locked = vm.snapshot.env_overridden.contains("reminders.enabled");
        global.set_reminders_enabled_env_locked(enabled_locked);
        global.set_reminders_enabled_on_restart(
            vm.snapshot
                .applies
                .get("reminders.enabled")
                .is_some_and(|a| a.apply == crate::requests::SectionApply::OnRestart),
        );
        let sink_index = match vm.snapshot.reminder_sink {
            crate::requests::ReminderSink::Auto => 0,
            crate::requests::ReminderSink::Log => 1,
            crate::requests::ReminderSink::Desktop => 2,
        };
        global.set_sink_index(sink_index);
        global.set_sink_env_locked(vm.snapshot.env_overridden.contains("reminders.sink"));
        global.set_sink_on_restart(
            vm.snapshot
                .applies
                .get("reminders.sink")
                .is_some_and(|a| a.apply == crate::requests::SectionApply::OnRestart),
        );
        // The focus choice echoes (commit-on-pick, the reminders
        // grammar): index 0 = On, 1 = Off.
        global.set_focus_enabled_index(i32::from(!vm.snapshot.focus_enabled));
        global.set_focus_enabled_env_locked(vm.snapshot.env_overridden.contains("focus.enabled"));
        global.set_focus_enabled_on_restart(
            vm.snapshot
                .applies
                .get("focus.enabled")
                .is_some_and(|a| a.apply == crate::requests::SectionApply::OnRestart),
        );
        global.set_focus_chain_index(i32::from(!vm.snapshot.focus_chain));
        global.set_focus_chain_env_locked(vm.snapshot.env_overridden.contains("focus.chain"));
        global.set_focus_chain_on_restart(
            vm.snapshot
                .applies
                .get("focus.chain")
                .is_some_and(|a| a.apply == crate::requests::SectionApply::OnRestart),
        );
        global.set_focus_notify_index(i32::from(!vm.snapshot.focus_notify));
        global.set_focus_notify_env_locked(vm.snapshot.env_overridden.contains("focus.notify"));
        global.set_focus_notify_on_restart(
            vm.snapshot
                .applies
                .get("focus.notify")
                .is_some_and(|a| a.apply == crate::requests::SectionApply::OnRestart),
        );
        let rows: Vec<crate::SettingsFactRow> = vm
            .snapshot
            .facts
            .iter()
            .map(|fact| crate::SettingsFactRow {
                key: fact.key.into(),
                value: fact.value.clone().into(),
                on_restart: fact.apply == crate::requests::SectionApply::OnRestart,
                env_locked: vm.snapshot.env_overridden.contains(fact.key),
            })
            .collect();
        self.models.push_settings_facts(global, &rows);
    }

    /// Mirrors one edited field into the draft (the buffer precedent).
    /// Insert engagement is the D20 focus fact now — the field's
    /// focused publish masks the chain; this seam only mirrors.
    pub(crate) fn settings_field_edited(&mut self, field: usize, text: &str, window: &AppWindow) {
        let target = match field {
            1 => &mut self.settings.draft.server_url,
            2 => &mut self.settings.draft.username,
            3 => {
                // The password mirror: write-only — it never enters the
                // pushed model (the draft never renders it back).
                self.settings.draft.password.clear();
                self.settings.draft.password.push_str(text);
                self.push_settings(window);
                return;
            }
            4 => {
                // The scale field's staged buffer (round-3 D20a): the
                // draft becomes the rendered echo and the flavour's
                // input; nothing else writes the echo while the field
                // holds a draft (r4 D38: the drag no longer previews).
                self.settings.scale_draft = Some(text.to_owned());
                self.push_settings(window);
                return;
            }
            // The general fields' staged buffers (the GUI settings
            // editor): the same grammar — draft per keystroke,
            // discarded on Return commit and on focus loss.
            5 => {
                self.settings.day_start_draft = Some(text.to_owned());
                self.push_settings(window);
                return;
            }
            6 => {
                self.settings.tick_draft = Some(text.to_owned());
                self.push_settings(window);
                return;
            }
            7 => {
                self.settings.before_draft = Some(text.to_owned());
                self.push_settings(window);
                return;
            }
            8 => {
                self.settings.poll_draft = Some(text.to_owned());
                self.push_settings(window);
                return;
            }
            9 => {
                self.settings.backoff_initial_draft = Some(text.to_owned());
                self.push_settings(window);
                return;
            }
            10 => {
                self.settings.backoff_max_draft = Some(text.to_owned());
                self.push_settings(window);
                return;
            }
            11 => {
                self.settings.due_soon_draft = Some(text.to_owned());
                self.push_settings(window);
                return;
            }
            GeneralField::FOCUS_WORK_INDEX => {
                self.settings.focus_work_draft = Some(text.to_owned());
                self.push_settings(window);
                return;
            }
            GeneralField::FOCUS_BREAK_INDEX => {
                self.settings.focus_break_draft = Some(text.to_owned());
                self.push_settings(window);
                return;
            }
            GeneralField::FOCUS_LONG_BREAK_INDEX => {
                self.settings.focus_long_break_draft = Some(text.to_owned());
                self.push_settings(window);
                return;
            }
            GeneralField::FOCUS_EVERY_INDEX => {
                self.settings.focus_every_draft = Some(text.to_owned());
                self.push_settings(window);
                return;
            }
            GeneralField::FILES_POLL_INDEX => {
                self.settings.files_poll_secs_draft = Some(text.to_owned());
                self.push_settings(window);
                return;
            }
            _ => return,
        };
        target.clear();
        target.push_str(text);
        self.push_settings(window);
    }

    /// The Sign-in seam (round-5 chunk 8: the Save ceremony with a
    /// derived name): mirror the final field texts (the commit
    /// callbacks carry them), then send the verify-first ceremony
    /// request with an empty name — the app derives it from the
    /// identity. The password clears off the draft after the send —
    /// write-only (D26b).
    pub(crate) fn settings_save(
        &mut self,
        window: &AppWindow,
        server: &str,
        username: &str,
        password: &str,
    ) {
        let draft = ConnectionDraft {
            server_url: server.to_owned(),
            username: username.to_owned(),
            password: password.to_owned(),
        };
        // The visible ceremony (r2 D34, F21): every click reacts. A
        // refused click is a *visible* typed refusal — never a silent
        // absorb, and never a routed request (the lane stays clean).
        if self.settings.pending.is_some() {
            // T-38 (#821): the busy refusal names its posture — the
            // typed `RequestPending` detail rides the same lane as
            // every other refusal (never a silent generic).
            self.set_settings_outcome(
                window,
                SettingsOutcome::Rejected,
                Some(SettingsError::RequestPending),
            );
            return;
        }
        if let Some(err) = settings_form::validate(&draft) {
            // The chunk-5 mapping renders the exact reason sentence
            // with zero new presentation code.
            let detail = match err {
                settings_form::DraftError::Invalid { field, kind } => SettingsError::Invalid {
                    field: map_form_field(field),
                    kind: map_form_kind(kind),
                },
            };
            self.set_settings_outcome(window, SettingsOutcome::Rejected, Some(detail));
            return;
        }
        let (reply, reply_rx) = oneshot::channel();
        let request = UiRequest::SaveConnection {
            profile: ConnectionProfile {
                name: String::new(),
                server_url: draft.server_url.clone(),
                username: draft.username.clone(),
                password: Secret(draft.password.clone()),
            },
            reply,
        };
        self.settings.draft.password.clear();
        let sent = self.settings.send(
            &self.requests,
            request,
            PendingKind::Save,
            PendingReply::List(reply_rx),
            "",
        );
        self.set_settings_outcome(
            window,
            if sent {
                SettingsOutcome::Busy
            } else {
                SettingsOutcome::Failed
            },
            None,
        );
    }

    /// Sends one connections request whose reply is the fresh pair,
    /// with the busy ceremony on `(connector, remote)`.
    fn send_connections_op(
        &mut self,
        window: &AppWindow,
        request: UiRequest,
        reply_rx: oneshot::Receiver<Result<requests::ConnectionsSnapshot, SettingsError>>,
        kind: PendingKind,
        connector: &str,
        remote: Option<RemoteBoardId>,
    ) {
        let sent = self.settings.send(
            &self.requests,
            request,
            kind,
            PendingReply::List(reply_rx),
            connector,
        );
        if sent {
            self.settings.busy_remote = remote;
        }
        self.set_settings_outcome(
            window,
            if sent {
                SettingsOutcome::Busy
            } else {
                SettingsOutcome::Failed
            },
            None,
        );
    }

    /// The account card's Remove seam (round-5 chunk 8; UX R-2 D-4,
    /// #787): the click ARMS — the card becomes the one armed subject
    /// (no lane state, no send); the Confirm button routes the exact
    /// `RemoveConnection`. A stale or non-card index is absorbed
    /// silently.
    pub(crate) fn settings_remove_item(&mut self, window: &AppWindow, item: usize) {
        self.settings_arm_item(item);
        self.push_settings(window);
    }

    /// The armed strip's arm step (UX R-2 D-4): records the addressed
    /// item as the one armed subject — arming touches no lane state,
    /// dispatches nothing. A stale index is absorbed silently; arming
    /// another item displaces the previous (one armed subject
    /// surface-wide).
    pub(crate) fn settings_arm_item(&mut self, item: usize) {
        let Some(addr) = self.settings.items.get(item).cloned() else {
            return;
        };
        self.settings.armed_item = Some(addr);
    }

    /// The armed strip's disarm (UX R-2 D-4): the Cancel buttons, a
    /// displaced arm, a stale confirm — clears the flag, dispatches
    /// nothing.
    pub(crate) fn settings_disarm_item(&mut self) {
        self.settings.armed_item = None;
    }

    /// The remove/disconnect confirm seam (UX R-2 D-4): matches the
    /// armed `ItemAddr` against the addressed item — the armed
    /// address types the verb (`RemoveConnection` for a card,
    /// `DetachConnector` for a bound remote row) — clears the flag
    /// BEFORE the send, then dispatches the exact existing request.
    /// A stale index (not the armed subject) disarms silently.
    pub(crate) fn settings_remove_confirmed(&mut self, window: &AppWindow, item: usize) {
        let Some(addr) = self.settings.items.get(item).cloned() else {
            self.settings_disarm_item();
            return;
        };
        let Some(armed) = self.settings.armed_item.take() else {
            return;
        };
        if armed != addr {
            return; // a stale confirm: already disarmed, nothing dispatches
        }
        match armed {
            ItemAddr::Account { connector } => self.settings_send_remove(window, &connector),
            ItemAddr::Remote {
                connector,
                remote,
                bound: Some(board),
            } => self.settings_send_detach(window, &connector, remote, board),
            _ => {}
        }
    }

    /// The Files roster's Stop-Sync confirm seam (UX R-2 D-4): the
    /// armed `FilesBoard` address types the verb; clears the flag
    /// BEFORE the send. A stale index disarms silently — the Move and
    /// Publish branches never arm (non-destructive, one-step).
    pub(crate) fn settings_files_action_confirmed(&mut self, window: &AppWindow, item: usize) {
        let Some(addr) = self.settings.items.get(item).cloned() else {
            self.settings_disarm_item();
            return;
        };
        let Some(armed) = self.settings.armed_item.take() else {
            return;
        };
        let ItemAddr::FilesBoard { board, connector } = armed.clone() else {
            return;
        };
        if armed != addr {
            return;
        }
        self.settings_stop_files_sync(window, board, &connector);
    }

    /// The exact `RemoveConnection` send (round-5 chunk 8's body,
    /// moved verbatim into the confirm path).
    fn settings_send_remove(&mut self, window: &AppWindow, connector: &str) {
        if self.settings.pending.is_some() {
            return;
        }
        let (reply, reply_rx) = oneshot::channel();
        let request = UiRequest::RemoveConnection {
            name: connector.to_owned(),
            reply,
        };
        self.send_connections_op(
            window,
            request,
            reply_rx,
            PendingKind::Remove,
            connector,
            None,
        );
    }

    /// The exact `DetachConnector` send (r2 D33's body, moved
    /// verbatim into the confirm path).
    fn settings_send_detach(
        &mut self,
        window: &AppWindow,
        connector: &str,
        remote: RemoteBoardId,
        board: BoardId,
    ) {
        if self.settings.pending.is_some() {
            return;
        }
        let (reply, reply_rx) = oneshot::channel();
        let request = UiRequest::DetachConnector {
            board,
            connector: connector.to_owned(),
            reply,
        };
        self.send_connections_op(
            window,
            request,
            reply_rx,
            PendingKind::Detach,
            connector,
            Some(remote),
        );
    }

    /// The exact `StopBoardFilesSync` send (round-7 D-7.3's Stop
    /// branch body, moved verbatim into the confirm path; the row
    /// marker set before the send).
    fn settings_stop_files_sync(&mut self, window: &AppWindow, board: BoardId, connector: &str) {
        if self.settings.pending.is_some() {
            return;
        }
        let (reply, reply_rx) = oneshot::channel();
        let request = UiRequest::StopBoardFilesSync { board, reply };
        // The row marker is set BEFORE the send: `send_connections_op`
        // ends in the busy push that renders it.
        self.settings.busy_board = Some(board);
        self.send_connections_op(
            window,
            request,
            reply_rx,
            PendingKind::StopFilesSync,
            connector,
            None,
        );
    }

    /// The Deck row's Refresh seam (round-5 chunk 8, D-8.3): lists the
    /// connector's remote boards (the listing verifies the stored
    /// credentials). A stale or non-Deck index is absorbed silently.
    pub(crate) fn settings_refresh_item(&mut self, window: &AppWindow, item: usize) {
        let Some(ItemAddr::Deck { connector }) = self.settings.items.get(item).cloned() else {
            return;
        };
        if self.settings.pending.is_some() {
            return;
        }
        // Explicit intent overrides the backoff (US-3 AC-2): the
        // entry clears before the send; failure re-arms it.
        self.settings.auto_list_backoff.remove(&connector);
        let (reply, reply_rx) = oneshot::channel();
        let request = UiRequest::ListRemoteBoards {
            connector: connector.clone(),
            reply,
        };
        let sent = self.settings.send(
            &self.requests,
            request,
            PendingKind::ListRemote,
            PendingReply::Remotes(connector.clone(), reply_rx),
            &connector,
        );
        self.set_settings_outcome(
            window,
            if sent {
                SettingsOutcome::Busy
            } else {
                SettingsOutcome::Failed
            },
            None,
        );
    }

    /// The Deck row's publish seam (round-6 D-6.9, #699): picking an
    /// unbound local board publishes it onto this connector's server.
    /// A stale item/option index is absorbed silently.
    pub(crate) fn settings_publish_picked(
        &mut self,
        window: &AppWindow,
        item: usize,
        option: usize,
    ) {
        let Some(ItemAddr::Deck { connector }) = self.settings.items.get(item).cloned() else {
            return;
        };
        let Some(board) = self.settings.local_choices.get(option).copied() else {
            return;
        };
        if self.settings.pending.is_some() {
            return;
        }
        let (reply, reply_rx) = oneshot::channel();
        let request = UiRequest::PublishBoard {
            board: BoardId::from_uuid(board),
            connector: connector.clone(),
            reply,
        };
        self.send_connections_op(
            window,
            request,
            reply_rx,
            PendingKind::Publish,
            &connector,
            None,
        );
    }

    /// The files roster's per-row action seam (round-7 D-7.3, W-3/D1):
    /// the row's one action, re-derived at the trust boundary — the
    /// binding's channel is read structurally from the engine state
    /// fresh (D-BF4; the widget never names the action), the lane-on
    /// fact re-derives from the snapshot exactly as the render pass
    /// computed it, and the D6 enabled rule (`files_on ∧ !archived`)
    /// gates the dispatch — a disabled or stale row's click is
    /// absorbed silently. The Deck branch keys on the binding's home
    /// connector (the files-sync-status plan US-10, D-11): own card →
    /// the Deck→Files move; a foreign card's rebind row needs no
    /// files lane (its gate is the archived filter alone).
    /// The undo-window Restore seam (the files-sync-status plan US-6,
    /// D-7): routes the exact `UndoBoardMove` — the ceremony's undo
    /// leg is the typed gate. The pending machine takes the move
    /// grammar (the row's busy dot rides `PendingKind::Move`).
    pub(crate) fn settings_files_board_restore(&mut self, window: &AppWindow, item: usize) {
        let Some(ItemAddr::FilesBoard { board, connector }) =
            self.settings.items.get(item).cloned()
        else {
            return;
        };
        if self.settings.pending.is_some() {
            return;
        }
        let (reply, reply_rx) = oneshot::channel();
        let request = UiRequest::UndoBoardMove { board, reply };
        let sent = self.settings.send(
            &self.requests,
            request,
            PendingKind::Move,
            PendingReply::List(reply_rx),
            &connector,
        );
        self.set_settings_outcome(
            window,
            if sent {
                SettingsOutcome::Busy
            } else {
                SettingsOutcome::Failed
            },
            None,
        );
    }

    pub(crate) fn settings_files_board_action(&mut self, window: &AppWindow, item: usize) {
        let Some(ItemAddr::FilesBoard { board, connector }) =
            self.settings.items.get(item).cloned()
        else {
            return;
        };
        if self.settings.pending.is_some() {
            return;
        }
        let files_on = self.settings.snapshot.files_sync_enabled
            && self.settings.snapshot.files_sync_anchor.as_deref() == Some(connector.as_str());
        // The binding's channel AND its home connector, re-derived at
        // the trust boundary: a Deck-bound board riding this card's
        // connector is the own-card switch; one riding another
        // connector is the rebind row (US-10, D-11 — needs no files
        // lane, so its gate is the archived filter alone).
        let bound = {
            let state = self.state.load();
            state
                .boards
                .get(&board)
                .filter(|row| !row.deleted && !row.archived)
                .and_then(|row| row.remote.as_ref())
                .map(|binding| {
                    (
                        crate::presentation::bound_channel_of(&binding.board),
                        binding.connector.as_str() == connector.as_str(),
                    )
                })
        };
        let channel = bound.map(|(channel, _)| channel);
        let deck_own = matches!(bound, Some((crate::requests::BoardChannel::Deck, true)));
        // The D6 enabled rule re-checked at the trust boundary: a
        // disabled posture (lane off, archived) or a stale row
        // (gone / re-bound mid-flight) absorbs the click silently —
        // nothing dispatches.
        let kind = match (channel, files_on) {
            // The destructive branch (UX R-2 D-4, #787): the click
            // ARMS — the row becomes the one armed subject; the
            // strip's Confirm routes the exact Stop request.
            (Some(crate::requests::BoardChannel::Files), true) => {
                self.settings_arm_item(item);
                self.push_settings(window);
                return;
            }
            (Some(crate::requests::BoardChannel::Deck), _) => PendingKind::Move,
            (None, true) => PendingKind::PublishFiles,
            _ => return,
        };
        let (reply, reply_rx) = oneshot::channel();
        let request = match kind {
            PendingKind::PublishFiles => UiRequest::PublishBoardToFiles { board, reply },
            // The move family, keyed by the binding's home (the fold's
            // own/action split re-derived): own card → the Deck→Files
            // ceremony reused verbatim (US-3); foreign card → the
            // Deck-to-Deck rebind (US-10), this card's connector the
            // target. Both ride `PendingKind::Move`'s busy grammar on
            // the target connector's Files row.
            PendingKind::Move => {
                if deck_own {
                    UiRequest::MoveBoardToFiles { board, reply }
                } else {
                    UiRequest::MoveBoardToDeck {
                        board,
                        target_connector: connector.clone(),
                        reply,
                    }
                }
            }
            _ => unreachable!("the roster action dispatches only its one-step verbs"),
        };
        // The row marker is set BEFORE the send: `send_connections_op`
        // ends in the busy push that renders it.
        if matches!(kind, PendingKind::PublishFiles) {
            self.settings.busy_board = Some(board);
        }
        self.send_connections_op(window, request, reply_rx, kind, &connector, None);
    }

    /// The remote row's picker seam (round-5 chunk 8, D-8.4): picking
    /// an unbound local board binds it to this remote explicitly. A
    /// stale item/option index, or a bound row, is absorbed silently.
    pub(crate) fn settings_remote_bind_picked(
        &mut self,
        window: &AppWindow,
        item: usize,
        option: usize,
    ) {
        let Some(ItemAddr::Remote {
            connector,
            remote,
            bound: None,
        }) = self.settings.items.get(item).cloned()
        else {
            return;
        };
        let Some(board) = self.settings.local_choices.get(option).copied() else {
            return;
        };
        if self.settings.pending.is_some() {
            return;
        }
        let (reply, reply_rx) = oneshot::channel();
        let request = UiRequest::AttachConnector {
            board: BoardId::from_uuid(board),
            connector: connector.clone(),
            remote,
            reply,
        };
        self.send_connections_op(
            window,
            request,
            reply_rx,
            PendingKind::Attach,
            &connector,
            Some(remote),
        );
    }

    /// The remote row's Create seam (round-5 chunk 8, D-8.5): a new
    /// local board from the remote's title, bound to it. A stale index
    /// or a bound row is absorbed silently.
    pub(crate) fn settings_create_item(&mut self, window: &AppWindow, item: usize) {
        let Some(ItemAddr::Remote {
            connector,
            remote,
            bound: None,
        }) = self.settings.items.get(item).cloned()
        else {
            return;
        };
        if self.settings.pending.is_some() {
            return;
        }
        let (reply, reply_rx) = oneshot::channel();
        let request = UiRequest::CreateLocalBoard {
            connector: connector.clone(),
            remote,
            reply,
        };
        self.send_connections_op(
            window,
            request,
            reply_rx,
            PendingKind::CreateBind,
            &connector,
            Some(remote),
        );
    }

    /// The bound remote row's Disconnect seam (r2 D33's detach, moved
    /// onto the row by round-5 chunk 8; UX R-2 D-4, #787): the click
    /// ARMS — the row becomes the one armed subject; the Confirm
    /// button routes the exact `DetachConnector` + multi-board guard.
    /// A stale index or an unbound row is absorbed silently.
    pub(crate) fn settings_disconnect_item(&mut self, window: &AppWindow, item: usize) {
        self.settings_arm_item(item);
        self.push_settings(window);
    }

    /// A Files row's toggle seam (round-5 chunk 8 D-8.6; round-6
    /// D-6.6; commit-on-pick): 0 = On, 1 = Off. On a non-anchor card
    /// re-anchors to it and enables — one request carries both values
    /// (one user action, one overlay write); On the anchor card, or
    /// any Off, leaves the anchor key untouched. `files_on` is
    /// recomputed from the snapshot at the seam, never from the
    /// render-pass model. An env-locked row (either key) absorbs the
    /// pick (the app-side guard is the authority).
    pub(crate) fn settings_files_picked(&mut self, window: &AppWindow, item: usize, index: usize) {
        let Some(enabled) = [true, false].get(index).copied() else {
            return;
        };
        let Some(ItemAddr::Files { connector }) = self.settings.items.get(item).cloned() else {
            return;
        };
        if self.settings.pending.is_some() {
            // T-38 (#821): the busy refusal names its posture — the
            // typed `RequestPending` detail rides the same lane as
            // every other refusal (never a silent generic).
            self.set_settings_outcome(
                window,
                SettingsOutcome::Rejected,
                Some(SettingsError::RequestPending),
            );
            return;
        }
        let env_locked = self
            .settings
            .snapshot
            .env_overridden
            .contains("files_sync.enabled")
            || self
                .settings
                .snapshot
                .env_overridden
                .contains("files_sync.anchor");
        if env_locked {
            // T-16 (#799): the doctrine's letter — never a silent
            // absorb. The pick mutates nothing (the lane stays clean)
            // but the refusal renders typed at the point of action.
            tracing::debug!("files sync pick on an env-locked row; refused typed");
            self.set_settings_outcome(
                window,
                SettingsOutcome::Rejected,
                Some(SettingsError::EnvLocked {
                    field: crate::requests::Field::FilesSyncEnabled,
                }),
            );
            return;
        }
        let files_on = self.settings.snapshot.files_sync_enabled
            && self.settings.snapshot.files_sync_anchor.as_deref() == Some(connector.as_str());
        let anchor = if enabled && !files_on {
            Some(connector.clone())
        } else {
            None
        };
        let (reply, reply_rx) = oneshot::channel();
        let request = UiRequest::SetFilesSyncEnabled {
            enabled,
            anchor,
            reply,
        };
        let sent = self.settings.send(
            &self.requests,
            request,
            PendingKind::General(crate::requests::Field::FilesSyncEnabled),
            PendingReply::Snapshot(reply_rx),
            "",
        );
        self.set_settings_outcome(
            window,
            if sent {
                SettingsOutcome::Busy
            } else {
                SettingsOutcome::Failed
            },
            None,
        );
    }

    /// The preset picker seam: the optimistic marker, then the request
    /// (the reply swaps the keymap live, D9).
    pub(crate) fn settings_preset_picked(&mut self, window: &AppWindow, index: usize) {
        let Some(preset) = [
            crate::keymap::Preset::Standard,
            crate::keymap::Preset::Emacs,
            crate::keymap::Preset::Vi,
        ]
        .get(index)
        .copied() else {
            return;
        };
        if self.settings.pending.is_some() {
            return;
        }
        let (reply, reply_rx) = oneshot::channel();
        let request = UiRequest::SetKeymapPreset { preset, reply };
        let sent = self.settings.send(
            &self.requests,
            request,
            PendingKind::Preset,
            PendingReply::Keymap(reply_rx),
            "",
        );
        self.set_settings_outcome(
            window,
            if sent {
                SettingsOutcome::Busy
            } else {
                SettingsOutcome::Failed
            },
            None,
        );
    }

    /// The scale commit core (r2 D29): live-apply first (the mount
    /// precedent), then persist over the lane; an `Err` reply reverts
    /// to `previous`. Both input paths (field Return, slider release)
    /// share it, so both clamp identically by construction.
    fn apply_scale_commit(&mut self, window: &AppWindow, scale: f64, previous: f64) {
        // The commit consumed the field's staged buffer (round-3
        // D20a): the canonical echo re-renders from the live value.
        self.settings.scale_draft = None;
        // The scroll anchor captures PRE-layout (r3 D20c): the row
        // offset and viewport offset are read before the write below
        // re-lays the content out.
        request_scale_anchor(window);
        #[allow(clippy::cast_possible_truncation)]
        window.global::<crate::Theme>().set_base_scale(scale as f32);
        let (reply, reply_rx) = oneshot::channel();
        let request = UiRequest::SetScale { scale, reply };
        let sent = self.settings.send(
            &self.requests,
            request,
            PendingKind::Scale,
            PendingReply::Scale(reply_rx, previous),
            "",
        );
        self.set_settings_outcome(
            window,
            if sent {
                SettingsOutcome::Busy
            } else {
                SettingsOutcome::Failed
            },
            None,
        );
    }

    /// The scale commit seam (field, Return-authoritative): live-apply
    /// first, then persist over the lane; an `Err` reply reverts (D9).
    pub(crate) fn settings_scale_committed(&mut self, window: &AppWindow, text: &str) {
        if self.settings.pending.is_some() {
            // T-38 (#821): the busy refusal names its posture — the
            // typed `RequestPending` detail rides the same lane as
            // every other refusal (never a silent generic).
            self.set_settings_outcome(
                window,
                SettingsOutcome::Rejected,
                Some(SettingsError::RequestPending),
            );
            return;
        }
        // The typed refusal (round-3 D20a, recorded decision): the
        // same validator the flavour renders drives the status
        // detail, so the error line and the status line agree; the
        // draft is KEPT on failure — the typo stays visible under
        // its invalid flavour until focus loss or a good commit.
        if let Some(kind) = settings_form::scale_invalid_kind(text) {
            self.set_settings_outcome(
                window,
                SettingsOutcome::Rejected,
                Some(SettingsError::Invalid {
                    field: crate::requests::Field::Scale,
                    kind: map_form_kind(kind),
                }),
            );
            return;
        }
        #[allow(clippy::cast_precision_loss)]
        let scale = text.trim().parse::<f64>().expect("validated parse");
        let previous = f64::from(window.global::<crate::Theme>().get_base_scale());
        self.apply_scale_commit(window, scale, previous);
    }

    /// The slider gesture's press (r2 D29): record the pre-gesture
    /// base-scale as the revert point. The pending gate is silent
    /// (the `settings_preset_picked` precedent).
    pub(crate) fn settings_scale_pressed(&mut self, window: &AppWindow) {
        if self.settings.pending.is_some() {
            return;
        }
        self.scale_drag = Some(f64::from(window.global::<crate::Theme>().get_base_scale()));
    }

    /// The scale field's focus-loss seam (round-3 D20a writer 7): a
    /// Return-less edit never applies — the staged draft dies and the
    /// push re-renders the echo from the live scale (the chunk-1
    /// focus-loss seam is the mechanism).
    pub(crate) fn settings_scale_focus_lost(&mut self, window: &AppWindow) {
        if self.settings.scale_draft.take().is_none() {
            return;
        }
        self.push_settings(window);
    }

    /// The general fields' commit seam (the GUI settings editor;
    /// Return-authoritative — the scale field's grammar): the mirror
    /// predicate refuses typed before the lane; a valid mint
    /// dispatches the field's typed intent; the reply echo re-renders
    /// the rows. Env-locked keys never dispatch — the row renders
    /// read-only, and this arm is the defence-in-depth refusal. The
    /// `initial ≤ max` ordering (plan D8) refuses at the seam against
    /// the committed counterpart, the error attributed to the
    /// offending field; the app-side gate re-proves the whole
    /// document authoritatively.
    // The minting match is one arm per field of the closed
    // GeneralField table — the arm count tracks the table, not the
    // seam's own complexity.
    #[allow(clippy::too_many_lines)]
    pub(crate) fn settings_general_committed(
        &mut self,
        window: &AppWindow,
        field: usize,
        text: &str,
    ) {
        let Some(gf) = GeneralField::from_index(field) else {
            return;
        };
        if self.settings.pending.is_some() {
            // T-38 (#821): the busy refusal names its posture — the
            // typed `RequestPending` detail rides the same lane as
            // every other refusal (never a silent generic).
            self.set_settings_outcome(
                window,
                SettingsOutcome::Rejected,
                Some(SettingsError::RequestPending),
            );
            return;
        }
        if self.settings.snapshot.env_overridden.contains(gf.key()) {
            // T-16 (#799): the defence-in-depth arm renders the typed
            // refusal instead of absorbing (the doctrine: never a
            // silent absorb).
            tracing::debug!(
                key = gf.key(),
                "general commit on an env-locked row; refused typed"
            );
            self.set_settings_outcome(
                window,
                SettingsOutcome::Rejected,
                Some(SettingsError::EnvLocked {
                    field: gf.lane_field(),
                }),
            );
            return;
        }
        let typed_refusal = |this: &mut Self, kind| {
            this.set_settings_outcome(
                window,
                SettingsOutcome::Rejected,
                Some(SettingsError::Invalid {
                    field: gf.lane_field(),
                    kind: map_form_kind(kind),
                }),
            );
        };
        if let Some(kind) = gf.invalid(text) {
            // The draft stays: the typo stays visible under its
            // invalid flavour until focus loss or a good commit (the
            // scale precedent).
            typed_refusal(self, kind);
            return;
        }
        let trimmed = text.trim();
        // The mirror's empty contract is Neutral (no flavour on an
        // untouched field), so an empty commit reaches here — and the
        // mints below `expect` a validated parse. `accepted` fires on
        // Return regardless of content, so an empty Return is a real
        // keyboard path: it refuses typed, never mints.
        if trimmed.is_empty() {
            typed_refusal(self, settings_form::InvalidKind::Empty);
            return;
        }
        if let Some(ceiling) = gf.ordering_ceiling(&self.settings.snapshot) {
            let crossing = match gf {
                GeneralField::BackoffInitial => trimmed
                    .parse::<u64>()
                    .is_ok_and(|initial| initial > ceiling),
                GeneralField::BackoffMax => trimmed.parse::<u64>().is_ok_and(|max| max < ceiling),
                _ => false,
            };
            if crossing {
                typed_refusal(self, settings_form::InvalidKind::OutOfRange);
                return;
            }
        }
        let (reply, reply_rx) = oneshot::channel();
        let request = match gf {
            GeneralField::DayStart => UiRequest::SetDayStart {
                time: trimmed.parse().expect("validated day-start parse"),
                reply,
            },
            GeneralField::TickSecs => UiRequest::SetReminderTick {
                secs: trimmed.parse().expect("validated seconds parse"),
                reply,
            },
            GeneralField::ReminderBefore => UiRequest::SetReminderBefore {
                before: taskboard_domain::ReminderOffset::parse_text_form(trimmed)
                    .expect("validated before parse"),
                reply,
            },
            GeneralField::PollInterval => UiRequest::SetPollInterval {
                secs: trimmed.parse().expect("validated seconds parse"),
                reply,
            },
            GeneralField::BackoffInitial => UiRequest::SetBackoffInitial {
                secs: trimmed.parse().expect("validated seconds parse"),
                reply,
            },
            GeneralField::BackoffMax => UiRequest::SetBackoffMax {
                secs: trimmed.parse().expect("validated seconds parse"),
                reply,
            },
            GeneralField::DueSoonDays => UiRequest::SetDueSoonDays {
                days: trimmed.parse().expect("validated days parse"),
                reply,
            },
            GeneralField::FocusWorkMins => UiRequest::SetFocusWorkMins {
                mins: trimmed.parse().expect("validated focus-minutes parse"),
                reply,
            },
            GeneralField::FocusBreakMins => UiRequest::SetFocusBreakMins {
                mins: trimmed.parse().expect("validated focus-minutes parse"),
                reply,
            },
            GeneralField::FocusLongBreakMins => UiRequest::SetFocusLongBreakMins {
                mins: trimmed.parse().expect("validated focus-minutes parse"),
                reply,
            },
            GeneralField::FocusLongBreakEvery => UiRequest::SetFocusLongBreakEvery {
                every: trimmed.parse().expect("validated focus-every parse"),
                reply,
            },
            GeneralField::FilesPollSecs => UiRequest::SetFilesSyncPollSecs {
                secs: trimmed.parse().expect("validated seconds parse"),
                reply,
            },
        };
        self.clear_general_draft(gf);
        let sent = self.settings.send(
            &self.requests,
            request,
            PendingKind::General(gf.lane_field()),
            PendingReply::Snapshot(reply_rx),
            "",
        );
        self.set_settings_outcome(
            window,
            if sent {
                SettingsOutcome::Busy
            } else {
                SettingsOutcome::Failed
            },
            None,
        );
    }

    /// The reminders toggle's seam (commit-on-pick, the keymap
    /// preset grammar): index 0 = On, 1 = Off.
    pub(crate) fn settings_reminders_picked(&mut self, window: &AppWindow, index: usize) {
        let Some(enabled) = [true, false].get(index).copied() else {
            return;
        };
        if self.settings.pending.is_some() {
            // T-38 (#821): the busy refusal names its posture — the
            // typed `RequestPending` detail rides the same lane as
            // every other refusal (never a silent generic).
            self.set_settings_outcome(
                window,
                SettingsOutcome::Rejected,
                Some(SettingsError::RequestPending),
            );
            return;
        }
        if self
            .settings
            .snapshot
            .env_overridden
            .contains("reminders.enabled")
        {
            // Choice commits are typed booleans — env-locked picks
            // refuse typed at the defence arm (the segments render
            // disabled; the row's source is named by the badge; the
            // T-16 refusal renders at the point of action).
            tracing::debug!("reminders pick on an env-locked row; refused typed");
            self.set_settings_outcome(
                window,
                SettingsOutcome::Rejected,
                Some(SettingsError::EnvLocked {
                    field: crate::requests::Field::RemindersEnabled,
                }),
            );
            return;
        }
        let (reply, reply_rx) = oneshot::channel();
        let request = UiRequest::SetRemindersEnabled { enabled, reply };
        let sent = self.settings.send(
            &self.requests,
            request,
            PendingKind::General(crate::requests::Field::RemindersEnabled),
            PendingReply::Snapshot(reply_rx),
            "",
        );
        self.set_settings_outcome(
            window,
            if sent {
                SettingsOutcome::Busy
            } else {
                SettingsOutcome::Failed
            },
            None,
        );
    }

    /// The focus mode toggle's seam (commit-on-pick, the reminders
    /// grammar): index 0 = On, 1 = Off.
    pub(crate) fn settings_focus_enabled_picked(&mut self, window: &AppWindow, index: usize) {
        let Some(enabled) = [true, false].get(index).copied() else {
            return;
        };
        if self.settings.pending.is_some() {
            // T-38 (#821): the busy refusal names its posture — the
            // typed `RequestPending` detail rides the same lane as
            // every other refusal (never a silent generic).
            self.set_settings_outcome(
                window,
                SettingsOutcome::Rejected,
                Some(SettingsError::RequestPending),
            );
            return;
        }
        if self
            .settings
            .snapshot
            .env_overridden
            .contains("focus.enabled")
        {
            tracing::debug!("focus enabled pick on an env-locked row; refused typed");
            self.set_settings_outcome(
                window,
                SettingsOutcome::Rejected,
                Some(SettingsError::EnvLocked {
                    field: crate::requests::Field::FocusEnabled,
                }),
            );
            return;
        }
        let (reply, reply_rx) = oneshot::channel();
        let request = UiRequest::SetFocusEnabled { enabled, reply };
        let sent = self.settings.send(
            &self.requests,
            request,
            PendingKind::General(crate::requests::Field::FocusEnabled),
            PendingReply::Snapshot(reply_rx),
            "",
        );
        self.set_settings_outcome(
            window,
            if sent {
                SettingsOutcome::Busy
            } else {
                SettingsOutcome::Failed
            },
            None,
        );
    }

    /// The auto-chain toggle's seam (commit-on-pick): 0 = On, 1 = Off.
    pub(crate) fn settings_focus_chain_picked(&mut self, window: &AppWindow, index: usize) {
        let Some(enabled) = [true, false].get(index).copied() else {
            return;
        };
        if self.settings.pending.is_some() {
            // T-38 (#821): the busy refusal names its posture — the
            // typed `RequestPending` detail rides the same lane as
            // every other refusal (never a silent generic).
            self.set_settings_outcome(
                window,
                SettingsOutcome::Rejected,
                Some(SettingsError::RequestPending),
            );
            return;
        }
        if self
            .settings
            .snapshot
            .env_overridden
            .contains("focus.chain")
        {
            tracing::debug!("focus chain pick on an env-locked row; refused typed");
            self.set_settings_outcome(
                window,
                SettingsOutcome::Rejected,
                Some(SettingsError::EnvLocked {
                    field: crate::requests::Field::FocusChain,
                }),
            );
            return;
        }
        let (reply, reply_rx) = oneshot::channel();
        let request = UiRequest::SetFocusChain { enabled, reply };
        let sent = self.settings.send(
            &self.requests,
            request,
            PendingKind::General(crate::requests::Field::FocusChain),
            PendingReply::Snapshot(reply_rx),
            "",
        );
        self.set_settings_outcome(
            window,
            if sent {
                SettingsOutcome::Busy
            } else {
                SettingsOutcome::Failed
            },
            None,
        );
    }

    /// The boundary-notification toggle's seam (commit-on-pick):
    /// 0 = On, 1 = Off.
    pub(crate) fn settings_focus_notify_picked(&mut self, window: &AppWindow, index: usize) {
        let Some(enabled) = [true, false].get(index).copied() else {
            return;
        };
        if self.settings.pending.is_some() {
            // T-38 (#821): the busy refusal names its posture — the
            // typed `RequestPending` detail rides the same lane as
            // every other refusal (never a silent generic).
            self.set_settings_outcome(
                window,
                SettingsOutcome::Rejected,
                Some(SettingsError::RequestPending),
            );
            return;
        }
        if self
            .settings
            .snapshot
            .env_overridden
            .contains("focus.notify")
        {
            tracing::debug!("focus notify pick on an env-locked row; refused typed");
            self.set_settings_outcome(
                window,
                SettingsOutcome::Rejected,
                Some(SettingsError::EnvLocked {
                    field: crate::requests::Field::FocusNotify,
                }),
            );
            return;
        }
        let (reply, reply_rx) = oneshot::channel();
        let request = UiRequest::SetFocusNotify { enabled, reply };
        let sent = self.settings.send(
            &self.requests,
            request,
            PendingKind::General(crate::requests::Field::FocusNotify),
            PendingReply::Snapshot(reply_rx),
            "",
        );
        self.set_settings_outcome(
            window,
            if sent {
                SettingsOutcome::Busy
            } else {
                SettingsOutcome::Failed
            },
            None,
        );
    }

    /// The notifier's seam (commit-on-pick): 0 = auto, 1 = log,
    /// 2 = desktop.
    pub(crate) fn settings_sink_picked(&mut self, window: &AppWindow, index: usize) {
        let Some(sink) = [
            crate::requests::ReminderSink::Auto,
            crate::requests::ReminderSink::Log,
            crate::requests::ReminderSink::Desktop,
        ]
        .get(index)
        .copied() else {
            return;
        };
        if self.settings.pending.is_some() {
            // T-38 (#821): the busy refusal names its posture — the
            // typed `RequestPending` detail rides the same lane as
            // every other refusal (never a silent generic).
            self.set_settings_outcome(
                window,
                SettingsOutcome::Rejected,
                Some(SettingsError::RequestPending),
            );
            return;
        }
        if self
            .settings
            .snapshot
            .env_overridden
            .contains("reminders.sink")
        {
            tracing::debug!("sink pick on an env-locked row; refused typed");
            self.set_settings_outcome(
                window,
                SettingsOutcome::Rejected,
                Some(SettingsError::EnvLocked {
                    field: crate::requests::Field::ReminderSink,
                }),
            );
            return;
        }
        let (reply, reply_rx) = oneshot::channel();
        let request = UiRequest::SetReminderSink { sink, reply };
        let sent = self.settings.send(
            &self.requests,
            request,
            PendingKind::General(crate::requests::Field::ReminderSink),
            PendingReply::Snapshot(reply_rx),
            "",
        );
        self.set_settings_outcome(
            window,
            if sent {
                SettingsOutcome::Busy
            } else {
                SettingsOutcome::Failed
            },
            None,
        );
    }

    /// The general fields' focus-loss seam (the scale field's
    /// grammar): a Return-less edit never dispatches — the staged
    /// draft dies and the echo re-renders from the committed value.
    /// The Files row's poll cadence field's commit seam (#1083, W-6):
    /// the general fields' funnel verbatim, under the plan row's own
    /// seam address (`files-poll-secs-committed`) — one machine, the
    /// whole-snapshot lane's single writer (R-4).
    pub(crate) fn files_poll_secs_committed(
        &mut self,
        window: &AppWindow,
        field: usize,
        text: &str,
    ) {
        self.settings_general_committed(window, field, text);
    }

    pub(crate) fn settings_general_focus_lost(&mut self, window: &AppWindow, field: usize) {
        let Some(gf) = GeneralField::from_index(field) else {
            return;
        };
        let cleared = match gf {
            GeneralField::DayStart => self.settings.day_start_draft.take().is_some(),
            GeneralField::TickSecs => self.settings.tick_draft.take().is_some(),
            GeneralField::ReminderBefore => self.settings.before_draft.take().is_some(),
            GeneralField::PollInterval => self.settings.poll_draft.take().is_some(),
            GeneralField::BackoffInitial => self.settings.backoff_initial_draft.take().is_some(),
            GeneralField::BackoffMax => self.settings.backoff_max_draft.take().is_some(),
            GeneralField::DueSoonDays => self.settings.due_soon_draft.take().is_some(),
            GeneralField::FocusWorkMins => self.settings.focus_work_draft.take().is_some(),
            GeneralField::FocusBreakMins => self.settings.focus_break_draft.take().is_some(),
            GeneralField::FocusLongBreakMins => {
                self.settings.focus_long_break_draft.take().is_some()
            }
            GeneralField::FocusLongBreakEvery => self.settings.focus_every_draft.take().is_some(),
            GeneralField::FilesPollSecs => self.settings.files_poll_secs_draft.take().is_some(),
        };
        if cleared {
            self.push_settings(window);
        }
    }

    fn clear_general_draft(&mut self, gf: GeneralField) {
        match gf {
            GeneralField::DayStart => self.settings.day_start_draft = None,
            GeneralField::TickSecs => self.settings.tick_draft = None,
            GeneralField::ReminderBefore => self.settings.before_draft = None,
            GeneralField::PollInterval => self.settings.poll_draft = None,
            GeneralField::BackoffInitial => self.settings.backoff_initial_draft = None,
            GeneralField::BackoffMax => self.settings.backoff_max_draft = None,
            GeneralField::DueSoonDays => self.settings.due_soon_draft = None,
            GeneralField::FocusWorkMins => self.settings.focus_work_draft = None,
            GeneralField::FocusBreakMins => self.settings.focus_break_draft = None,
            GeneralField::FocusLongBreakMins => self.settings.focus_long_break_draft = None,
            GeneralField::FocusLongBreakEvery => self.settings.focus_every_draft = None,
            GeneralField::FilesPollSecs => self.settings.files_poll_secs_draft = None,
        }
    }

    /// The settings surface's mount (r3 D20a writer hygiene, plan
    /// §5.1 draft invariants): the scale field's staged buffer dies at
    /// the mount — a Return-less edit from a previous visit never
    /// survives the surface's unmount, and the re-push renders the
    /// canonical live echo. (The connection draft persists by design;
    /// only the scale buffer's lifetime is bound to its surface.)
    pub(crate) fn settings_view_mounted(&mut self, window: &AppWindow) {
        // The armed strip is surface-bound state (cross-surface
        // friction plan D-4, #836): the mount clears any identity a
        // missed close path left behind — the strip is only visible
        // while mounted (the scale_draft lifetime precedent, r3 D20a).
        self.settings_disarm_item();
        self.settings.scale_draft = None;
        // The general drafts share the scale buffer's lifetime: bound
        // to the surface, never surviving the unmount.
        self.settings.day_start_draft = None;
        self.settings.tick_draft = None;
        self.settings.before_draft = None;
        self.settings.poll_draft = None;
        self.settings.backoff_initial_draft = None;
        self.settings.backoff_max_draft = None;
        self.settings.due_soon_draft = None;
        self.settings.focus_work_draft = None;
        self.settings.focus_break_draft = None;
        self.settings.focus_long_break_draft = None;
        self.settings.focus_every_draft = None;
        // The mount auto-list (round-6 D-6.7): the ONLY call site —
        // the listing cost lands on the first settings visit, never
        // at boot.
        self.fire_auto_listings();
        self.push_settings(window);
    }

    /// The slider gesture's release (r2 D29, r4 D38): one gate, one
    /// commit — the ONLY mid-gesture seam the adapter observes (the
    /// drag itself lives inside the widget). An out-of-gate value
    /// reverts to the pre-gesture value with a seam-local Rejected;
    /// an in-gate value goes through the shared commit core with the
    /// pre-gesture value as the revert point, so a failed commit
    /// undoes the gesture too.
    pub(crate) fn settings_scale_released(&mut self, window: &AppWindow, value: f64) {
        let Some(previous) = self.scale_drag.take() else {
            return;
        };
        if self.settings.pending.is_some() {
            // T-38 (#821): the busy refusal names its posture (the
            // typed `RequestPending` detail, the other gates' shape).
            self.set_settings_outcome(
                window,
                SettingsOutcome::Rejected,
                Some(SettingsError::RequestPending),
            );
            return;
        }
        if !value.is_finite() || !(0.5..=3.0).contains(&value) {
            // The out-of-gate revert re-anchors too (r3 D20c).
            request_scale_anchor(window);
            #[allow(clippy::cast_possible_truncation)]
            window
                .global::<crate::Theme>()
                .set_base_scale(previous as f32);
            self.set_settings_outcome(window, SettingsOutcome::Rejected, None);
            return;
        }
        self.apply_scale_commit(window, value, previous);
    }
}

/// The settings back affordance (round-1 D6.6): the dedicated seam
/// routing the same close path the Escape binding takes — an adapter
/// method, not a registry action (closing is not a routable intent).
pub(in crate::adapter) fn install_settings_back_seam(
    window: &AppWindow,
    adapter: &Rc<RefCell<Adapter>>,
) {
    install_seam!(window: window.on_settings_back_clicked, adapter: adapter;
    bind mut (a, window), args (), {        Adapter::dismiss_tooltip(&window);
    let mut outcomes = a.view_state.close_settings();
    // The armed strip dies with the surface (cross-surface friction
    // plan D-4, #836): leaving settings disarms.
    a.settings_disarm_item();
    // US-5 (#706): the restore's subject-liveness guard (see the
    // OpenSettings close arm).
    {
        let state = a.state.load_full();
        outcomes.extend(a.view_state.repair_restored_root(&state));
    }
    a.apply_outcomes(&window, &outcomes);
    a.sync_view_context(&window);
    a.sync(&window);

    });
}
/// The settings seams (round-1 chunk 3, D7/D9; the r2 D29 gesture
/// trio joins): the form's mirror and send callbacks resolve straight
/// onto the [`Adapter`] settings VM — never ad-hoc logic. One wiring
/// sequence, kept flat for review.
#[allow(clippy::too_many_lines)]
pub(in crate::adapter) fn install_settings_seams(
    window: &AppWindow,
    adapter: &Rc<RefCell<Adapter>>,
) {
    {
        install_seam!(window: window.on_settings_field_edited, adapter: adapter;
        bind mut (a, window), args (field, text), {            let Ok(field) = usize::try_from(field) else {
                return;
            };
            a
                .settings_field_edited(field, &text, &window);

        });
    }
    {
        install_seam!(window: window.on_settings_save_clicked, adapter: adapter;
        bind mut (a, window), args (server, username, password), {            a
                .settings_save(&window, &server, &username, &password);

        });
    }
    {
        install_seam!(window: window.on_settings_remove_clicked, adapter: adapter;
        bind mut (a, window), args (item), {            let Ok(item) = usize::try_from(item) else {
                return;
            };
            a.settings_remove_item(&window, item);

        });
    }
    {
        install_seam!(window: window.on_settings_refresh_clicked, adapter: adapter;
        bind mut (a, window), args (item), {            let Ok(item) = usize::try_from(item) else {
                return;
            };
            a.settings_refresh_item(&window, item);

        });
    }
    {
        install_seam!(window: window.on_settings_create_clicked, adapter: adapter;
        bind mut (a, window), args (item), {            let Ok(item) = usize::try_from(item) else {
                return;
            };
            a.settings_create_item(&window, item);

        });
    }
    {
        install_seam!(window: window.on_settings_disconnect_clicked, adapter: adapter;
        bind mut (a, window), args (item), {            let Ok(item) = usize::try_from(item) else {
                return;
            };
            a.settings_disconnect_item(&window, item);

        });
    }
    {
        install_seam!(window: window.on_settings_files_picked, adapter: adapter;
        bind mut (a, window), args (item, index), {            let (Ok(item), Ok(index)) = (usize::try_from(item), usize::try_from(index)) else {
                return;
            };
            a.settings_files_picked(&window, item, index);

        });
    }
    {
        install_seam!(window: window.on_settings_remote_bind_picked, adapter: adapter;
        bind mut (a, window), args (item, option), {            let (Ok(item), Ok(option)) = (usize::try_from(item), usize::try_from(option)) else {
                return;
            };
            a.settings_remote_bind_picked(&window, item, option);

        });
    }
    {
        install_seam!(window: window.on_settings_publish_picked, adapter: adapter;
        bind mut (a, window), args (item, option), {            let (Ok(item), Ok(option)) = (usize::try_from(item), usize::try_from(option)) else {
                return;
            };
            a.settings_publish_picked(&window, item, option);

        });
    }
    {
        install_seam!(window: window.on_settings_remove_confirmed, adapter: adapter; // slint: remove-confirmed
        bind mut (a, window), args (item), {            let Ok(item) = usize::try_from(item) else {
                return;
            };
            a.settings_remove_confirmed(&window, item);

        });
    }
    {
        install_seam!(window: window.on_settings_remove_cancelled, adapter: adapter; // slint: remove-cancelled
        bind mut (a, window), args (), {            a.settings_disarm_item();
            a.push_settings(&window);

        });
    }
    {
        install_seam!(window: window.on_settings_files_action_confirmed, adapter: adapter; // slint: files-action-confirmed
        bind mut (a, window), args (item), {            let Ok(item) = usize::try_from(item) else {
                return;
            };
            a.settings_files_action_confirmed(&window, item);

        });
    }
    {
        install_seam!(window: window.on_settings_files_action_cancelled, adapter: adapter; // slint: files-action-cancelled
        bind mut (a, window), args (), {            a.settings_disarm_item();
            a.push_settings(&window);

        });
    }
    {
        install_seam!(window: window.on_settings_files_board_action_clicked, adapter: adapter; // slint: files-board-action-clicked
        bind mut (a, window), args (item), {            let Ok(item) = usize::try_from(item) else {
                return;
            };
            a.settings_files_board_action(&window, item);

        });
        install_seam!(window: window.on_settings_files_board_restore_clicked, adapter: adapter; // slint: files-board-restore-clicked (US-6, D-7)
        bind mut (a, window), args (item), {            let Ok(item) = usize::try_from(item) else {
                return;
            };
            a.settings_files_board_restore(&window, item);

        });
    }
    {
        install_seam!(window: window.on_settings_preset_picked, adapter: adapter;
        bind mut (a, window), args (index), {            let Ok(index) = usize::try_from(index) else {
                return;
            };
            a.settings_preset_picked(&window, index);

        });
    }
    {
        install_seam!(window: window.on_settings_view_mounted, adapter: adapter;
        bind mut (a, window), args (), {            a.settings_view_mounted(&window);

        });
    }
    {
        install_seam!(window: window.on_settings_scale_committed, adapter: adapter;
        bind mut (a, window), args (text), {            a
                .settings_scale_committed(&window, &text);

        });
    }
    {
        install_seam!(window: window.on_settings_scale_pressed, adapter: adapter;
        bind mut (a, window), args (), {            a.settings_scale_pressed(&window);

        });
    }
    {
        install_seam!(window: window.on_settings_scale_released, adapter: adapter;
        bind mut (a, window), args (value: f32), {            a
                .settings_scale_released(&window, f64::from(value));

        });
    }
    {
        install_seam!(window: window.on_settings_scale_focus_lost, adapter: adapter;
        bind mut (a, window), args (), {            a.settings_scale_focus_lost(&window);

        });
    }
    // The settings editor's general seams (the GUI settings editor
    // chunk): the same weak-handle wiring shape, one block per seam.
    {
        // The Files row's poll cadence commit seam (#1083, W-6): the
        // general fields' funnel under the plan row's own address.
        install_seam!(window: window.on_files_poll_secs_committed, adapter: adapter; // files-poll-secs-committed
        bind mut (a, window), args (field, text), {            let Ok(field) = usize::try_from(field) else {
                return;
            };
            a
                .files_poll_secs_committed(&window, field, &text);

        });
        install_seam!(window: window.on_settings_general_committed, adapter: adapter;
        bind mut (a, window), args (field, text), {            let Ok(field) = usize::try_from(field) else {
                return;
            };
            a
                .settings_general_committed(&window, field, &text);

        });
    }
    {
        install_seam!(window: window.on_settings_general_focus_lost, adapter: adapter;
        bind mut (a, window), args (field), {            let Ok(field) = usize::try_from(field) else {
                return;
            };
            a
                .settings_general_focus_lost(&window, field);

        });
    }
    {
        install_seam!(window: window.on_settings_reminders_picked, adapter: adapter;
        bind mut (a, window), args (index), {            let Ok(index) = usize::try_from(index) else {
                return;
            };
            a
                .settings_reminders_picked(&window, index);

        });
    }
    {
        install_seam!(window: window.on_settings_sink_picked, adapter: adapter;
        bind mut (a, window), args (index), {            let Ok(index) = usize::try_from(index) else {
                return;
            };
            a.settings_sink_picked(&window, index);

        });
    }
    // The focus section's pick seams (the focus settings editor,
    // #625): the reminders grammar, one block per toggle.
    {
        install_seam!(window: window.on_settings_focus_enabled_picked, adapter: adapter;
        bind mut (a, window), args (index), {            let Ok(index) = usize::try_from(index) else {
                return;
            };
            a
                .settings_focus_enabled_picked(&window, index);

        });
    }
    {
        install_seam!(window: window.on_settings_focus_chain_picked, adapter: adapter;
        bind mut (a, window), args (index), {            let Ok(index) = usize::try_from(index) else {
                return;
            };
            a
                .settings_focus_chain_picked(&window, index);

        });
    }
    {
        install_seam!(window: window.on_settings_focus_notify_picked, adapter: adapter;
        bind mut (a, window), args (index), {            let Ok(index) = usize::try_from(index) else {
                return;
            };
            a
                .settings_focus_notify_picked(&window, index);

        });
    }
}
