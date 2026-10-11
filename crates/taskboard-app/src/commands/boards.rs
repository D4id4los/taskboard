// SPDX-License-Identifier: AGPL-3.0-only
//! `taskboard boards` (ADR 0009): `list` is a direct remote read (no
//! core boot, no lock — the first-run discovery surface); `select` is
//! the one-time single-board binding ceremony: pre-validate against a
//! fresh listing, boot the core, refuse a rebind, `SetBoard`, wait for
//! the adoption that persists the binding.

use std::sync::Arc;
use std::time::{Duration, Instant};

use taskboard_domain::{
    CommandOutcome, RemoteBoardId, StateCommand, SyncCommand, SyncPhase, SyncStatus,
};

use crate::commands::output::{
    BoardReceipt, BoardsListEntry, PublishReceipt, SelectReceipt, SyncStatusView,
};
use crate::commands::{CommandError, map_deck_error};
use crate::config::AppConfig;
use crate::secrets::CredentialStore;
use taskboard_sync_nextcloud::{DeckClient, DeckColor};

/// The publish/create color fallback when the stored (lenient) board
/// color is not valid hex (CLI-validated input makes this defensive).
pub(crate) const DEFAULT_BOARD_COLOR: &str = "0082c9";

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
    crate::commands::require_connector(config)?;
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

/// Creates the local board (the stand-alone first-run command; works
/// under both backends). The single-board refusal is the engine's own
/// `BoardExists` rejection, surfaced with the `board-exists` kind.
///
/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
pub async fn create(
    config: AppConfig,
    store: Arc<dyn CredentialStore>,
    title: &str,
    color: Option<&str>,
) -> Result<BoardReceipt, CommandError> {
    let title = title.to_owned();
    // Lenient by domain design; `publish` maps defensively via
    // `DeckColor::from_hex` with a fallback.
    let color = color.map_or_else(
        || taskboard_domain::Color::new(DEFAULT_BOARD_COLOR),
        taskboard_domain::Color::new,
    );
    let color_str = color.as_str().to_owned();
    crate::commands::tasks::with_core(config, store, async |app| {
        create_on(app, title, color, color_str).await
    })
    .await
}

pub async fn rename(
    config: AppConfig,
    store: Arc<dyn CredentialStore>,
    title: &str,
) -> Result<BoardReceipt, CommandError> {
    let title = title.to_owned();
    crate::commands::tasks::with_core(config, store, async |app| rename_on(app, title).await).await
}

/// The `boards rename` steps on a booted core (the REPL/session seam).
pub(crate) async fn rename_on(app: &crate::App, title: String) -> Result<BoardReceipt, CommandError> {
    app.engine
        .execute(StateCommand::RenameBoard {
            title: title.clone(),
        })
        .await?;
    let state = app.engine.shared_state().load_full();
    let board = state
        .boards
        .values()
        .find(|b| !b.deleted)
        .ok_or(CommandError::NoBoard)?;
    Ok(BoardReceipt {
        id: board.id.as_uuid().to_string(),
        title: board.title.clone(),
        color: board.color.as_str().to_owned(),
    })
}

/// The `boards create` steps on a booted core (the REPL/session seam).
pub(crate) async fn create_on(
    app: &crate::App,
    title: String,
    color: taskboard_domain::Color,
    color_str: String,
) -> Result<BoardReceipt, CommandError> {
    let outcome = app
        .engine
        .execute(StateCommand::CreateBoard {
            title: title.clone(),
            color,
        })
        .await?;
    let CommandOutcome::CreatedBoard(board_id) = outcome else {
        return Err(CommandError::Engine(Box::new(
            taskboard_state::ExecuteError::Rejected(taskboard_domain::CommandError::BoardExists),
        )));
    };
    Ok(BoardReceipt {
        id: board_id.as_uuid().to_string(),
        title,
        color: color_str,
    })
}

/// The publish ceremony: create a fresh Deck board from the local one
/// and bind to it, letting the first cycle drain the outbox onto the
/// new board. A transport failure mid-ceremony is safe-but-messy: the
/// board may exist server-side unbound (retrying publishes a second
/// board; delete the orphan in the Nextcloud UI — the duplicate-create
/// crash-window backlog entry owns the general problem).
///
/// # Errors
///
/// [`CommandError`] — typed: `NoBoard`/`BoardAlreadyBound` from the
/// engine, sync classes while the first cycle settles.
pub async fn publish(
    config: AppConfig,
    store: Arc<dyn CredentialStore>,
    timeout: Duration,
) -> Result<PublishReceipt, CommandError> {
    crate::commands::require_connector(&config)?;
    // The client is built from config + store before the core boots
    // (the same shape `select` uses); the dispatcher already refused
    // the standalone backend.
    let client = client_for(&config, &store)?;
    let app = crate::bootstrap::bootstrap(config, store).await?;
    let result = publish_on(&app, &client, timeout).await;
    let _ = app.shutdown().await;
    result
}

/// The publish steps on a booted core; the caller owns the shutdown
/// (the session dispatcher calls this directly against the session core).
pub(crate) async fn publish_on(
    app: &crate::App,
    client: &DeckClient,
    timeout: Duration,
) -> Result<PublishReceipt, CommandError> {
    let state = app.engine.shared_state().load_full();
    let local = state.boards.values().find(|b| !b.deleted);
    let Some(local) = local else {
        return Err(CommandError::Engine(Box::new(
            taskboard_state::ExecuteError::Rejected(taskboard_domain::CommandError::NoBoard),
        )));
    };
    if let Some(current) = local.remote {
        return Err(CommandError::BoardAlreadyBound { current: current.0 });
    }
    let title = local.title.clone();
    // Defensive color mapping: the stored value is lenient, Deck's is
    // strict six-hex.
    let color = DeckColor::from_hex(local.color.as_str())
        .or_else(|_| DeckColor::from_hex(DEFAULT_BOARD_COLOR))
        .expect("the fallback constant is valid hex");
    drop(state);

    let created = client
        .create_board(&title, &color)
        .await
        .map_err(map_deck_error)?;

    // Bind locally, then hand the binding to the connector: the next
    // cycle's pull matches by `remote` (merge, not duplicate) and the
    // outbox drains onto the fresh board.
    let outcome = app
        .engine
        .execute(StateCommand::BindBoard {
            remote: RemoteBoardId(created.id),
        })
        .await?;
    debug_assert!(matches!(
        outcome,
        CommandOutcome::Applied | CommandOutcome::NoOp
    ));
    send_set_board(app, created.id).await?;

    let startup = app.engine.shared_state().load_full().sync.clone();
    let receipt = match crate::commands::sync::wait_for_sync_result(app, &startup, timeout).await {
        crate::commands::sync::SyncOutcome::Succeeded(view)
        | crate::commands::sync::SyncOutcome::NoWait(view) => view,
        crate::commands::sync::SyncOutcome::Failed(kind, _) => {
            return Err(CommandError::SyncFailed(kind));
        }
        crate::commands::sync::SyncOutcome::TimedOut(_) => {
            return Err(CommandError::SyncTimedOut);
        }
    };
    Ok(PublishReceipt {
        board: BoardsListEntry {
            id: created.id,
            title: created.title.clone(),
            archived: created.archived,
            deleted: !created.is_live(),
        },
        sync: receipt,
    })
}

/// Sends `SetBoard` through the sync seam; a closed channel (a
/// standalone-configured core reaching here despite the gate) is the
/// engine-gone class, not a panic.
async fn send_set_board(app: &crate::App, board: u64) -> Result<(), CommandError> {
    app.sync_commands()
        .send(SyncCommand::SetBoard(RemoteBoardId(board)))
        .await
        .map_err(|_| CommandError::Engine(Box::new(taskboard_state::ExecuteError::EngineGone)))
}

/// The binding ceremony: pre-validate → boot → (migrate or refuse) →
/// bind/select → `SetBoard` → wait for adoption → receipt. With a live
/// local-only board present, plain select refuses (`LocalBoardExists`)
/// and `--bind-local-board` migrates: `BindBoard` sets the binding, the
/// next cycle merges remote content alongside the queued local ops.
///
/// # Errors
///
/// [`CommandError`] — typed: [`CommandError::UnknownBoard`] before any
/// core boot, [`CommandError::LocalBoardExists`] /
/// [`CommandError::BoardAlreadyBound`] for the board invariants,
/// [`CommandError::SyncFailed`]/[`CommandError::SyncTimedOut`] while
/// waiting for adoption.
pub async fn select(
    config: AppConfig,
    store: Arc<dyn CredentialStore>,
    board: u64,
    timeout: Duration,
    bind_local_board: bool,
) -> Result<SelectReceipt, CommandError> {
    crate::commands::require_connector(&config)?;
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
    let result = select_on(&app, board, timeout, bind_local_board).await;
    let _ = app.shutdown().await;
    result
}

/// Runs the binding steps on a booted core; the caller owns the
/// shutdown (the session dispatcher calls this directly against the
/// session core).
pub(crate) async fn select_on(
    app: &crate::App,
    board: u64,
    timeout: Duration,
    bind_local_board: bool,
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
    let has_local_board = bound.is_some();
    drop(state);

    if has_local_board {
        if !bind_local_board {
            return Err(CommandError::LocalBoardExists);
        }
        // Migration: attach the local board onto the remote one. The
        // next pull matches by `remote` and merges instead of adopting
        // a duplicate; the outbox then drains onto the remote board.
        let outcome = app
            .engine
            .execute(StateCommand::BindBoard {
                remote: RemoteBoardId(board),
            })
            .await?;
        debug_assert!(matches!(
            outcome,
            CommandOutcome::Applied | CommandOutcome::NoOp
        ));
    }

    // Snapshot the hydrated status before the send: a *stale* persisted
    // `Failed` (e.g. from a `sync` run before any binding) must not mask
    // the fresh adoption cycle — the same S0 guard `wait_for_sync_result`
    // applies.
    let startup = app.engine.shared_state().load_full().sync.clone();
    send_set_board(app, board).await?;

    wait_for_adoption(app, board, &startup, timeout).await
}

/// Predicate-polls (event-driven with a polling fallback) until the
/// remote board is adopted locally, a *fresh* sync failure lands, or the
/// deadline expires. Freshness is decided against the pre-`SetBoard`
/// snapshot `startup` (the sync status is persisted and hydrated at
/// boot, so a board-less db can boot carrying an old `Failed` phase that
/// nothing clears until a cycle settles).
async fn wait_for_adoption(
    app: &crate::App,
    board: u64,
    startup: &SyncStatus,
    timeout: Duration,
) -> Result<SelectReceipt, CommandError> {
    let mut signals = app.engine.subscribe();
    let mut seen_change = false;
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
        let fresh_failure = matches!(state.sync.phase, SyncPhase::Failed { .. })
            && (seen_change || state.sync != *startup);
        if fresh_failure {
            let SyncPhase::Failed { last_error } = state.sync.phase else {
                unreachable!("checked by fresh_failure");
            };
            return Err(CommandError::SyncFailed(last_error));
        }
        if Instant::now() >= deadline {
            tracing::warn!("board adoption timed out");
            return Err(CommandError::SyncTimedOut);
        }
        // Signals prompt an early re-check; the 200 ms poll is the
        // fallback if events lag.
        if tokio::time::timeout(Duration::from_millis(200), signals.recv())
            .await
            .is_ok()
        {
            seen_change = true;
        }
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

/// The human-readable select/publish receipt.
#[must_use]
pub fn render_text_select(receipt: &SelectReceipt) -> String {
    format!(
        "bound to board {} ({}) — sync: {}",
        receipt.board.id, receipt.board.title, receipt.sync.phase
    )
}

/// The human-readable create receipt.
#[must_use]
pub fn render_text_create(receipt: &BoardReceipt) -> String {
    format!("created board \"{}\" ({})", receipt.title, receipt.id)
}

/// The human-readable rename receipt.
#[must_use]
pub fn render_text_rename(receipt: &BoardReceipt) -> String {
    format!("renamed board \"{}\" ({})", receipt.title, receipt.id)
}
