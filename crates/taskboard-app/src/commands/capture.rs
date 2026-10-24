// SPDX-License-Identifier: AGPL-3.0-only
//! `taskboard capture` (#347, chunk 10): the quick-capture verb over
//! the in-process core. No flag = the global Inbox
//! (`CaptureTask { board: None }`); `--board` = the board-directed
//! variant (that board's Backlog, created on demand). The engine
//! never errors on the happy path — resolution is its job.

use std::sync::Arc;

use crate::commands::output::{MutationReceipt, mutation_receipt};
use crate::commands::tasks::with_core;
use crate::commands::{CommandError, resolve_live_board};
use crate::config::AppConfig;
use crate::secrets::CredentialStore;

/// Runs `capture` over a freshly booted core.
///
/// # Errors
///
/// [`CommandError`] — typed; a bad `--board` selector is the one
/// user-facing rejection.
pub async fn capture(
    config: AppConfig,
    store: Arc<dyn CredentialStore>,
    title: String,
    board: Option<String>,
) -> Result<MutationReceipt, CommandError> {
    with_core(config, store, async move |app| {
        capture_on(app, title, board.as_deref()).await
    })
    .await
}

/// The `capture` steps on a booted core (the REPL/session seam): the
/// board selector resolves typed against the *live* boards, then the
/// `CaptureTask` reply names the task id and the receipt reads the
/// landing context back from state.
pub(crate) async fn capture_on(
    app: &crate::App,
    title: String,
    board: Option<&str>,
) -> Result<MutationReceipt, CommandError> {
    let state = app.engine.shared_state().load_full();
    let board = board
        .map(|selector| resolve_live_board(&state, selector))
        .transpose()?;
    drop(state);
    let outcome = app
        .engine
        .execute(taskboard_domain::StateCommand::CaptureTask {
            title: title.clone(),
            board,
        })
        .await?;
    let taskboard_domain::CommandOutcome::CreatedTask(task_id) = outcome else {
        return Err(CommandError::UnknownTask(title));
    };
    let state = app.engine.shared_state().load_full();
    let task = state
        .tasks
        .get(&task_id)
        .filter(|t| t.is_live())
        .ok_or_else(|| CommandError::UnknownTask(title))?;
    Ok(mutation_receipt(&state, task))
}
