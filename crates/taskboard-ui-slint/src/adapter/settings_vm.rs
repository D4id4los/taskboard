// SPDX-License-Identifier: AGPL-3.0-only

use std::collections::BTreeMap;

use slint::language::KeyboardModifiers;
use slint::{ComponentHandle as _, SharedString};
use tokio::sync::mpsc;

use taskboard_domain::{BoardId, RemoteBoardId};

use tokio::sync::oneshot;

use super::replies::{AutoListBackoff, ItemAddr, PendingKind, PendingReply};
use crate::AppWindow;
use crate::requests::{self, ConnectorInfo, InvalidKind, SettingsError, UiRequest};
use crate::settings_form::{self, ConnectionDraft};

/// The typed last-outcome marker the model renders structurally
/// (R5: never message text).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SettingsOutcome {
    #[default]
    None,
    Ok,
    Busy,
    /// The operation was refused (validation, verify rejected,
    /// persist rejected).
    Rejected,
    /// The server/store was unreachable.
    Unavailable,
    /// Everything else (store/attach failures).
    Failed,
}

/// The status panel's VM (OP3 W-1..W-3, journey plan
/// `2026-11-21-parity-restore-journeys-status-surface.md`): the single
/// in-flight `StatusQuery` reply half. The folded data lives slint-side
/// (the `StatusPanelFacts` global's typed properties — the panel
/// renders from its facts, the adapter only dispatches and folds).
#[derive(Default)]
pub(in crate::adapter) struct StatusPanelVm {
    pub(in crate::adapter) pending: Option<oneshot::Receiver<crate::requests::StatusSnapshot>>,
    /// The armed remediation verb (the D2 command plan W-4): pure
    /// presentation state — nothing dispatches until the strip's
    /// Confirm; Cancel disarms. One armed subject (the shared
    /// grammar: arming a different verb re-arms).
    pub(in crate::adapter) armed: Option<crate::requests::StatusRemediationAction>,
    /// The in-flight remediation's reply half (W-6): `Some` between
    /// the Confirm dispatch and the settled reply.
    pub(in crate::adapter) remediation_pending:
        Option<oneshot::Receiver<crate::requests::StatusRemediationOutcome>>,
}

#[derive(Debug, Default)]
pub(in crate::adapter) struct SettingsVm {
    pub(in crate::adapter) draft: ConnectionDraft,
    pub(in crate::adapter) rows: Vec<ConnectorInfo>,
    /// Connector name → down-override (the `ConnectorLost`/
    /// `ConnectorRestored` episodes, D8); absent = no override.
    pub(in crate::adapter) liveness: BTreeMap<String, bool>,
    /// The last listing per connector (round-5 chunk 8): the remote
    /// boards each Deck row maps; bindings render from the live state.
    pub(in crate::adapter) remotes: BTreeMap<String, Vec<requests::RemoteBoardRow>>,
    /// The mount auto-lists' in-flight replies (round-6 D-6.7),
    /// connector → parked reply half. Deliberately NOT the shared
    /// `replies`/`pending` machine: that drain's settle block clears
    /// `pending`/`busy_target` and stamps a global outcome for every
    /// settled reply — a background listing landing mid-Save would
    /// wipe the user's op's busy marker and banner. Here a settled
    /// reply only fills `remotes` (silent by construction); the slot
    /// occupies the map until the reply lands, which is the rapid-
    /// remount double-fire guard. One listing request per connector
    /// until its reply lands.
    pub(in crate::adapter) auto_listings:
        BTreeMap<String, oneshot::Receiver<Result<Vec<requests::RemoteBoardRow>, SettingsError>>>,
    /// The files watch's per-board states (US-8, D-8): updated by
    /// `drain_files_sync`, consumed by the Files roster's row fold.
    pub(in crate::adapter) files_board_states:
        std::collections::BTreeMap<uuid::Uuid, crate::presentation::FilesBoardState>,
    /// The mount auto-list's typed backoff (the files-sync-status UX
    /// plan US-3, D-4): per-connector consecutive-failure record — a
    /// display posture, not a sync decision. The mount auto-list
    /// checks it before building the request; the manual refresh
    /// clears the entry (explicit intent) and failure re-arms.
    pub(in crate::adapter) auto_list_backoff: BTreeMap<String, AutoListBackoff>,
    /// The rendered connections items' addresses, index-aligned with
    /// the pushed `connection-items` (the callbacks' resolution table).
    pub(in crate::adapter) items: Vec<ItemAddr>,
    /// The armed-confirm strip's subject (UX R-2 D-4, #787): the one
    /// armed item surface-wide, keyed by `ItemAddr` (identity, not
    /// index — a roster re-derivation keeps the address; a vanished
    /// subject's strip is dead). Arming touches no lane state; the
    /// confirm clears the flag BEFORE the send.
    pub(in crate::adapter) armed_item: Option<ItemAddr>,
    /// The pickers' option ids, index-aligned with `board-options`.
    pub(in crate::adapter) local_choices: Vec<uuid::Uuid>,
    /// The board the in-flight Stop/Publish op addresses (round-7
    /// D-7.3, D9 — the `busy_remote` precedent): the specific roster
    /// row's busy marker; cleared with `pending`.
    pub(in crate::adapter) busy_board: Option<BoardId>,
    /// The connector name the in-flight op addresses (r2 D34): set by
    /// the send seams, cleared with `pending`.
    pub(in crate::adapter) busy_target: String,
    /// The remote id the in-flight bind/create/detach addresses.
    pub(in crate::adapter) busy_remote: Option<RemoteBoardId>,
    pub(in crate::adapter) pending: Option<PendingKind>,
    pub(in crate::adapter) replies: Vec<PendingReply>,
    pub(in crate::adapter) outcome: SettingsOutcome,
    /// The concrete error retained past the coarse fold (r2 D28):
    /// the status text's typed granularity rides it.
    pub(in crate::adapter) detail: Option<SettingsError>,
    /// The last-known persisted preset (r2 D27): seeded at mount,
    /// changed only by `apply_keymap_update` — the reply, never the
    /// click, is the selection's source.
    pub(in crate::adapter) preset: crate::keymap::Preset,
    /// The scale field's staged buffer (round-3 D20a): set per
    /// keystroke on the scale field, discarded on Return commit and
    /// on focus loss. While present it is the field's rendered echo
    /// and the flavour's input; the live-scale echo renders only in
    /// its absence.
    pub(in crate::adapter) scale_draft: Option<String>,
    /// The general snapshot (the GUI settings editor): seeded from
    /// the hand-over, refreshed ONLY from write replies (the reply
    /// echoes the fresh file values). Its env-override set is
    /// hand-over state — replies carry it empty by contract and the
    /// VM preserves the standing set on every apply.
    pub(in crate::adapter) snapshot: crate::requests::SettingsSnapshot,
    /// The general text fields' staged buffers (the `scale_draft`
    /// precedent): set per keystroke, discarded on Return commit and
    /// on focus loss.
    pub(in crate::adapter) day_start_draft: Option<String>,
    pub(in crate::adapter) tick_draft: Option<String>,
    pub(in crate::adapter) before_draft: Option<String>,
    pub(in crate::adapter) poll_draft: Option<String>,
    pub(in crate::adapter) backoff_initial_draft: Option<String>,
    pub(in crate::adapter) backoff_max_draft: Option<String>,
    pub(in crate::adapter) due_soon_draft: Option<String>,
    /// The focus minutes' staged buffers (the focus settings editor,
    /// #625): the same grammar as the other general text fields.
    pub(in crate::adapter) focus_work_draft: Option<String>,
    pub(in crate::adapter) focus_break_draft: Option<String>,
    pub(in crate::adapter) focus_long_break_draft: Option<String>,
    pub(in crate::adapter) focus_every_draft: Option<String>,
    /// The Files row's poll cadence field's staged buffer (#1083, W-6):
    /// the general fields' grammar verbatim.
    pub(in crate::adapter) files_poll_secs_draft: Option<String>,
}

impl SettingsVm {
    /// Whether the draft may save (the Save button's enablement).
    pub(in crate::adapter) fn save_enabled(&self) -> bool {
        self.pending.is_none() && settings_form::draft_valid(&self.draft)
    }

    /// Sends one request with its reply half parked. Returns whether
    /// the lane accepted it — a full/closed inbox degrades to the
    /// typed outcome at the caller (the choke point), never a wedge.
    pub(in crate::adapter) fn send(
        &mut self,
        requests: &mpsc::Sender<UiRequest>,
        request: UiRequest,
        kind: PendingKind,
        reply: PendingReply,
        busy_target: &str,
    ) -> bool {
        if requests.try_send(request).is_err() {
            tracing::warn!("request channel full or closed; settings op dropped");
            return false;
        }
        self.pending = Some(kind);
        busy_target.clone_into(&mut self.busy_target);
        self.replies.push(reply);
        true
    }

    pub(in crate::adapter) fn outcome_of(err: &SettingsError) -> SettingsOutcome {
        match err {
            SettingsError::Invalid { .. }
            | SettingsError::NoSecretStored
            | SettingsError::StoreUnsupported
            | SettingsError::VerifyRejected
            | SettingsError::PersistRejected
            | SettingsError::Gone
            | SettingsError::BindRefused
            | SettingsError::EnvLocked { .. }
            // The busy lane's own posture (T-38, #821): the surface
            // runs one request at a time — a click under a pending
            // request is a refusal, not a failure.
            | SettingsError::RequestPending => SettingsOutcome::Rejected,
            SettingsError::VerifyUnreachable
            | SettingsError::StoreFailed
            // The publish budget expired with the ceremony still
            // moving (D-6.9): transient, not a failure — observe the
            // board's sync state instead of retrying the publish.
            | SettingsError::SyncTimedOut => SettingsOutcome::Unavailable,
            SettingsError::VerifyFailed
            | SettingsError::PersistFailed
            | SettingsError::AttachBusy
            | SettingsError::AttachFailed
            // The move's step-failure class (round-6 D-6.8 phase 2,
            // D-5) folds every ceremony step class; the concrete
            // `MoveError` rode the app's log. The undo window's lost
            // coordinates fold the same failed class (US-6).
            | SettingsError::MoveFailed
            | SettingsError::UndoCoordinatesLost => SettingsOutcome::Failed,
            // The move's refusal classes (D-5): a lane-off or
            // busy-outbox world refused the attempt; the undo
            // window's raced/closed stages refuse too (US-6).
            SettingsError::FilesSyncDisabled
            | SettingsError::OutboxNotQuiescent
            | SettingsError::NothingToUndo
            | SettingsError::UndoClosed => {
                SettingsOutcome::Rejected
            }
        }
    }
}

/// The settings editor's general text fields (the GUI settings
/// editor chunk): the seam addresses, their dotted config keys (the
/// env-override set's comparison term), the lane fields, the mirror
/// predicates, and the canonical echoes — one table, no per-field
/// fork in the seams.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::adapter) enum GeneralField {
    DayStart,
    /// The Files row's poll cadence (#1083, the small-gated journeys
    /// plan W-6): rendered on the Files card, but the general fields'
    /// staged-edit grammar verbatim — one machine, no second lane.
    FilesPollSecs,
    TickSecs,
    ReminderBefore,
    PollInterval,
    BackoffInitial,
    BackoffMax,
    DueSoonDays,
    FocusWorkMins,
    FocusBreakMins,
    FocusLongBreakMins,
    FocusLongBreakEvery,
}

impl GeneralField {
    /// The `field-edited` index contract the view's general rows
    /// carry (the connector form owns 0–3, the scale field 4, the
    /// general fields 5–11, the focus minutes 12–15).
    pub(in crate::adapter) const FOCUS_WORK_INDEX: usize = 12;
    pub(in crate::adapter) const FOCUS_BREAK_INDEX: usize = 13;
    pub(in crate::adapter) const FOCUS_LONG_BREAK_INDEX: usize = 14;
    pub(in crate::adapter) const FOCUS_EVERY_INDEX: usize = 15;
    /// The Files row's poll cadence field (#1083): rides the general
    /// fields' index contract (the focus minutes' 12–15 block's next
    /// slot).
    pub(in crate::adapter) const FILES_POLL_INDEX: usize = 16;

    pub(in crate::adapter) fn from_index(index: usize) -> Option<Self> {
        match index {
            5 => Some(Self::DayStart),
            6 => Some(Self::TickSecs),
            7 => Some(Self::ReminderBefore),
            8 => Some(Self::PollInterval),
            9 => Some(Self::BackoffInitial),
            10 => Some(Self::BackoffMax),
            11 => Some(Self::DueSoonDays),
            Self::FOCUS_WORK_INDEX => Some(Self::FocusWorkMins),
            Self::FOCUS_BREAK_INDEX => Some(Self::FocusBreakMins),
            Self::FOCUS_LONG_BREAK_INDEX => Some(Self::FocusLongBreakMins),
            Self::FOCUS_EVERY_INDEX => Some(Self::FocusLongBreakEvery),
            Self::FILES_POLL_INDEX => Some(Self::FilesPollSecs),
            _ => None,
        }
    }

    /// The dotted config key — the env-override set's comparison
    /// term and the apply map's key.
    pub(in crate::adapter) fn key(self) -> &'static str {
        match self {
            Self::DayStart => "preferences.day_start_time",
            Self::TickSecs => "reminders.tick_secs",
            Self::ReminderBefore => "reminders.default_before",
            Self::PollInterval => "sync.poll_interval_secs",
            Self::BackoffInitial => "sync.backoff_initial_secs",
            Self::BackoffMax => "sync.backoff_max_secs",
            Self::DueSoonDays => "ui.due_soon_days",
            Self::FocusWorkMins => "focus.work_mins",
            Self::FocusBreakMins => "focus.break_mins",
            Self::FocusLongBreakMins => "focus.long_break_mins",
            Self::FocusLongBreakEvery => "focus.long_break_every",
            Self::FilesPollSecs => "files_sync.poll_secs",
        }
    }

    pub(in crate::adapter) fn lane_field(self) -> crate::requests::Field {
        match self {
            Self::DayStart => crate::requests::Field::DayStart,
            Self::TickSecs => crate::requests::Field::TickSecs,
            Self::ReminderBefore => crate::requests::Field::ReminderBefore,
            Self::PollInterval => crate::requests::Field::PollInterval,
            Self::BackoffInitial => crate::requests::Field::BackoffInitial,
            Self::BackoffMax => crate::requests::Field::BackoffMax,
            Self::DueSoonDays => crate::requests::Field::DueSoonDays,
            Self::FocusWorkMins => crate::requests::Field::FocusWorkMins,
            Self::FocusBreakMins => crate::requests::Field::FocusBreakMins,
            Self::FocusLongBreakMins => crate::requests::Field::FocusLongBreakMins,
            Self::FocusLongBreakEvery => crate::requests::Field::FocusLongBreakEvery,
            Self::FilesPollSecs => crate::requests::Field::FilesPollSecs,
        }
    }

    /// The mirror predicate (`settings_form`): the commit seam's
    /// refusal source — the same function the flavour renders.
    pub(in crate::adapter) fn invalid(self, text: &str) -> Option<settings_form::InvalidKind> {
        match self {
            Self::DayStart => settings_form::day_start_invalid_kind(text),
            Self::TickSecs
            | Self::PollInterval
            | Self::BackoffInitial
            | Self::BackoffMax
            | Self::FilesPollSecs => settings_form::secs_invalid_kind(text),
            Self::ReminderBefore => settings_form::before_invalid_kind(text),
            Self::DueSoonDays => settings_form::due_soon_invalid_kind(text),
            Self::FocusWorkMins | Self::FocusBreakMins | Self::FocusLongBreakMins => {
                settings_form::focus_mins_invalid_kind(text)
            }
            Self::FocusLongBreakEvery => settings_form::focus_every_invalid_kind(text),
        }
    }

    /// The committed counterpart this field's ordering rule checks
    /// against, when one exists (the `initial ≤ max` rule, plan D8).
    pub(in crate::adapter) fn ordering_ceiling(
        self,
        snapshot: &crate::requests::SettingsSnapshot,
    ) -> Option<u64> {
        match self {
            Self::BackoffInitial => Some(snapshot.backoff_max_secs),
            Self::BackoffMax => Some(snapshot.backoff_initial_secs),
            _ => None,
        }
    }

    /// The canonical echo of the committed snapshot value
    /// (draft-when-present, committed-otherwise — the scale echo's
    /// grammar).
    pub(in crate::adapter) fn echo_of(
        self,
        snapshot: &crate::requests::SettingsSnapshot,
    ) -> String {
        match self {
            Self::DayStart => snapshot.day_start_time.format("%H:%M").to_string(),
            Self::TickSecs => snapshot.reminder_tick_secs.to_string(),
            Self::ReminderBefore => snapshot.reminder_before.clone(),
            Self::PollInterval => snapshot.poll_interval_secs.to_string(),
            Self::BackoffInitial => snapshot.backoff_initial_secs.to_string(),
            Self::BackoffMax => snapshot.backoff_max_secs.to_string(),
            Self::DueSoonDays => snapshot.due_soon_days.to_string(),
            Self::FocusWorkMins => snapshot.focus_work_mins.to_string(),
            Self::FocusBreakMins => snapshot.focus_break_mins.to_string(),
            Self::FocusLongBreakMins => snapshot.focus_long_break_mins.to_string(),
            Self::FocusLongBreakEvery => snapshot.focus_long_break_every.to_string(),
            // The raw `Option` echo: unset renders empty (the honest
            // "the default cadence applies" fact), a value renders its
            // seconds.
            Self::FilesPollSecs => snapshot
                .files_sync_poll_secs
                .map_or(String::new(), |secs| secs.to_string()),
        }
    }
}

/// The picker's index contract (r2 D27): `[Standard, Emacs, Vi]` —
/// the same order the request seam maps back to presets
/// (`settings_preset_picked`). One source for the push; the slint
/// property is an `int`.
pub(in crate::adapter) fn preset_index(preset: crate::keymap::Preset) -> i32 {
    match preset {
        crate::keymap::Preset::Standard => 0,
        crate::keymap::Preset::Emacs => 1,
        crate::keymap::Preset::Vi => 2,
    }
}

/// The form-mirror field → lane field mapping (r2 D34): the save
/// click-through renders the same typed lattice the app-side
/// validation would answer with.
pub(in crate::adapter) fn map_form_field(field: settings_form::Field) -> requests::Field {
    match field {
        settings_form::Field::ServerUrl => requests::Field::ServerUrl,
        settings_form::Field::Username => requests::Field::Username,
        settings_form::Field::Password => requests::Field::Password,
    }
}

/// The kind mirror → lane kind mapping (the same one-for-one lattice;
/// `NotFinite`/`OutOfRange` joined with the scale field's parity,
/// round-3 D20a — the sentences already exist in `settings_status`).
pub(in crate::adapter) fn map_form_kind(kind: settings_form::InvalidKind) -> InvalidKind {
    match kind {
        settings_form::InvalidKind::Empty => InvalidKind::Empty,
        settings_form::InvalidKind::NotKebab => InvalidKind::NotKebab,
        settings_form::InvalidKind::BadUrl => InvalidKind::BadUrl,
        settings_form::InvalidKind::NotFinite => InvalidKind::NotFinite,
        settings_form::InvalidKind::OutOfRange => InvalidKind::OutOfRange,
    }
}

/// Converts a delivered key event to a [`crate::keymap::Stroke`]
/// (D18): `None` for anything that is not exactly one bindable key —
/// modifier-only/control events are rejected outright so text input
/// and traversal keep flowing.
pub(in crate::adapter) fn to_stroke(
    text: &SharedString,
    modifiers: KeyboardModifiers,
) -> Option<crate::keymap::Stroke> {
    let mut chars = text.chars();
    let key = chars.next()?;
    if chars.next().is_some() {
        return None;
    }
    crate::keymap::Stroke::from_event(
        key,
        modifiers.control,
        modifiers.alt,
        modifiers.shift,
        modifiers.meta,
    )
}

/// The scale commit's scroll-anchor capture (round-3 D20c): the
/// viewport offset of the scale row, read PRE-layout (the caller
/// writes `Theme.base_scale` right after, re-laying the content out
/// on the next pass), stored with the one-shot flag. The markup's
/// `changed content-height` hook consumes it by restoring the row's
/// viewport position — measured, not predicted.
pub(in crate::adapter) fn request_scale_anchor(window: &AppWindow) {
    let facts = window.global::<crate::SettingsScrollFacts>();
    facts.set_anchor_offset(facts.get_scale_row_y() + facts.get_viewport_y());
    facts.set_anchor_requested(true);
}
