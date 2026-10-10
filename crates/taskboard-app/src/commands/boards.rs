// SPDX-License-Identifier: AGPL-3.0-only
//! `taskboard boards` (ADR 0009): `list` is a direct remote read (no
//! core boot, no lock — the first-run discovery surface); `select` is
//! the one-time single-board binding ceremony: pre-validate against a
//! fresh listing, boot the core, refuse a rebind, `SetBoard`, wait for
//! the adoption that persists the binding.

use std::sync::Arc;
use std::time::{Duration, Instant};

use taskboard_domain::{RemoteBoardId, SyncCommand, SyncPhase};

use crate::commands::output::{BoardsListEntry, SelectReceipt, SyncStatusView};
use crate::commands::{CommandError, map_deck_error};
use crate::config::AppConfig;
use crate::secrets::CredentialStore;
use taskboard_sync_nextcloud::DeckClient;

/// Builds the Deck client from config + credential store (the shared
/// construction of `bootstrap`, `boards list`, and `login`).
pub(crate) fn client_for(
    config: &AppConfig,
    store: &Arc<dyn CredentialStore>,
) -> Result<DeckClient, CommandError> {
    let password = store.load(config.server_url(), config.username())?;
    Ok(DeckClient::new(
        config.server_url(),
        config.username(),
        password.expose(),
    )?)
}

/// Lists the server's boards (remote ids feed `boards select`).
///
/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
pub async fn list(
    config: &AppConfig,
    store: &Arc<dyn CredentialStore>,
) -> Result<Vec<BoardsListEntry>, CommandError> {
    let client = client_for(config, store)?;
    let boards = client.boards().await.map_err(map_deck_error)?;
    let mut entries: Vec<BoardsListEntry> = boards
        .iter()
        .map(|b| BoardsListEntry {
            id: b.id,
            title: b.title.clone(),
            archived: b.archived,
            deleted: !b.is_live(),
        })
        .collect();
    entries.sort_by_key(|e| e.id);
    Ok(entries)
}

/// The one-time binding ceremony: pre-validate → boot → refuse rebind →
/// `SetBoard` → wait for adoption → receipt. The adoption persists the
/// binding (the adopted board row); a later daemon resumes it from the
/// database.
///
/// # Errors
///
/// [`CommandError`] — typed: [`CommandError::UnknownBoard`] before any
/// core boot, [`CommandError::BoardAlreadyBound`] for the single-board
/// invariant, [`CommandError::SyncFailed`]/[`CommandError::SyncTimedOut`]
/// while waiting for adoption.
pub async fn select(
    config: AppConfig,
    store: Arc<dyn CredentialStore>,
    board: u64,
    timeout: Duration,
) -> Result<SelectReceipt, CommandError> {
    // Pre-validation: a bogus id inside a sync cycle would only surface
    // as the generic `Failed { Server }` — check the fresh listing
    // first, before anything boots.
    let client = client_for(&config, &store)?;
    let listing = client.boards().await.map_err(map_deck_error)?;
    if !listing
        .iter()
        .any(|b| b.id == board && !b.archived && b.is_live())
    {
        return Err(CommandError::UnknownBoard(board));
    }

    let app = crate::bootstrap::bootstrap(config, store).await?;
    let result = select_locked(&app, board, timeout).await;
    let _ = app.shutdown().await;
    result
}

/// Runs the binding steps on a booted core; the caller owns the
/// shutdown.
async fn select_locked(
    app: &crate::App,
    board: u64,
    timeout: Duration,
) -> Result<SelectReceipt, CommandError> {
    let state = app.engine.shared_state().load_full();
    let bound = state.boards.values().find(|b| !b.deleted);
    if let Some(bound) = bound
        && let Some(remote) = bound.remote
    {
        if remote.0 == board {
            // Re-selecting the bound id is an idempotent success.
            return Ok(SelectReceipt {
                board: BoardsListEntry {
                    id: remote.0,
                    title: bound.title.clone(),
                    archived: bound.archived,
                    deleted: bound.deleted,
                },
                sync: SyncStatusView::new(&state.sync),
            });
        }
        let current = remote.0;
        return Err(CommandError::BoardAlreadyBound { current });
    }
    drop(state);

    app.sync_commands()
        .send(SyncCommand::SetBoard(RemoteBoardId(board)))
        .await
        .map_err(|_| CommandError::Engine(Box::new(taskboard_state::ExecuteError::EngineGone)))?;

    wait_for_adoption(app, board, timeout).await
}

/// Predicate-polls (event-driven with a polling fallback) until the
/// remote board is adopted locally, a fresh sync failure lands, or the
/// deadline expires.
async fn wait_for_adoption(
    app: &crate::App,
    board: u64,
    timeout: Duration,
) -> Result<SelectReceipt, CommandError> {
    let mut signals = app.engine.subscribe();
    let deadline = Instant::now() + timeout;
    loop {
        let state = app.engine.shared_state().load_full();
        if let Some(adopted) = state
            .boards
            .values()
            .find(|b| !b.deleted && b.remote.is_some_and(|r| r.0 == board))
        {
            return Ok(SelectReceipt {
                board: BoardsListEntry {
                    id: board,
                    title: adopted.title.clone(),
                    archived: adopted.archived,
                    deleted: adopted.deleted,
                },
                sync: SyncStatusView::new(&state.sync),
            });
        }
        if let SyncPhase::Failed { last_error } = state.sync.phase {
            return Err(CommandError::SyncFailed(last_error));
        }
        if Instant::now() >= deadline {
            tracing::warn!("board adoption timed out");
            return Err(CommandError::SyncTimedOut);
        }
        // Signals prompt an early re-check; the 200 ms poll is the
        // fallback if events lag.
        let _ = tokio::time::timeout(Duration::from_millis(200), signals.recv()).await;
    }
}

/// The human-readable boards listing.
#[must_use]
pub fn render_text_list(entries: &[BoardsListEntry]) -> String {
    if entries.is_empty() {
        return "no boards on the server".to_owned();
    }
    let mut lines = Vec::new();
    for entry in entries {
        let mut line = format!("{}\t{}", entry.id, entry.title);
        if entry.archived {
            line.push_str("\t[archived]");
        }
        if entry.deleted {
            line.push_str("\t[deleted]");
        }
        lines.push(line);
    }
    lines.join("\n")
}

/// The human-readable select receipt.
#[must_use]
pub fn render_text_select(receipt: &SelectReceipt) -> String {
    format!(
        "bound to board {} ({}) — sync: {}",
        receipt.board.id, receipt.board.title, receipt.sync.phase
    )
}
