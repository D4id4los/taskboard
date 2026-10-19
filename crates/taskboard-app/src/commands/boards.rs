// SPDX-License-Identifier: AGPL-3.0-only
//! `taskboard boards` (ADR 0009): `list` is a direct remote read (no
//! core boot, no lock — the first-run discovery surface); `select` is
//! the one-time single-board binding ceremony: pre-validate against a
//! fresh listing, boot the core, refuse a rebind, bind (the engine mints
//! the tagged binding), and wait for the adoption cycle that settles the
//! board's own sync row.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

use taskboard_domain::{CommandOutcome, RemoteBoardId, StateCommand, SyncPhase, SyncStatus};

use crate::commands::output::{
    BoardDestructReceipt, BoardReceipt, BoardsListEntry, PublishReceipt, ReconcileReceipt,
    SelectReceipt, SyncStatusView, UnbindReceipt,
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
    /// Live descendant refs (the rows a future different-remote rebind
    /// wipes; a same-remote rebind re-adopts them).
    descendant_refs: usize,
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
    let (board_id, bound_remote, stacks, tasks, board_labels, descendant_refs) = {
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
        let descendant_refs = state
            .stacks
            .values()
            .filter(|s| s.board == board.id && !s.deleted && s.remote.is_some())
            .count()
            + state
                .tasks
                .values()
                .filter(|t| t.is_live() && stacks.contains(&t.stack) && t.remote.is_some())
                .count()
            + state
                .labels
                .iter()
                .filter(|(_, l)| l.board == board.id && !l.deleted && l.remote.is_some())
                .count();
        (
            board.id,
            board.remote.as_ref().map(|r| r.board.0),
            stacks,
            tasks,
            labels,
            descendant_refs,
        )
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
        descendant_refs,
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

/// The `boards unbind` steps on a booted core: refuse without
/// `--confirm` (typed, stating the scope), else dispatch `UnbindBoard`
/// and report the landed receipt.
pub(crate) async fn unbind_on(
    app: &crate::App,
    board: Option<taskboard_domain::BoardId>,
    confirm: bool,
) -> Result<UnbindReceipt, CommandError> {
    let board_id = crate::commands::resolve_board(app, board)?;
    let scope = read_scope(app, board_id).await?;
    if !confirm {
        return Err(CommandError::UnbindConfirmationRequired {
            board: board_id.as_uuid().to_string(),
            bound_remote: scope.bound_remote,
            pending_ops: scope.pending_ops,
            descendant_refs: scope.descendant_refs,
        });
    }
    let outcome = app
        .engine
        .execute(StateCommand::UnbindBoard { board: board_id })
        .await?;
    let stranded_ops = match outcome {
        CommandOutcome::Unbound { stranded_ops } => stranded_ops,
        // Already unbound: the idempotent no-op receipt.
        CommandOutcome::NoOp => 0,
        other => unreachable!("UnbindBoard answers Unbound or NoOp, got {other:?}"),
    };
    Ok(UnbindReceipt {
        board: board_id.as_uuid().to_string(),
        bound_remote: scope.bound_remote,
        stranded_ops,
        kept_refs: scope.descendant_refs,
    })
}

/// The `boards reconcile` steps on a booted core: without `--confirm`,
/// the pure planner's counts ride the typed refusal (a count, not a
/// dry-run mode); with it, the pass lands and its counts are reported.
pub(crate) async fn reconcile_on(
    app: &crate::App,
    board: Option<taskboard_domain::BoardId>,
    confirm: bool,
) -> Result<ReconcileReceipt, CommandError> {
    let board_id = crate::commands::resolve_board(app, board)?;
    if !confirm {
        let state = app.engine.shared_state().load_full();
        let persisted = taskboard_domain::SyncStateReader::read_state(&app.engine)
            .await
            .map_err(|_| {
                CommandError::Engine(Box::new(taskboard_state::ExecuteError::EngineGone))
            })?;
        let (_, counts) = taskboard_domain::plan_reconcile(&state, &persisted.outbox, board_id);
        drop(state);
        return Err(CommandError::ReconcileConfirmationRequired {
            board: board_id.as_uuid().to_string(),
            stacks: counts.stacks,
            labels: counts.labels,
            tasks: counts.tasks,
            cancelled_ops: counts.cancelled_ops,
        });
    }
    let outcome = app
        .engine
        .execute(StateCommand::ReconcileByTitle { board: board_id })
        .await?;
    let (stacks, labels, tasks, cancelled_ops) = match outcome {
        CommandOutcome::Reconciled {
            stacks,
            labels,
            tasks,
            cancelled_ops,
        } => (stacks, labels, tasks, cancelled_ops),
        // Nothing to match: the idempotent zero-count receipt.
        CommandOutcome::NoOp => (0, 0, 0, 0),
        other => unreachable!("ReconcileByTitle answers Reconciled or NoOp, got {other:?}"),
    };
    Ok(ReconcileReceipt {
        board: board_id.as_uuid().to_string(),
        stacks,
        labels,
        tasks,
        cancelled_ops,
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

/// Unbinds the board (both backends). The `--confirm` ceremony lives
/// here and in the session dispatcher; the domain command stays pure.
///
/// # Errors
///
/// [`CommandError`] — typed: [`CommandError::UnbindConfirmationRequired`]
/// without `--confirm`, engine classes otherwise.
pub async fn unbind(
    config: AppConfig,
    store: Arc<dyn CredentialStore>,
    board: Option<taskboard_domain::BoardId>,
    confirm: bool,
) -> Result<UnbindReceipt, CommandError> {
    crate::commands::tasks::with_core(config, store, async move |app| {
        unbind_on(app, board, confirm).await
    })
    .await
}

/// Reconciles the board's same-title duplicates (both backends); same
/// ceremony as [`unbind`].
///
/// # Errors
///
/// [`CommandError`] — typed: [`CommandError::ReconcileConfirmationRequired`]
/// without `--confirm`, engine classes otherwise.
pub async fn reconcile(
    config: AppConfig,
    store: Arc<dyn CredentialStore>,
    board: Option<taskboard_domain::BoardId>,
    confirm: bool,
) -> Result<ReconcileReceipt, CommandError> {
    crate::commands::tasks::with_core(config, store, async move |app| {
        reconcile_on(app, board, confirm).await
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
    // The bound-board guard folds into the engine's typed refusal
    // *shape* (one rule, one shape): the remote create must not run for
    // an already-bound board (that would publish a duplicate), and the
    // engine cannot see it until after the create. The bind below
    // remains the rule's only enforcer for the racy case.
    if let Some(current) = local.remote.as_ref() {
        return Err(CommandError::Engine(Box::new(
            taskboard_state::ExecuteError::Rejected(
                taskboard_domain::CommandError::BoardAlreadyBound {
                    board: board_id,
                    bound: current.board,
                },
            ),
        )));
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

    // Bind locally, then nudge the engine: the next cycle's pull
    // matches by binding (merge, not duplicate) and the outbox drains
    // onto the fresh board. (A concurrent bind loses here as
    // `BoardAlreadyBound` / `RemoteBoardInUse` — the engine's rule.)
    let outcome = app
        .engine
        .execute(StateCommand::BindBoard {
            board: board_id,
            remote: RemoteBoardId(created.id),
        })
        .await?;
    let wiped_refs = match outcome {
        CommandOutcome::Bound { wiped_refs } => (wiped_refs > 0).then_some(wiped_refs),
        CommandOutcome::NoOp => None,
        other => unreachable!("BindBoard answers Bound or NoOp, got {other:?}"),
    };
    bind_nudge_wait_reconcile(
        app,
        board_id,
        RemoteBoardId(created.id),
        wiped_refs,
        timeout,
    )
    .await?;
    let state = app.engine.shared_state().load_full();
    let settled = state.boards[&board_id].clone();
    Ok(PublishReceipt {
        board: BoardsListEntry {
            id: created.id,
            title: created.title.clone(),
            archived: created.archived,
            deleted: !created.is_live(),
        },
        sync: SyncStatusView::new(state.sync.get(board_id).unwrap_or(&SyncStatus::default())),
        wiped_refs,
    })
    .map(|receipt| receipt_with_board(receipt, &settled))
}

/// Overlays the settled local board's title/flags onto the receipt (the
/// pull's field-wise merge may have adjusted the minted shell).
fn receipt_with_board(
    mut receipt: PublishReceipt,
    board: &taskboard_domain::Board,
) -> PublishReceipt {
    receipt.board.title.clone_from(&board.title);
    receipt.board.archived = board.archived;
    receipt.board.deleted = board.deleted;
    receipt
}

/// The binding ceremony: pre-validate (fresh listing, the validated
/// entry feeds the mint) → boot → resolve-or-mint the local board →
/// `BindBoard` → nudge → wait for that board's adoption → automatic
/// reconcile → receipt. With a live unbound local board and no
/// `--bind-local-board`, plain select refuses (`LocalBoardExists`); the
/// fresh-install path mints a shell board from the remote's title and
/// binds it (D3-1).
///
/// # Errors
///
/// [`CommandError`] — typed: [`CommandError::UnknownBoard`] before any
/// core boot, [`CommandError::LocalBoardExists`] for the ceremony gate,
/// the engine's typed refusals in pass-through
/// ([`CommandError::Engine`] wrapping `UnknownBoard` / `BoardDeleted` /
/// `BoardAlreadyBound` / `RemoteBoardInUse`),
/// [`CommandError::SyncFailed`]/[`CommandError::SyncTimedOut`] while
/// waiting for adoption.
pub async fn select(
    config: AppConfig,
    store: Arc<dyn CredentialStore>,
    board: u64,
    local_board: Option<taskboard_domain::BoardId>,
    timeout: Duration,
    bind_local_board: bool,
) -> Result<SelectReceipt, CommandError> {
    let entry = select_prelude(&config, &store, board).await?;

    let app =
        crate::bootstrap::bootstrap(config, store, Arc::new(crate::notifications::LogSink)).await?;
    let result = select_on(&app, entry, timeout, bind_local_board, local_board).await;
    let _ = app.shutdown().await;
    result
}

/// The pre-boot half of `boards select` (the standalone gate + the
/// fresh-listing pre-validation): shared verbatim by the one-shot
/// wrapper and the session dispatcher. Returns the validated listing
/// entry — its real title feeds the fresh path's mint (D3-1).
///
/// # Errors
///
/// [`CommandError`] — typed: `SyncDisabled`, credential/transport
/// classes, [`CommandError::UnknownBoard`].
pub(crate) async fn select_prelude(
    config: &AppConfig,
    store: &Arc<dyn CredentialStore>,
    board: u64,
) -> Result<BoardsListEntry, CommandError> {
    crate::commands::require_connector(config)?;
    // Pre-validation: a bogus id inside a sync cycle would only surface
    // as the generic `Failed { Server }` — check the fresh listing
    // first, before anything boots.
    let client = client_for(config, store)?;
    let listing = client.boards().await.map_err(map_deck_error)?;
    let entry = listing
        .iter()
        .find(|b| b.id == board && !b.archived && b.is_live())
        .map(|b| BoardsListEntry {
            id: b.id,
            title: b.title.clone(),
            archived: b.archived,
            deleted: !b.is_live(),
        });
    entry.ok_or(CommandError::UnknownBoard(board))
}

/// The bind ceremony's shared tail (D3-7): nudge the engine
/// (`RequestSync` — no targeted send exists anymore), wait for *this*
/// board's adoption, then run the ungated automatic reconcile (D3-8).
/// `wiped_refs` rides the receipt only when the bind actually cleared
/// some.
async fn bind_nudge_wait_reconcile(
    app: &crate::App,
    board_id: taskboard_domain::BoardId,
    remote: RemoteBoardId,
    wiped_refs: Option<usize>,
    timeout: Duration,
) -> Result<SelectReceipt, CommandError> {
    // The board-row snapshot for the freshness decision, taken before
    // the nudge (the S0 guard: a stale persisted phase must not mask
    // the fresh cycle).
    let startup_row = app
        .engine
        .shared_state()
        .load_full()
        .sync
        .get(board_id)
        .cloned();
    // The nudge: the engine fans `SyncNow` to every registered connector
    // and marks the bound boards `Syncing`. A connector-less core
    // answers `NoOp` — the honest `sync-unavailable` class.
    let outcome = app.engine.execute(StateCommand::RequestSync).await?;
    if outcome == CommandOutcome::NoOp {
        return Err(CommandError::SyncUnavailable);
    }
    let view = wait_for_adoption(app, board_id, remote, startup_row.as_ref(), timeout).await?;
    // The automatic post-adoption reconcile (ungated: the operator's
    // consent was the ceremony itself). A settled same-title sibling
    // merges here; a `NoOp` is the common shape.
    let _reconciled = app
        .engine
        .execute(StateCommand::ReconcileByTitle { board: board_id })
        .await?;
    Ok(SelectReceipt {
        board: view.board,
        sync: view.sync,
        wiped_refs,
    })
}

/// Runs the binding steps on a booted core; the caller owns the
/// shutdown (the session dispatcher calls this directly against the
/// session core). `entry` is the pre-validated listing entry; `flag`
/// (`--board`) names the local board when the operator pinned one.
///
/// # Errors
///
/// [`CommandError`] — typed, see [`select`].
pub async fn select_on(
    app: &crate::App,
    entry: BoardsListEntry,
    timeout: Duration,
    bind_local_board: bool,
    flag: Option<taskboard_domain::BoardId>,
) -> Result<SelectReceipt, CommandError> {
    let remote = RemoteBoardId(entry.id);
    // Resolution (D3-7): the `--board` flag wins; else the single live
    // board; else — and only for a board-less install — the fresh path
    // mints a shell from the remote's real title (D3-1).
    let resolved = match flag {
        Some(id) => Some(id),
        None => crate::commands::resolve_board_or_none_for_bind(app)?,
    };
    // The ceremony gate (`LocalBoardExists`): a live *unbound* local
    // board needs `--bind-local-board` before a bind merges remote
    // content alongside it. The engine cannot know the operator's
    // intent; the flag gate is UX policy, caller-owned (DF-2).
    if let Some(board_id) = resolved {
        let state = app.engine.shared_state().load_full();
        let unbound_live = state
            .boards
            .get(&board_id)
            .is_some_and(|b| !b.deleted && b.remote.is_none());
        drop(state);
        if unbound_live && !bind_local_board {
            return Err(CommandError::LocalBoardExists);
        }
    }
    let board_id = match resolved {
        Some(board_id) => board_id,
        None => mint_fresh_board(app, &entry).await?,
    };

    // Resolve-or-mint is done; the engine's typed refusals are the only
    // board-state rule from here (one rule, one home).
    let outcome = app
        .engine
        .execute(StateCommand::BindBoard {
            board: board_id,
            remote,
        })
        .await?;
    match outcome {
        // Same-remote re-select: the row is already settled — immediate
        // receipt from current state, no wait.
        CommandOutcome::NoOp => {
            let state = app.engine.shared_state().load_full();
            let settled = &state.boards[&board_id];
            Ok(SelectReceipt {
                board: BoardsListEntry {
                    id: entry.id,
                    title: settled.title.clone(),
                    archived: settled.archived,
                    deleted: settled.deleted,
                },
                sync: SyncStatusView::new(
                    state.sync.get(board_id).unwrap_or(&SyncStatus::default()),
                ),
                wiped_refs: None,
            })
        }
        CommandOutcome::Bound { wiped_refs } => {
            let receipt = bind_nudge_wait_reconcile(
                app,
                board_id,
                remote,
                (wiped_refs > 0).then_some(wiped_refs),
                timeout,
            )
            .await?;
            Ok(receipt)
        }
        other => unreachable!("BindBoard answers Bound or NoOp, got {other:?}"),
    }
}

/// The fresh-install path (D3-1): mint a shell board from the
/// remote's real title and bind it — the pull then merges by binding
/// (`RULE_R6`) and the epoch-clock shell adopts the remote's
/// title/color/archived.
async fn mint_fresh_board(
    app: &crate::App,
    entry: &BoardsListEntry,
) -> Result<taskboard_domain::BoardId, CommandError> {
    let outcome = app
        .engine
        .execute(StateCommand::CreateBoard {
            title: entry.title.clone(),
            color: taskboard_domain::Color::new(DEFAULT_BOARD_COLOR),
        })
        .await?;
    let CommandOutcome::CreatedBoard(board_id) = outcome else {
        // CreateBoard always mints (the planner's arm is total over its
        // inputs); unreachable from user input.
        unreachable!("CreateBoard always returns CreatedBoard");
    };
    Ok(board_id)
}

/// Predicate-polls (event-driven with a polling fallback) until *this
/// board's* sync row settles fresh (D3-11), a fresh sync failure lands,
/// or the deadline expires. Freshness is decided against the pre-nudge
/// snapshot of that board's row (the status is persisted and hydrated
/// at boot, so a board can boot carrying an old phase that nothing
/// clears until a cycle settles). Binding-present stays a conjunct of
/// success (the receipt promises both).
async fn wait_for_adoption(
    app: &crate::App,
    board: taskboard_domain::BoardId,
    remote: RemoteBoardId,
    startup_row: Option<&SyncStatus>,
    timeout: Duration,
) -> Result<SelectReceipt, CommandError> {
    let mut signals = app.engine.subscribe();
    let mut seen_signal = false;
    let deadline = Instant::now() + timeout;
    loop {
        let state = app.engine.shared_state().load_full();
        let row = state.sync.get(board);
        let phase = row.map_or(SyncPhase::Idle, |r| r.phase);
        let fresh_settled = phase != SyncPhase::Syncing && (seen_signal || row != startup_row);
        let binding_present = state
            .boards
            .get(&board)
            .is_some_and(|b| !b.deleted && b.remote.as_ref().map(|r| r.board) == Some(remote));
        if fresh_settled && phase == SyncPhase::Idle && binding_present {
            let adopted = &state.boards[&board];
            return Ok(SelectReceipt {
                board: BoardsListEntry {
                    id: remote.0,
                    title: adopted.title.clone(),
                    archived: adopted.archived,
                    deleted: adopted.deleted,
                },
                sync: SyncStatusView::new(row.unwrap_or(&SyncStatus::default())),
                wiped_refs: None,
            });
        }
        let fresh_failure = fresh_settled && matches!(phase, SyncPhase::Failed { .. });
        if fresh_failure {
            let SyncPhase::Failed { last_error } = phase else {
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
            seen_signal = true;
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

/// The human-readable select/publish receipt (the wipe count surfaces
/// only when the bind actually cleared refs).
#[must_use]
pub fn render_text_select(receipt: &SelectReceipt) -> String {
    let base = format!(
        "bound to board {} ({}) — sync: {}",
        receipt.board.id, receipt.board.title, receipt.sync.phase
    );
    match receipt.wiped_refs {
        Some(count) => format!(
            "{base} — {count} stale remote ref(s) wiped; stranded updates dead-letter on the next cycle if they cannot re-match"
        ),
        None => base,
    }
}

/// The human-readable unbind receipt.
#[must_use]
pub fn render_text_unbind(receipt: &UnbindReceipt) -> String {
    let mut line = format!(
        "unbound board {} — {} pending op(s) now wait for a re-bind",
        receipt.board, receipt.stranded_ops
    );
    if let Some(remote) = receipt.bound_remote {
        line.push_str("; was bound to remote board ");
        line.push_str(&remote.to_string());
    }
    line.push_str("; ");
    line.push_str(&receipt.kept_refs.to_string());
    line.push_str(" remote ref(s) kept (a same-remote re-bind re-adopts them)");
    line
}

/// The human-readable reconcile receipt.
#[must_use]
pub fn render_text_reconcile(receipt: &ReconcileReceipt) -> String {
    format!(
        "reconciled board {} — {} stacks, {} labels, {} tasks merged, {} duplicate create-op(s) cancelled",
        receipt.board, receipt.stacks, receipt.labels, receipt.tasks, receipt.cancelled_ops
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
