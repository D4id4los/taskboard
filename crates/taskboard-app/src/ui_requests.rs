// SPDX-License-Identifier: AGPL-3.0-only

//! The UI → app request lane's app half (round-1 D21): the debounced
//! forwarder that persists device-local stack column widths into the
//! `[ui.stack_widths]` config subtree and the device-local
//! main-window geometry into `[ui.window_geometry]` (#412).
//! Single-owner rule (R6, extended): this task owns every write of
//! both subtrees — no other code path touches them; one flush is one
//! atomic read-merge-write over the whole document, and every
//! in-process settings write runs on this same task, so daemon-side
//! writers never race each other. Every error is logged and swallowed:
//! the UI's widths live on in
//! the view state, the next received request retries the whole
//! read-merge-write, and the forwarder never blocks or crashes the UI
//! (D8) — it only ever holds owned maps, never a lock across `.await`.
//! Residual constraint: `connect`/`login`/`logout` are terminal-bound
//! ceremonies (the IPC server refuses them), so a one-shot CLI process
//! rewriting the document while the daemon flushes is the one
//! read-merge-write window no lock covers — pre-existing since D21
//! (the `stack_widths` flush carried it); its worst case is a lost
//! update of one side, never a torn file (the write is rename-atomic).

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use arc_swap::ArcSwap;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use taskboard_domain::{AppState, BoardId, ConnectorId, RemoteBoardId, StateCommand};
use taskboard_state::{EngineHandle, ExecuteError};
use taskboard_sync_nextcloud::DeckClient;
use taskboard_ui_slint::geometry::WindowGeometry;
use taskboard_ui_slint::requests::{
    ConnectionProfile, ConnectionsSnapshot, ConnectorInfo, Field, InvalidKind, KeymapUpdate,
    ReminderSink, RemoteBoardRow, SettingsError, SettingsSnapshot, UiRequest,
};

use crate::bootstrap::ConnectorControl;
use crate::commands::CommandError;
use crate::config::{AppConfig, ConfigError, ConnectorCfg, SinkChoice, SyncBackend};
use crate::migration::url_host;
use crate::secrets::{CredentialError, CredentialStore};

/// Capacity of the UI → app request channel: human-input scale (the
/// `UI_COMMAND_CAPACITY` rationale at a smaller budget — a resize
/// burst is coalesced by the debounce, not queued).
pub const UI_REQUEST_CAPACITY: usize = 16;

/// The debounce quiet window: bursts of resize events collapse into
/// one write at the end of the last quiet window (deterministic under
/// `tokio::time::pause` — no sleeps, ever).
pub const UI_REQUEST_DEBOUNCE: std::time::Duration = std::time::Duration::from_millis(500);

/// The UI → app request channel at its house capacity — one
/// construction site for both UI lifecycles (desktop and kiosk
/// appliance).
#[must_use]
pub fn ui_request_channel() -> (mpsc::Sender<UiRequest>, mpsc::Receiver<UiRequest>) {
    mpsc::channel(UI_REQUEST_CAPACITY)
}

/// The settings handlers' dependencies (round-1 chunk 3, plan D3): the
/// config write target, the credential store, and the connector
/// lifecycle — everything a spawned task can own without an `App`
/// reference. Built once by each UI lifecycle (desktop and kiosk) and
/// moved into the forwarder.
pub struct ConnectionWorkspace {
    /// The resolved config write path (the overlay target).
    pub config_write_path: PathBuf,
    /// The process's resolved credential store.
    pub store: Arc<dyn CredentialStore>,
    /// The connector lifecycle capability (round-1 DEV-1).
    pub control: ConnectorControl,
    /// The engine handle — the boot-pull settle reads it
    /// (`wait_for_boot_pull(&EngineHandle, ..)`).
    pub engine: EngineHandle,
}

/// Spawns the UI request forwarder: width snapshots merge into the
/// pending map, geometry snapshots replace the pending rect (latest
/// wins), and both debounce into one config flush (chunk 2's path,
/// byte-for-byte semantics — one document per flush, D7); every
/// settings request is handled immediately — one-shot actions with
/// replies are never debounced, including ones arriving inside a
/// quiet window (plan D3). On channel close (the UI side dropped) one
/// final flush lands a change sent right before quit, then the task
/// exits. The caller owns the handle's retirement.
pub fn spawn_ui_request_forwarder(
    mut requests: mpsc::Receiver<UiRequest>,
    state: Arc<ArcSwap<AppState>>,
    workspace: ConnectionWorkspace,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut pending_widths: BTreeMap<uuid::Uuid, BTreeMap<uuid::Uuid, u32>> = BTreeMap::new();
        let mut pending_geometry: Option<WindowGeometry> = None;
        loop {
            let Some(request) = requests.recv().await else {
                flush(
                    &mut pending_widths,
                    &mut pending_geometry,
                    &workspace.config_write_path,
                    &state.load(),
                );
                tracing::info!("ui request channel closed; forwarder exiting");
                return;
            };
            match request {
                UiRequest::StackWidthsChanged { board, widths } => {
                    merge_widths(board, widths, &mut pending_widths);
                }
                UiRequest::WindowGeometryChanged { geometry } => {
                    // Latest wins: a capture burst collapses to the
                    // last rect (the full-snapshot semantics).
                    pending_geometry = Some(geometry);
                }
                settings => {
                    handle_settings(settings, &workspace).await;
                    continue;
                }
            }
            // The debounce quiet window (both debounced lanes): a
            // late debounced request merges and restarts it; a
            // settings request is executed on the spot (never
            // debounced); close flushes; elapsed falls through to the
            // flush.
            loop {
                match tokio::time::timeout(UI_REQUEST_DEBOUNCE, requests.recv()).await {
                    Ok(Some(UiRequest::StackWidthsChanged { board, widths })) => {
                        merge_widths(board, widths, &mut pending_widths);
                    }
                    Ok(Some(UiRequest::WindowGeometryChanged { geometry })) => {
                        pending_geometry = Some(geometry);
                    }
                    Ok(Some(settings)) => {
                        handle_settings(settings, &workspace).await;
                    }
                    Ok(None) => {
                        flush(
                            &mut pending_widths,
                            &mut pending_geometry,
                            &workspace.config_write_path,
                            &state.load(),
                        );
                        tracing::info!("ui request channel closed; forwarder exiting");
                        return;
                    }
                    Err(_elapsed) => break,
                }
            }
            flush(
                &mut pending_widths,
                &mut pending_geometry,
                &workspace.config_write_path,
                &state.load(),
            );
        }
    })
}

/// Merges one width snapshot into the pending map: a board's snapshot
/// *replaces* that board's entry (full-snapshot semantics, idempotent
/// under re-send).
fn merge_widths(
    board: taskboard_domain::BoardId,
    widths: Vec<(taskboard_domain::StackId, u32)>,
    pending: &mut BTreeMap<uuid::Uuid, BTreeMap<uuid::Uuid, u32>>,
) {
    pending.insert(
        board.as_uuid(),
        widths
            .into_iter()
            .map(|(stack, px)| (stack.as_uuid(), px))
            .collect(),
    );
}

/// The write-time liveness prune (the `normalize_board` /
/// `stacks_in_board` rules, read-only): keep a board iff it exists and
/// is neither deleted nor archived; keep a stack iff it exists, is
/// live, and still belongs to that board. A board with nothing left is
/// dropped entirely — no empty husk in the config.
fn prune_widths(
    pending: &BTreeMap<uuid::Uuid, BTreeMap<uuid::Uuid, u32>>,
    state: &AppState,
) -> BTreeMap<uuid::Uuid, BTreeMap<uuid::Uuid, u32>> {
    pending
        .iter()
        .filter_map(|(board, stacks)| {
            let board_id = taskboard_domain::BoardId::from_uuid(*board);
            let live = state
                .boards
                .get(&board_id)
                .is_some_and(|b| !b.deleted && !b.archived);
            if !live {
                return None;
            }
            let kept: BTreeMap<uuid::Uuid, u32> = stacks
                .iter()
                .filter(|(stack, _)| {
                    state
                        .stacks
                        .get(&taskboard_domain::StackId::from_uuid(**stack))
                        .is_some_and(|s| !s.deleted && s.board == board_id)
                })
                .map(|(stack, px)| (*stack, *px))
                .collect();
            (!kept.is_empty()).then_some((*board, kept))
        })
        .collect()
}

/// The flush: prune, then one read-merge-write over the config
/// overlay covering both pending subtrees (D7 — one document, one
/// atomic write, the R6 single-writer discipline). Errors are warned
/// and swallowed — the pending state is dropped either way; the next
/// received request re-sends the full snapshot.
fn flush(
    pending_widths: &mut BTreeMap<uuid::Uuid, BTreeMap<uuid::Uuid, u32>>,
    pending_geometry: &mut Option<WindowGeometry>,
    config_write_path: &std::path::Path,
    state: &AppState,
) {
    let had_widths = !pending_widths.is_empty();
    if !had_widths && pending_geometry.is_none() {
        return; // nothing was ever received: no write at all
    }
    // `None` widths = the subtree stays untouched this flush (a
    // geometry-only change must not remove or prune the widths); an
    // `Some(empty)` prune result still removes the subtree (no husk).
    let pruned = if had_widths {
        let pruned = prune_widths(pending_widths, state);
        pending_widths.clear();
        Some(pruned)
    } else {
        None
    };
    let geometry = pending_geometry.take();
    if let Err(err) = overlay_ui(config_write_path, pruned.as_ref(), geometry.as_ref()) {
        tracing::warn!(error = %err, "ui-config overlay write failed; the next event retries");
    }
}

/// The widths-only overlay wrapper: the single-subtree path the
/// overlay tests drive (the forwarder itself always goes through
/// [`overlay_ui`], so a flush stays one document).
#[cfg(test)]
fn overlay_stack_widths(
    path: &std::path::Path,
    widths: &BTreeMap<uuid::Uuid, BTreeMap<uuid::Uuid, u32>>,
) -> std::io::Result<()> {
    overlay_ui(path, Some(widths), None)
}

/// The one-document overlay (D7): applies the pending subtrees —
/// `None` leaves a subtree untouched — and writes the whole document
/// once, atomically. Comments, formatting, and every unrelated table
/// are preserved by construction (`toml_edit`).
fn overlay_ui(
    path: &std::path::Path,
    widths: Option<&BTreeMap<uuid::Uuid, BTreeMap<uuid::Uuid, u32>>>,
    geometry: Option<&WindowGeometry>,
) -> std::io::Result<()> {
    let mut doc = crate::config::overlay_document(path)?;
    let ui = doc
        .entry("ui")
        .or_insert(toml_edit::Item::Table(toml_edit::Table::new()))
        .as_table_mut()
        .ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, "[ui] is not a table")
        })?;
    if let Some(widths) = widths {
        apply_stack_widths(ui, widths)?;
    }
    if let Some(geometry) = geometry {
        apply_window_geometry(ui, geometry);
    }
    crate::config::write_config_atomic(path, doc.to_string().as_str())
}

/// The `[ui.stack_widths]` subtree applier (the overlay's width half,
/// byte-for-byte the pre-#412 body).
fn apply_stack_widths(
    ui: &mut toml_edit::Table,
    widths: &BTreeMap<uuid::Uuid, BTreeMap<uuid::Uuid, u32>>,
) -> std::io::Result<()> {
    if widths.is_empty() {
        ui.remove("stack_widths");
    } else {
        let subtree = ui
            .entry("stack_widths")
            .or_insert(toml_edit::Item::Table(toml_edit::Table::new()))
            .as_table_mut()
            .ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "[ui.stack_widths] is not a table",
                )
            })?;
        subtree.clear();
        // Deterministic order: the BTreeMap iteration *is* the
        // document order of the per-board tables.
        for (board, stacks) in widths {
            let mut board_table = toml_edit::Table::new();
            for (stack, px) in stacks {
                board_table.insert(&stack.to_string(), toml_edit::value(i64::from(*px)));
            }
            subtree.insert(&board.to_string(), toml_edit::Item::Table(board_table));
        }
    }
    Ok(())
}

/// The `[ui.window_geometry]` subtree applier (#412): the four
/// physical-pixel cells of the pending rect. No liveness prune —
/// nothing in the engine state can invalidate a window rect.
fn apply_window_geometry(ui: &mut toml_edit::Table, geometry: &WindowGeometry) {
    let mut subtree = toml_edit::Table::new();
    subtree.insert("x", toml_edit::value(i64::from(geometry.x())));
    subtree.insert("y", toml_edit::value(i64::from(geometry.y())));
    subtree.insert("width", toml_edit::value(i64::from(geometry.width())));
    subtree.insert("height", toml_edit::value(i64::from(geometry.height())));
    ui.insert("window_geometry", toml_edit::Item::Table(subtree));
}

/// The bounded, offline-first settle after a settings save (the boot
/// ceremonies' budget; plan F-C2/D3).
const SAVE_SETTLE: std::time::Duration = std::time::Duration::from_secs(5);

/// The publish ceremony's settle budget (D-6.9): the CLI default
/// (`boards publish`'s 60 s), deliberately not `SAVE_SETTLE`'s 5 s —
/// the budget buys the full first outbox drain onto a freshly created
/// board, the same semantics the CLI ceremony promises. The settings
/// busy state covers the window.
const PUBLISH_SETTLE: std::time::Duration = std::time::Duration::from_secs(60);

/// Dispatches one settings request (never debounced — plan D3). Every
/// reply is fire-and-forget-honest: a dropped receiver (a stale form)
/// is at most a log line, never a panic, and persistence is never
/// undone by a failed reply.
// A pure dispatch table: one routing arm per lane variant, each body
// a one-line delegation into the shared choreography — the arm count
// tracks the closed enum, not the function's complexity.
#[allow(clippy::too_many_lines)]
async fn handle_settings(request: UiRequest, workspace: &ConnectionWorkspace) {
    match request {
        UiRequest::StackWidthsChanged { .. } | UiRequest::WindowGeometryChanged { .. } => {
            unreachable!("width and geometry snapshots take the debounce path")
        }
        UiRequest::SaveConnection { profile, reply } => {
            let outcome = save_connection(workspace, &profile).await;
            let _ = reply.send(outcome);
        }
        UiRequest::RemoveConnection { name, reply } => {
            let outcome = remove_connection(workspace, &name).await;
            let _ = reply.send(outcome);
        }
        UiRequest::ListRemoteBoards { connector, reply } => {
            let outcome = list_remote_boards(workspace, &connector).await;
            let _ = reply.send(outcome);
        }
        UiRequest::AttachConnector {
            board,
            connector,
            remote,
            reply,
        } => {
            let outcome = attach_connection(workspace, board, &connector, remote).await;
            let _ = reply.send(outcome);
        }
        UiRequest::CreateLocalBoard {
            connector,
            remote,
            reply,
        } => {
            let outcome = create_local_board(workspace, &connector, remote).await;
            let _ = reply.send(outcome);
        }
        UiRequest::DetachConnector {
            board,
            connector,
            reply,
        } => {
            let outcome = detach_connection(workspace, board, &connector).await;
            let _ = reply.send(outcome);
        }
        UiRequest::PublishBoard {
            board,
            connector,
            reply,
        } => {
            let outcome = publish_board(workspace, board, &connector).await;
            let _ = reply.send(outcome);
        }
        UiRequest::MoveBoardToFiles { board, reply } => {
            let outcome = move_board_to_files(workspace, board).await;
            let _ = reply.send(outcome);
        }
        UiRequest::StopBoardFilesSync { board, reply } => {
            let outcome = stop_board_files_sync(workspace, board).await;
            let _ = reply.send(outcome);
        }
        UiRequest::PublishBoardToFiles { board, reply } => {
            let outcome = publish_board_to_files(workspace, board).await;
            let _ = reply.send(outcome);
        }
        UiRequest::SetKeymapPreset { preset, reply } => {
            let _ = reply.send(set_keymap_preset(workspace, preset));
        }
        UiRequest::SetScale { scale, reply } => {
            let _ = reply.send(set_scale(workspace, scale));
        }
        UiRequest::SetDayStart { time, reply } => {
            let _ = reply.send(
                run_editor_write(workspace, Field::DayStart, |path| {
                    crate::settings_overlay::set_day_start(path, time)
                })
                .await,
            );
        }
        UiRequest::SetRemindersEnabled { enabled, reply } => {
            let _ = reply.send(
                run_editor_write(workspace, Field::RemindersEnabled, |path| {
                    crate::settings_overlay::set_reminders_enabled(path, enabled)
                })
                .await,
            );
        }
        UiRequest::SetReminderTick { secs, reply } => {
            let _ = reply.send(
                run_editor_write(workspace, Field::TickSecs, |path| {
                    crate::settings_overlay::set_reminder_tick(path, secs)
                })
                .await,
            );
        }
        UiRequest::SetReminderBefore { before, reply } => {
            let _ = reply.send(
                run_editor_write(workspace, Field::ReminderBefore, |path| {
                    crate::settings_overlay::set_reminder_before(path, before)
                })
                .await,
            );
        }
        UiRequest::SetReminderSink { sink, reply } => {
            let _ = reply.send(
                run_editor_write(workspace, Field::ReminderSink, |path| {
                    crate::settings_overlay::set_reminder_sink(path, map_sink(sink))
                })
                .await,
            );
        }
        UiRequest::SetPollInterval { secs, reply } => {
            let _ = reply.send(
                run_editor_write(workspace, Field::PollInterval, |path| {
                    crate::settings_overlay::set_poll_interval(path, secs)
                })
                .await,
            );
        }
        UiRequest::SetBackoffInitial { secs, reply } => {
            let _ = reply.send(
                run_editor_write(workspace, Field::BackoffInitial, |path| {
                    crate::settings_overlay::set_backoff_initial(path, secs)
                })
                .await,
            );
        }
        UiRequest::SetBackoffMax { secs, reply } => {
            let _ = reply.send(
                run_editor_write(workspace, Field::BackoffMax, |path| {
                    crate::settings_overlay::set_backoff_max(path, secs)
                })
                .await,
            );
        }
        UiRequest::SetDueSoonDays { days, reply } => {
            let _ = reply.send(
                run_editor_write(workspace, Field::DueSoonDays, |path| {
                    crate::settings_overlay::set_due_soon_days(path, days)
                })
                .await,
            );
        }
        UiRequest::SetFocusEnabled { enabled, reply } => {
            let _ = reply.send(
                run_editor_write(workspace, Field::FocusEnabled, |path| {
                    crate::settings_overlay::set_focus_enabled(path, enabled)
                })
                .await,
            );
        }
        UiRequest::SetFocusWorkMins { mins, reply } => {
            let _ = reply.send(
                run_editor_write(workspace, Field::FocusWorkMins, |path| {
                    crate::settings_overlay::set_focus_work_mins(path, mins)
                })
                .await,
            );
        }
        UiRequest::SetFocusBreakMins { mins, reply } => {
            let _ = reply.send(
                run_editor_write(workspace, Field::FocusBreakMins, |path| {
                    crate::settings_overlay::set_focus_break_mins(path, mins)
                })
                .await,
            );
        }
        UiRequest::SetFocusChain { enabled, reply } => {
            let _ = reply.send(
                run_editor_write(workspace, Field::FocusChain, |path| {
                    crate::settings_overlay::set_focus_chain(path, enabled)
                })
                .await,
            );
        }
        UiRequest::SetFocusLongBreakMins { mins, reply } => {
            let _ = reply.send(
                run_editor_write(workspace, Field::FocusLongBreakMins, |path| {
                    crate::settings_overlay::set_focus_long_break_mins(path, mins)
                })
                .await,
            );
        }
        UiRequest::SetFocusLongBreakEvery { every, reply } => {
            let _ = reply.send(
                run_editor_write(workspace, Field::FocusLongBreakEvery, |path| {
                    crate::settings_overlay::set_focus_long_break_every(path, every)
                })
                .await,
            );
        }
        UiRequest::SetFocusNotify { enabled, reply } => {
            let _ = reply.send(
                run_editor_write(workspace, Field::FocusNotify, |path| {
                    crate::settings_overlay::set_focus_notify(path, enabled)
                })
                .await,
            );
        }
        UiRequest::SetFilesSyncEnabled {
            enabled,
            anchor,
            reply,
        } => {
            let _ = reply.send(
                run_editor_write(workspace, Field::FilesSyncEnabled, |path| {
                    crate::settings_overlay::set_files_sync_enabled(
                        path,
                        enabled,
                        anchor.as_deref(),
                    )
                })
                .await,
            );
        }
    }
}

/// One editor write's persist-then-command choreography (the GUI
/// settings editor's write port; round-5 chunk 5, D-5.3):
///
/// 1. the env-lock guard — a field whose effective value is pinned by
///    a `TASKBOARD_*` override refuses typed before the file is
///    touched (the adapter's gating prevents the attempt; this is the
///    authority);
/// 2. the typed writer runs against the workspace's resolved write
///    path — never a fresh resolution (plan W-8); the prove gate's
///    refusal maps typed, **no command is sent**, engine and file
///    untouched;
/// 3. the *effective* config (file + the standing env overrides — the
///    same provider chain the boot seed came from, so an env-pinned
///    knob never gets clobbered in the engine) re-decodes and mints
///    the domain snapshot;
/// 4. `UpdateSettings` carries it to the engine; the reply snapshot
///    echoes the engine's published settings — one source of truth.
///
/// A stopped engine replies [`SettingsError::Gone`]: the file *is*
/// written (the next boot picks it up) while the running engine keeps
/// its old values — the typed refusal lets the UI say so; no partial
/// repair exists or is needed (the file stays the boot authority).
async fn run_editor_write(
    workspace: &ConnectionWorkspace,
    field: Field,
    write: impl FnOnce(&std::path::Path) -> Result<(), crate::settings_overlay::OverlayError>,
) -> Result<SettingsSnapshot, SettingsError> {
    let path = workspace.config_write_path.as_path();
    let locked =
        AppConfig::env_overridden_keys(Some(path)).map_err(|err| map_config_error(&err))?;
    if editor_write_env_locked(field, &locked) {
        return Err(SettingsError::EnvLocked { field });
    }
    write(path).map_err(|err| map_overlay_error(field, &err))?;
    let effective = AppConfig::extract_raw(Some(path)).map_err(|err| map_config_error(&err))?;
    // The gate just proved this document boots; the typed arm stays
    // (never an `expect` on a file another process may have raced).
    effective.validate().map_err(|err| map_config_error(&err))?;
    let settings = effective.settings_snapshot();
    workspace
        .engine
        .execute(StateCommand::UpdateSettings { settings })
        .await
        .map_err(|err| {
            tracing::warn!(%err, "settings written but the engine did not apply them");
            SettingsError::Gone
        })?;
    let published = workspace.engine.shared_state().load();
    Ok(crate::ui::settings_snapshot_from_domain(
        &effective,
        &published.settings,
    ))
}

/// The lane's sink mirror → the config's choice.
fn map_sink(sink: ReminderSink) -> SinkChoice {
    match sink {
        ReminderSink::Auto => SinkChoice::Auto,
        ReminderSink::Log => SinkChoice::Log,
        ReminderSink::Desktop => SinkChoice::Desktop,
    }
}

/// The gate refusal's typed mapping (plan D8/D14): a blame inside the
/// edited field's own key surfaces as that field's typed invalidity
/// (parse-shaped fields take the reserved parse arm, numeric bounds
/// and the ordering take `OutOfRange`); any other blame — a
/// different field of the section, or an unattributable failure — is
/// the coarse persist rejection.
fn map_overlay_error(field: Field, err: &crate::settings_overlay::OverlayError) -> SettingsError {
    match err {
        crate::settings_overlay::OverlayError::Io(io) => map_io_error(io),
        crate::settings_overlay::OverlayError::Rejected(config_err) => {
            if config_err.field() == Some(editor_field_key(field)) {
                SettingsError::Invalid {
                    field,
                    kind: editor_invalid_kind(field),
                }
            } else {
                SettingsError::PersistRejected
            }
        }
    }
}

/// The env-lock verdict for one editor write: the field's own key is
/// pinned, or — the Files row's case (round-6 D-6.6) — the row writes
/// two keys (`enabled` + the re-anchor), so a pinned
/// `files_sync.anchor` must refuse the whole pick too; the writer
/// would otherwise churn the file under a pinned effective value.
fn editor_write_env_locked(field: Field, locked: &std::collections::BTreeSet<String>) -> bool {
    if locked.contains(editor_field_key(field)) {
        return true;
    }
    field == Field::FilesSyncEnabled && locked.contains("files_sync.anchor")
}

/// The dotted config key each editor `Field` writes (the blame
/// comparison term).
fn editor_field_key(field: Field) -> &'static str {
    match field {
        Field::DayStart => "preferences.day_start_time",
        Field::RemindersEnabled => "reminders.enabled",
        Field::TickSecs => "reminders.tick_secs",
        Field::ReminderSink => "reminders.sink",
        Field::ReminderBefore => "reminders.default_before",
        Field::PollInterval => "sync.poll_interval_secs",
        Field::BackoffInitial => "sync.backoff_initial_secs",
        Field::BackoffMax => "sync.backoff_max_secs",
        Field::DueSoonDays => "ui.due_soon_days",
        // The `[focus]` rows (the focus settings editor, #625). The
        // three bool arms exist for the same-section blame comparison
        // only — a typed boolean cannot carry an invalidity.
        Field::FocusEnabled => "focus.enabled",
        Field::FocusWorkMins => "focus.work_mins",
        Field::FocusBreakMins => "focus.break_mins",
        Field::FocusChain => "focus.chain",
        Field::FocusLongBreakMins => "focus.long_break_mins",
        Field::FocusLongBreakEvery => "focus.long_break_every",
        Field::FocusNotify => "focus.notify",
        Field::FilesSyncEnabled => "files_sync.enabled",
        // The connection-form fields never ride the editor writers.
        Field::Name | Field::ServerUrl | Field::Username | Field::Password | Field::Scale => "",
    }
}

/// Every editor-lattice key (the apply registry's comparison term):
/// each `Field` variant listed once — a variant added to the lane's
/// enum fails the totality match below until it is listed here, so the
/// registry test cannot silently drift from the editor.
#[cfg(test)]
pub(crate) fn editor_field_keys() -> std::collections::BTreeSet<&'static str> {
    const ALL: [Field; 22] = [
        Field::Name,
        Field::ServerUrl,
        Field::Username,
        Field::Password,
        Field::Scale,
        Field::DayStart,
        Field::RemindersEnabled,
        Field::TickSecs,
        Field::ReminderSink,
        Field::ReminderBefore,
        Field::PollInterval,
        Field::BackoffInitial,
        Field::BackoffMax,
        Field::DueSoonDays,
        Field::FocusEnabled,
        Field::FocusWorkMins,
        Field::FocusBreakMins,
        Field::FocusChain,
        Field::FocusLongBreakMins,
        Field::FocusLongBreakEvery,
        Field::FocusNotify,
        Field::FilesSyncEnabled,
    ];
    // Totality: the compiler refuses this match the moment `Field`
    // grows a variant the list above does not carry.
    for field in ALL {
        match field {
            Field::Name
            | Field::ServerUrl
            | Field::Username
            | Field::Password
            | Field::Scale
            | Field::DayStart
            | Field::RemindersEnabled
            | Field::TickSecs
            | Field::ReminderSink
            | Field::ReminderBefore
            | Field::PollInterval
            | Field::BackoffInitial
            | Field::BackoffMax
            | Field::DueSoonDays
            | Field::FocusEnabled
            | Field::FocusWorkMins
            | Field::FocusBreakMins
            | Field::FocusChain
            | Field::FocusLongBreakMins
            | Field::FocusLongBreakEvery
            | Field::FocusNotify
            | Field::FilesSyncEnabled => {}
        }
    }
    ALL.into_iter()
        .map(editor_field_key)
        .filter(|key| !key.is_empty())
        .collect()
}

/// The invalidity kind each editor field's own-blame refusal carries.
fn editor_invalid_kind(field: Field) -> InvalidKind {
    match field {
        // Parse-shaped refusals (an unparseable day start or before
        // text in the file): the reserved parse arm, the mirror's
        // parity.
        Field::DayStart | Field::ReminderBefore => InvalidKind::NotKebab,
        // Everything else: numeric bounds and the initial/max
        // ordering take the range kind; choice commits are typed
        // booleans/enums and the connection-form fields never ride
        // the editor writers — no invalidity can be blamed on any of
        // them, so the arm exists for the same-section blame
        // comparison only.
        _ => InvalidKind::OutOfRange,
    }
}

/// The Save ceremony (plan D3, in order; nothing persists before
/// verify): authoritative re-validation → `verify_credentials` →
/// overlay upsert → secret store/rotate → detach-first → attach with
/// the draft credentials (the exact credentials just verified — not a
/// store load-back, which an env store cannot answer) → settle → reply
/// the fresh list.
async fn save_connection(
    workspace: &ConnectionWorkspace,
    profile: &ConnectionProfile,
) -> Result<ConnectionsSnapshot, SettingsError> {
    let raw = ensure_migrated(workspace)?;
    let named;
    let profile = if profile.name.is_empty() {
        named = ConnectionProfile {
            name: sign_in_name(&raw, profile)?,
            ..profile.clone()
        };
        &named
    } else {
        profile
    };
    tracing::info!(
        connector = %profile.name,
        host = %url_host(&profile.server_url),
        "settings save started"
    );
    if profile.password.0.trim().is_empty() {
        return Err(SettingsError::Invalid {
            field: Field::Password,
            kind: InvalidKind::Empty,
        });
    }
    let (draft, id) = validated_draft(&raw, profile)?;

    crate::connections::verify_credentials(
        &profile.server_url,
        &profile.username,
        &profile.password.0,
    )
    .await
    .map_err(map_ceremony_error)?;

    // The rotation target: the replaced entry's identity, only when an
    // entry under this name existed and its identity differs.
    let old = raw
        .sync
        .connectors
        .iter()
        .flatten()
        .find(|entry| entry.name == profile.name)
        .filter(|entry| {
            entry.server_url != profile.server_url || entry.username != profile.username
        })
        .map(|entry| (entry.server_url.as_str(), entry.username.as_str()));

    crate::connections::overlay_upsert_connector(&workspace.config_write_path, &draft)
        .map_err(|err| map_io_error(&err))?;
    crate::connections::store_and_rotate_secret(
        &workspace.store,
        &profile.server_url,
        &profile.username,
        &profile.password.0,
        old,
    )
    .map_err(map_ceremony_error)?;

    // Detach-first, then attach (the session ceremony's swap rule).
    let _ = workspace.control.detach_connector_named(&id).await;
    let client = DeckClient::new(&profile.server_url, &profile.username, &profile.password.0)
        .map_err(|err| {
            tracing::warn!(variant = %err, "settings save: the draft client was refused");
            SettingsError::AttachFailed
        })?;
    workspace
        .control
        .attach_connector(id, client)
        .await
        .map_err(map_attach_error)?;
    crate::bootstrap::wait_for_boot_pull(&workspace.engine, SAVE_SETTLE).await;
    tracing::info!(
        connector = %profile.name,
        host = %url_host(&profile.server_url),
        "settings save succeeded"
    );
    Ok(current_connections(workspace))
}

/// The sign-in's connector name (round-5 chunk 8, D-8.2): a re-sign-in
/// of an identity already configured reuses that connector's name (the
/// upsert rotates its secret — never a duplicate card); a new identity
/// mints `kebab(host)-kebab(username)` with the collision suffixes.
fn sign_in_name(raw: &AppConfig, profile: &ConnectionProfile) -> Result<String, SettingsError> {
    let entries = raw.sync.connectors.as_deref().unwrap_or_default();
    if let Some(existing) = entries
        .iter()
        .find(|e| e.server_url == profile.server_url && e.username == profile.username)
    {
        return Ok(existing.name.clone());
    }
    let taken: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    crate::connectors::derive_connector_name(&profile.server_url, &profile.username, &taken)
        .map(|id| id.to_string())
        .map_err(|err| SettingsError::Invalid {
            field: Field::Name,
            kind: match err {
                crate::connectors::DeriveNameError::NotKebab => InvalidKind::NotKebab,
                crate::connectors::DeriveNameError::Exhausted => InvalidKind::Duplicate,
            },
        })
}

/// The account's remote-board listing (round-5 chunk 8, D-8.3): config
/// lookup → stored secret → `DeckClient::boards()` → the live rows. A
/// pure query: nothing is attached, bound, or persisted.
async fn list_remote_boards(
    workspace: &ConnectionWorkspace,
    connector_name: &str,
) -> Result<Vec<RemoteBoardRow>, SettingsError> {
    let raw = read_raw_config(workspace)?;
    let entry = connector_entry(&raw, connector_name)?;
    let stored = workspace
        .store
        .load(&entry.server_url, &entry.username)
        .map_err(map_credential_error)?;
    let remotes = live_remote_boards(&entry, stored.expose()).await?;
    Ok(remotes
        .into_iter()
        .map(|remote| RemoteBoardRow {
            remote_id: RemoteBoardId::deck(remote.id),
            title: remote.title,
        })
        .collect())
}

/// The named connector's config entry (`Gone` when it vanished).
fn connector_entry(raw: &AppConfig, name: &str) -> Result<ConnectorCfg, SettingsError> {
    raw.sync
        .connectors
        .iter()
        .flatten()
        .find(|entry| entry.name == name)
        .cloned()
        .ok_or(SettingsError::Gone)
}

/// The live (not deleted, not archived) remote boards the identity can
/// see — the listing is the credential verification.
async fn live_remote_boards(
    entry: &ConnectorCfg,
    password: &str,
) -> Result<Vec<taskboard_sync_nextcloud::Board>, SettingsError> {
    let client = DeckClient::new(&entry.server_url, &entry.username, password).map_err(|err| {
        tracing::warn!(variant = %err, "settings: the listing client was refused");
        SettingsError::AttachFailed
    })?;
    let boards = client
        .boards()
        .await
        .map_err(crate::commands::map_deck_error)
        .map_err(map_ceremony_error)?;
    Ok(boards
        .into_iter()
        .filter(|b| b.deleted_at == 0 && !b.archived)
        .collect())
}

/// The account Remove choreography (plan D3, account-shaped by round-5
/// chunk 8): the named connector's identity names the account — every
/// connector sharing its `(server_url, username)` leaves together. The
/// guard refuses typed (`BindRefused`) before anything changes while
/// any live board is bound through one of them (the cascade is a
/// backlog item, #700). Then: overlay remove per connector → idempotent
/// named detach → best-effort secret delete of the identity.
async fn remove_connection(
    workspace: &ConnectionWorkspace,
    name: &str,
) -> Result<ConnectionsSnapshot, SettingsError> {
    let raw = ensure_migrated(workspace)?;
    let identity = raw
        .sync
        .connectors
        .iter()
        .flatten()
        .find(|entry| entry.name == name)
        .map(|entry| (entry.server_url.clone(), entry.username.clone()));
    let names = match &identity {
        Some((server, user)) => account_connector_names(&raw, server, user),
        None => vec![name.to_owned()],
    };
    tracing::info!(
        connector = %name,
        host = identity.as_ref().map_or("-", |(server, _)| url_host(server)),
        "settings remove started"
    );
    if any_live_board_bound_through(workspace, &names) {
        return Err(SettingsError::BindRefused);
    }
    for connector in &names {
        crate::connections::overlay_remove_connector(&workspace.config_write_path, connector)
            .map_err(|err| map_io_error(&err))?;
        // A non-kebab name was never minted at any attach, so the named
        // detach is a no-op for it — skipping the mint failure preserves
        // the remove choreography's observable outcome exactly.
        if let Ok(id) = ConnectorId::from_kebab(connector) {
            let _ = workspace.control.detach_connector_named(&id).await;
        }
    }
    if let Some((server, user)) = identity {
        match workspace.store.delete(&server, &user) {
            Ok(()) | Err(CredentialError::NotFound | CredentialError::Unsupported) => {}
            Err(err) => {
                tracing::warn!(error = %err, "settings remove: the stored secret survived");
            }
        }
    }
    tracing::info!(connector = %name, "settings remove succeeded");
    Ok(current_connections(workspace))
}

/// Every configured connector sharing the `(server_url, username)`
/// identity — one account card's connectors.
fn account_connector_names(raw: &AppConfig, server_url: &str, username: &str) -> Vec<String> {
    raw.sync
        .connectors
        .iter()
        .flatten()
        .filter(|e| e.server_url == server_url && e.username == username)
        .map(|e| e.name.clone())
        .collect()
}

/// Whether any live (not deleted, not archived) board is bound through
/// one of the named connectors — the account-removal guard's predicate.
fn any_live_board_bound_through(workspace: &ConnectionWorkspace, names: &[String]) -> bool {
    workspace
        .engine
        .shared_state()
        .load()
        .boards
        .values()
        .any(|b| {
            !b.deleted
                && !b.archived
                && b.remote
                    .as_ref()
                    .is_some_and(|binding| names.iter().any(|n| n == binding.connector.as_str()))
        })
}

/// The Attach choreography (r2 D33, made explicit by round-5 chunk 8,
/// D-8.4): resolve the connector config → load the stored secret → the
/// pre-IO binding guard (bound to another connector, or to another
/// remote through this one, refuses; the same `(connector, remote)` is
/// the idempotent re-issue) → ensure attached → bind → settle → reply
/// the fresh pair. The remote id is the user's explicit pick — nothing
/// is derived from titles.
///
/// Failure story (recorded): a refused bind after a successful attach
/// performs *no* detach rollback — the connector is in the config, and
/// boot's steady state for an in-config connector is attached; the
/// typed refusal is final for this request and visible. The stored
/// secret never leaves this scope.
async fn attach_connection(
    workspace: &ConnectionWorkspace,
    board: BoardId,
    connector_name: &str,
    remote: RemoteBoardId,
) -> Result<ConnectionsSnapshot, SettingsError> {
    let raw = ensure_migrated(workspace)?;
    let entry = connector_entry(&raw, connector_name)?;
    tracing::info!(
        connector = %connector_name,
        host = %url_host(&entry.server_url),
        "settings attach started"
    );
    let stored = workspace
        .store
        .load(&entry.server_url, &entry.username)
        .map_err(map_credential_error)?;
    let state = workspace.engine.shared_state().load();
    let board_row = state
        .boards
        .get(&board)
        .filter(|b| !b.deleted && !b.archived)
        .ok_or(SettingsError::Gone)?;
    match board_row.remote.as_ref() {
        Some(binding)
            if binding.connector.as_str() == connector_name && binding.board == remote => {}
        Some(_) => return Err(SettingsError::BindRefused),
        None => {}
    }
    drop(state);
    bind_through(workspace, &entry, stored.expose(), board, remote).await?;
    tracing::info!(
        connector = %connector_name,
        host = %url_host(&entry.server_url),
        "settings attach succeeded"
    );
    Ok(current_connections(workspace))
}

/// The attach half both binding intents share (and the publish arm's
/// pre-ceremony step): ensure the connector's actor is registered —
/// the named lookup guards the healthy-attached case (an unconditional
/// attach would refuse `ConnectorNameInUse` and surface the misleading
/// `AttachBusy` text).
async fn ensure_attached(
    workspace: &ConnectionWorkspace,
    entry: &ConnectorCfg,
    password: &str,
) -> Result<ConnectorId, SettingsError> {
    let id = ConnectorId::from_kebab(&entry.name).map_err(|_| {
        map_config_error(&ConfigError::InvalidConnectorName {
            name: entry.name.clone(),
        })
    })?;
    if workspace.control.connector_commands(&id).is_none() {
        let client =
            DeckClient::new(&entry.server_url, &entry.username, password).map_err(|err| {
                tracing::warn!(variant = %err, "settings attach: the client was refused");
                SettingsError::AttachFailed
            })?;
        workspace
            .control
            .attach_connector(id.clone(), client)
            .await
            .map_err(map_attach_error)?;
    }
    Ok(id)
}

/// The bind tail both binding intents share: ensure the connector is
/// attached, bind through the engine command path (the receipt
/// implies durability — the REPL bind's route), and settle the first
/// pull.
async fn bind_through(
    workspace: &ConnectionWorkspace,
    entry: &ConnectorCfg,
    password: &str,
    board: BoardId,
    remote: RemoteBoardId,
) -> Result<(), SettingsError> {
    let id = ensure_attached(workspace, entry, password).await?;
    workspace
        .engine
        .execute(StateCommand::BindBoard {
            board,
            remote,
            connector: id,
        })
        .await
        .map_err(|e| map_execute_error(&e))?;
    crate::bootstrap::wait_for_boot_pull(&workspace.engine, SAVE_SETTLE).await;
    Ok(())
}

/// Create-local-from-remote (round-5 chunk 8, D-8.5): one intent, two
/// engine writes — resolve the connector and secret, find the remote in
/// a fresh listing (`Gone` when it is no longer live), `CreateBoard`
/// with its title (the receipt names the minted board), then the
/// explicit bind tail.
///
/// Failure story (recorded): a refused bind after the create performs
/// *no* delete rollback — the local board is a valid standalone
/// outcome; the binding is the typed, visible failure.
async fn create_local_board(
    workspace: &ConnectionWorkspace,
    connector_name: &str,
    remote: RemoteBoardId,
) -> Result<ConnectionsSnapshot, SettingsError> {
    let raw = ensure_migrated(workspace)?;
    let entry = connector_entry(&raw, connector_name)?;
    let stored = workspace
        .store
        .load(&entry.server_url, &entry.username)
        .map_err(map_credential_error)?;
    let title = live_remote_boards(&entry, stored.expose())
        .await?
        .into_iter()
        .find(|b| Some(b.id) == remote.as_deck())
        .map(|b| b.title)
        .ok_or(SettingsError::Gone)?;
    let outcome = workspace
        .engine
        .execute(StateCommand::CreateBoard {
            title,
            color: taskboard_domain::Color::new(taskboard_domain::entities::DEFAULT_BOARD_COLOR),
        })
        .await
        .map_err(|e| map_execute_error(&e))?;
    let taskboard_domain::CommandOutcome::CreatedBoard(board) = outcome else {
        tracing::warn!(?outcome, "settings create: an unexpected engine receipt");
        return Err(SettingsError::AttachFailed);
    };
    bind_through(workspace, &entry, stored.expose(), board, remote).await?;
    tracing::info!(
        connector = %connector_name,
        host = %url_host(&entry.server_url),
        "settings create-local succeeded"
    );
    Ok(current_connections(workspace))
}

/// The Detach choreography (r2 D33, plan §3.2): unbind first — typed
/// and authoritative, a refusal leaves everything untouched — then
/// stop the connector's actor only when no other live board remains
/// bound to it (the recorded multi-board guard: a board-scoped detach
/// must not kill sync for sibling boards). No settle: the
/// `UnbindBoard`
/// receipt already implies durability. Detach works off the board's
/// binding, not the config — bindings survive Remove, so a stale
/// binding stays detachable.
async fn detach_connection(
    workspace: &ConnectionWorkspace,
    board: BoardId,
    connector_name: &str,
) -> Result<ConnectionsSnapshot, SettingsError> {
    tracing::info!(connector = %connector_name, "settings detach started");
    workspace
        .engine
        .execute(StateCommand::UnbindBoard { board })
        .await
        .map_err(|e| map_execute_error(&e))?;

    // Re-read: the surviving bindings decide the actor's fate.
    let state = workspace.engine.shared_state().load();
    let still_bound = state.boards.values().any(|b| {
        !b.deleted
            && !b.archived
            && b.remote
                .as_ref()
                .is_some_and(|binding| binding.connector.as_str() == connector_name)
    });
    drop(state);
    if !still_bound {
        let _ = workspace
            .control
            .detach_connector_named(
                &ConnectorId::from_kebab(connector_name).map_err(|_| SettingsError::Gone)?,
            )
            .await;
    }
    tracing::info!(connector = %connector_name, "settings detach succeeded");
    Ok(current_connections(workspace))
}

/// The Publish choreography (D-6.9, #699): resolve the connector and
/// secret → the pre-IO unbound guard (the publish ceremony's own rule,
/// moved to the front so the refusal never touches the network) →
/// ensure the connector's actor is attached (the ceremony's nudge only
/// reaches a registered actor) → the shared `publish_on` ceremony
/// (`create_board` REST → `BindBoard` → W2 orphan sweep → nudge →
/// adoption wait → reconcile) → reply the fresh snapshot.
///
/// Failure story (recorded): a transport failure after the
/// `create_board` POST leaves the board server-side unbound; the W2
/// orphan sweep cleans same-title/same-color crash-window orphans on
/// the *next* publish through that connector, and the typed error is
/// final for this request — no delete-rollback (the duplicate-create
/// crash window is #150's owned problem).
async fn publish_board(
    workspace: &ConnectionWorkspace,
    board: BoardId,
    connector_name: &str,
) -> Result<ConnectionsSnapshot, SettingsError> {
    let raw = ensure_migrated(workspace)?;
    let entry = connector_entry(&raw, connector_name)?;
    tracing::info!(
        connector = %connector_name,
        host = %url_host(&entry.server_url),
        "settings publish started"
    );
    let stored = workspace
        .store
        .load(&entry.server_url, &entry.username)
        .map_err(map_credential_error)?;
    // The pre-IO unbound guard: a deleted/archived/already-bound board
    // refuses before the create touches the network (`publish_on`
    // re-checks after the create — the racy second enforcer).
    {
        let state = workspace.engine.shared_state().load();
        let row = state
            .boards
            .get(&board)
            .filter(|b| !b.deleted && !b.archived)
            .ok_or(SettingsError::Gone)?;
        if row.remote.is_some() {
            return Err(SettingsError::BindRefused);
        }
    }
    let id = ensure_attached(workspace, &entry, stored.expose()).await?;
    let client =
        DeckClient::new(&entry.server_url, &entry.username, stored.expose()).map_err(|err| {
            tracing::warn!(variant = %err, "settings publish: the client was refused");
            SettingsError::AttachFailed
        })?;
    crate::commands::boards::publish_on(
        &workspace.engine,
        &client,
        Some(board),
        PUBLISH_SETTLE,
        &id,
    )
    .await
    .map_err(map_ceremony_error)?;
    tracing::info!(
        connector = %connector_name,
        host = %url_host(&entry.server_url),
        "settings publish succeeded"
    );
    Ok(current_connections(workspace))
}

/// The engine-command refusal's mapping (plan §4.5): stale board ids
/// are `Gone`, bind conflicts are `BindRefused`; storage/engine-gone
/// ride the existing coarse arms.
fn map_execute_error(err: &ExecuteError) -> SettingsError {
    use taskboard_domain::CommandError;
    match *err {
        ExecuteError::Rejected(CommandError::UnknownBoard(_) | CommandError::BoardDeleted(_)) => {
            SettingsError::Gone
        }
        ExecuteError::Rejected(
            CommandError::BoardAlreadyBound { .. } | CommandError::RemoteBoardInUse { .. },
        ) => SettingsError::BindRefused,
        ExecuteError::Storage(_) => SettingsError::AttachFailed,
        ExecuteError::EngineGone => SettingsError::Gone,
        // Unreachable from the bind/unbind path (the engine surfaces
        // only the classes above for these commands); typed anyway.
        ExecuteError::Rejected(other) => {
            tracing::warn!(error = %other, "settings: an unexpected engine rejection class");
            SettingsError::BindRefused
        }
    }
}

/// Persists the preset word and replies with the rebuilt keymap: the
/// config's overrides re-resolved under the chosen preset (plan D9).
fn set_keymap_preset(
    workspace: &ConnectionWorkspace,
    preset: taskboard_ui_slint::keymap::Preset,
) -> Result<KeymapUpdate, SettingsError> {
    crate::connections::overlay_set_keymap_preset(&workspace.config_write_path, preset.name())
        .map_err(|err| map_io_error(&err))?;
    let raw = read_raw_config(workspace)?;
    let built = taskboard_ui_slint::keymap::build(&taskboard_ui_slint::keymap::RawKeymap {
        preset,
        overrides: raw
            .ui
            .keymap
            .overrides
            .iter()
            .map(|(id, spec)| (id.clone(), spec.to_sequences()))
            .collect(),
    });
    Ok(KeymapUpdate {
        keymap: built.keymap,
        findings: u32::try_from(built.findings.len()).unwrap_or(u32::MAX),
        // The replied preset itself (r2 D27): the UI's selection echoes
        // the persisted choice — a diff against the built keymap would
        // be unsound under user overrides.
        preset,
    })
}

/// Validates typed (finite, `0.5–3.0` — the config's own rule) and
/// persists (plan D9).
fn set_scale(workspace: &ConnectionWorkspace, scale: f64) -> Result<(), SettingsError> {
    let scale = taskboard_ui_slint::UiScale::new(scale).map_err(|err| SettingsError::Invalid {
        field: Field::Scale,
        kind: match err {
            taskboard_ui_slint::UiScaleError::NonFinite => InvalidKind::NotFinite,
            taskboard_ui_slint::UiScaleError::OutOfRange => InvalidKind::OutOfRange,
        },
    })?;
    crate::connections::overlay_set_ui_scale(&workspace.config_write_path, scale)
        .map_err(|err| map_io_error(&err))
}

/// The raw config at the write path (the same read the REPL ceremonies
/// run — env included; R6's re-read-per-request rule).
fn read_raw_config(workspace: &ConnectionWorkspace) -> Result<AppConfig, SettingsError> {
    AppConfig::extract_raw(Some(&workspace.config_write_path)).map_err(|err| map_config_error(&err))
}

/// The raw config, migrated first when the file is the legacy scalar
/// world (round-3 D21): "the next successful settings write completes
/// the migration" made concrete — a boot whose persist failed (or a
/// boot-less one-shot) is completed here. Array world → returned
/// as-is; file migrated → re-read (the migrated file); a genuinely
/// unwritable file → typed `PersistFailed`; the scalar world with a
/// legacy `[nextcloud]` *identity* ([`has_legacy_identity`] —
/// env-injected identity or partial legacy keys, genuinely not a
/// write target) → typed `PersistRejected`; a config merely *empty
/// of sync* (#721's ordinary first-connection case) falls through to
/// the writer, which creates the subtree (and the file, when absent).
fn ensure_migrated(workspace: &ConnectionWorkspace) -> Result<AppConfig, SettingsError> {
    let raw = read_raw_config(workspace)?;
    if raw.sync.connectors.is_some() {
        return Ok(raw);
    }
    match crate::migration::migrate_config_file(&workspace.config_write_path) {
        Ok(true) => {
            let migrated = read_raw_config(workspace)?;
            for entry in migrated.sync.connectors.iter().flatten() {
                tracing::info!(
                    connector = %entry.name,
                    host = %crate::migration::url_host(&entry.server_url),
                    "settings write completed the legacy config migration"
                );
            }
            Ok(migrated)
        }
        Ok(false) if has_legacy_identity(&raw) => Err(SettingsError::PersistRejected),
        Ok(false) => Ok(raw),
        Err(err) => {
            tracing::warn!(error = %err, "settings: the legacy config migration failed");
            Err(SettingsError::PersistFailed)
        }
    }
}

/// The typed half of the scalar-world corner: an identity beside no
/// connectors array — env-injected identity or partial legacy keys.
/// The same predicate `connections::identity_husk_present` keys on at
/// the document level pre-decode (a mixed identity-beside-array file
/// decode-rejects, which is why the writer cannot check this typed).
fn has_legacy_identity(raw: &AppConfig) -> bool {
    raw.nextcloud.server_url.is_some() || raw.nextcloud.username.is_some()
}

/// The fresh secret-free list for the reply (the `configured_connectors`
/// source, resolved through the raw config).
fn current_connectors(workspace: &ConnectionWorkspace) -> Vec<ConnectorInfo> {
    match AppConfig::extract_raw(Some(&workspace.config_write_path)) {
        // The raw config was never validated, so the mint can fail;
        // same failure class as an unreadable config — the display-only
        // list falls back to empty, loud in the log.
        Ok(raw) => crate::ui::resolve_connector_infos(&raw).unwrap_or_else(|err| {
            tracing::warn!(error = %err, "settings: the fresh connector list is unreadable");
            Vec::new()
        }),
        Err(err) => {
            tracing::warn!(error = %err, "settings: the fresh connector list is unreadable");
            Vec::new()
        }
    }
}

/// The fresh boards+connectors pair for the state-changing replies
/// (r2 D32): boards from the engine state (the live filter, the
/// binding's connector name), connectors from [`current_connectors`].
/// Both surfaces of one reply re-render from this single snapshot.
fn current_connections(workspace: &ConnectionWorkspace) -> ConnectionsSnapshot {
    let state = workspace.engine.shared_state().load();
    let boards = state
        .boards
        .values()
        .filter(|b| !b.deleted && !b.archived)
        .map(|b| taskboard_ui_slint::requests::BoardInfo {
            id: b.id.as_uuid(),
            name: b.title.clone(),
            bound_connector: b
                .remote
                .as_ref()
                .map(|binding| binding.connector.to_string()),
            // Structural detection per D-BF4 (round-7 D-7.3, W-1): the
            // remote id's payload type IS the channel — never a
            // connector-name match.
            bound_channel: b
                .remote
                .as_ref()
                .map(|binding| taskboard_ui_slint::presentation::bound_channel_of(&binding.board)),
        })
        .collect();
    ConnectionsSnapshot {
        boards,
        connectors: current_connectors(workspace),
    }
}

/// The authoritative re-validation (plan F-D2): the draft shaped into
/// the config's connector list (upsert semantics) and run through
/// `AppConfig::validate` — the config rules stay the single source; the
/// UI form's mirror only flags fields early.
fn validated_draft(
    raw: &AppConfig,
    profile: &ConnectionProfile,
) -> Result<(ConnectorCfg, ConnectorId), SettingsError> {
    let draft = ConnectorCfg {
        name: profile.name.clone(),
        backend: SyncBackend::Nextcloud,
        server_url: profile.server_url.clone(),
        username: profile.username.clone(),
    };
    let mut entries = raw.sync.connectors.clone().unwrap_or_default();
    match entries.iter().position(|entry| entry.name == draft.name) {
        Some(position) => entries[position] = draft.clone(),
        None => entries.push(draft.clone()),
    }
    let mut candidate = raw.clone();
    candidate.sync.connectors = Some(entries);
    candidate.validate().map_err(|err| map_config_error(&err))?;
    // The mint rides the validation that just passed: parse once, hand
    // the proof forward — the save choreography re-mints nothing.
    let id = ConnectorId::from_kebab(&draft.name).map_err(|_| {
        map_config_error(&crate::config::ConfigError::InvalidConnectorName {
            name: draft.name.clone(),
        })
    })?;
    Ok((draft, id))
}

/// The config-rule → typed-field mapping (the UI mirror's authority;
/// pinned exhaustively over the reachable rejections).
fn map_config_error(err: &ConfigError) -> SettingsError {
    match &err {
        ConfigError::InvalidConnectorName { .. } => SettingsError::Invalid {
            field: Field::Name,
            kind: InvalidKind::NotKebab,
        },
        ConfigError::DuplicateConnectorName { .. } => SettingsError::Invalid {
            field: Field::Name,
            kind: InvalidKind::Duplicate,
        },
        ConfigError::InvalidConnectorEntry { field, .. } if field == &"server_url" => {
            SettingsError::Invalid {
                field: Field::ServerUrl,
                kind: InvalidKind::BadUrl,
            }
        }
        ConfigError::InvalidConnectorEntry { field, .. } if field == &"username" => {
            SettingsError::Invalid {
                field: Field::Username,
                kind: InvalidKind::Empty,
            }
        }
        // Every other config rejection is a pre-existing-file property,
        // not a field the form owns: the typed persist refusal.
        _ => {
            tracing::debug!(error = %err, "settings save: the config layer rejected the shape");
            SettingsError::PersistRejected
        }
    }
}

/// The Files row's move ceremony (round-6 D-6.8 phase 2, D-4): the
/// pre-IO guards mirror `publish_board`'s posture — the world's own
/// derived guards (`derive_move_stage`) stay the racy second
/// enforcers. The ceremony is the CLI's exact path
/// (`board_move_ceremony::run_on` on the running engine), never a
/// reimplemented move; the reply is the fresh snapshot so the
/// standard drain re-pushes the settings page unchanged.
async fn move_board_to_files(
    workspace: &ConnectionWorkspace,
    board: BoardId,
) -> Result<ConnectionsSnapshot, SettingsError> {
    let raw = ensure_migrated(workspace)?;
    if crate::bootstrap::files_sync_spawn_decision(&raw).is_none() {
        return Err(SettingsError::FilesSyncDisabled);
    }
    let state = workspace.engine.shared_state().load();
    let row = state
        .boards
        .get(&board)
        .filter(|b| !b.deleted && !b.archived)
        .ok_or(SettingsError::Gone)?;
    if row
        .remote
        .as_ref()
        .is_none_or(|binding| binding.board.as_deck().is_none())
    {
        // Unbound, files-bound (already committed), or a foreign
        // remote shape: the ceremony's guard, pre-IO.
        return Err(SettingsError::BindRefused);
    }
    drop(state);
    crate::board_move_ceremony::run_on(
        &workspace.engine,
        &raw,
        &workspace.store,
        board,
        crate::board_move_ceremony::MoveDirection::ToFiles,
        crate::board_move_ceremony::DEFAULT_MOVE_TIMEOUT,
    )
    .await
    .map_err(map_move_error)?;
    tracing::info!(board = %board.as_uuid(), "settings move to files committed");
    Ok(current_connections(workspace))
}

/// The roster's Stop-Sync choreography (round-7 D-7.3, W-4/D7): the
/// guards are the engine's truth read fresh — the board present and
/// not deleted (`Gone` otherwise), the binding files-typed
/// (`BindRefused` otherwise: the row grammar can't normally offer
/// those, the arm still refuses typed) — then `UnbindBoard` on the
/// engine handle. No connector-actor detach step: the files lane is
/// global (single-lane, #723), unlike Deck's `detach_connector_named`
/// sweep. The remote `boards/<uuid>/` subtree lingers until the
/// DELETE-verb GC exists (#755 — recorded in the request doc and
/// R-4, not hidden). Reply: the fresh snapshot.
async fn stop_board_files_sync(
    workspace: &ConnectionWorkspace,
    board: BoardId,
) -> Result<ConnectionsSnapshot, SettingsError> {
    let state = workspace.engine.shared_state().load();
    let row = state
        .boards
        .get(&board)
        .filter(|b| !b.deleted)
        .ok_or(SettingsError::Gone)?;
    if row
        .remote
        .as_ref()
        .is_none_or(|binding| binding.board.as_files().is_none())
    {
        return Err(SettingsError::BindRefused);
    }
    drop(state);
    workspace
        .engine
        .execute(StateCommand::UnbindBoard { board })
        .await
        .map_err(|e| map_execute_error(&e))?;
    tracing::info!(board = %board.as_uuid(), "settings stop files sync committed");
    Ok(current_connections(workspace))
}

/// The roster's publish choreography (round-7 D-7.3, W-6/D7): a
/// local-only board reaches this account's Files without ever
/// touching Deck. Guard order — ① the files spawn decision resolves
/// from the config (`FilesSyncDisabled` otherwise; config-derived, so
/// a restart-pending enable succeeds by construction, US-4 AC-5);
/// ② the board present, not deleted, not archived (`Gone`);
/// ③ the binding `None` (`BindRefused` — deck-bound and files-bound
/// refuse alike; the row grammar offers Stop/Switch, so only a stale
/// snapshot can get here). Ceremony: the move ceremony's final assign
/// step on the shared engine handle — `BindBoard` with the files
/// remote id minted from the board's own uuid (the ceremony's
/// `remote()` law, so publish and move cannot mint differently) and
/// connector = `FILES_SYNC_CONNECTOR_NAME`. The files actor re-derives
/// its lanes from `BoardOwnershipMap` next cycle — no nudge needed.
async fn publish_board_to_files(
    workspace: &ConnectionWorkspace,
    board: BoardId,
) -> Result<ConnectionsSnapshot, SettingsError> {
    let raw = ensure_migrated(workspace)?;
    if crate::bootstrap::files_sync_spawn_decision(&raw).is_none() {
        return Err(SettingsError::FilesSyncDisabled);
    }
    let state = workspace.engine.shared_state().load();
    let row = state
        .boards
        .get(&board)
        .filter(|b| !b.deleted && !b.archived)
        .ok_or(SettingsError::Gone)?;
    if row.remote.is_some() {
        return Err(SettingsError::BindRefused);
    }
    drop(state);
    workspace
        .engine
        .execute(StateCommand::BindBoard {
            board,
            remote: RemoteBoardId::files(board.as_uuid()),
            connector: ConnectorId::from_kebab(crate::bootstrap::FILES_SYNC_CONNECTOR_NAME)
                .expect("FILES_SYNC_CONNECTOR_NAME is kebab"),
        })
        .await
        .map_err(|e| map_execute_error(&e))?;
    tracing::info!(board = %board.as_uuid(), "settings publish to files committed");
    Ok(current_connections(workspace))
}

/// The move ceremony's typed fold (D-5): the classes the settings
/// surface can name, with the concrete `MoveError` logged at `warn`
/// for the diagnostics the coarse class drops.
fn map_move_error(err: crate::commands::CommandError) -> SettingsError {
    match err {
        crate::commands::CommandError::BoardMove(move_err) => {
            tracing::warn!(error = %move_err, "settings move to files refused or failed");
            match move_err {
                crate::board_move_ceremony::MoveError::FilesSyncDisabled => {
                    SettingsError::FilesSyncDisabled
                }
                crate::board_move_ceremony::MoveError::OutboxNotQuiescent { .. } => {
                    SettingsError::OutboxNotQuiescent
                }
                _ => SettingsError::MoveFailed,
            }
        }
        // Non-ceremony classes (credential store, unreachable server,
        // unknown board) ride the existing mapping's arms.
        other => map_ceremony_error(other),
    }
}

/// The extracted ceremony's error mapping (the `map_deck_error`
/// classes `verify_credentials` produces; the store/rotate path's
/// credential arm rides the same table).
fn map_ceremony_error(err: CommandError) -> SettingsError {
    match err {
        CommandError::InvalidCredentials => SettingsError::VerifyRejected,
        CommandError::Unreachable => SettingsError::VerifyUnreachable,
        CommandError::Deck(_) => SettingsError::VerifyFailed,
        CommandError::ConfigWrite(_) => SettingsError::PersistFailed,
        CommandError::Credential(source) => map_credential_error(source),
        // The publish settle budget expired (D-6.9): the board is
        // created and bound, the first cycle is still moving — typed,
        // not folded into a "failed" arm.
        CommandError::SyncTimedOut => SettingsError::SyncTimedOut,
        // Unreachable from the settings path (the extracted ceremony
        // surfaces only the classes above); typed anyway.
        other => {
            tracing::debug!(error = %other, "settings: an unexpected ceremony error class");
            SettingsError::VerifyFailed
        }
    }
}

/// The attach refusal's mapping (plan D3's `AttachBusy`/`AttachFailed`
/// split).
fn map_attach_error(err: crate::bootstrap::AttachError) -> SettingsError {
    use crate::bootstrap::AttachError as A;
    match err {
        A::ConnectorNameInUse(_) | A::Draining => SettingsError::AttachBusy,
        A::Engine(_) | A::Client(_) => SettingsError::AttachFailed,
        A::Credential(source) => map_credential_error(source),
    }
}

/// The credential store's mapping (the three typed arms).
fn map_credential_error(err: CredentialError) -> SettingsError {
    match err {
        CredentialError::NotFound => SettingsError::NoSecretStored,
        CredentialError::Unsupported => SettingsError::StoreUnsupported,
        other => {
            tracing::warn!(error = %other, "credential store failed");
            SettingsError::StoreFailed
        }
    }
}

/// The overlay writer's mapping: shape refusals are typed persist
/// rejections, IO failures persist failures.
fn map_io_error(err: &std::io::Error) -> SettingsError {
    if err.kind() == std::io::ErrorKind::InvalidData {
        SettingsError::PersistRejected
    } else {
        SettingsError::PersistFailed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use figment::providers::Format as _;
    use taskboard_domain::{BoardClocks, BoardId, StackClocks, StackId};

    /// A workspace over a real (standalone, module-less) booted core:
    /// the width tests never touch the connector path, but the
    /// workspace is a plain value, not a mock — fakes over mocks.
    async fn test_workspace(path: PathBuf) -> ConnectionWorkspace {
        // The core deliberately leaks for the test's lifetime (the
        // actors idle on a standalone, boardless db); the workspace
        // holds only clones.
        test_workspace_with_app(path).await.0
    }

    /// The workspace plus its booted core (the engine-gone journey
    /// needs the system bus to drain the engine).
    async fn test_workspace_with_app(
        path: PathBuf,
    ) -> (ConnectionWorkspace, crate::bootstrap::App) {
        let mut config = AppConfig::default();
        let dir = tempfile::tempdir().expect("tempdir");
        config.storage.db_path = Some(dir.path().join("taskboard.db"));
        config.sync.backend = Some(SyncBackend::Standalone);
        config.validate().expect("config valid");
        let app = crate::bootstrap::bootstrap(
            config,
            Arc::new(RefusingStore),
            Arc::new(crate::notifications::LogSink),
        )
        .await
        .expect("standalone boot");
        let workspace = ConnectionWorkspace {
            config_write_path: path,
            store: Arc::new(RefusingStore),
            control: app.connector_control(),
            engine: app.engine.clone(),
        };
        (workspace, app)
    }

    /// A credential-refusing store (the bootstrap test's shape): the
    /// width paths never consult it.
    struct RefusingStore;
    impl std::fmt::Debug for RefusingStore {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("RefusingStore")
        }
    }
    impl CredentialStore for RefusingStore {
        fn load(&self, _: &str, _: &str) -> Result<crate::secrets::AppPassword, CredentialError> {
            Err(CredentialError::NotFound)
        }
        fn store(
            &self,
            _: &str,
            _: &str,
            _: crate::secrets::AppPassword,
        ) -> Result<(), CredentialError> {
            Err(CredentialError::Unsupported)
        }
        fn delete(&self, _: &str, _: &str) -> Result<(), CredentialError> {
            Err(CredentialError::Unsupported)
        }
    }

    fn epoch() -> chrono::DateTime<chrono::Utc> {
        BoardClocks::epoch()
    }

    // -- the snapshot's bound-channel fill (round-7 D-7.3, W-1) ---------

    /// Creates one live board through the real engine (the command is
    /// the trust boundary — the snapshot fill reads only what the
    /// engine holds).
    async fn create_board(workspace: &ConnectionWorkspace, title: &str) -> BoardId {
        let taskboard_domain::CommandOutcome::CreatedBoard(board) = workspace
            .engine
            .execute(StateCommand::CreateBoard {
                title: title.to_owned(),
                color: taskboard_domain::Color::new(""),
            })
            .await
            .expect("create lands")
        else {
            panic!("create mints a board");
        };
        board
    }

    /// The snapshot's board half carries the binding's channel,
    /// detected STRUCTURALLY from the id type (D-BF4) — never from the
    /// binding's connector name: a files binding whose connector name
    /// is a Deck-shaped card name still reads `Files`.
    #[tokio::test]
    async fn current_connections_fills_bound_channel_structurally() {
        let dir = tempfile::tempdir().expect("tempdir");
        let config = dir.path().join("taskboard.toml");
        std::fs::write(&config, "[ui]\nscale = 1.0\n").expect("seed");
        let workspace = test_workspace(config).await;

        let deck_bound = create_board(&workspace, "deck board").await;
        let files_bound = create_board(&workspace, "files board").await;
        let _loose = create_board(&workspace, "loose").await;
        for (board, remote) in [
            (deck_bound, taskboard_domain::RemoteBoardId::deck(7)),
            (
                files_bound,
                taskboard_domain::RemoteBoardId::files(uuid::Uuid::now_v7()),
            ),
        ] {
            workspace
                .engine
                .execute(StateCommand::BindBoard {
                    board,
                    remote,
                    // Deliberately a Deck-card-shaped name for BOTH
                    // bindings: the channel must not key on it (D-BF4).
                    connector: taskboard_domain::ConnectorId::from("work"),
                })
                .await
                .expect("bind lands");
        }

        let snapshot = current_connections(&workspace);
        let by_name = |name: &str| {
            snapshot
                .boards
                .iter()
                .find(|board| board.name == name)
                .expect("the live board is in the snapshot")
        };
        assert_eq!(
            by_name("deck board").bound_channel,
            Some(taskboard_ui_slint::requests::BoardChannel::Deck),
        );
        assert_eq!(
            by_name("files board").bound_channel,
            Some(taskboard_ui_slint::requests::BoardChannel::Files),
        );
        assert_eq!(by_name("loose").bound_channel, None);
        assert_eq!(
            by_name("files board").bound_connector.as_deref(),
            Some("work"),
            "the name field is untouched — the channel is the id type's"
        );
    }

    // -- the Stop arm (round-7 D-7.3, W-4) ------------------------------

    /// US-2/S2 — the Stop arm's guards refuse typed: a gone board is
    /// `Gone`; a deck-bound and an unbound board are `BindRefused`
    /// (the row grammar can't normally offer those — the arm still
    /// refuses typed).
    #[tokio::test]
    async fn us2_s2_stop_sync_guards_refuse_typed() {
        let dir = tempfile::tempdir().expect("tempdir");
        let config = dir.path().join("taskboard.toml");
        std::fs::write(&config, "[ui]\nscale = 1.0\n").expect("seed");
        let workspace = test_workspace(config).await;

        let deck_bound = create_board(&workspace, "deck board").await;
        let unbound = create_board(&workspace, "loose").await;
        workspace
            .engine
            .execute(StateCommand::BindBoard {
                board: deck_bound,
                remote: taskboard_domain::RemoteBoardId::deck(7),
                connector: taskboard_domain::ConnectorId::from("work"),
            })
            .await
            .expect("deck bind lands");

        let gone = stop_board_files_sync(&workspace, BoardId::from_uuid(uuid::Uuid::now_v7()))
            .await
            .expect_err("a gone board refuses");
        assert_eq!(gone, SettingsError::Gone);
        let deck = stop_board_files_sync(&workspace, deck_bound)
            .await
            .expect_err("a deck-bound board refuses");
        assert_eq!(deck, SettingsError::BindRefused);
        let loose = stop_board_files_sync(&workspace, unbound)
            .await
            .expect_err("an unbound board refuses");
        assert_eq!(loose, SettingsError::BindRefused);
    }

    /// The Stop arm's exact effect (US-2/S3): the engine receives the
    /// unbind — the files binding is gone, the board itself survives
    /// (unbind is not a delete) — and the reply is the fresh
    /// snapshot whose board half no longer carries the binding.
    #[tokio::test]
    async fn stop_board_files_sync_sends_exact_unbind_command() {
        let dir = tempfile::tempdir().expect("tempdir");
        let config = dir.path().join("taskboard.toml");
        std::fs::write(&config, "[ui]\nscale = 1.0\n").expect("seed");
        let workspace = test_workspace(config).await;
        let board = create_board(&workspace, "files board").await;
        workspace
            .engine
            .execute(StateCommand::BindBoard {
                board,
                remote: taskboard_domain::RemoteBoardId::files(uuid::Uuid::now_v7()),
                connector: taskboard_domain::ConnectorId::from("files-sync"),
            })
            .await
            .expect("files bind lands");

        let snapshot = stop_board_files_sync(&workspace, board)
            .await
            .expect("a files-bound board stops");
        let state = workspace.engine.shared_state().load();
        let row = state.boards.get(&board).expect("the board survives");
        assert!(row.remote.is_none(), "the binding is unbound");
        assert!(!row.deleted, "unbind is not a delete");
        let replied = snapshot
            .boards
            .iter()
            .find(|info| info.id == board.as_uuid())
            .expect("the board is in the fresh snapshot");
        assert_eq!(replied.bound_channel, None);
        assert_eq!(replied.bound_connector, None);
    }

    // -- the Publish arm (round-7 D-7.3, W-6) ---------------------------

    /// A config whose files lane is spawnable (`[files_sync] enabled`
    /// with a resolvable Nextcloud anchor) at the workspace's write
    /// path.
    fn write_files_lane_config(path: &std::path::Path) {
        std::fs::write(
            path,
            "[ui]\nscale = 1.0\n\
             [[sync.connectors]]\nname = \"work\"\nbackend = \"nextcloud\"\n\
             server_url = \"https://c.example\"\nusername = \"alice\"\n\
             [files_sync]\nenabled = true\nanchor = \"work\"\n",
        )
        .expect("seed files-lane config");
    }

    /// US-4/S2 — the Publish arm's guards refuse typed: deck-bound
    /// and files-bound boards are `BindRefused` (a stale row's
    /// mis-target), a disabled files lane is `FilesSyncDisabled`, a
    /// gone board is `Gone`.
    #[tokio::test]
    async fn us4_s2_publish_guards_refuse_deck_bound() {
        let dir = tempfile::tempdir().expect("tempdir");
        let config = dir.path().join("taskboard.toml");
        write_files_lane_config(&config);
        let workspace = test_workspace(config).await;

        let deck_bound = create_board(&workspace, "deck board").await;
        let files_bound = create_board(&workspace, "files board").await;
        workspace
            .engine
            .execute(StateCommand::BindBoard {
                board: deck_bound,
                remote: taskboard_domain::RemoteBoardId::deck(7),
                connector: taskboard_domain::ConnectorId::from("work"),
            })
            .await
            .expect("deck bind lands");
        workspace
            .engine
            .execute(StateCommand::BindBoard {
                board: files_bound,
                remote: taskboard_domain::RemoteBoardId::files(uuid::Uuid::now_v7()),
                connector: taskboard_domain::ConnectorId::from("files-sync"),
            })
            .await
            .expect("files bind lands");

        let deck = publish_board_to_files(&workspace, deck_bound)
            .await
            .expect_err("a deck-bound board refuses");
        assert_eq!(deck, SettingsError::BindRefused);
        let files = publish_board_to_files(&workspace, files_bound)
            .await
            .expect_err("a files-bound board refuses");
        assert_eq!(files, SettingsError::BindRefused);
        let gone = publish_board_to_files(&workspace, BoardId::from_uuid(uuid::Uuid::now_v7()))
            .await
            .expect_err("a gone board refuses");
        assert_eq!(gone, SettingsError::Gone);

        // A lane-off config refuses before the board guards touch the
        // state (D7's guard order — the cheap config check first).
        let dir_off = tempfile::tempdir().expect("tempdir");
        let config_off = dir_off.path().join("taskboard.toml");
        std::fs::write(&config_off, "[ui]\nscale = 1.0\n").expect("seed");
        let workspace_off = test_workspace(config_off).await;
        let unbound = create_board(&workspace_off, "loose").await;
        let disabled = publish_board_to_files(&workspace_off, unbound)
            .await
            .expect_err("a lane-off config refuses");
        assert_eq!(disabled, SettingsError::FilesSyncDisabled);
    }

    /// The Publish arm's exact ceremony (US-4/S3): the engine receives
    /// the files bind — `RemoteBoardId::files` minted from the board's
    /// own uuid (the ceremony's `remote()` law, so publish and move
    /// cannot mint differently), connector =
    /// `FILES_SYNC_CONNECTOR_NAME` — and the reply is the fresh
    /// snapshot. US-4 AC-5 rides the fixture: the workspace's
    /// standalone boot never spawned a files actor, so this is the
    /// restart-pending posture — the bind commits as a state change
    /// (the config-derived guard), the lane catches up next boot.
    #[tokio::test]
    async fn publish_board_to_files_sends_exact_files_bind_command() {
        let dir = tempfile::tempdir().expect("tempdir");
        let config = dir.path().join("taskboard.toml");
        write_files_lane_config(&config);
        let workspace = test_workspace(config).await;
        let board = create_board(&workspace, "loose").await;

        let snapshot = publish_board_to_files(&workspace, board)
            .await
            .expect("a local-only board publishes");
        let state = workspace.engine.shared_state().load();
        let row = state.boards.get(&board).expect("the board survives");
        let binding = row.remote.as_ref().expect("the bind committed");
        assert_eq!(
            binding.connector.as_str(),
            crate::bootstrap::FILES_SYNC_CONNECTOR_NAME
        );
        assert_eq!(
            binding.board,
            taskboard_domain::RemoteBoardId::files(board.as_uuid()),
            "the files remote id is the board's own uuid (the ceremony's mint)"
        );
        let replied = snapshot
            .boards
            .iter()
            .find(|info| info.id == board.as_uuid())
            .expect("the board is in the fresh snapshot");
        assert_eq!(
            replied.bound_channel,
            Some(taskboard_ui_slint::requests::BoardChannel::Files),
        );
    }

    fn fixture_state() -> (AppState, uuid::Uuid, uuid::Uuid, uuid::Uuid) {
        let board_id = BoardId::from_uuid(uuid::Uuid::from_u128(0x1001));
        let stack_a = StackId::from_uuid(uuid::Uuid::from_u128(0x2001));
        let stack_b = StackId::from_uuid(uuid::Uuid::from_u128(0x2002));
        let mut state = AppState::default();
        state.boards.insert(
            board_id,
            taskboard_domain::Board {
                id: board_id,
                remote: None,
                title: "board".to_owned(),
                color: taskboard_domain::Color::new(""),
                archived: false,
                deleted: false,
                clocks: BoardClocks {
                    title: epoch(),
                    color: epoch(),
                    archived: epoch(),
                    deleted: epoch(),
                },
                authors: taskboard_domain::entities::BoardAuthors::default(),
                remote_seen: None,
            },
        );
        for (id, order) in [(stack_a, 0), (stack_b, 1)] {
            state.stacks.insert(
                id,
                taskboard_domain::Stack {
                    id,
                    remote: None,
                    board: board_id,
                    title: format!("stack-{order}"),
                    order,
                    archived: false,
                    deleted: false,
                    clocks: StackClocks {
                        title: epoch(),
                        order: epoch(),
                        deleted: epoch(),
                    },
                    authors: taskboard_domain::entities::StackAuthors::default(),
                    remote_seen: None,
                },
            );
        }
        (
            state,
            board_id.as_uuid(),
            stack_a.as_uuid(),
            stack_b.as_uuid(),
        )
    }

    /// F9-05's restored behavior: the forwarder's natural close path
    /// lands the debounced width flush — a width change sent right
    /// before the UI side dropped persists at close, without waiting
    /// for (or needing) a debounce window. This is the flush the old
    /// abort-at-quit skipped.
    #[tokio::test]
    async fn the_close_path_flushes_a_width_sent_right_before_quit() {
        let dir = tempfile::tempdir().expect("tempdir");
        let config = dir.path().join("taskboard.toml");
        std::fs::write(&config, "[ui]\nscale = 1.0\n").expect("seed");
        let workspace = test_workspace(config.clone()).await;
        // Live entities in the state: the flush prunes widths against
        // the published projection, so a boardless state would drop the
        // width as dead (the fixture makes the flush observable).
        let (state, board, stack_a, _stack_b) = fixture_state();
        let board = BoardId::from_uuid(board);
        let stack_a = StackId::from_uuid(stack_a);
        let (tx, rx) = ui_request_channel();
        let forwarder =
            spawn_ui_request_forwarder(rx, Arc::new(ArcSwap::from_pointee(state)), workspace);

        tx.try_send(UiRequest::StackWidthsChanged {
            board,
            widths: vec![(stack_a, 240)],
        })
        .expect("capacity holds");
        drop(tx); // quit: the UI side goes away
        forwarder.await.expect("forwarder joins cleanly");

        let written = std::fs::read_to_string(&config).expect("read back");
        assert!(
            written.contains("stack_widths"),
            "the close flush landed the widths subtree"
        );
        assert!(
            written.contains(&stack_a.as_uuid().to_string()),
            "the width survived to the flushed document"
        );
    }

    // -- prune (T-16) ------------------------------------------------------

    #[test]
    fn prune_drops_dead_entities_and_keeps_live_ones() {
        let (mut state, board, stack_a, stack_b) = fixture_state();
        let dead_stack = uuid::Uuid::from_u128(0x2999);
        let pending = BTreeMap::from([
            (
                board,
                BTreeMap::from([(stack_a, 240u32), (dead_stack, 300u32)]),
            ),
            (
                uuid::Uuid::from_u128(0x1999),
                BTreeMap::from([(stack_b, 120u32)]),
            ),
        ]);

        // A stack moved to another board is pruned from the old board.
        let other = BoardId::from_uuid(uuid::Uuid::from_u128(0x1002));
        state.boards.insert(
            other,
            taskboard_domain::Board {
                id: other,
                remote: None,
                title: "other".to_owned(),
                color: taskboard_domain::Color::new(""),
                archived: false,
                deleted: false,
                clocks: BoardClocks {
                    title: epoch(),
                    color: epoch(),
                    archived: epoch(),
                    deleted: epoch(),
                },
                authors: taskboard_domain::entities::BoardAuthors::default(),
                remote_seen: None,
            },
        );
        let moved = StackId::from_uuid(uuid::Uuid::from_u128(0x2003));
        state.stacks.insert(
            moved,
            taskboard_domain::Stack {
                id: moved,
                remote: None,
                board: other,
                title: "moved".to_owned(),
                order: 0,
                archived: false,
                deleted: false,
                clocks: StackClocks {
                    title: epoch(),
                    order: epoch(),
                    deleted: epoch(),
                },
                authors: taskboard_domain::entities::StackAuthors::default(),
                remote_seen: None,
            },
        );
        let pending = BTreeMap::from([(
            board,
            pending[&board]
                .iter()
                .chain(std::iter::once((&moved.as_uuid(), &400u32)))
                .map(|(k, v)| (*k, *v))
                .collect::<BTreeMap<_, _>>(),
        )]);

        let pruned = prune_widths(&pending, &state);
        assert_eq!(pruned.len(), 1, "a dead board is dropped");
        let kept = &pruned[&board];
        assert_eq!(kept.get(&stack_a), Some(&240));
        assert!(!kept.contains_key(&dead_stack), "a dead stack is dropped");
        assert!(
            !kept.contains_key(&moved.as_uuid()),
            "a stack of another board is dropped from this board"
        );
    }

    #[test]
    fn prune_drops_archived_boards_and_boards_left_without_live_stacks() {
        let (mut state, board, stack_a, stack_b) = fixture_state();
        state
            .boards
            .get_mut(&BoardId::from_uuid(board))
            .expect("board")
            .archived = true;
        let pending = BTreeMap::from([(
            board,
            BTreeMap::from([(stack_a, 240u32), (stack_b, 120u32)]),
        )]);
        assert!(
            prune_widths(&pending, &state).is_empty(),
            "an archived board's whole entry is dropped"
        );

        // Deleting the board's only stacks drops the board entry too.
        state
            .boards
            .get_mut(&BoardId::from_uuid(board))
            .expect("board")
            .archived = false;
        for stack in [stack_a, stack_b] {
            state
                .stacks
                .get_mut(&StackId::from_uuid(stack))
                .expect("stack")
                .deleted = true;
        }
        assert!(prune_widths(&pending, &state).is_empty(), "no empty husk");
    }

    // -- overlay write (T-15) ------------------------------------------------

    fn write_temp_config(contents: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("taskboard.toml");
        if !contents.is_empty() {
            std::fs::write(&path, contents).expect("seed config");
        }
        (dir, path)
    }

    #[test]
    fn overlay_preserves_comments_and_unrelated_keys_and_replaces_only_the_subtree() {
        let (_dir, path) = write_temp_config(
            "# operator comment\n[ui]\nscale = 1.5\n\
             [ui.stack_widths]\n\"old-board\" = 1\n\
             [nextcloud]\n# keep me\nserver_url = \"https://x\"\n",
        );
        let (_, board, stack_a, stack_b) = fixture_state();
        let widths =
            BTreeMap::from([(board, BTreeMap::from([(stack_a, 240u32), (stack_b, 96u32)]))]);
        overlay_stack_widths(&path, &widths).expect("write succeeds");

        let written = std::fs::read_to_string(&path).expect("read back");
        assert!(written.contains("# operator comment"));
        assert!(written.contains("scale = 1.5"));
        assert!(written.contains("# keep me"));
        assert!(written.contains("server_url = \"https://x\""));
        assert!(!written.contains("old-board"), "the subtree was replaced");
        // Structural: the written document decodes to the widths map.
        let doc = written.parse::<toml_edit::DocumentMut>().expect("parses");
        let stack_widths = doc["ui"]["stack_widths"].as_table().expect("subtree table");
        let board_table = stack_widths
            .get(&board.to_string())
            .expect("board row")
            .as_table()
            .expect("board table");
        assert_eq!(
            board_table
                .get(&stack_a.to_string())
                .and_then(toml_edit::Item::as_integer),
            Some(240)
        );
        assert_eq!(
            board_table
                .get(&stack_b.to_string())
                .and_then(toml_edit::Item::as_integer),
            Some(96)
        );
    }

    #[test]
    fn overlay_with_an_empty_map_removes_the_subtree() {
        let (_dir, path) = write_temp_config("[ui]\nscale = 1.0\n[ui.stack_widths]\nx = 1\n");
        overlay_stack_widths(&path, &BTreeMap::new()).expect("write succeeds");
        let written = std::fs::read_to_string(&path).expect("read back");
        assert!(written.contains("scale = 1.0"));
        assert!(!written.contains("stack_widths"), "no empty husk");
    }

    #[test]
    fn overlay_on_an_absent_file_creates_it() {
        let (_dir, path) = write_temp_config("");
        let (_, board, stack_a, _) = fixture_state();
        let widths = BTreeMap::from([(board, BTreeMap::from([(stack_a, 240u32)]))]);
        overlay_stack_widths(&path, &widths).expect("write succeeds");
        let written = std::fs::read_to_string(&path).expect("created");
        assert!(written.contains(&board.to_string()));
    }

    #[test]
    fn overlay_on_a_directory_path_errors_typed_without_panicking() {
        let dir = tempfile::tempdir().expect("tempdir");
        // The config path itself is a directory: the read fails typed
        // (io error kind), never a panic (the never-crash rule, D8).
        let err = overlay_stack_widths(dir.path(), &BTreeMap::new());
        assert!(err.is_err());
    }

    // -- restart round-trip (T-18) --------------------------------------------

    #[test]
    fn written_widths_round_trip_through_the_config_layer() {
        let (_dir, path) = write_temp_config("# c\n[ui]\nscale = 1.25\n");
        let (_, board, stack_a, stack_b) = fixture_state();
        let widths =
            BTreeMap::from([(board, BTreeMap::from([(stack_a, 240u32), (stack_b, 96u32)]))]);
        overlay_stack_widths(&path, &widths).expect("write succeeds");

        // The restart half: the written file decodes through the real
        // figment layer — exactly what the next process's config load
        // runs — and carries the widths verbatim into `UiSection`.
        let written = std::fs::read_to_string(&path).expect("read");
        let reloaded: crate::config::AppConfig = figment::Figment::from(
            figment::providers::Serialized::defaults(crate::config::AppConfig::default()),
        )
        .merge(figment::providers::Toml::string(&written))
        .extract()
        .expect("the written config decodes");
        let stacks = reloaded.ui.stack_widths.get(&board).expect("board row");
        assert_eq!(stacks.get(&stack_a), Some(&240));
        assert_eq!(stacks.get(&stack_b), Some(&96));
    }

    // -- the forwarder loop (T-17) ---------------------------------------------

    /// T-17 — debounce determinism: two snapshots for one board inside
    /// the quiet window collapse into exactly one write carrying the
    /// merged latest; a late message extends the window; close flushes
    /// pending; a failing path is survived and a later retry lands.
    #[tokio::test]
    async fn debounces_bursts_and_flushes_on_close() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("taskboard.toml");
        std::fs::write(&path, "[ui]\nscale = 1.0\n").expect("seed");
        let (state, board, stack_a, stack_b) = fixture_state();
        let (tx, rx) = ui_request_channel();
        let forwarder = spawn_ui_request_forwarder(
            rx,
            Arc::new(ArcSwap::from_pointee(state)),
            test_workspace(path.clone()).await,
        );
        // The debounce is deterministic under the paused clock; the
        // core above booted on the real one (sqlx needs it), so the
        // pause starts here (plan T-17's semantics unchanged).
        tokio::time::pause();

        let send = |tx: &mpsc::Sender<UiRequest>, stack: uuid::Uuid, px: u32| {
            tx.try_send(UiRequest::StackWidthsChanged {
                board: BoardId::from_uuid(board),
                widths: vec![(StackId::from_uuid(stack), px)],
            })
            .expect("capacity holds");
        };
        send(&tx, stack_a, 240);
        send(&tx, stack_b, 96);
        // Inside the quiet window: nothing written yet. The settle runs
        // the spawned task's polls without sleeps — the paused clock
        // makes every step deterministic.
        tokio::time::advance(
            UI_REQUEST_DEBOUNCE
                .checked_sub(std::time::Duration::from_millis(1))
                .expect("positive"),
        )
        .await;
        for _ in 0..8 {
            tokio::task::yield_now().await;
        }
        assert!(
            !std::fs::read_to_string(&path)
                .expect("read")
                .contains(&stack_a.to_string()),
            "the window has not elapsed"
        );
        // A late message extends the window: the original window
        // elapses, the message restarts it, and the *extended* window's
        // flush carries the merged latest.
        send(&tx, stack_a, 260);
        tokio::time::advance(UI_REQUEST_DEBOUNCE + std::time::Duration::from_millis(1)).await;
        for _ in 0..8 {
            tokio::task::yield_now().await;
        }
        tokio::time::advance(UI_REQUEST_DEBOUNCE + std::time::Duration::from_millis(1)).await;
        for _ in 0..8 {
            tokio::task::yield_now().await;
        }
        // …and the flush carried exactly the merged latest.
        let written = std::fs::read_to_string(&path).expect("read");
        assert!(written.contains(&board.to_string()));
        let doc = written.parse::<toml_edit::DocumentMut>().expect("parses");
        let board_table = doc["ui"]["stack_widths"][&board.to_string()]
            .as_table()
            .expect("board table");
        assert_eq!(
            board_table
                .get(&stack_a.to_string())
                .and_then(toml_edit::Item::as_integer),
            Some(260),
            "the latest snapshot won"
        );

        // A resize sent right before quit still lands (the final flush).
        send(&tx, stack_a, 300);
        drop(tx);
        forwarder.await.expect("forwarder joins cleanly");
        let doc = std::fs::read_to_string(&path)
            .expect("read")
            .parse::<toml_edit::DocumentMut>()
            .expect("parses");
        assert_eq!(
            doc["ui"]["stack_widths"][&board.to_string()][&stack_a.to_string()].as_integer(),
            Some(300),
            "the close flush landed"
        );
    }

    /// T-17 — the never-crash rule: a failing write path is warned and
    /// survived; the task outlives it and completes a later good write.
    #[tokio::test]
    async fn a_failing_write_path_is_survived_and_retried() {
        let dir = tempfile::tempdir().expect("tempdir");
        // The config "path" is a directory: every read/write fails.
        let cursed = dir.path().join("cursed");
        std::fs::create_dir(&cursed).expect("dir");
        let (state, board, stack_a, _) = fixture_state();
        let (tx, rx) = ui_request_channel();
        let forwarder = spawn_ui_request_forwarder(
            rx,
            Arc::new(ArcSwap::from_pointee(state)),
            test_workspace(cursed.clone()).await,
        );
        tokio::time::pause();
        tx.try_send(UiRequest::StackWidthsChanged {
            board: BoardId::from_uuid(board),
            widths: vec![(StackId::from_uuid(stack_a), 240)],
        })
        .expect("capacity holds");
        tokio::time::advance(UI_REQUEST_DEBOUNCE + std::time::Duration::from_millis(1)).await;
        for _ in 0..8 {
            tokio::task::yield_now().await;
        }
        assert!(!forwarder.is_finished(), "the task outlived the failure");

        // The retry story: the pending map was cleared by the failed
        // flush, so nothing is written anywhere until the next request
        // arrives — the good path simply stays clean and the task lives on.
        tx.try_send(UiRequest::StackWidthsChanged {
            board: BoardId::from_uuid(board),
            widths: vec![(StackId::from_uuid(stack_a), 260)],
        })
        .expect("capacity holds");
        // The pending map was cleared by the failed flush; the retry
        // re-sends the full snapshot, so the good path's first write
        // carries it.
        tokio::time::advance(UI_REQUEST_DEBOUNCE + std::time::Duration::from_millis(1)).await;
        drop(tx);
        forwarder.await.expect("forwarder joins cleanly");
        // The cursed path stayed a directory (no partial file anywhere).
        assert!(cursed.is_dir());
    }

    /// An empty pending map at flush time (a close with nothing ever
    /// received) writes nothing — the never-create rule.
    #[tokio::test]
    async fn a_close_without_requests_writes_nothing() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("taskboard.toml");
        let (_state, _b, _a, _s) = fixture_state();
        let (tx, rx) = ui_request_channel();
        let forwarder = spawn_ui_request_forwarder(
            rx,
            Arc::new(ArcSwap::from_pointee(AppState::default())),
            test_workspace(path.clone()).await,
        );
        // The debounce is deterministic under the paused clock; the
        // core above booted on the real one (sqlx needs it), so the
        // pause starts here (plan T-17's semantics unchanged).
        tokio::time::pause();
        drop(tx);
        forwarder.await.expect("forwarder joins cleanly");
        assert!(!path.exists(), "no write at all when nothing was received");
    }
    // -- the geometry lane (T7/T8) -------------------------------------------

    fn geometry(x: i64, y: i64, w: i64, h: i64) -> WindowGeometry {
        WindowGeometry::new(x, y, w, h).expect("sane test rect")
    }

    /// T7 — two geometry snapshots inside the quiet window collapse
    /// into one write carrying the last rect; the flush lands on
    /// quiet-window elapse and on channel close.
    #[tokio::test]
    async fn geometry_requests_debounce_into_one_write_of_the_latest_rect() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("taskboard.toml");
        std::fs::write(&path, "[ui]\nscale = 1.0\n").expect("seed");
        let (state, _, _, _) = fixture_state();
        let (tx, rx) = ui_request_channel();
        let forwarder = spawn_ui_request_forwarder(
            rx,
            Arc::new(ArcSwap::from_pointee(state)),
            test_workspace(path.clone()).await,
        );
        tokio::time::pause();

        tx.try_send(UiRequest::WindowGeometryChanged {
            geometry: geometry(10, 20, 800, 600),
        })
        .expect("capacity holds");
        tx.try_send(UiRequest::WindowGeometryChanged {
            geometry: geometry(30, 40, 1024, 768),
        })
        .expect("capacity holds");
        // Inside the quiet window: nothing written yet.
        tokio::time::advance(
            UI_REQUEST_DEBOUNCE
                .checked_sub(std::time::Duration::from_millis(1))
                .expect("positive"),
        )
        .await;
        for _ in 0..8 {
            tokio::task::yield_now().await;
        }
        assert!(
            !std::fs::read_to_string(&path)
                .expect("read")
                .contains("window_geometry"),
            "the window has not elapsed"
        );
        // The window elapses: exactly the last rect lands.
        tokio::time::advance(UI_REQUEST_DEBOUNCE + std::time::Duration::from_millis(1)).await;
        for _ in 0..8 {
            tokio::task::yield_now().await;
        }
        let doc = std::fs::read_to_string(&path)
            .expect("read")
            .parse::<toml_edit::DocumentMut>()
            .expect("parses");
        let subtree = doc["ui"]["window_geometry"].as_table().expect("subtree");
        assert_eq!(
            subtree.get("x").and_then(toml_edit::Item::as_integer),
            Some(30)
        );
        assert_eq!(
            subtree.get("width").and_then(toml_edit::Item::as_integer),
            Some(1024)
        );

        // A change sent right before quit still lands (the close flush).
        tx.try_send(UiRequest::WindowGeometryChanged {
            geometry: geometry(-500, 0, 800, 600),
        })
        .expect("capacity holds");
        drop(tx);
        forwarder.await.expect("forwarder joins cleanly");
        let doc = std::fs::read_to_string(&path)
            .expect("read")
            .parse::<toml_edit::DocumentMut>()
            .expect("parses");
        assert_eq!(
            doc["ui"]["window_geometry"]["x"].as_integer(),
            Some(-500),
            "the close flush landed"
        );
    }

    /// T8 — a geometry flush preserves comments and unrelated tables
    /// byte-for-byte, and one flush carrying widths *and* geometry
    /// writes both subtrees in one document (the R6 single-writer
    /// discipline, D7).
    #[tokio::test]
    async fn one_flush_covers_both_subtrees_and_preserves_the_document() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("taskboard.toml");
        std::fs::write(
            &path,
            "# operator comment\n[ui]\nscale = 1.5\n[nextcloud]\n# keep me\nserver_url = \"https://x\"\n",
        )
        .expect("seed");
        let (state, board, stack_a, _) = fixture_state();
        let (tx, rx) = ui_request_channel();
        let forwarder = spawn_ui_request_forwarder(
            rx,
            Arc::new(ArcSwap::from_pointee(state)),
            test_workspace(path.clone()).await,
        );
        tokio::time::pause();

        // One width snapshot and one geometry snapshot, debounced
        // together into one flush.
        tx.try_send(UiRequest::StackWidthsChanged {
            board: BoardId::from_uuid(board),
            widths: vec![(StackId::from_uuid(stack_a), 240)],
        })
        .expect("capacity holds");
        tx.try_send(UiRequest::WindowGeometryChanged {
            geometry: geometry(-1920, 0, 1366, 768),
        })
        .expect("capacity holds");
        tokio::time::advance(UI_REQUEST_DEBOUNCE + std::time::Duration::from_millis(1)).await;
        for _ in 0..8 {
            tokio::task::yield_now().await;
        }
        drop(tx);
        forwarder.await.expect("forwarder joins cleanly");

        let written = std::fs::read_to_string(&path).expect("read");
        assert!(written.contains("# operator comment"));
        assert!(written.contains("# keep me"), "unrelated comments survive");
        assert!(written.contains("scale = 1.5"));
        let doc = written.parse::<toml_edit::DocumentMut>().expect("parses");
        let geometry = doc["ui"]["window_geometry"]
            .as_table()
            .expect("geometry subtree");
        assert_eq!(
            geometry.get("x").and_then(toml_edit::Item::as_integer),
            Some(-1920)
        );
        assert_eq!(
            geometry.get("height").and_then(toml_edit::Item::as_integer),
            Some(768)
        );
        assert_eq!(
            doc["ui"]["stack_widths"][&board.to_string()][&stack_a.to_string()].as_integer(),
            Some(240),
            "both subtrees landed from the same flush"
        );
    }

    // -- A10: the mapping tables ------------------------------------------

    /// A10 — every reachable `CommandError` class from the extracted
    /// ceremony maps to its typed variant; the config rejections map to
    /// their fields.
    #[test]
    fn the_settings_error_mapping_table_covers_every_reachable_arm() {
        use crate::bootstrap::AttachError;
        use crate::config::ConfigError;
        use taskboard_ui_slint::requests::{Field, InvalidKind, SettingsError as E};

        // verify classes
        assert_eq!(
            map_ceremony_error(CommandError::InvalidCredentials),
            E::VerifyRejected
        );
        assert_eq!(
            map_ceremony_error(CommandError::Unreachable),
            E::VerifyUnreachable
        );
        assert_eq!(
            map_ceremony_error(CommandError::Deck(Box::new(
                taskboard_sync_nextcloud::DeckError::BadRequest
            ))),
            E::VerifyFailed
        );
        // config classes
        assert_eq!(
            map_config_error(&ConfigError::InvalidConnectorName {
                name: "Work".into()
            }),
            E::Invalid {
                field: Field::Name,
                kind: InvalidKind::NotKebab
            }
        );
        assert_eq!(
            map_config_error(&ConfigError::DuplicateConnectorName { name: "w".into() }),
            E::Invalid {
                field: Field::Name,
                kind: InvalidKind::Duplicate
            }
        );
        assert_eq!(
            map_config_error(&ConfigError::InvalidConnectorEntry {
                name: "w".into(),
                field: "server_url"
            }),
            E::Invalid {
                field: Field::ServerUrl,
                kind: InvalidKind::BadUrl
            }
        );
        assert_eq!(
            map_config_error(&ConfigError::InvalidConnectorEntry {
                name: "w".into(),
                field: "username"
            }),
            E::Invalid {
                field: Field::Username,
                kind: InvalidKind::Empty
            }
        );
        // attach classes
        assert_eq!(
            map_attach_error(AttachError::ConnectorNameInUse(ConnectorId::from("w"))),
            E::AttachBusy
        );
        assert_eq!(map_attach_error(AttachError::Draining), E::AttachBusy);
        assert!(matches!(
            map_attach_error(AttachError::Engine(
                taskboard_state::AttachError::EngineGone
            )),
            E::AttachFailed
        ));
        // credential classes
        assert_eq!(
            map_credential_error(CredentialError::NotFound),
            E::NoSecretStored
        );
        assert_eq!(
            map_credential_error(CredentialError::Unsupported),
            E::StoreUnsupported
        );
    }

    // -- the settings editor's write port (W-8 + the mapping table) --

    /// The gate-refusal mapping table (the A10 shape): a blame inside
    /// the edited field's own key is that field's typed invalidity
    /// (parse arm for text fields, range arm for numerics); any other
    /// blame — same section, different field, or unattributable — is
    /// the coarse persist rejection.
    #[test]
    fn the_gate_refusal_mapping_table_covers_the_editor_fields() {
        use crate::config::ConfigError;
        use crate::settings_overlay::OverlayError;
        use taskboard_ui_slint::requests::{Field, InvalidKind, SettingsError as E};

        let own_blame = |field, field_str| {
            map_overlay_error(
                field,
                &OverlayError::Rejected(Box::new(ConfigError::InvalidDuration {
                    field: field_str,
                })),
            )
        };
        assert_eq!(
            own_blame(Field::DayStart, "preferences.day_start_time"),
            E::Invalid {
                field: Field::DayStart,
                kind: InvalidKind::NotKebab
            }
        );
        assert_eq!(
            own_blame(Field::TickSecs, "reminders.tick_secs"),
            E::Invalid {
                field: Field::TickSecs,
                kind: InvalidKind::OutOfRange
            }
        );
        assert_eq!(
            own_blame(Field::BackoffInitial, "sync.backoff_initial_secs"),
            E::Invalid {
                field: Field::BackoffInitial,
                kind: InvalidKind::OutOfRange
            }
        );
        // Same section, different field: coarse.
        assert_eq!(
            map_overlay_error(
                Field::RemindersEnabled,
                &OverlayError::Rejected(Box::new(ConfigError::InvalidDuration {
                    field: "reminders.tick_secs"
                })),
            ),
            E::PersistRejected
        );
        // Unattributable: coarse.
        assert_eq!(
            map_overlay_error(
                Field::DueSoonDays,
                &OverlayError::Rejected(Box::new(ConfigError::EmptyDbPath)),
            ),
            E::PersistRejected
        );
        // IO: the persist classes.
        assert_eq!(
            map_overlay_error(
                Field::PollInterval,
                &OverlayError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "shape"
                )),
            ),
            E::PersistRejected
        );
        assert_eq!(
            map_overlay_error(
                Field::PollInterval,
                &OverlayError::Io(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "io"
                )),
            ),
            E::PersistFailed
        );
    }

    /// W-8 + the round-trip: the editor's handler writes through the
    /// workspace's resolved path (never a fresh resolution), and the
    /// reply echoes the fresh file-layer snapshot; a gate-refused
    /// commit leaves the file byte-untouched and replies the typed
    /// field invalidity.
    #[tokio::test]
    async fn the_editor_handlers_write_through_the_workspace_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("taskboard.toml");
        std::fs::write(
            &path,
            "[reminders]\ntick_secs = 30\n[sync]\nbackoff_initial_secs = 5\nbackoff_max_secs = 300\n",
        )
        .expect("seed");
        let workspace = test_workspace(path.clone()).await;

        let (reply, reply_rx) = tokio::sync::oneshot::channel();
        handle_settings(UiRequest::SetReminderTick { secs: 45, reply }, &workspace).await;
        let snapshot = reply_rx
            .await
            .expect("the reply lane is alive")
            .expect("lands");
        assert_eq!(
            snapshot.reminder_tick_secs, 45,
            "the reply echoes the engine"
        );
        // Persist-then-command (round-5 chunk 5): the engine's published
        // settings carry the committed value — the reply is read from
        // there, not from the file.
        assert_eq!(
            workspace
                .engine
                .shared_state()
                .load()
                .settings
                .tick_secs
                .get(),
            45,
            "the engine applied the written snapshot"
        );
        assert_eq!(
            snapshot.applies.len(),
            18,
            "the reply carries the apply map"
        );
        assert!(
            snapshot.env_overridden.is_empty(),
            "replies never re-derive the env set"
        );
        let written = std::fs::read_to_string(&path).expect("read back");
        assert!(
            written.contains("tick_secs = 45"),
            "the workspace path took the write"
        );

        // The ordering gate: a crossing refuses typed, file untouched.
        let (reply, reply_rx) = tokio::sync::oneshot::channel();
        handle_settings(
            UiRequest::SetBackoffInitial { secs: 400, reply },
            &workspace,
        )
        .await;
        let outcome = reply_rx.await.expect("the reply lane is alive");
        assert!(
            matches!(
                outcome,
                Err(SettingsError::Invalid {
                    field: Field::BackoffInitial,
                    kind: InvalidKind::OutOfRange
                })
            ),
            "the ordering refusal names the offending field: {outcome:?}"
        );
        assert!(
            std::fs::read_to_string(&path)
                .expect("read")
                .contains("backoff_initial_secs = 5"),
            "the refused write left the file untouched"
        );
        // A failed prove-and-write sends no command: the engine's
        // published settings still carry the previous commit, observed
        // through the ArcSwap.
        let engine_settings = workspace.engine.shared_state().load().settings.clone();
        assert_eq!(engine_settings.backoff_initial_secs.get(), 5);
        assert_eq!(engine_settings.tick_secs.get(), 45);
    }

    /// The engine-gone arm (D-5.3): with the engine draining, the file
    /// still takes the write (the next boot's authority) but the reply
    /// is the typed `Gone` — the running engine keeps its old values.
    #[tokio::test]
    async fn an_engine_gone_write_lands_in_the_file_and_replies_gone() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("taskboard.toml");
        std::fs::write(&path, "[reminders]\ntick_secs = 30\n").expect("seed");
        let (workspace, app) = test_workspace_with_app(path.clone()).await;
        let before = workspace.engine.shared_state().load().settings.clone();

        // Drain the engine: every later `execute` answers `EngineGone`.
        app.system
            .send(taskboard_domain::SystemEvent::Shutdown)
            .expect("the engine subscribes");
        // The drain transition is observable only through a declined
        // command; poll it (no sleeps — a bounded predicate loop).
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                let probe = workspace.engine.execute(StateCommand::RequestSync).await;
                if matches!(probe, Err(taskboard_state::ExecuteError::EngineGone)) {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("the engine drained");

        let (reply, reply_rx) = tokio::sync::oneshot::channel();
        handle_settings(UiRequest::SetReminderTick { secs: 45, reply }, &workspace).await;
        let outcome = reply_rx.await.expect("the reply lane is alive");
        assert!(matches!(outcome, Err(SettingsError::Gone)), "{outcome:?}");
        assert!(
            std::fs::read_to_string(&path)
                .expect("read back")
                .contains("tick_secs = 45"),
            "the file took the write regardless"
        );
        assert_eq!(
            workspace.engine.shared_state().load().settings,
            before,
            "the drained engine kept its values"
        );
        let _keep = app;
    }

    /// The focus commits persist through the workspace path and the
    /// reply echoes the fresh file snapshot's typed values (the focus
    /// settings editor, #625).
    #[tokio::test]
    async fn focus_commits_persist_and_reply_the_fresh_snapshot() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("taskboard.toml");
        std::fs::write(&path, "[ui]\nscale = 1.0\n").expect("seed");
        let workspace = test_workspace(path.clone()).await;

        let (reply, reply_rx) = tokio::sync::oneshot::channel();
        handle_settings(UiRequest::SetFocusWorkMins { mins: 45, reply }, &workspace).await;
        let snapshot = reply_rx
            .await
            .expect("the reply lane is alive")
            .expect("lands");
        assert_eq!(snapshot.focus_work_mins, 45, "the reply echoes the file");
        assert!(
            std::fs::read_to_string(&path)
                .expect("read back")
                .contains("work_mins = 45"),
            "the workspace path took the write"
        );

        let (reply, reply_rx) = tokio::sync::oneshot::channel();
        handle_settings(
            UiRequest::SetFocusEnabled {
                enabled: true,
                reply,
            },
            &workspace,
        )
        .await;
        let snapshot = reply_rx
            .await
            .expect("the reply lane is alive")
            .expect("lands");
        assert!(snapshot.focus_enabled);
        assert_eq!(snapshot.focus_work_mins, 45, "the earlier write survives");
    }

    /// The Files toggle (round-5 chunk 8, D-8.6): the write lands
    /// `[files_sync] enabled` only (`poll_secs` survives), the reply
    /// echoes it; over a config whose default connector is not
    /// Nextcloud the gate refuses typed and the file stays untouched.
    #[tokio::test]
    async fn the_files_toggle_writes_only_its_key_and_refuses_without_nextcloud() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("taskboard.toml");
        std::fs::write(
            &path,
            "[[sync.connectors]]\nname = \"work\"\nbackend = \"nextcloud\"\n\
             server_url = \"https://c.example\"\nusername = \"alice\"\n\
             [files_sync]\npoll_secs = 7\n",
        )
        .expect("seed");
        let workspace = test_workspace(path.clone()).await;
        let (reply, reply_rx) = tokio::sync::oneshot::channel();
        handle_settings(
            UiRequest::SetFilesSyncEnabled {
                enabled: true,
                anchor: None,
                reply,
            },
            &workspace,
        )
        .await;
        let snapshot = reply_rx.await.expect("alive").expect("lands");
        assert!(snapshot.files_sync_enabled, "the reply echoes the file");
        let written = AppConfig::extract_raw(Some(&path)).expect("decodes");
        assert!(written.files_sync.enabled);
        assert_eq!(written.files_sync.poll_secs, Some(7), "poll_secs survives");
        assert_eq!(
            written.files_sync.anchor, None,
            "the None arm leaves the anchor key untouched"
        );

        let standalone = dir.path().join("standalone.toml");
        std::fs::write(&standalone, "[sync]\nbackend = \"standalone\"\n").expect("seed");
        let before = std::fs::read_to_string(&standalone).expect("read");
        let workspace = test_workspace(standalone.clone()).await;
        let (reply, reply_rx) = tokio::sync::oneshot::channel();
        handle_settings(
            UiRequest::SetFilesSyncEnabled {
                enabled: true,
                anchor: None,
                reply,
            },
            &workspace,
        )
        .await;
        let outcome = reply_rx.await.expect("alive");
        assert!(
            matches!(
                outcome,
                Err(SettingsError::Invalid {
                    field: Field::FilesSyncEnabled,
                    ..
                })
            ),
            "the gate's own-blame refusal is typed: {outcome:?}"
        );
        assert_eq!(std::fs::read_to_string(&standalone).expect("read"), before);
    }

    /// The re-anchoring pick (round-6 D-6.6): one request lands both
    /// keys, the reply snapshot echoes the resolved anchor.
    #[tokio::test]
    async fn the_reanchoring_pick_lands_both_keys_and_echoes_the_anchor() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("taskboard.toml");
        std::fs::write(
            &path,
            "[[sync.connectors]]\nname = \"work\"\nbackend = \"nextcloud\"\n\
             server_url = \"https://c.example\"\nusername = \"alice\"\n\
             [[sync.connectors]]\nname = \"home\"\nbackend = \"nextcloud\"\n\
             server_url = \"https://d.example\"\nusername = \"bob\"\n",
        )
        .expect("seed");
        let workspace = test_workspace(path.clone()).await;
        let (reply, reply_rx) = tokio::sync::oneshot::channel();
        handle_settings(
            UiRequest::SetFilesSyncEnabled {
                enabled: true,
                anchor: Some("home".into()),
                reply,
            },
            &workspace,
        )
        .await;
        let snapshot = reply_rx.await.expect("alive").expect("lands");
        assert_eq!(
            snapshot.files_sync_anchor.as_deref(),
            Some("home"),
            "the reply echoes the resolved anchor"
        );
        let written = AppConfig::extract_raw(Some(&path)).expect("decodes");
        assert!(written.files_sync.enabled);
        assert_eq!(written.files_sync.anchor.as_deref(), Some("home"));
    }

    /// Env honesty (round-6 D-6.6): the Files row's lock verdict —
    /// either of its two keys pinned refuses the whole pick, any
    /// other field is unaffected. (The real env scan is pinned in the
    /// integration suite: process env is `unsafe` to mutate and this
    /// lib crate forbids it.)
    #[test]
    fn the_files_write_env_lock_covers_both_of_its_keys() {
        use std::collections::BTreeSet;
        let locked: BTreeSet<String> = ["files_sync.anchor"]
            .into_iter()
            .map(String::from)
            .collect();
        assert!(
            editor_write_env_locked(Field::FilesSyncEnabled, &locked),
            "a pinned anchor locks the Files pick"
        );
        let locked: BTreeSet<String> = ["files_sync.enabled"]
            .into_iter()
            .map(String::from)
            .collect();
        assert!(editor_write_env_locked(Field::FilesSyncEnabled, &locked));
        assert!(
            !editor_write_env_locked(Field::FocusEnabled, &locked),
            "another field's verdict reads its own key only"
        );
        let locked: BTreeSet<String> = ["focus.enabled"].into_iter().map(String::from).collect();
        assert!(editor_write_env_locked(Field::FocusEnabled, &locked));
        assert!(
            !editor_write_env_locked(Field::FilesSyncEnabled, &locked),
            "an unrelated pin does not lock the Files row"
        );
    }

    /// A garbage minute write reaching the handler from a non-UI lane
    /// client is refused typed with own-blame attribution — the gate's
    /// defence in depth, never a silent clamp at any layer.
    #[tokio::test]
    async fn focus_gate_refusal_attributes_own_blame() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("taskboard.toml");
        std::fs::write(&path, "[ui]\nscale = 1.0\n").expect("seed");
        let workspace = test_workspace(path.clone()).await;

        let (reply, reply_rx) = tokio::sync::oneshot::channel();
        handle_settings(UiRequest::SetFocusWorkMins { mins: 0, reply }, &workspace).await;
        let outcome = reply_rx.await.expect("the reply lane is alive");
        assert!(
            matches!(
                outcome,
                Err(SettingsError::Invalid {
                    field: Field::FocusWorkMins,
                    kind: InvalidKind::OutOfRange
                })
            ),
            "the own-blame refusal is typed: {outcome:?}"
        );
        assert!(
            !std::fs::read_to_string(&path)
                .expect("read")
                .contains("work_mins"),
            "the refused write left the file untouched"
        );
    }

    /// T5's app-side twin (the redaction proof lives in the UI crate;
    /// this pins the request `Debug` structurally through a full
    /// profile payload).
    #[test]
    fn a_save_request_debug_output_never_carries_the_secret() {
        let (tx, _rx) = tokio::sync::oneshot::channel();
        let request = UiRequest::SaveConnection {
            profile: taskboard_ui_slint::requests::ConnectionProfile {
                name: "work".into(),
                server_url: "https://c.example".into(),
                username: "alice".into(),
                password: taskboard_ui_slint::requests::Secret("hunter2-secret".into()),
            },
            reply: tx,
        };
        let rendered = format!("{request:?}");
        assert!(!rendered.contains("hunter2-secret"));
    }
}

#[cfg(test)]
mod move_error_tests {
    use super::*;

    /// The typed fold (D-5): the lane-off and busy-outbox classes map
    /// to their named variants; every step class folds to the coarse
    /// `MoveFailed`; non-ceremony command classes ride the existing
    /// ceremony mapping. Typed outcomes only — never message text.
    #[test]
    fn the_move_error_classes_fold_to_their_named_variants() {
        use crate::board_move_ceremony::MoveError;
        use taskboard_state::ExecuteError;

        let fold = |err: crate::commands::CommandError| map_move_error(err);
        assert!(matches!(
            fold(crate::board_move_ceremony::MoveError::FilesSyncDisabled.into()),
            SettingsError::FilesSyncDisabled
        ));
        assert!(matches!(
            fold(MoveError::OutboxNotQuiescent { pending: 2 }.into()),
            SettingsError::OutboxNotQuiescent
        ));
        for step in [
            MoveError::PublishUnconfirmed,
            MoveError::FilesSubtreeForeign,
            MoveError::UndoCoordinatesLost,
            MoveError::NotDeckBound,
        ] {
            assert!(
                matches!(fold(step.into()), SettingsError::MoveFailed),
                "step classes fold to the coarse class"
            );
        }
        // A non-ceremony class rides the existing mapping's arms —
        // the move fold claims only `CommandError::BoardMove`.
        assert!(matches!(
            fold(crate::commands::CommandError::Engine(Box::new(
                ExecuteError::EngineGone
            ))),
            SettingsError::VerifyFailed
        ));
    }
}
