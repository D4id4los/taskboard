// SPDX-License-Identifier: AGPL-3.0-only
//! `taskboard boards` (ADR 0009): `list` is a direct remote read (no
//! core boot, no lock — the first-run discovery surface); `select` is
//! the one-time single-board binding ceremony: pre-validate against a
//! fresh listing, boot the core, refuse a rebind, `SetBoard`, wait for
//! the adoption that persists the binding.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

use taskboard_domain::{
    CommandOutcome, RemoteBoardId, StateCommand, SyncCommand, SyncPhase, SyncStatus,
};

use crate::commands::output::{
    BoardDestructReceipt, BoardReceipt, BoardsListEntry, PublishReceipt, SelectReceipt,
    SyncStatusView,
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

/// The board color for `create`: lenient by domain design, with the
/// default constant when omitted (`publish` maps defensively via
/// `DeckColor::from_hex` with a fallback).
pub(crate) fn resolved_color(color: Option<&str>) -> taskboard_domain::Color {
    color.map_or_else(
        || taskboard_domain::Color::new(DEFAULT_BOARD_COLOR),
        taskboard_domain::Color::new,
    )
}

/// Creates a local board (the stand-alone first-run command; works
/// under both backends, always mints — titles are not uniqueness keys).
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
    let color = resolved_color(color);
    let color_str = color.as_str().to_owned();
    crate::commands::tasks::with_core(config, store, async |app| {
        create_on(app, title, color, color_str).await
    })
    .await
}

/// Renames a local board (both backends). The engine stamps the title
/// clock and enqueues the push op when the board is bound; a same-title
/// repeat is an accepted no-op (exit 0).
///
/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
pub async fn rename(
    config: AppConfig,
    store: Arc<dyn CredentialStore>,
    board: Option<taskboard_domain::BoardId>,
    title: &str,
) -> Result<BoardReceipt, CommandError> {
    let title = title.to_owned();
    crate::commands::tasks::with_core(config, store, async |app| {
        rename_on(app, board, title).await
    })
    .await
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
        // CreateBoard always mints (the planner's arm is total over its
        // inputs); any other accepted outcome is an engine contract
        // break, unreachable from user input.
        unreachable!("CreateBoard always returns CreatedBoard");
    };
    Ok(BoardReceipt {
        id: board_id.as_uuid().to_string(),
        title,
        color: color_str,
    })
}

/// The `boards rename` steps on a booted core (the REPL/session seam).
pub(crate) async fn rename_on(
    app: &crate::App,
    board: Option<taskboard_domain::BoardId>,
    title: String,
) -> Result<BoardReceipt, CommandError> {
    let board_id = crate::commands::resolve_board(app, board)?;
    app.engine
        .execute(StateCommand::RenameBoard {
            board: board_id,
            title: title.clone(),
        })
        .await?;
    board_receipt(app, board_id)
}

/// The live board's content and queued-op scope — the shared input of
/// the confirmation refusal and the landed receipt (one pre-count, one
/// truth).
struct BoardScope {
    stacks: usize,
    tasks: usize,
    labels: usize,
    pending_ops: usize,
    bound_remote: Option<u64>,
}

/// Snapshots the scope from the booted core: the published state's live
/// content plus the engine's persisted outbox (the queue the delete
/// would cancel / the reset would clear).
async fn read_scope(
    app: &crate::App,
    board_id: taskboard_domain::BoardId,
) -> Result<BoardScope, CommandError> {
    let state = app.engine.shared_state().load_full();
    // The id resolved a moment ago; a miss means the board died between
    // resolve and scope-read (an engine contract break class, not user
    // input).
    let Some(board) = state.boards.get(&board_id) else {
        return Err(CommandError::Engine(Box::new(
            taskboard_state::ExecuteError::Rejected(taskboard_domain::CommandError::UnknownBoard(
                board_id,
            )),
        )));
    };
    let (board_id, bound_remote, stacks, tasks, board_labels) = {
        let stacks: BTreeSet<taskboard_domain::StackId> = state
            .stacks
            .values()
            .filter(|s| s.board == board.id)
            .map(|s| s.id)
            .collect();
        let tasks: BTreeSet<taskboard_domain::TaskId> = state
            .tasks
            .values()
            .filter(|t| t.is_live() && stacks.contains(&t.stack))
            .map(|t| t.id)
            .collect();
        let labels: BTreeSet<taskboard_domain::LabelId> = state
            .labels
            .iter()
            .filter(|(_, l)| l.board == board.id && !l.deleted)
            .map(|(id, _)| *id)
            .collect();
        (board.id, board.remote.map(|r| r.0), stacks, tasks, labels)
    };
    let labels = board_labels.len();
    drop(state);
    let persisted = taskboard_domain::SyncStateReader::read_state(&app.engine)
        .await
        .map_err(|_| CommandError::Engine(Box::new(taskboard_state::ExecuteError::EngineGone)))?;
    let pending_ops = persisted
        .outbox
        .iter()
        .filter(|pending| match pending.op {
            taskboard_domain::LocalOp::UpdateBoard(id)
            | taskboard_domain::LocalOp::DeleteBoard(id) => id == board_id,
            taskboard_domain::LocalOp::CreateTask(id)
            | taskboard_domain::LocalOp::UpdateTask(id)
            | taskboard_domain::LocalOp::MoveTask(id)
            | taskboard_domain::LocalOp::DeleteTask(id)
            | taskboard_domain::LocalOp::AssignLabel(id, _)
            | taskboard_domain::LocalOp::UnassignLabel(id, _) => tasks.contains(&id),
            taskboard_domain::LocalOp::CreateStack(id)
            | taskboard_domain::LocalOp::RenameStack(id)
            | taskboard_domain::LocalOp::MoveStack(id)
            | taskboard_domain::LocalOp::DeleteStack(id) => stacks.contains(&id),
            taskboard_domain::LocalOp::CreateLabel(id)
            | taskboard_domain::LocalOp::UpdateLabel(id)
            | taskboard_domain::LocalOp::DeleteLabel(id) => board_labels.contains(&id),
        })
        .count();
    Ok(BoardScope {
        stacks: stacks.len(),
        tasks: tasks.len(),
        labels,
        pending_ops,
        bound_remote,
    })
}

/// The shared ceremony body of `boards delete` / `boards reset`: refuse
/// without `--confirm` (typed, stating what would go), else dispatch and
/// report the landed counts.
async fn destruct_on(
    app: &crate::App,
    board_id: taskboard_domain::BoardId,
    verb: &'static str,
    confirm: bool,
    command: StateCommand,
) -> Result<BoardDestructReceipt, CommandError> {
    let scope = read_scope(app, board_id).await?;
    if !confirm {
        return Err(CommandError::ConfirmationRequired {
            verb: verb.to_owned(),
            stacks: scope.stacks,
            tasks: scope.tasks,
            labels: scope.labels,
            pending_ops: scope.pending_ops,
            bound: scope.bound_remote.is_some(),
        });
    }
    app.engine.execute(command).await?;
    Ok(BoardDestructReceipt {
        board: board_id.as_uuid().to_string(),
        verb: verb.to_owned(),
        stacks: scope.stacks,
        tasks: scope.tasks,
        labels: scope.labels,
        pending_ops: scope.pending_ops,
        bound_remote: scope.bound_remote,
    })
}

/// The `boards color` steps on a booted core (the REPL/session seam).
pub(crate) async fn color_on(
    app: &crate::App,
    board: Option<taskboard_domain::BoardId>,
    color: taskboard_domain::Color,
) -> Result<BoardReceipt, CommandError> {
    let board_id = crate::commands::resolve_board(app, board)?;
    app.engine
        .execute(StateCommand::SetBoardColor {
            board: board_id,
            color,
        })
        .await?;
    board_receipt(app, board_id)
}

/// The `boards archive`/`boards unarchive` steps on a booted core.
pub(crate) async fn archive_on(
    app: &crate::App,
    board: Option<taskboard_domain::BoardId>,
    archived: bool,
) -> Result<BoardReceipt, CommandError> {
    let board_id = crate::commands::resolve_board(app, board)?;
    app.engine
        .execute(StateCommand::SetBoardArchived {
            board: board_id,
            archived,
        })
        .await?;
    board_receipt(app, board_id)
}

/// The `boards delete` steps on a booted core.
pub(crate) async fn delete_on(
    app: &crate::App,
    board: Option<taskboard_domain::BoardId>,
    confirm: bool,
) -> Result<BoardDestructReceipt, CommandError> {
    let board_id = crate::commands::resolve_board(app, board)?;
    destruct_on(
        app,
        board_id,
        "delete",
        confirm,
        StateCommand::DeleteBoard { board: board_id },
    )
    .await
}

/// The `boards reset` steps on a booted core.
pub(crate) async fn reset_on(
    app: &crate::App,
    board: Option<taskboard_domain::BoardId>,
    confirm: bool,
) -> Result<BoardDestructReceipt, CommandError> {
    let board_id = crate::commands::resolve_board(app, board)?;
    destruct_on(
        app,
        board_id,
        "reset",
        confirm,
        StateCommand::ResetBoard { board: board_id },
    )
    .await
}

/// The post-edit board projection shared by the edit-verb receipts,
/// rendered from the named board.
fn board_receipt(
    app: &crate::App,
    board_id: taskboard_domain::BoardId,
) -> Result<BoardReceipt, CommandError> {
    let state = app.engine.shared_state().load_full();
    let board = state.boards.get(&board_id).ok_or_else(|| {
        CommandError::Engine(Box::new(taskboard_state::ExecuteError::Rejected(
            taskboard_domain::CommandError::UnknownBoard(board_id),
        )))
    })?;
    Ok(BoardReceipt {
        id: board.id.as_uuid().to_string(),
        title: board.title.clone(),
        color: board.color.as_str().to_owned(),
    })
}

/// Sets the board color (both backends). Malformed input never reaches
/// here: clap's value parser makes it a usage error.
///
/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
pub async fn color(
    config: AppConfig,
    store: Arc<dyn CredentialStore>,
    board: Option<taskboard_domain::BoardId>,
    color: taskboard_domain::Color,
) -> Result<BoardReceipt, CommandError> {
    crate::commands::tasks::with_core(config, store, async move |app| {
        color_on(app, board, color).await
    })
    .await
}

/// Archives (`true`) or unarchives (`false`) the board (both backends).
///
/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
pub async fn archive(
    config: AppConfig,
    store: Arc<dyn CredentialStore>,
    board: Option<taskboard_domain::BoardId>,
    archived: bool,
) -> Result<BoardReceipt, CommandError> {
    crate::commands::tasks::with_core(config, store, async move |app| {
        archive_on(app, board, archived).await
    })
    .await
}

/// Deletes the board (both backends). The `--confirm` ceremony lives
/// here and in the session dispatcher; the domain command stays pure.
///
/// # Errors
///
/// [`CommandError`] — typed: [`CommandError::ConfirmationRequired`]
/// without `--confirm`, engine classes otherwise.
pub async fn delete(
    config: AppConfig,
    store: Arc<dyn CredentialStore>,
    board: Option<taskboard_domain::BoardId>,
    confirm: bool,
) -> Result<BoardDestructReceipt, CommandError> {
    crate::commands::tasks::with_core(config, store, async move |app| {
        delete_on(app, board, confirm).await
    })
    .await
}

/// Resets the board's local content (both backends); same ceremony as
/// [`delete`].
///
/// # Errors
///
/// [`CommandError`] — typed: [`CommandError::ConfirmationRequired`]
/// without `--confirm`, engine classes otherwise.
pub async fn reset(
    config: AppConfig,
    store: Arc<dyn CredentialStore>,
    board: Option<taskboard_domain::BoardId>,
    confirm: bool,
) -> Result<BoardDestructReceipt, CommandError> {
    crate::commands::tasks::with_core(config, store, async move |app| {
        reset_on(app, board, confirm).await
    })
    .await
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
/// [`CommandError`] — typed: the named-id board refusals /
/// `BoardAlreadyBound` (CLI-mapped), sync classes while the first cycle
/// settles.
pub async fn publish(
    config: AppConfig,
    store: Arc<dyn CredentialStore>,
    board: Option<taskboard_domain::BoardId>,
    timeout: Duration,
) -> Result<PublishReceipt, CommandError> {
    let client = publish_prelude(&config, &store)?;

    let app =
        crate::bootstrap::bootstrap(config, store, Arc::new(crate::notifications::LogSink)).await?;
    let result = publish_on(&app, &client, board, timeout).await;
    let _ = app.shutdown().await;
    result
}

/// The pre-boot half of `boards publish` (the standalone gate + the
/// client construction): shared verbatim by the one-shot wrapper and
/// the session dispatcher.
///
/// # Errors
///
/// [`CommandError`] — typed: `SyncDisabled`, credential/transport
/// classes.
pub(crate) fn publish_prelude(
    config: &AppConfig,
    store: &Arc<dyn CredentialStore>,
) -> Result<DeckClient, CommandError> {
    crate::commands::require_connector(config)?;
    // The client is built from config + store before the core boots
    // (the same shape `select` uses); the dispatcher already refused
    // the standalone backend.
    client_for(config, store)
}

/// The publish steps on a booted core; the caller owns the shutdown
/// (the session dispatcher calls this directly against the session core).
pub(crate) async fn publish_on(
    app: &crate::App,
    client: &DeckClient,
    board: Option<taskboard_domain::BoardId>,
    timeout: Duration,
) -> Result<PublishReceipt, CommandError> {
    let board_id = crate::commands::resolve_board(app, board)?;
    let state = app.engine.shared_state().load_full();
    // The id resolved a moment ago; a miss means the board died between
    // resolve and read (an engine contract break class, not user input).
    let Some(local) = state.boards.get(&board_id) else {
        return Err(CommandError::Engine(Box::new(
            taskboard_state::ExecuteError::Rejected(taskboard_domain::CommandError::UnknownBoard(
                board_id,
            )),
        )));
    };
    if let Some(current) = local.remote {
        return Err(CommandError::BoardAlreadyBound {
            board: board_id.as_uuid().to_string(),
            current: current.0,
        });
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
            board: board_id,
            remote: RemoteBoardId(created.id),
        })
        .await?;
    debug_assert!(matches!(
        outcome,
        CommandOutcome::Applied | CommandOutcome::NoOp
    ));
    send_set_board(app, created.id).await?;

    let startup = app.engine.shared_state().load_full().sync.aggregate();
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
    select_prelude(&config, &store, board).await?;

    let app =
        crate::bootstrap::bootstrap(config, store, Arc::new(crate::notifications::LogSink)).await?;
    let result = select_on(&app, board, timeout, bind_local_board).await;
    let _ = app.shutdown().await;
    result
}

/// The pre-boot half of `boards select` (the standalone gate + the
/// fresh-listing pre-validation): shared verbatim by the one-shot
/// wrapper and the session dispatcher.
///
/// # Errors
///
/// [`CommandError`] — typed: `SyncDisabled`, credential/transport
/// classes, [`CommandError::UnknownBoard`].
pub(crate) async fn select_prelude(
    config: &AppConfig,
    store: &Arc<dyn CredentialStore>,
    board: u64,
) -> Result<(), CommandError> {
    crate::commands::require_connector(config)?;
    // Pre-validation: a bogus id inside a sync cycle would only surface
    // as the generic `Failed { Server }` — check the fresh listing
    // first, before anything boots.
    let client = client_for(config, store)?;
    let listing = client.boards().await.map_err(map_deck_error)?;
    if !listing
        .iter()
        .any(|b| b.id == board && !b.archived && b.is_live())
    {
        return Err(CommandError::UnknownBoard(board));
    }
    Ok(())
}

/// Runs the binding steps on a booted core; the caller owns the
/// shutdown (the session dispatcher calls this directly against the
/// session core).
/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
pub async fn select_on(
    app: &crate::App,
    board: u64,
    timeout: Duration,
    bind_local_board: bool,
) -> Result<SelectReceipt, CommandError> {
    let state = app.engine.shared_state().load_full();
    // INTERIM(multi-board-2 → chunk-3): inline first-live-board resolve
    // (deterministic board-id order) for the migration-path `BindBoard`
    // until the ceremony redesign folds the pre-check into engine-refusal
    // pass-through. A board-less db takes the fresh adoption path below —
    // no local board exists to refuse about.
    let bound = state
        .boards
        .values()
        .filter(|b| !b.deleted)
        .min_by_key(|b| b.id);
    let local_board_id = bound.map(|b| b.id);
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
                sync: SyncStatusView::new(&state.sync.aggregate()),
            });
        }
        let current = remote.0;
        return Err(CommandError::BoardAlreadyBound {
            board: bound.id.as_uuid().to_string(),
            current,
        });
    }
    drop(state);

    if let Some(local_board_id) = local_board_id {
        if !bind_local_board {
            return Err(CommandError::LocalBoardExists);
        }
        // Migration: attach the local board onto the remote one. The
        // next pull matches by `remote` and merges instead of adopting
        // a duplicate; the outbox then drains onto the remote board.
        let outcome = app
            .engine
            .execute(StateCommand::BindBoard {
                board: local_board_id,
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
    let startup = app.engine.shared_state().load_full().sync.aggregate();
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
                sync: SyncStatusView::new(&state.sync.aggregate()),
            });
        }
        let fresh_failure = matches!(state.sync.aggregate().phase, SyncPhase::Failed { .. })
            && (seen_change || state.sync.aggregate() != *startup);
        if fresh_failure {
            let SyncPhase::Failed { last_error } = state.sync.aggregate().phase else {
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

/// The human-readable color/archive receipt.
#[must_use]
pub fn render_text_board_edit(receipt: &BoardReceipt) -> String {
    format!(
        "board \"{}\" is now {} ({})",
        receipt.title, receipt.color, receipt.id
    )
}

/// The human-readable delete/reset receipt.
#[must_use]
pub fn render_text_destruct(receipt: &BoardDestructReceipt) -> String {
    let mut line = format!(
        "{} board {} — {} stacks, {} tasks, {} labels, {} pending ops",
        receipt.verb,
        receipt.board,
        receipt.stacks,
        receipt.tasks,
        receipt.labels,
        receipt.pending_ops
    );
    if let Some(remote) = receipt.bound_remote {
        line.push_str("; bound remote board ");
        line.push_str(&remote.to_string());
        line.push_str(" is destroyed on the next cycle");
    }
    line
}
