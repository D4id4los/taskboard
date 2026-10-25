// SPDX-License-Identifier: AGPL-3.0-only

//! The UI → app request lane's app half (round-1 D21): the debounced
//! forwarder that persists device-local stack column widths into the
//! `[ui.stack_widths]` config subtree. Single-owner rule (R6): this
//! task owns every write of that subtree — no other code path touches
//! it. Every error is logged and swallowed: the UI's widths live on in
//! the view state, the next received request retries the whole
//! read-merge-write, and the forwarder never blocks or crashes the UI
//! (D8) — it only ever holds owned maps, never a lock across `.await`.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use arc_swap::ArcSwap;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use taskboard_domain::{AppState, BoardId, ConnectorId, RemoteBoardId, StateCommand};
use taskboard_state::{EngineHandle, ExecuteError};
use taskboard_sync_nextcloud::DeckClient;
use taskboard_ui_slint::requests::{
    ConnectionProfile, ConnectionTarget, ConnectionsSnapshot, ConnectorInfo, Field, InvalidKind,
    KeymapUpdate, SettingsError, UiRequest,
};

use crate::bootstrap::ConnectorControl;
use crate::commands::CommandError;
use crate::config::{AppConfig, ConfigError, ConnectorCfg, SyncBackend};
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
/// pending map and debounce into one config flush (chunk 2's path,
/// byte-for-byte semantics); every settings request is handled
/// immediately — one-shot actions with replies are never debounced,
/// including ones arriving inside a width quiet window (plan D3).
/// On channel close (the UI side dropped) one final flush lands a
/// resize sent right before quit, then the task exits. The caller owns
/// the handle's retirement.
pub fn spawn_ui_request_forwarder(
    mut requests: mpsc::Receiver<UiRequest>,
    state: Arc<ArcSwap<AppState>>,
    workspace: ConnectionWorkspace,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut pending: BTreeMap<uuid::Uuid, BTreeMap<uuid::Uuid, u32>> = BTreeMap::new();
        loop {
            let Some(request) = requests.recv().await else {
                flush(&mut pending, &workspace.config_write_path, &state.load());
                tracing::info!("ui request channel closed; forwarder exiting");
                return;
            };
            match request {
                UiRequest::StackWidthsChanged { board, widths } => {
                    merge_widths(board, widths, &mut pending);
                    // The debounce quiet window: a late width snapshot
                    // merges and restarts it; a settings request is
                    // executed on the spot (never debounced); close
                    // flushes; elapsed falls through to the flush.
                    loop {
                        match tokio::time::timeout(UI_REQUEST_DEBOUNCE, requests.recv()).await {
                            Ok(Some(UiRequest::StackWidthsChanged { board, widths })) => {
                                merge_widths(board, widths, &mut pending);
                            }
                            Ok(Some(settings)) => {
                                handle_settings(settings, &workspace).await;
                            }
                            Ok(None) => {
                                flush(&mut pending, &workspace.config_write_path, &state.load());
                                tracing::info!("ui request channel closed; forwarder exiting");
                                return;
                            }
                            Err(_elapsed) => break,
                        }
                    }
                    flush(&mut pending, &workspace.config_write_path, &state.load());
                }
                settings => handle_settings(settings, &workspace).await,
            }
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
/// overlay. Errors are warned and swallowed — the pending map is
/// dropped either way; the next received request re-sends the full
/// snapshot.
fn flush(
    pending: &mut BTreeMap<uuid::Uuid, BTreeMap<uuid::Uuid, u32>>,
    config_write_path: &std::path::Path,
    state: &AppState,
) {
    if pending.is_empty() {
        return; // nothing was ever received: no write at all
    }
    let pruned = prune_widths(pending, state);
    pending.clear();
    if let Err(err) = overlay_stack_widths(config_write_path, &pruned) {
        tracing::warn!(error = %err, "stack-widths overlay write failed; the next event retries");
    }
}

/// Merges the pruned widths into the `[ui.stack_widths]` subtree of
/// the config file via `toml_edit` (comments, formatting, and every
/// other table preserved by construction; comments *inside* the
/// subtree are machine territory). An empty map *removes* the subtree
/// instead — no empty husk. The whole-document write is atomic
/// (temp + rename; the file's mode is preserved, a created file is
/// owner-only).
fn overlay_stack_widths(
    path: &std::path::Path,
    widths: &BTreeMap<uuid::Uuid, BTreeMap<uuid::Uuid, u32>>,
) -> std::io::Result<()> {
    let mut doc = crate::config::overlay_document(path)?;
    let ui = doc
        .entry("ui")
        .or_insert(toml_edit::Item::Table(toml_edit::Table::new()))
        .as_table_mut()
        .ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, "[ui] is not a table")
        })?;
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
    crate::config::write_config_atomic(path, doc.to_string().as_str())
}

/// The bounded, offline-first settle after a settings save (the boot
/// ceremonies' budget; plan F-C2/D3).
const SAVE_SETTLE: std::time::Duration = std::time::Duration::from_secs(5);

/// Dispatches one settings request (never debounced — plan D3). Every
/// reply is fire-and-forget-honest: a dropped receiver (a stale form)
/// is at most a log line, never a panic, and persistence is never
/// undone by a failed reply.
async fn handle_settings(request: UiRequest, workspace: &ConnectionWorkspace) {
    match request {
        UiRequest::StackWidthsChanged { .. } => {
            unreachable!("width snapshots take the debounce path")
        }
        UiRequest::TestConnection { target, reply } => {
            let outcome = test_connection(workspace, &target).await;
            let _ = reply.send(outcome);
        }
        UiRequest::SaveConnection { profile, reply } => {
            let outcome = save_connection(workspace, &profile).await;
            let _ = reply.send(outcome);
        }
        UiRequest::RemoveConnection { name, reply } => {
            let outcome = remove_connection(workspace, &name).await;
            let _ = reply.send(outcome);
        }
        UiRequest::AttachConnector {
            board,
            connector,
            reply,
        } => {
            let outcome = attach_connection(workspace, board, &connector).await;
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
        UiRequest::SetKeymapPreset { preset, reply } => {
            let _ = reply.send(set_keymap_preset(workspace, preset));
        }
        UiRequest::SetScale { scale, reply } => {
            let _ = reply.send(set_scale(workspace, scale));
        }
    }
}

/// The per-row Test (DEV-3): load the stored secret, verify it against
/// the row's server. Nothing is ever persisted.
async fn test_connection(
    workspace: &ConnectionWorkspace,
    target: &ConnectionTarget,
) -> Result<(), SettingsError> {
    let password = workspace
        .store
        .load(&target.server_url, &target.username)
        .map_err(map_credential_error)?;
    crate::connections::verify_credentials(&target.server_url, &target.username, password.expose())
        .await
        .map_err(map_ceremony_error)
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
    tracing::info!(
        connector = %profile.name,
        host = %url_host(&profile.server_url),
        "settings save started"
    );
    let raw = read_raw_config(workspace)?;
    if raw.sync.connectors.is_none() {
        return Err(SettingsError::LegacyConfig);
    }
    if profile.password.0.trim().is_empty() {
        return Err(SettingsError::Invalid {
            field: Field::Password,
            kind: InvalidKind::Empty,
        });
    }
    let draft = validated_draft(&raw, profile)?;

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
    let id = ConnectorId::from(profile.name.clone());
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

/// The Remove choreography (plan D3): typed legacy refusal → overlay
/// remove → idempotent named detach → best-effort secret delete of the
/// removed identity. Board bindings to the connector stay (the detach
/// semantic: local operation continues).
async fn remove_connection(
    workspace: &ConnectionWorkspace,
    name: &str,
) -> Result<ConnectionsSnapshot, SettingsError> {
    let raw = read_raw_config(workspace)?;
    if raw.sync.connectors.is_none() {
        return Err(SettingsError::LegacyConfig);
    }
    let identity = raw
        .sync
        .connectors
        .iter()
        .flatten()
        .find(|entry| entry.name == name)
        .map(|entry| (entry.server_url.clone(), entry.username.clone()));
    tracing::info!(
        connector = %name,
        host = identity.as_ref().map_or("-", |(server, _)| url_host(server)),
        "settings remove started"
    );
    crate::connections::overlay_remove_connector(&workspace.config_write_path, name)
        .map_err(|err| map_io_error(&err))?;
    let _ = workspace
        .control
        .detach_connector_named(&ConnectorId::from(name.to_owned()))
        .await;
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

/// The Attach choreography (r2 D33, plan §3.1 in order): resolve the
/// connector config → load the stored secret → derive the remote id
/// (idempotent re-bind skips the listing; unbound adopts by exact
/// title over live remote boards, lowest id wins; bound elsewhere
/// refuses pre-IO) → ensure attached (the named-lookup guard) → bind
/// through the engine command path (the REPL bind's route) → settle
/// the first pull → reply the fresh pair.
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
) -> Result<ConnectionsSnapshot, SettingsError> {
    let raw = read_raw_config(workspace)?;
    if raw.sync.connectors.is_none() {
        return Err(SettingsError::LegacyConfig);
    }
    let entry = raw
        .sync
        .connectors
        .iter()
        .flatten()
        .find(|entry| entry.name == connector_name)
        .cloned()
        .ok_or(SettingsError::Gone)?;
    tracing::info!(
        connector = %connector_name,
        host = %url_host(&entry.server_url),
        "settings attach started"
    );
    let stored = workspace
        .store
        .load(&entry.server_url, &entry.username)
        .map_err(map_credential_error)?;

    // Derive the remote id from the board row (the state read is
    // authoritative for liveness and the current binding).
    let state = workspace.engine.shared_state().load();
    let board_row = state
        .boards
        .get(&board)
        .filter(|b| !b.deleted && !b.archived)
        .ok_or(SettingsError::Gone)?;
    let remote = match board_row.remote.as_ref() {
        Some(binding) if binding.connector.as_str() == connector_name => {
            // Idempotent re-issue: the binding already names this
            // connector — nothing to discover, no network call.
            binding.board
        }
        Some(_) => return Err(SettingsError::BindRefused),
        None => {
            // Fresh listing verify-and-resolve: the listing *is* the
            // verification (the same call `verify_credentials` makes).
            let client = DeckClient::new(&entry.server_url, &entry.username, stored.expose())
                .map_err(|err| {
                    tracing::warn!(variant = %err, "settings attach: the client was refused");
                    SettingsError::AttachFailed
                })?;
            let remote_boards = client
                .boards()
                .await
                .map_err(crate::commands::map_deck_error)
                .map_err(map_ceremony_error)?;
            let title = board_row.title.as_str();
            remote_boards
                .iter()
                .filter(|b| b.deleted_at == 0 && !b.archived)
                .filter(|b| b.title == title)
                .min_by_key(|b| b.id)
                .map(|b| RemoteBoardId(b.id))
                .ok_or(SettingsError::NoRemoteMatch)?
        }
    };
    drop(state);

    // Ensure attached: the named lookup guards the healthy-attached
    // case — an unconditional attach would refuse ConnectorNameInUse
    // and surface the misleading AttachBusy text.
    let id = ConnectorId::from(connector_name.to_owned());
    if workspace.control.connector_commands(&id).is_none() {
        let client = DeckClient::new(&entry.server_url, &entry.username, stored.expose()).map_err(
            |err| {
                tracing::warn!(variant = %err, "settings attach: the client was refused");
                SettingsError::AttachFailed
            },
        )?;
        workspace
            .control
            .attach_connector(id, client)
            .await
            .map_err(map_attach_error)?;
    }

    // Bind through the engine command path — the receipt implies
    // durability (the same route the REPL bind uses).
    workspace
        .engine
        .execute(StateCommand::BindBoard {
            board,
            remote,
            connector: ConnectorId::from(connector_name.to_owned()),
        })
        .await
        .map_err(|e| map_execute_error(&e))?;

    crate::bootstrap::wait_for_boot_pull(&workspace.engine, SAVE_SETTLE).await;
    tracing::info!(
        connector = %connector_name,
        host = %url_host(&entry.server_url),
        "settings attach succeeded"
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
            .detach_connector_named(&ConnectorId::from(connector_name.to_owned()))
            .await;
    }
    tracing::info!(connector = %connector_name, "settings detach succeeded");
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

/// The log-safe host of a server URL (r2 D34): scheme- and
/// userinfo-stripped, up to the first path/query/fragment character.
/// Tracing fields carry host and name only — never the full URL,
/// never credentials.
fn url_host(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let rest = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    rest.rsplit_once('@').map_or(rest, |(_, host)| host)
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
    if !scale.is_finite() {
        return Err(SettingsError::Invalid {
            field: Field::Scale,
            kind: InvalidKind::NotFinite,
        });
    }
    if !(0.5..=3.0).contains(&scale) {
        return Err(SettingsError::Invalid {
            field: Field::Scale,
            kind: InvalidKind::OutOfRange,
        });
    }
    crate::connections::overlay_set_ui_scale(&workspace.config_write_path, scale)
        .map_err(|err| map_io_error(&err))
}

/// The raw config at the write path (the same read the REPL ceremonies
/// run — env included; R6's re-read-per-request rule).
fn read_raw_config(workspace: &ConnectionWorkspace) -> Result<AppConfig, SettingsError> {
    AppConfig::extract_raw(Some(&workspace.config_write_path)).map_err(|err| map_config_error(&err))
}

/// The fresh secret-free list for the reply (the `configured_connectors`
/// source, resolved through the raw config).
fn current_connectors(workspace: &ConnectionWorkspace) -> Vec<ConnectorInfo> {
    match AppConfig::extract_raw(Some(&workspace.config_write_path)) {
        Ok(raw) => crate::ui::resolve_connector_infos(&raw),
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
) -> Result<ConnectorCfg, SettingsError> {
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
    Ok(draft)
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
        ConnectionWorkspace {
            config_write_path: path,
            store: Arc::new(RefusingStore),
            control: app.connector_control(),
            engine: app.engine.clone(),
        }
        // The core deliberately leaks for the test's lifetime (the
        // actors idle on a standalone, boardless db); the workspace
        // holds only clones.
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
    // -- A10: the mapping tables ------------------------------------------

    /// r2 D34 — the log-safe host extraction: scheme, userinfo, path,
    /// query, and fragment all stripped; a schemeless URL keeps its
    /// authority.
    #[test]
    fn url_host_strips_everything_but_the_authority() {
        assert_eq!(
            url_host("https://cloud.example.com/nc"),
            "cloud.example.com"
        );
        assert_eq!(
            url_host("http://user:pw@host.example:8080/?x=1"),
            "host.example:8080"
        );
        assert_eq!(url_host("https://a.example#frag"), "a.example");
        assert_eq!(url_host("host.example/path"), "host.example");
        assert_eq!(url_host("https://bare.example"), "bare.example");
    }

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
