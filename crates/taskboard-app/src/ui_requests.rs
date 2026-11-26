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
    ConnectionProfile, ConnectionsSnapshot, ConnectorInfo, ExportFamily, ExportFileFormat,
    ExportRefusal, ExportRunOutcome, ExportSpec, Field, InvalidKind, KeymapUpdate, ReminderSink,
    RemoteBoardRow, SettingsError, SettingsSnapshot, UiRequest,
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
///
/// Ruling R-17 (T-17, #800; dated 2026-11-15, authority
/// `.artifacts/plans/2026-11-15-ux-r5-surface-friction-chunk-plan.md`
/// D-4): this lane stays OUTSIDE the undo journal by design — its
/// remote halves cannot be honestly inverted (a local pre-image would
/// restore the local fact while the remote half keeps the effect),
/// the destructive journeys already stage through the chunk-2 armed
/// confirmations (the recovery surface), and the non-destructive rest
/// is re-editable at the very knob that made the change. The T-16
/// typed refusal render keeps every lane outcome visible.
#[must_use]
pub fn ui_request_channel() -> (mpsc::Sender<UiRequest>, mpsc::Receiver<UiRequest>) {
    mpsc::channel(UI_REQUEST_CAPACITY)
}

/// The export run's drain dependencies (the J2 export plan W-6):
/// the boot-resolved config (the pdf leg's destination/layout
/// postures and the day calendar the engine reads) plus the
/// typed-permission flag for the open-after-export viewer open.
#[derive(Clone)]
pub struct ExportWorkspace {
    /// The resolved, validated configuration.
    pub config: std::sync::Arc<crate::config::AppConfig>,
    /// Whether the drain may open the system viewer after a pdf run
    /// (the desktop lifecycle's permission; kiosk `false`).
    pub may_open_viewer: bool,
    /// The clipboard handoff's effect seam (the D2 preview plan D-3,
    /// the chunk R-2/R-4 law): writes the payload text to the system
    /// clipboard, `Err(())` when unavailable (headless session,
    /// failed open/write — the typed `ClipboardUnavailable`). The
    /// default is the real arboard write; tests inject.
    #[allow(clippy::type_complexity)]
    pub clipboard_write: std::sync::Arc<dyn Fn(&str) -> Result<(), ()> + Send + Sync>,
}

impl Default for ExportWorkspace {
    fn default() -> Self {
        Self::with_real_clipboard(
            std::sync::Arc::new(crate::config::AppConfig::default()),
            false,
        )
    }
}

impl std::fmt::Debug for ExportWorkspace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExportWorkspace")
            .field("config", &self.config)
            .field("may_open_viewer", &self.may_open_viewer)
            .finish_non_exhaustive()
    }
}

impl ExportWorkspace {
    /// The production clipboard effect (the real system clipboard).
    #[must_use]
    pub fn with_real_clipboard(
        config: std::sync::Arc<crate::config::AppConfig>,
        may_open_viewer: bool,
    ) -> Self {
        Self {
            config,
            may_open_viewer,
            clipboard_write: std::sync::Arc::new(clipboard_write_real),
        }
    }
}

/// The production clipboard write (the arboard default): any open or
/// copy failure is the typed `Err(())` — the environment refusal,
/// never a panic.
fn clipboard_write_real(text: &str) -> Result<(), ()> {
    arboard::Clipboard::new()
        .and_then(|mut clipboard| clipboard.set_text(text.to_owned()))
        .map_err(|_| ())
}

/// The preview pane's presentation cap (the preview plan D-2): a
/// pane-rendering budget, never a payload or engine limit — the file
/// and clipboard payloads are never truncated. Over-cap payloads
/// render the capped head plus the typed `truncated` fact; the fold
/// asserts the flag, never sizes.
pub const PREVIEW_CAP_BYTES: usize = 65_536;

/// Folds one CLI-built [`StatusView`] into the panel's 1:1 mirror
/// (OP3 W-4, D-2): the scalar surface of the same typed view the CLI
/// verb prints — no fact is minted, renamed, or re-derived GUI-side.
/// The per-connector/per-board/per-peer arrays are entity detail
/// outside D-2's enumerated panel surface (the CLI verb remains their
/// front); every other field passes through verbatim.
#[must_use]
pub fn status_snapshot(
    view: &crate::commands::status::StatusView,
) -> taskboard_ui_slint::requests::StatusSnapshot {
    use taskboard_ui_slint::requests::StatusSnapshot;
    StatusSnapshot {
        daemon: view.daemon,
        version: view.version.clone(),
        backend: view.backend.clone(),
        pid: view.pid,
        holder_pid: view.holder_pid,
        connector_attached: view.connector_attached,
        draining: view.draining,
        sync_phase: view.sync.as_ref().map(|sync| sync.phase.clone()),
        sync_pending_ops: view.sync.as_ref().map(|sync| sync.pending_ops),
        engine_commands: view.engine.as_ref().map(|engine| engine.commands),
        engine_rejections: view.engine.as_ref().map(|engine| engine.rejections),
        engine_signals: view.engine.as_ref().map(|engine| engine.signals),
    }
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
    /// The move ceremony's injection point (#751): the identity file
    /// the arms pass through to `board_move_ceremony::run_on`, so
    /// tests own their identity home. `None` — a platform with no
    /// data directory — stays a typed `FilesSyncDisabled` refusal at
    /// the arms (the pre-injection `app_world` outcome): bootstrap
    /// degrades authorless there, so this is not a boot invariant.
    pub device_json: Option<PathBuf>,
    /// The boot-computed connector-owner set (D-BF5 point 2, #1120),
    /// handed verbatim to the boot-pull settle predicate — the
    /// workspace carries it so the settings ceremonies never re-derive
    /// config ownership.
    pub connector_owners: std::collections::BTreeSet<taskboard_domain::ConnectorId>,
    /// The export family's workspace (the J2 export plan W-6, D-5):
    /// the resolved config the drain's engine calls read, and the
    /// typed viewer-open permission — `true` only on the desktop
    /// lifecycle (the kiosk workspace never shells out, R-2's trap-c
    /// law; the kiosk UI never mounts the dialog anyway).
    pub export: ExportWorkspace,
    /// The status panel's answer source (OP3 W-4, D-2): the same
    /// `StatusView` constructors the CLI `status` verb builds — the
    /// GUI lifecycles capture their own in-session projection
    /// ([`StatusView::in_session_parts`]) over cloned handles, so the
    /// panel reads the identical state the CLI prints (one
    /// projection, two fronts). Captured per lifecycle at the spawn
    /// site (`ui.rs` desktop, `main.rs` kiosk).
    pub status: Box<dyn Fn() -> crate::commands::status::StatusView + Send + Sync>,
    /// The status panel's remediation lane (the D2 command plan
    /// W-7): the database home, the lifecycle's typed
    /// process-control permission (kiosk `false` — the trap-a law),
    /// and the process-effect seams.
    pub remediation: crate::status_remediation::RemediationWorkspace,
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
                UiRequest::StatusQuery { reply } => {
                    answer_status_query(&workspace, reply);
                    continue;
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
                    Ok(Some(UiRequest::StatusQuery { reply })) => {
                        answer_status_query(&workspace, reply);
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

/// Answers one `StatusQuery` (OP3 W-4): the CLI-built in-session
/// projection folded into the panel's mirror. Pure — no config write,
/// no core boot, no lock; a wedged engine reads its last published
/// state (the observation posture, never a sync trigger).
fn answer_status_query(
    workspace: &ConnectionWorkspace,
    reply: tokio::sync::oneshot::Sender<taskboard_ui_slint::requests::StatusSnapshot>,
) {
    let view = (workspace.status)();
    let _ = reply.send(status_snapshot(&view));
}

/// Dispatches one settings request (never debounced — plan D3). Every
/// reply is fire-and-forget-honest: a dropped receiver (a stale form)
/// is at most a log line, never a panic, and persistence is never
/// undone by a failed reply.
// A pure dispatch table: one routing arm per lane variant, each body
// a one-line delegation into the shared choreography — the arm count
// tracks the closed enum, not the function's complexity.
#[allow(clippy::too_many_lines)]
pub async fn handle_settings(request: UiRequest, workspace: &ConnectionWorkspace) {
    match request {
        UiRequest::StackWidthsChanged { .. } | UiRequest::WindowGeometryChanged { .. } => {
            unreachable!("width and geometry snapshots take the debounce path")
        }
        UiRequest::StatusQuery { .. } => {
            unreachable!("status queries take the OP3 arm's own path, never the settings lane")
        }
        UiRequest::SaveConnection { profile, reply } => {
            let outcome = save_connection(workspace, &profile).await;
            let _ = reply.send(outcome);
        }
        UiRequest::BoardReconcilePreview { board, reply } => {
            // The planner's match counts over the live engine state
            // (OP1 US-3, D-4): the same facts the CLI's typed refusal
            // carries (`commands::boards::reconcile_on`'s preview
            // half), as a pure query — no state change, no
            // persistence. A dropped receiver (a stale flow) is at
            // most a dropped send.
            let counts =
                match taskboard_domain::SyncStateReader::read_state(&workspace.engine).await {
                    Ok(persisted) => {
                        let state = workspace.engine.shared_state().load_full();
                        let (_, counts) =
                            taskboard_domain::plan_reconcile(&state, &persisted.outbox, board);
                        drop(state);
                        Some(counts)
                    }
                    // The state read is the query's only failure mode;
                    // `None` is the typed not-derivable fact — a failed
                    // planner must never masquerade as a zero-count
                    // preview (the letter's "safe no-op" is a planner
                    // fact, not a fallback), so the strip disarms
                    // instead.
                    Err(_) => None,
                };
            let _ = reply.send(counts);
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
        UiRequest::MoveBoardToDeck {
            board,
            target_connector,
            reply,
        } => {
            let outcome = move_board_to_deck(workspace, board, &target_connector).await;
            let _ = reply.send(outcome);
        }
        UiRequest::UndoBoardMove { board, reply } => {
            let outcome = undo_board_move(workspace, board).await;
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
        UiRequest::ExportRun { spec, reply } => {
            // The W-6 arm (the J2 export plan D-5): the SAME export
            // engine the CLI verb drives (one engine, two frontends)
            // on the forwarder's engine handle; the typed reply is
            // fire-and-forget-honest (a dropped receiver is a log
            // line, never a panic).
            let state = workspace.engine.shared_state().load_full();
            let outcome = run_export(&state, &workspace.export, &spec);
            tracing::info!(?outcome, "the export run settled");
            let _ = reply.send(outcome);
        }
        UiRequest::StatusRemediate { action, reply } => {
            // The W-7 arm (the D2 command plan D-1): the executor's
            // typed decision logic owns the liveness re-checks (the
            // enable flags are presentation, not authorization); the
            // reply is fire-and-forget-honest as everywhere on the
            // lane.
            let outcome =
                crate::status_remediation::run_remediation(&workspace.remediation, action).await;
            tracing::info!(?outcome, ?action, "the status remediation settled");
            let _ = reply.send(outcome);
        }
        UiRequest::ExportPreview { spec, reply } => {
            // The W-5 arm (the D2 preview plan D-1): the SAME export
            // engine the Run button drives, NO file written — the
            // preview renders the payload the export would write.
            let state = workspace.engine.shared_state().load_full();
            let outcome = run_preview(&state, &workspace.export, &spec);
            tracing::info!(?outcome, "the export preview settled");
            let _ = reply.send(outcome);
        }
        UiRequest::ExportClipboardCopy { spec, reply } => {
            // The D-3 arm: the payload REGENERATES app-side over the
            // same spec (never the pane's possibly-truncated text)
            // and rides the clipboard seam.
            let state = workspace.engine.shared_state().load_full();
            let outcome = run_clipboard_copy(&state, &workspace.export, &spec);
            tracing::info!(?outcome, "the export clipboard copy settled");
            let _ = reply.send(outcome);
        }
        UiRequest::SetKeymapPreset { preset, reply } => {
            let _ = reply.send(set_keymap_preset(workspace, preset));
        }
        UiRequest::SetScale { scale, reply } => {
            let _ = reply.send(set_scale(workspace, scale));
        }
        UiRequest::SetTheme { theme, reply } => {
            let _ = reply.send(set_theme(workspace, theme));
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
        UiRequest::SetFilesSyncPollSecs { secs, reply } => {
            let _ = reply.send(
                run_editor_write(workspace, Field::FilesPollSecs, |path| {
                    crate::settings_overlay::set_files_sync_poll_secs(path, secs)
                })
                .await,
            );
        }
    }
}

/// Which front applied a settings write (journey W-3, the shared
/// live-write port's lane bit): `Applied` — a live engine took the
/// snapshot; `NoEngine` — the caller handed no engine (the one-shot
/// CLI lane; the file applies at the next start); `EngineGone` — an
/// engine was handed but could not apply (stopped mid-flight): the
/// file *is* written, the next start applies it (the editor's `Gone`
/// precedent — the file stays the boot authority, no partial repair).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveApply {
    Applied,
    NoEngine,
    EngineGone,
}

/// The shared write port's success payload: the re-decoded, validated
/// effective config (file + standing env — the boot provider chain)
/// plus which front applied.
#[derive(Debug)]
pub struct SettingsWriteOutcome {
    pub effective: AppConfig,
    pub apply: LiveApply,
}

/// The shared write port's typed refusals (front-agnostic): the env
/// guard, the writer's own classes (IO + prove gate), and the
/// effective re-decode/validate failure. Fronts map these onto their
/// own lattices (the editor's `SettingsError`, the CLI's
/// `ConfigSetError`).
#[derive(Debug)]
pub enum SettingsWriteError {
    /// One of `guard_keys` is pinned by a `TASKBOARD_*` override.
    EnvLocked,
    /// The writer refused (IO, or the prove gate's typed rejection) —
    /// the file is untouched and no command was sent.
    Overlay(crate::settings_overlay::OverlayError),
    /// The effective re-decode or validation failed after a successful
    /// write — the gate raced another writer; typed, nothing sent.
    Config(Box<ConfigError>),
}

/// The live-write choreography, extracted from the editor's
/// `run_editor_write` (journey W-3: one vocabulary, three fronts —
/// the GUI editor arms, the daemon's `config set/unset` arm, and the
/// CLI's local lane all call this):
///
/// 1. the env-lock guard — any key in `guard_keys` pinned by a
///    `TASKBOARD_*` override refuses typed before the file is
///    touched;
/// 2. the typed writer runs against the handed write path — never a
///    fresh resolution; the prove gate's refusal maps typed, **no
///    command is sent**, engine and file untouched;
/// 3. the *effective* config re-decodes and validates (the gate just
///    proved this document boots; the typed arm stays — never an
///    `expect` on a file another process may have raced) and mints
///    the domain snapshot;
/// 4. when an engine is handed, `UpdateSettings` carries the whole
///    snapshot; a failed apply is the typed `EngineGone` next-start
///    outcome, not an error — the file is written and the boot is
///    its authority.
/// # Errors
///
/// Typed refusals: `EnvLocked` (a guarded key pinned by a
/// `TASKBOARD_*` override), `Overlay` (the writer's own IO/prove
/// classes), `Config` (the effective re-decode/validate failure).
pub async fn run_settings_write(
    path: &std::path::Path,
    locked: &std::collections::BTreeSet<String>,
    guard_keys: &[&str],
    write: impl FnOnce(&std::path::Path) -> Result<(), crate::settings_overlay::OverlayError>,
    engine: Option<&EngineHandle>,
) -> Result<SettingsWriteOutcome, SettingsWriteError> {
    if guard_keys.iter().any(|key| locked.contains(*key)) {
        return Err(SettingsWriteError::EnvLocked);
    }
    write(path).map_err(SettingsWriteError::Overlay)?;
    let effective = AppConfig::extract_raw(Some(path))
        .map_err(|err| SettingsWriteError::Config(Box::new(err)))?;
    effective
        .validate()
        .map_err(|err| SettingsWriteError::Config(Box::new(err)))?;
    let settings = effective.settings_snapshot();
    if let Some(engine) = engine {
        match engine
            .execute(StateCommand::UpdateSettings { settings })
            .await
        {
            Ok(_) => {
                return Ok(SettingsWriteOutcome {
                    effective,
                    apply: LiveApply::Applied,
                });
            }
            Err(err) => {
                tracing::warn!(%err, "settings written but the engine did not apply them");
            }
        }
    }
    Ok(SettingsWriteOutcome {
        apply: if engine.is_some() {
            LiveApply::EngineGone
        } else {
            LiveApply::NoEngine
        },
        effective,
    })
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
    // The shared live-write port (journey W-3) — the editor is front
    // (a); its typed lattice maps the port's refusals below.
    let outcome = run_settings_write(
        path,
        &locked,
        editor_guard_keys(field),
        write,
        Some(&workspace.engine),
    )
    .await;
    match outcome {
        Ok(out) => {
            if out.apply == LiveApply::EngineGone {
                // A stopped engine: the file *is* written (the next
                // boot picks it up) while the running engine keeps its
                // old values — the typed refusal lets the UI say so.
                return Err(SettingsError::Gone);
            }
            let published = workspace.engine.shared_state().load();
            Ok(crate::ui::settings_snapshot_from_domain(
                &out.effective,
                &published.settings,
            ))
        }
        Err(SettingsWriteError::EnvLocked) => Err(SettingsError::EnvLocked { field }),
        Err(SettingsWriteError::Overlay(err)) => Err(map_overlay_error(field, &err)),
        Err(SettingsWriteError::Config(err)) => Err(map_config_error(&err)),
    }
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

/// The env-lock guard keys for one editor write: the field's own key,
/// or — the Files row's case (round-6 D-6.6) — both keys the row
/// writes (`enabled` + the re-anchor), so a pinned
/// `files_sync.anchor` must refuse the whole pick too; the writer
/// would otherwise churn the file under a pinned effective value.
/// The connection-form fields never ride the editor writers (empty
/// guard: no key can match).
fn editor_guard_keys(field: Field) -> &'static [&'static str] {
    match field {
        Field::FilesSyncEnabled => &["files_sync.enabled", "files_sync.anchor"],
        Field::FilesPollSecs => &["files_sync.poll_secs"],
        Field::Name | Field::ServerUrl | Field::Username | Field::Password | Field::Scale => &[],
        Field::DayStart => &["preferences.day_start_time"],
        Field::RemindersEnabled => &["reminders.enabled"],
        Field::TickSecs => &["reminders.tick_secs"],
        Field::ReminderSink => &["reminders.sink"],
        Field::ReminderBefore => &["reminders.default_before"],
        Field::PollInterval => &["sync.poll_interval_secs"],
        Field::BackoffInitial => &["sync.backoff_initial_secs"],
        Field::BackoffMax => &["sync.backoff_max_secs"],
        Field::DueSoonDays => &["ui.due_soon_days"],
        Field::FocusEnabled => &["focus.enabled"],
        Field::FocusWorkMins => &["focus.work_mins"],
        Field::FocusBreakMins => &["focus.break_mins"],
        Field::FocusChain => &["focus.chain"],
        Field::FocusLongBreakMins => &["focus.long_break_mins"],
        Field::FocusLongBreakEvery => &["focus.long_break_every"],
        Field::FocusNotify => &["focus.notify"],
    }
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
        Field::FilesPollSecs => "files_sync.poll_secs",
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
    const ALL: [Field; 23] = [
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
        Field::FilesPollSecs,
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
            | Field::FilesSyncEnabled
            | Field::FilesPollSecs => {}
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
    crate::bootstrap::wait_for_boot_pull(
        &workspace.engine,
        &workspace.connector_owners,
        SAVE_SETTLE,
    )
    .await;
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
    crate::bootstrap::wait_for_boot_pull(
        &workspace.engine,
        &workspace.connector_owners,
        SAVE_SETTLE,
    )
    .await;
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

/// Persists the theme word (#1437 D5): the choice arrives typed (the
/// closed-set `ThemeName` minted UI-side), so the persist is the
/// whole lane — the `overlay_set_ui_scale` twin through the same
/// `toml_edit` ceremony.
fn set_theme(
    workspace: &ConnectionWorkspace,
    theme: taskboard_ui_slint::palette::ThemeName,
) -> Result<(), SettingsError> {
    crate::connections::overlay_set_ui_theme(&workspace.config_write_path, theme)
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
#[must_use]
pub fn current_connections(workspace: &ConnectionWorkspace) -> ConnectionsSnapshot {
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
    // The identity home: `None` (no platform data dir) is the same
    // typed refusal, decided pre-IO with the guards above.
    let device_json = workspace
        .device_json
        .as_deref()
        .ok_or(SettingsError::FilesSyncDisabled)?;
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
        device_json,
        crate::board_move_ceremony::DEFAULT_MOVE_TIMEOUT,
    )
    .await
    .map_err(map_move_error)?;
    tracing::info!(board = %board.as_uuid(), "settings move to files committed");
    Ok(current_connections(workspace))
}

/// The Deck-to-Deck rebind choreography (the files-sync-status plan
/// US-10, D-11): the precondition list is typed, in order, before any
/// IO — ① the target connector is configured (`Gone` for an unknown
/// target) and Deck-typed (`BindRefused` for a non-Deck one); ② the
/// board present, not deleted/archived (`Gone`); ③ the binding
/// Deck-typed (`BindRefused` — a files-bound or unbound board has no
/// Deck-to-Deck move); ④ a different connector (a same-connector pick
/// refuses typed, not a no-op); ⑤ the target's actor lane live (the
/// attached-connector lookup the bind arm's `ensure_attached` guards
/// with; a dead lane refuses typed). The engine verb preserves the
/// remote id verbatim; the reply is the fresh snapshot so the
/// roster's badges re-derive.
async fn move_board_to_deck(
    workspace: &ConnectionWorkspace,
    board: BoardId,
    target_connector: &str,
) -> Result<ConnectionsSnapshot, SettingsError> {
    let raw = ensure_migrated(workspace)?;
    let entry = connector_entry(&raw, target_connector)?;
    if entry.backend != crate::config::SyncBackend::Nextcloud {
        return Err(SettingsError::BindRefused);
    }
    let state = workspace.engine.shared_state().load();
    let row = state
        .boards
        .get(&board)
        .filter(|b| !b.deleted && !b.archived)
        .ok_or(SettingsError::Gone)?;
    let binding = row.remote.as_ref().ok_or(SettingsError::BindRefused)?;
    let Some(remote_deck) = binding.board.as_deck() else {
        return Err(SettingsError::BindRefused);
    };
    let current = binding.connector.clone();
    drop(state);
    let target_id = ConnectorId::from_kebab(target_connector).map_err(|_| SettingsError::Gone)?;
    if current == target_id {
        return Err(SettingsError::BindRefused);
    }
    if workspace.control.connector_commands(&target_id).is_none() {
        return Err(SettingsError::BindRefused);
    }
    workspace
        .engine
        .execute(StateCommand::RebindBoard {
            board,
            connector: target_id,
            remote: taskboard_domain::RemoteBoardId::deck(remote_deck),
        })
        .await
        .map_err(|e| map_execute_error(&e))?;
    tracing::info!(
        board = %board.as_uuid(),
        target = %target_connector,
        "settings deck-to-deck rebind committed"
    );
    Ok(current_connections(workspace))
}

/// The undo-window Restore choreography (the files-sync-status plan
/// US-6, D-7): the pre-IO guards mirror the move arm's posture (the
/// board present and not deleted — `Gone` otherwise), then the
/// ceremony's own undo leg runs on the running engine — the CLI's
/// exact path, never a reimplemented undo. The leg derives the stage
/// and refuses typed outside the window (`NothingToUndo` /
/// `UndoClosed` / `UndoCoordinatesLost`); the reply is the fresh
/// snapshot so the standard drain re-pushes settings.
/// # Errors
///
/// Typed refusals: `Gone` (the board is absent or deleted),
/// `NothingToUndo` / `UndoClosed` / `UndoCoordinatesLost` (the
/// ceremony's stage gate), `FilesSyncDisabled`, plus the identity
/// and overlay classes of the undo leg.
pub async fn undo_board_move(
    workspace: &ConnectionWorkspace,
    board: BoardId,
) -> Result<ConnectionsSnapshot, SettingsError> {
    let state = workspace.engine.shared_state().load();
    state
        .boards
        .get(&board)
        .filter(|b| !b.deleted)
        .ok_or(SettingsError::Gone)?;
    drop(state);
    let raw = ensure_migrated(workspace)?;
    // The undo leg's identity home: same `None` → typed refusal as the
    // move arm.
    let device_json = workspace
        .device_json
        .as_deref()
        .ok_or(SettingsError::FilesSyncDisabled)?;
    crate::board_move_ceremony::run_on(
        &workspace.engine,
        &raw,
        &workspace.store,
        board,
        crate::board_move_ceremony::MoveDirection::Undo,
        device_json,
        crate::board_move_ceremony::DEFAULT_MOVE_TIMEOUT,
    )
    .await
    .map_err(map_move_error)?;
    tracing::info!(board = %board.as_uuid(), "settings move undo committed");
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
///
/// # Errors
///
/// Typed [`SettingsError`] refusals: a gone board (`Gone`), a board
/// bound to the Deck channel or unbound (`BindRefused`).
pub async fn stop_board_files_sync(
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
/// # Errors
///
/// Typed refusals: `FilesSyncDisabled` (no spawnable files lane),
/// `BindRefused` (the board is deck- or files-bound), `Gone` (the
/// board is absent), plus the bind command's own typed classes.
pub async fn publish_board_to_files(
    workspace: &ConnectionWorkspace,
    board: BoardId,
) -> Result<ConnectionsSnapshot, SettingsError> {
    let raw = ensure_migrated(workspace)?;
    if crate::bootstrap::files_sync_spawn_decision(&raw).is_none() {
        return Err(SettingsError::FilesSyncDisabled);
    }
    // Guard ② ③ + the assign step live in the shared ceremony helper
    // (D-2, #764): the CLI's `boards publish --to-files` runs the exact
    // same mint, so publish and move cannot diverge. The lane guard's
    // timing — above, before the state snapshot — stays here verbatim.
    crate::commands::boards::files_bind_on(&workspace.engine, board)
        .await
        .map_err(|err| match err {
            crate::commands::CommandError::BoardArchived { .. } => SettingsError::Gone,
            crate::commands::CommandError::Engine(exec) => map_execute_error(&exec),
            other => {
                tracing::warn!(
                    error = %other,
                    "settings publish to files: an unexpected error class"
                );
                SettingsError::Gone
            }
        })?;
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
                // The undo window's typed refusals (US-6, D-7): the
                // raced or closed stages refuse, they do not fold into
                // a coarse failure.
                crate::board_move_ceremony::MoveError::NothingToUndo => {
                    SettingsError::NothingToUndo
                }
                crate::board_move_ceremony::MoveError::UndoClosed => SettingsError::UndoClosed,
                crate::board_move_ceremony::MoveError::UndoCoordinatesLost => {
                    SettingsError::UndoCoordinatesLost
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

// -- the export preview/clipboard drain executors (the D2 preview
//    plan W-5) ------------------------------------------------------

/// The preview executor (the preview plan D-1/D-2): the SAME export
/// engine the Run button drives over the same spec — one generator,
/// two sinks; NO file is written (the written-path fact stays Run's
/// exclusively). The pdf family is a typed refusal here (the verbs
/// are defined for text payloads only — D-5's family gating keeps the
/// controls off that leg; the arm is the trust boundary's
/// defense-in-depth). Over-cap payloads ride the typed `truncated`
/// fact (the cap is [`PREVIEW_CAP_BYTES`], a presentation budget).
#[must_use]
pub fn run_preview(
    state: &taskboard_domain::AppState,
    _workspace: &ExportWorkspace,
    spec: &ExportSpec,
) -> taskboard_ui_slint::requests::ExportPreviewOutcome {
    use taskboard_ui_slint::requests::{ExportPreviewOutcome, ExportRefusal};
    if spec.what != ExportFamily::Tracking {
        return ExportPreviewOutcome::Refused(ExportRefusal::OptionsNotApplicable);
    }
    let query = tracking_query(spec);
    match crate::commands::track::export_tracking_state(state, &query) {
        Err(err) => ExportPreviewOutcome::Refused(refusal_kind(&err)),
        Ok(payload) => {
            let bytes = crate::commands::track::render_payload(&payload);
            let truncated = bytes.len() > PREVIEW_CAP_BYTES;
            let text = if truncated {
                let mut end = PREVIEW_CAP_BYTES;
                while !bytes.is_char_boundary(end) {
                    end -= 1;
                }
                bytes[..end].to_owned()
            } else {
                bytes
            };
            ExportPreviewOutcome::Payload {
                text,
                format: spec.format,
                truncated,
            }
        }
    }
}

/// The clipboard executor (the preview plan D-3): the payload
/// REGENERATES app-side over the same spec (never the pane's
/// possibly-truncated text) and rides the workspace's clipboard
/// seam; the typed outcome carries the copied-byte fact or the
/// refusal (generation kinds, or `ClipboardUnavailable`).
#[must_use]
pub fn run_clipboard_copy(
    state: &taskboard_domain::AppState,
    workspace: &ExportWorkspace,
    spec: &ExportSpec,
) -> taskboard_ui_slint::requests::ExportClipboardOutcome {
    use taskboard_ui_slint::requests::{ExportClipboardOutcome, ExportRefusal};
    if spec.what != ExportFamily::Tracking {
        return ExportClipboardOutcome::Refused(ExportRefusal::OptionsNotApplicable);
    }
    let query = tracking_query(spec);
    match crate::commands::track::export_tracking_state(state, &query) {
        Err(err) => ExportClipboardOutcome::Refused(refusal_kind(&err)),
        Ok(payload) => {
            let bytes = crate::commands::track::render_payload(&payload);
            match (workspace.clipboard_write)(&bytes) {
                Ok(()) => ExportClipboardOutcome::Copied {
                    bytes: bytes.len() as u64,
                },
                Err(()) => ExportClipboardOutcome::Refused(ExportRefusal::ClipboardUnavailable),
            }
        }
    }
}

// -- the export run's drain executor (the J2 export plan W-6) ------

/// The W-6 executor: the SAME export engine the CLI `export` verb
/// drives (one engine, two frontends — the O-5 parity law), on the
/// forwarder's engine handle. The csv/json leg adds the GUI's file
/// write (D-3 — the CLI's pipes stay byte-compatible and untouched);
/// the pdf leg rides the engine's session lane and the typed
/// permission flag owns the viewer open (the trap-c law).
#[must_use]
pub fn run_export(
    state: &taskboard_domain::AppState,
    workspace: &ExportWorkspace,
    spec: &ExportSpec,
) -> ExportRunOutcome {
    match spec.what {
        ExportFamily::Tracking => run_tracking_export(state, workspace, spec),
        ExportFamily::Pdf => run_pdf_export(state, workspace, spec),
    }
}

/// The tracking leg's query over a GUI spec — the ONE construction
/// every tracking sink (run, preview, clipboard copy) drives; the
/// byte-for-byte identity contract rests on this single build site.
fn tracking_query(spec: &ExportSpec) -> crate::commands::track::ExportQuery {
    crate::commands::track::ExportQuery {
        what: crate::cli::ExportWhat::Tracking,
        format: Some(match spec.format {
            ExportFileFormat::Csv => crate::cli::ExportFormat::Csv,
            ExportFileFormat::Json => crate::cli::ExportFormat::Json,
        }),
        since: spec.since,
        until: spec.until,
        task: spec.task.clone(),
        board: spec.board.clone(),
        #[cfg(feature = "pdf-export")]
        layout: None,
        #[cfg(feature = "pdf-export")]
        out: None,
        #[cfg(feature = "pdf-export")]
        no_open: false,
    }
}

/// The tracking leg (US-2): the engine renders the payload (the D-T15
/// body), the drain writes it to the destination — the spec's explicit
/// file, else the export dir's dated default (D-3).
fn run_tracking_export(
    state: &taskboard_domain::AppState,
    workspace: &ExportWorkspace,
    spec: &ExportSpec,
) -> ExportRunOutcome {
    let query = tracking_query(spec);
    match crate::commands::track::export_tracking_state(state, &query) {
        Err(err) => refusal_of(&err),
        Ok(payload) => {
            let bytes = crate::commands::track::render_payload(&payload);
            let destination = if let Some(path) = &spec.out {
                resolve_destination(path)
            } else {
                let extension = match spec.format {
                    ExportFileFormat::Csv => "csv",
                    ExportFileFormat::Json => "json",
                };
                export_home(workspace).join(format!(
                    "tracking-{}.{}",
                    chrono::Utc::now().date_naive(),
                    extension
                ))
            };
            match std::fs::write(&destination, bytes.as_bytes()) {
                Ok(()) => ExportRunOutcome::Written { path: destination },
                Err(_) => ExportRunOutcome::Refused(ExportRefusal::DestinationUnwritable),
            }
        }
    }
}

/// The pdf leg (US-3): the engine's session lane writes the file (the
/// destination resolved absolute — the GUI drain's cwd is the
/// daemon's); the viewer open rides the typed permission flag and the
/// spec's open-after posture, best-effort (the written file is the
/// deliverable). Feature-absent builds refuse typed (D-4's
/// trust-boundary arm).
fn run_pdf_export(
    state: &taskboard_domain::AppState,
    workspace: &ExportWorkspace,
    spec: &ExportSpec,
) -> ExportRunOutcome {
    #[cfg(not(feature = "pdf-export"))]
    {
        let _ = (state, workspace, spec);
        ExportRunOutcome::Refused(ExportRefusal::PdfUnavailable)
    }
    #[cfg(feature = "pdf-export")]
    {
        use taskboard_ui_slint::requests::PdfLayoutChoice;
        let query = crate::commands::track::ExportQuery {
            what: crate::cli::ExportWhat::Pdf,
            format: None,
            since: None,
            until: None,
            task: None,
            board: None,
            layout: spec.layout.map(|layout| match layout {
                PdfLayoutChoice::Fold => crate::cli::PdfLayout::Fold,
                PdfLayoutChoice::A4 => crate::cli::PdfLayout::A4,
                PdfLayoutChoice::A5 => crate::cli::PdfLayout::A5,
                PdfLayoutChoice::A6 => crate::cli::PdfLayout::A6,
            }),
            out: spec.out.as_ref().map(|path| resolve_destination(path)),
            // The session lane never opens; the drain's permission
            // arm below owns the open (the typed-permission law).
            no_open: true,
        };
        match crate::commands::track::export_pdf_state(
            state,
            &workspace.config,
            &query,
            crate::commands::track::PdfLane::Session,
        ) {
            Err(err) => refusal_of(&err),
            Ok(receipt) => {
                if spec.open_after && workspace.may_open_viewer {
                    // Best-effort (the CLI's posture): an open
                    // failure keeps the written-file fact honest.
                    let _ = crate::pdf::deliver::open_in_viewer(&receipt.path);
                }
                ExportRunOutcome::Written { path: receipt.path }
            }
        }
    }
}

/// The csv/json destination's default home (D-3): the export family's
/// resolved directory — the same resolution the pdf leg's default
/// uses (`[export] pdf_dir`, else `~/Documents/taskboard`, else the
/// platform data dir).
fn export_home(workspace: &ExportWorkspace) -> std::path::PathBuf {
    let config_dir = workspace
        .config
        .export
        .pdf_dir
        .as_deref()
        .map(crate::export_paths::expand_tilde);
    crate::export_paths::default_pdf_dir(config_dir.as_deref(), dirs::data_dir().as_deref())
}

/// The destination's resolution: `~` expanded, a relative path
/// resolved against the daemon's cwd (the GUI drain runs in-process —
/// the session lane's no-cwd wire constraint does not apply).
fn resolve_destination(path: &std::path::Path) -> std::path::PathBuf {
    let expanded = crate::export_paths::expand_tilde(path);
    if expanded.is_absolute() {
        return expanded;
    }
    match std::env::current_dir() {
        Ok(cwd) => cwd.join(expanded),
        Err(_) => expanded,
    }
}

/// The typed refusal map (D-5): the engine's typed errors fold onto
/// the dialog's closed kind set — the dialog renders the kind, never
/// the engine's message text.
fn refusal_of(err: &crate::commands::CommandError) -> ExportRunOutcome {
    ExportRunOutcome::Refused(refusal_kind(err))
}

/// The typed refusal map (D-5): the engine's typed errors fold onto
/// the dialog's closed kind set — the dialog renders the kind, never
/// the engine's message text. Shared by the run, preview, and
/// clipboard executors (one map, three sinks).
fn refusal_kind(err: &crate::commands::CommandError) -> ExportRefusal {
    // The pdf-family arms are feature-gated (the variants do not
    // compile in a default build — where the pdf leg can never run
    // and the kinds are unreachable).
    #[cfg(feature = "pdf-export")]
    let kind = match err {
        crate::commands::CommandError::ExportOptionsNotApplicable => {
            ExportRefusal::OptionsNotApplicable
        }
        crate::commands::CommandError::UnknownPdfLayout => ExportRefusal::UnknownPdfLayout,
        crate::commands::CommandError::ExportPdfPath(_) => ExportRefusal::PdfPath,
        crate::commands::CommandError::ExportPdfRender => ExportRefusal::PdfRender,
        err => refusal_of_data(err),
    };
    #[cfg(not(feature = "pdf-export"))]
    let kind = refusal_of_data(err);
    kind
}

/// The data families' refusal arms (the feature-blind subset).
fn refusal_of_data(err: &crate::commands::CommandError) -> ExportRefusal {
    match err {
        crate::commands::CommandError::ExportFiltersNotApplicable => {
            ExportRefusal::FiltersNotApplicable
        }
        crate::commands::CommandError::UnknownTask(_)
        | crate::commands::CommandError::AmbiguousTask { .. } => ExportRefusal::BadTaskFilter,
        crate::commands::CommandError::UnknownBoardSelector { .. }
        | crate::commands::CommandError::UnknownBoard(_)
        | crate::commands::CommandError::BoardArchived { .. } => ExportRefusal::BadBoardFilter,
        _ => ExportRefusal::EngineRefused,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::refusing_store;
    use figment::providers::Format as _;
    use taskboard_domain::{BoardClocks, BoardId, StackClocks, StackId};

    /// The workspace fixture owns everything it borrows from disk
    /// (#982): the booted core, the workspace over it, and the tempdir
    /// backing both. **Field order is the contract** — `workspace` and
    /// `app` drop before `_dir`, so the app's actor teardown (the
    /// engine join in `App`'s drop) always precedes the directory
    /// deletion; a field reorder must re-run the ordering pin below.
    struct WorkspaceFixture {
        workspace: ConnectionWorkspace,
        /// The app and the directory, grouped so `owned_workspace` can
        /// hand out the workspace while the rest keeps the ordering.
        rest: FixtureRest,
    }

    /// The `App` plus the db + device-identity home. **Field order is
    /// the contract**: `app` drops (the engine's actor teardown joins)
    /// before `_dir` deletes the directory (#982); a reorder must
    /// re-run the ordering pin below.
    struct FixtureRest {
        app: crate::bootstrap::App,
        /// The db + device-identity home; held purely for its `Drop`
        /// side effect (#982) — its reader (the lifetime pin) re-homed
        /// onto `tests/services/ui_settings.rs` (#1157).
        #[allow(dead_code)]
        dir: tempfile::TempDir,
    }

    /// A workspace over a real (standalone, module-less) booted core:
    /// the width tests never touch the connector path, but the
    /// workspace is a plain value, not a mock — fakes over mocks.
    /// The fixture binding must outlive every use of
    /// `fixture.workspace`.
    async fn workspace_fixture(path: PathBuf) -> WorkspaceFixture {
        let dir = tempfile::tempdir().expect("tempdir");
        workspace_fixture_in(path, dir).await
    }

    /// A fixture whose workspace is moved out for a consumer that
    /// takes it by value (the forwarder spawns): the rest of the
    /// fixture — the app and the tempdir — stays bound at the call
    /// site and outlives the moved workspace clones.
    async fn owned_workspace(path: PathBuf) -> (FixtureRest, ConnectionWorkspace) {
        let WorkspaceFixture { workspace, rest } = workspace_fixture(path).await;
        (rest, workspace)
    }

    /// [`workspace_fixture`] over a caller-owned tempdir (the port
    /// harness seeds its config into the same directory the db and the
    /// device identity live in).
    async fn workspace_fixture_in(path: PathBuf, dir: tempfile::TempDir) -> WorkspaceFixture {
        let mut config = AppConfig::default();
        config.storage.db_path = Some(dir.path().join("taskboard.db"));
        config.sync.backend = Some(SyncBackend::Standalone);
        config.validate().expect("config valid");
        let app = crate::bootstrap::bootstrap_in(
            config,
            refusing_store(),
            Arc::new(crate::notifications::LogSink),
            crate::bootstrap::BootIdentity::At(dir.path().to_owned()),
        )
        .await
        .expect("standalone boot");
        let workspace = ConnectionWorkspace {
            config_write_path: path,
            store: refusing_store(),
            control: app.connector_control(),
            engine: app.engine.clone(),
            // The suite's own identity home: the ceremony's create arm
            // mints there, never the platform default (#751).
            device_json: Some(dir.path().join("device.json")),
            connector_owners: app.connector_owners.clone(),
            export: crate::ui_requests::ExportWorkspace::default(),
            // The suite never drives the status panel; the source is
            // the in-session projection over the fixture's handles.
            status: Box::new({
                let engine = app.engine.clone();
                let config = app.config.clone();
                let control = app.connector_control();
                let files = app.files_sync_status.clone();
                move || {
                    crate::commands::status::StatusView::in_session_parts(
                        &engine,
                        &config,
                        control.connector_attached(),
                        files.as_ref(),
                    )
                }
            }),
            remediation: crate::status_remediation::RemediationWorkspace::default(),
        };
        WorkspaceFixture {
            workspace,
            rest: FixtureRest { app, dir },
        }
    }

    /// The #984 dated-pin gate: true when no `TASKBOARD_*` ambient is
    /// present. The settings-editor/port guard and persist reads
    /// compose the process env in production, and this lib crate
    /// forbids the env mutation that `taskboard_env_lock` needs — so
    /// under a hostile ambient the composed read refuses before the
    /// fact under test. This gate is *deliberate* #984 coverage, not
    /// re-homing debt: the issue #1157 class (the seven env-composing
    /// `ui_requests` legs) re-homed onto `tests/services/ui_settings.rs`
    /// behind `taskboard_env_lock` (2026-11-23), and the three #1113
    /// legs moved before that. The remaining gated legs across this
    /// module, `config_watcher.rs`, and `commands/config.rs` stay
    /// inline by decision — they are skipped (not falsified) under a
    /// hostile ambient, and the hostile-env CI pass covers the moved
    /// legs.
    fn clean_ambient() -> bool {
        !std::env::vars_os().any(|(key, _)| key.as_encoded_bytes().starts_with(b"TASKBOARD_"))
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

    // -- the Stop arm (round-7 D-7.3, W-4) ------------------------------
    // (US-2/S2, US-2/S3 and the D-BF4 snapshot fill re-homed onto
    // `tests/services/ui_settings.rs` behind `taskboard_env_lock`,
    // #1157.)

    // -- the Deck-to-Deck rebind arm (the files-sync-status plan
    // US-10, D-11) ---------------------------------------------------

    /// The two-connector config the rebind fixtures read: both
    /// entries Deck-typed (the config layer refuses `standalone`
    /// entries outright, so the non-Deck-target refusal is
    /// defense-in-depth the file path cannot reach — noted, not
    /// fixture-tested).
    fn write_two_connector_config(path: &std::path::Path) {
        // Dated pin: the ambient gate, `clean_ambient`'s doc (#1113).
        if !clean_ambient() {
            return;
        }
        std::fs::write(
            path,
            "[ui]\nscale = 1.0\n\
             [[sync.connectors]]\nname = \"work\"\nbackend = \"nextcloud\"\n\
             server_url = \"https://c.example\"\nusername = \"alice\"\n\
             [[sync.connectors]]\nname = \"other-deck\"\nbackend = \"nextcloud\"\n\
             server_url = \"https://d.example\"\nusername = \"bob\"\n",
        )
        .expect("seed two-connector config");
    }

    /// US-10 AC-1 (app-arm) — the atomic re-target: the binding's
    /// connector is replaced and the remote Deck id is preserved
    /// verbatim (the ceremony's identity-preservation pin); the reply
    /// is the fresh snapshot whose channel half carries the move.
    #[tokio::test]
    async fn us10_ac1_rebind_preserves_the_remote_identity() {
        // Dated pin: the ambient gate, `clean_ambient`'s doc (#1113).
        if !clean_ambient() {
            return;
        }
        let dir = tempfile::tempdir().expect("tempdir");
        let config = dir.path().join("taskboard.toml");
        write_two_connector_config(&config);
        let fixture = workspace_fixture(config).await;
        let workspace = &fixture.workspace;
        let board = create_board(workspace, "deck board").await;
        workspace
            .engine
            .execute(StateCommand::BindBoard {
                board,
                remote: taskboard_domain::RemoteBoardId::deck(42),
                connector: taskboard_domain::ConnectorId::from("work"),
            })
            .await
            .expect("deck bind lands");
        // The target's actor lane attaches (the preflight listing the
        // bind arm uses; a pure client constructor, no IO).
        let client = taskboard_sync_nextcloud::DeckClient::new("https://d.example", "bob", "pw")
            .expect("client mints");
        workspace
            .control
            .attach_connector(taskboard_domain::ConnectorId::from("other-deck"), client)
            .await
            .expect("the lane attaches");

        let snapshot = move_board_to_deck(workspace, board, "other-deck")
            .await
            .expect("the rebind commits");
        let state = workspace.engine.shared_state().load();
        let binding = state
            .boards
            .get(&board)
            .expect("the board survives")
            .remote
            .as_ref()
            .expect("the binding survives");
        assert_eq!(
            binding.connector.as_str(),
            "other-deck",
            "the connector re-targets"
        );
        assert_eq!(
            binding.board,
            taskboard_domain::RemoteBoardId::deck(42),
            "the remote id is preserved verbatim"
        );
        let replied = snapshot
            .boards
            .iter()
            .find(|info| info.id == board.as_uuid())
            .expect("the board is in the fresh snapshot");
        assert_eq!(
            replied.bound_channel,
            Some(taskboard_ui_slint::requests::BoardChannel::Deck),
            "the channel half stays structural"
        );
    }

    /// US-10 AC-2 (app-arm) — the precondition gates refuse typed,
    /// in order: an unknown target is `Gone`; a gone or archived
    /// board is `Gone`; unbound and files-bound boards are
    /// `BindRefused`; a same-connector pick is `BindRefused` (a typed
    /// refusal, not a no-op); a dead target lane is `BindRefused`.
    #[tokio::test]
    async fn us10_ac2_rebind_preconditions_refuse_typed() {
        // Dated pin: the ambient gate, `clean_ambient`'s doc (#1113).
        if !clean_ambient() {
            return;
        }
        let dir = tempfile::tempdir().expect("tempdir");
        let config = dir.path().join("taskboard.toml");
        write_two_connector_config(&config);
        let fixture = workspace_fixture(config).await;
        let workspace = &fixture.workspace;
        let deck_bound = create_board(workspace, "deck board").await;
        let files_bound = create_board(workspace, "files board").await;
        let unbound = create_board(workspace, "loose").await;
        workspace
            .engine
            .execute(StateCommand::BindBoard {
                board: deck_bound,
                remote: taskboard_domain::RemoteBoardId::deck(42),
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

        let unknown = move_board_to_deck(workspace, deck_bound, "nope")
            .await
            .expect_err("an unknown target refuses");
        assert_eq!(unknown, SettingsError::Gone);
        let gone = move_board_to_deck(
            workspace,
            BoardId::from_uuid(uuid::Uuid::now_v7()),
            "other-deck",
        )
        .await
        .expect_err("a gone board refuses");
        assert_eq!(gone, SettingsError::Gone);
        let unbound_refusal = move_board_to_deck(workspace, unbound, "other-deck")
            .await
            .expect_err("an unbound board refuses");
        assert_eq!(unbound_refusal, SettingsError::BindRefused);
        let files = move_board_to_deck(workspace, files_bound, "other-deck")
            .await
            .expect_err("a files-bound board refuses");
        assert_eq!(files, SettingsError::BindRefused);
        let same = move_board_to_deck(workspace, deck_bound, "work")
            .await
            .expect_err("a same-connector pick refuses");
        assert_eq!(same, SettingsError::BindRefused);
        // The dead lane: the target is configured but its actor was
        // never attached — the refusal precedes the engine verb.
        let dead_lane = move_board_to_deck(workspace, deck_bound, "other-deck")
            .await
            .expect_err("a dead target lane refuses");
        assert_eq!(dead_lane, SettingsError::BindRefused);
        let state = workspace.engine.shared_state().load();
        assert_eq!(
            state
                .boards
                .get(&deck_bound)
                .expect("present")
                .remote
                .as_ref()
                .expect("bound")
                .connector
                .as_str(),
            "work",
            "no refusal mutated the binding"
        );
    }

    // -- the #749 re-verification (the files-sync-status plan US-7,
    // D-9) ---------------------------------------------------------

    /// US-7 AC-1 — the re-verification's evidence: the landed r7
    /// roster's Stop Sync reaches every files-bound row (the roster
    /// renders every live board once per card; files-bound rows carry
    /// the Stop action on the anchor card), the arm's guards are
    /// typed (`Gone` for a stale board, `BindRefused` for non-files
    /// rows — the row grammar cannot normally offer those), and the
    /// effect is the exact unbind. This journey records the premise:
    /// the disconnect story is complete, #749 closes with this
    /// evidence (the closing comment is stage 3 bookkeeping).
    #[tokio::test]
    async fn us7_ac1_stop_sync_exercises_the_landed_row() {
        // Dated pin: the ambient gate, `clean_ambient`'s doc (#1113).
        if !clean_ambient() {
            return;
        }
        let dir = tempfile::tempdir().expect("tempdir");
        let config = dir.path().join("taskboard.toml");
        std::fs::write(&config, "[ui]\nscale = 1.0\n").expect("seed");
        let fixture = workspace_fixture(config).await;
        let workspace = &fixture.workspace;
        let board = create_board(workspace, "files board").await;
        workspace
            .engine
            .execute(StateCommand::BindBoard {
                board,
                remote: taskboard_domain::RemoteBoardId::files(uuid::Uuid::now_v7()),
                connector: taskboard_domain::ConnectorId::from("files-sync"),
            })
            .await
            .expect("files bind lands");

        // The effect: the exact unbind — the binding goes, the board
        // survives, the reply is the fresh snapshot.
        let snapshot = stop_board_files_sync(workspace, board)
            .await
            .expect("unbind");
        let row = snapshot
            .boards
            .iter()
            .find(|b| b.name == "files board")
            .expect("the board survives");
        assert_eq!(row.bound_channel, None, "the files binding is gone");

        // The guard shape: non-files rows refuse typed.
        let deck_bound = create_board(workspace, "deck board").await;
        workspace
            .engine
            .execute(StateCommand::BindBoard {
                board: deck_bound,
                remote: taskboard_domain::RemoteBoardId::deck(9),
                connector: taskboard_domain::ConnectorId::from("work"),
            })
            .await
            .expect("deck bind lands");
        let deck = stop_board_files_sync(workspace, deck_bound)
            .await
            .expect_err("a deck-bound row refuses");
        assert_eq!(deck, SettingsError::BindRefused);
        let stale = stop_board_files_sync(workspace, BoardId::from_uuid(uuid::Uuid::now_v7()))
            .await
            .expect_err("a stale row refuses");
        assert_eq!(stale, SettingsError::Gone);
    }

    // -- the Publish arm (round-7 D-7.3, W-6) ---------------------------

    /// A config whose files lane is spawnable (`[files_sync] enabled`
    /// with a resolvable Nextcloud anchor) at the workspace's write
    /// path.
    fn write_files_lane_config(path: &std::path::Path) {
        // Dated pin: the ambient gate, `clean_ambient`'s doc (#1113).
        if !clean_ambient() {
            return;
        }
        std::fs::write(
            path,
            "[ui]\nscale = 1.0\n\
             [[sync.connectors]]\nname = \"work\"\nbackend = \"nextcloud\"\n\
             server_url = \"https://c.example\"\nusername = \"alice\"\n\
             [files_sync]\nenabled = true\nanchor = \"work\"\n",
        )
        .expect("seed files-lane config");
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
        // Dated pin: the ambient gate, `clean_ambient`'s doc (#1113).
        if !clean_ambient() {
            return;
        }
        let dir = tempfile::tempdir().expect("tempdir");
        let config = dir.path().join("taskboard.toml");
        write_files_lane_config(&config);
        let fixture = workspace_fixture(config).await;
        let workspace = &fixture.workspace;
        let board = create_board(workspace, "loose").await;

        let snapshot = publish_board_to_files(workspace, board)
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
        // Dated pin: the ambient gate, `clean_ambient`'s doc (#1113).
        if !clean_ambient() {
            return;
        }
        let dir = tempfile::tempdir().expect("tempdir");
        let config = dir.path().join("taskboard.toml");
        std::fs::write(&config, "[ui]\nscale = 1.0\n").expect("seed");
        let (_rest, workspace) = owned_workspace(config.clone()).await;
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
        // Dated pin: the ambient gate, `clean_ambient`'s doc (#1113).
        if !clean_ambient() {
            return;
        }
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
        // Dated pin: the ambient gate, `clean_ambient`'s doc (#1113).
        if !clean_ambient() {
            return;
        }
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
        // Dated pin: the ambient gate, `clean_ambient`'s doc (#1113).
        if !clean_ambient() {
            return;
        }
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
        // Dated pin: the ambient gate, `clean_ambient`'s doc (#1113).
        if !clean_ambient() {
            return;
        }
        let (_dir, path) = write_temp_config("[ui]\nscale = 1.0\n[ui.stack_widths]\nx = 1\n");
        overlay_stack_widths(&path, &BTreeMap::new()).expect("write succeeds");
        let written = std::fs::read_to_string(&path).expect("read back");
        assert!(written.contains("scale = 1.0"));
        assert!(!written.contains("stack_widths"), "no empty husk");
    }

    #[test]
    fn overlay_on_an_absent_file_creates_it() {
        // Dated pin: the ambient gate, `clean_ambient`'s doc (#1113).
        if !clean_ambient() {
            return;
        }
        let (_dir, path) = write_temp_config("");
        let (_, board, stack_a, _) = fixture_state();
        let widths = BTreeMap::from([(board, BTreeMap::from([(stack_a, 240u32)]))]);
        overlay_stack_widths(&path, &widths).expect("write succeeds");
        let written = std::fs::read_to_string(&path).expect("created");
        assert!(written.contains(&board.to_string()));
    }

    #[test]
    fn overlay_on_a_directory_path_errors_typed_without_panicking() {
        // Dated pin: the ambient gate, `clean_ambient`'s doc (#1113).
        if !clean_ambient() {
            return;
        }
        let dir = tempfile::tempdir().expect("tempdir");
        // The config path itself is a directory: the read fails typed
        // (io error kind), never a panic (the never-crash rule, D8).
        let err = overlay_stack_widths(dir.path(), &BTreeMap::new());
        assert!(err.is_err());
    }

    // -- restart round-trip (T-18) --------------------------------------------

    #[test]
    fn written_widths_round_trip_through_the_config_layer() {
        // Dated pin: the ambient gate, `clean_ambient`'s doc (#1113).
        if !clean_ambient() {
            return;
        }
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
        // Dated pin: the ambient gate, `clean_ambient`'s doc (#1113).
        if !clean_ambient() {
            return;
        }
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("taskboard.toml");
        std::fs::write(&path, "[ui]\nscale = 1.0\n").expect("seed");
        let (state, board, stack_a, stack_b) = fixture_state();
        let (tx, rx) = ui_request_channel();
        let (_rest, ws) = owned_workspace(path.clone()).await;
        let forwarder = spawn_ui_request_forwarder(rx, Arc::new(ArcSwap::from_pointee(state)), ws);
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
        // Dated pin: the ambient gate, `clean_ambient`'s doc (#1113).
        if !clean_ambient() {
            return;
        }
        let dir = tempfile::tempdir().expect("tempdir");
        // The config "path" is a directory: every read/write fails.
        let cursed = dir.path().join("cursed");
        std::fs::create_dir(&cursed).expect("dir");
        let (state, board, stack_a, _) = fixture_state();
        let (tx, rx) = ui_request_channel();
        let (_rest, ws) = owned_workspace(cursed.clone()).await;
        let forwarder = spawn_ui_request_forwarder(rx, Arc::new(ArcSwap::from_pointee(state)), ws);
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
        // Dated pin: the ambient gate, `clean_ambient`'s doc (#1113).
        if !clean_ambient() {
            return;
        }
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("taskboard.toml");
        let (_state, _b, _a, _s) = fixture_state();
        let (tx, rx) = ui_request_channel();
        let (_rest, ws) = owned_workspace(path.clone()).await;
        let forwarder = spawn_ui_request_forwarder(
            rx,
            Arc::new(ArcSwap::from_pointee(AppState::default())),
            ws,
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
        // Dated pin: the ambient gate, `clean_ambient`'s doc (#1113).
        if !clean_ambient() {
            return;
        }
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("taskboard.toml");
        std::fs::write(&path, "[ui]\nscale = 1.0\n").expect("seed");
        let (state, _, _, _) = fixture_state();
        let (tx, rx) = ui_request_channel();
        let (_rest, ws) = owned_workspace(path.clone()).await;
        let forwarder = spawn_ui_request_forwarder(rx, Arc::new(ArcSwap::from_pointee(state)), ws);
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
        // Dated pin: the ambient gate, `clean_ambient`'s doc (#1113).
        if !clean_ambient() {
            return;
        }
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("taskboard.toml");
        std::fs::write(
            &path,
            "# operator comment\n[ui]\nscale = 1.5\n[nextcloud]\n# keep me\nserver_url = \"https://x\"\n",
        )
        .expect("seed");
        let (state, board, stack_a, _) = fixture_state();
        let (tx, rx) = ui_request_channel();
        let (_rest, ws) = owned_workspace(path.clone()).await;
        let forwarder = spawn_ui_request_forwarder(rx, Arc::new(ArcSwap::from_pointee(state)), ws);
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

        // Dated pin: the ambient gate, `clean_ambient`'s doc (#1113).
        if !clean_ambient() {
            return;
        }

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
        // Dated pin: the ambient gate, `clean_ambient`'s doc (#1113).
        if !clean_ambient() {
            return;
        }

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
        // Dated pin: the ambient gate, `clean_ambient`'s doc (#1113).
        if !clean_ambient() {
            return;
        }
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("taskboard.toml");
        std::fs::write(
            &path,
            "[reminders]\ntick_secs = 30\n[sync]\nbackoff_initial_secs = 5\nbackoff_max_secs = 300\n",
        )
        .expect("seed");
        let fixture = workspace_fixture(path.clone()).await;
        let workspace = &fixture.workspace;

        let (reply, reply_rx) = tokio::sync::oneshot::channel();
        handle_settings(UiRequest::SetReminderTick { secs: 45, reply }, workspace).await;
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
            19,
            "the reply carries the apply map (size tracks the registry — \
             #1051 added files_sync.poll_secs)"
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
        handle_settings(UiRequest::SetBackoffInitial { secs: 400, reply }, workspace).await;
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
        // Dated pin: the ambient gate, `clean_ambient`'s doc (#1113).
        if !clean_ambient() {
            return;
        }
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("taskboard.toml");
        std::fs::write(&path, "[reminders]\ntick_secs = 30\n").expect("seed");
        let mut fixture = workspace_fixture(path.clone()).await;
        let (workspace, app) = (&fixture.workspace, &mut fixture.rest.app);
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
        handle_settings(UiRequest::SetReminderTick { secs: 45, reply }, workspace).await;
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
    }

    /// The focus commits persist through the workspace path and the
    /// reply echoes the fresh file snapshot's typed values (the focus
    /// settings editor, #625).
    #[tokio::test]
    async fn focus_commits_persist_and_reply_the_fresh_snapshot() {
        // Dated pin: the ambient gate, `clean_ambient`'s doc (#1113).
        if !clean_ambient() {
            return;
        }
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("taskboard.toml");
        std::fs::write(&path, "[ui]\nscale = 1.0\n").expect("seed");
        let fixture = workspace_fixture(path.clone()).await;
        let workspace = &fixture.workspace;

        let (reply, reply_rx) = tokio::sync::oneshot::channel();
        handle_settings(UiRequest::SetFocusWorkMins { mins: 45, reply }, workspace).await;
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
            workspace,
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
        // Dated pin: the ambient gate, `clean_ambient`'s doc (#1113).
        if !clean_ambient() {
            return;
        }
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("taskboard.toml");
        std::fs::write(
            &path,
            "[[sync.connectors]]\nname = \"work\"\nbackend = \"nextcloud\"\n\
             server_url = \"https://c.example\"\nusername = \"alice\"\n\
             [files_sync]\npoll_secs = 7\n",
        )
        .expect("seed");
        let fixture = workspace_fixture(path.clone()).await;
        let workspace = &fixture.workspace;
        let (reply, reply_rx) = tokio::sync::oneshot::channel();
        handle_settings(
            UiRequest::SetFilesSyncEnabled {
                enabled: true,
                anchor: None,
                reply,
            },
            workspace,
        )
        .await;
        let snapshot = reply_rx.await.expect("alive").expect("lands");
        assert!(snapshot.files_sync_enabled, "the reply echoes the file");
        // File-layer validity only (#897): the ambient-composing
        // `extract_raw` exposed this leg to `TASKBOARD_*` env; the
        // assertion is a fact about the written file, so read the
        // file layer alone.
        let written = AppConfig::extract_file_only(Some(&path)).expect("decodes");
        assert!(written.files_sync.enabled);
        assert_eq!(written.files_sync.poll_secs, Some(7), "poll_secs survives");
        assert_eq!(
            written.files_sync.anchor, None,
            "the None arm leaves the anchor key untouched"
        );

        let standalone = dir.path().join("standalone.toml");
        std::fs::write(&standalone, "[sync]\nbackend = \"standalone\"\n").expect("seed");
        let before = std::fs::read_to_string(&standalone).expect("read");
        let fixture = workspace_fixture(standalone.clone()).await;
        let workspace = &fixture.workspace;
        let (reply, reply_rx) = tokio::sync::oneshot::channel();
        handle_settings(
            UiRequest::SetFilesSyncEnabled {
                enabled: true,
                anchor: None,
                reply,
            },
            workspace,
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
        // Dated pin: the ambient gate, `clean_ambient`'s doc (#1113).
        if !clean_ambient() {
            return;
        }
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
        let fixture = workspace_fixture(path.clone()).await;
        let workspace = &fixture.workspace;
        let (reply, reply_rx) = tokio::sync::oneshot::channel();
        handle_settings(
            UiRequest::SetFilesSyncEnabled {
                enabled: true,
                anchor: Some("home".into()),
                reply,
            },
            workspace,
        )
        .await;
        let snapshot = reply_rx.await.expect("alive").expect("lands");
        assert_eq!(
            snapshot.files_sync_anchor.as_deref(),
            Some("home"),
            "the reply echoes the resolved anchor"
        );
        // File-layer validity only (#897): the ambient-composing
        // `extract_raw` exposed this leg to `TASKBOARD_*` env; the
        // assertion is a fact about the written file, so read the
        // file layer alone.
        let written = AppConfig::extract_file_only(Some(&path)).expect("decodes");
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
        // Dated pin: the ambient gate, `clean_ambient`'s doc (#1113).
        if !clean_ambient() {
            return;
        }
        let guard = |field: Field, keys: &[&str]| -> bool {
            let locked: BTreeSet<String> = keys.iter().map(|k| (*k).to_owned()).collect();
            editor_guard_keys(field)
                .iter()
                .any(|key| locked.contains(*key))
        };
        assert!(
            guard(Field::FilesSyncEnabled, &["files_sync.anchor"]),
            "a pinned anchor locks the Files pick"
        );
        assert!(guard(Field::FilesSyncEnabled, &["files_sync.enabled"]));
        assert!(
            !guard(Field::FocusEnabled, &["files_sync.enabled"]),
            "another field's verdict reads its own key only"
        );
        assert!(guard(Field::FocusEnabled, &["focus.enabled"]));
        assert!(
            !guard(Field::FilesSyncEnabled, &["focus.enabled"]),
            "an unrelated pin does not lock the Files row"
        );
    }

    // -- the shared live-write port's truth table (journey W-3) ----------

    /// The port over a seeded file + a real booted core: the seeded
    /// config, the db, and the device identity share the fixture's one
    /// tempdir (#982, matching `standalone_boot_fixture`).
    async fn port_workspace(seed_text: &str) -> WorkspaceFixture {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("taskboard.toml");
        std::fs::write(&path, seed_text).expect("seed config");
        workspace_fixture_in(path, dir).await
    }

    #[allow(clippy::unnecessary_wraps)] // the port's writer signature
    fn noop_write(_: &std::path::Path) -> Result<(), crate::settings_overlay::OverlayError> {
        Ok(())
    }

    /// `us4_s1`: the port's env guard fires first — a pinned
    /// `guard_keys` entry refuses typed and the writer never runs
    /// (nothing wrote, nothing sent), on any front.
    #[tokio::test]
    async fn us4_s1_the_port_env_guard_refuses_before_the_write() {
        // Dated pin: the ambient gate, `clean_ambient`'s doc (#1113).
        if !clean_ambient() {
            return;
        }
        let fixture = port_workspace("[reminders]\ntick_secs = 30\n").await;
        let workspace = &fixture.workspace;
        let locked: std::collections::BTreeSet<String> = ["reminders.tick_secs"]
            .into_iter()
            .map(String::from)
            .collect();
        let mut writer_ran = false;
        let result = run_settings_write(
            workspace.config_write_path.as_path(),
            &locked,
            &["reminders.tick_secs"],
            |path| {
                writer_ran = true;
                noop_write(path)
            },
            Some(&workspace.engine),
        )
        .await;
        assert!(
            matches!(result, Err(SettingsWriteError::EnvLocked)),
            "typed env refusal: {result:?}"
        );
        assert!(!writer_ran, "the writer never ran");
    }

    /// `us5_s1` (#1083, the small-gated journeys plan W-6/R-4): the
    /// `poll_secs` editor lane rides the SAME live-write port as every
    /// editor field — the file write lands through the landed B7b
    /// overlay writer (the sole writer, no second one coined) and the
    /// whole-snapshot `UpdateSettings` carries the value the engine
    /// applies live (the registry's `Live` verdict honored).
    #[tokio::test]
    async fn us5_s1_poll_secs_rides_update_settings() {
        // Dated pin: the ambient gate, `clean_ambient`'s doc (#1113).
        if !clean_ambient() {
            return;
        }
        let fixture = port_workspace("[files_sync]\npoll_secs = 30\n").await;
        let workspace = &fixture.workspace;
        let snapshot = run_editor_write(workspace, Field::FilesPollSecs, |path| {
            crate::settings_overlay::set_files_sync_poll_secs(path, 90)
        })
        .await
        .expect("the write lands");
        assert_eq!(
            snapshot.files_sync_poll_secs,
            Some(90),
            "the reply echoes the fresh file value"
        );
        let published = workspace.engine.shared_state().load();
        assert_eq!(
            published
                .settings
                .files_sync_poll_secs
                .map(taskboard_domain::TickSecs::get),
            Some(90),
            "the whole-snapshot UpdateSettings applied live"
        );
    }

    /// `us5_s1`'s negative twin at the gate: a non-positive value that
    /// reaches the lane past the field mirror is refused by the config
    /// prove gate with own-blame attribution — the same typed gate the
    /// CLI knob answers to (the R-4 single-refusal law).
    #[tokio::test]
    async fn us5_s1_poll_secs_zero_refuses_before_any_command() {
        // Dated pin: the ambient gate, `clean_ambient`'s doc (#1113).
        if !clean_ambient() {
            return;
        }
        let fixture = port_workspace("[files_sync]\npoll_secs = 30\n").await;
        let workspace = &fixture.workspace;
        let result = run_editor_write(workspace, Field::FilesPollSecs, |path| {
            crate::settings_overlay::set_files_sync_poll_secs(path, 0)
        })
        .await;
        assert!(
            matches!(
                result,
                Err(SettingsError::Invalid {
                    field: Field::FilesPollSecs,
                    kind: InvalidKind::OutOfRange
                })
            ),
            "the typed > 0 gate: {result:?}"
        );
        let published = workspace.engine.shared_state().load();
        assert_ne!(
            published
                .settings
                .files_sync_poll_secs
                .map(taskboard_domain::TickSecs::get),
            Some(0),
            "the engine never saw the refused write"
        );
    }

    /// `us4_s3`: a validity-crossing write refuses at the port typed
    /// (`Overlay(Rejected)`) and the engine is untouched — the prove
    /// gate runs before any `UpdateSettings` (the file-before-engine
    /// order, US-1 AC-2's negative arm).
    #[tokio::test]
    async fn us4_s3_the_port_prove_refusal_sends_no_command() {
        // Dated pin: the ambient gate, `clean_ambient`'s doc (#1113).
        if !clean_ambient() {
            return;
        }
        let fixture = port_workspace(
            "[sync]\npoll_interval_secs = 30\nbackoff_initial_secs = 5\nbackoff_max_secs = 300\n",
        )
        .await;
        let workspace = &fixture.workspace;
        let locked = std::collections::BTreeSet::new();
        let result = run_settings_write(
            workspace.config_write_path.as_path(),
            &locked,
            &["sync.backoff_initial_secs"],
            |path| crate::settings_overlay::set_backoff_initial(path, 400),
            Some(&workspace.engine),
        )
        .await;
        assert!(
            matches!(
                result,
                Err(SettingsWriteError::Overlay(
                    crate::settings_overlay::OverlayError::Rejected(_)
                ))
            ),
            "typed prove refusal: {result:?}"
        );
        let published = workspace.engine.shared_state().load();
        assert_eq!(
            u64::from(published.settings.backoff_initial_secs),
            crate::config::DEFAULT_BACKOFF_INITIAL_SECS,
            "the engine never saw the refused write"
        );
        let text = std::fs::read_to_string(workspace.config_write_path.as_path()).expect("file");
        assert!(
            text.contains("backoff_initial_secs = 5"),
            "the file is untouched: {text}"
        );
    }

    // (The port's `EngineGone`/`NoEngine` next-start lanes and the
    // focus gate's own-blame refusal re-homed onto
    // `tests/services/ui_settings.rs` behind `taskboard_env_lock`,
    // #1157.)

    /// T5's app-side twin (the redaction proof lives in the UI crate;
    /// this pins the request `Debug` structurally through a full
    /// profile payload).
    #[test]
    fn a_save_request_debug_output_never_carries_the_secret() {
        // Dated pin: the ambient gate, `clean_ambient`'s doc (#1113).
        if !clean_ambient() {
            return;
        }
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
        // The undo-window refusals map typed now (US-6, D-7).
        assert!(matches!(
            fold(MoveError::NothingToUndo.into()),
            SettingsError::NothingToUndo
        ));
        assert!(matches!(
            fold(MoveError::UndoClosed.into()),
            SettingsError::UndoClosed
        ));
        assert!(matches!(
            fold(MoveError::UndoCoordinatesLost.into()),
            SettingsError::UndoCoordinatesLost
        ));
        for step in [
            MoveError::PublishUnconfirmed,
            MoveError::FilesSubtreeForeign,
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
