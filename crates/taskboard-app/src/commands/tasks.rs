// SPDX-License-Identifier: AGPL-3.0-only
//! `taskboard tasks` (ADR 0009): the task verbs over the in-process
//! core. Every verb boots the core (one pull+push cycle rides along by
//! design), resolves selectors against the published state, executes a
//! domain command, prints the projection, and shuts the core down. The
//! `execute` reply means *durable locally* (outbox semantics); the
//! guarantee is "pushed by the next `sync`/daemon run".

use std::fmt::Write as _;
use std::sync::Arc;

use taskboard_domain::{AppState, Stack, StackId, StateCommand, TaskId};

use crate::commands::CommandError;
use crate::commands::output::{
    MutationReceipt, StackGroup, SyncStatusView, TaskList, mutation_receipt, task_row,
};
use crate::config::AppConfig;
use crate::secrets::CredentialStore;

/// Boots the core, runs `body`, and always shuts the core down.
async fn with_core<T>(
    config: AppConfig,
    store: Arc<dyn CredentialStore>,
    body: impl AsyncFnOnce(&crate::App) -> Result<T, CommandError>,
) -> Result<T, CommandError> {
    let app = crate::bootstrap::bootstrap(config, store).await?;
    let result = body(&app).await;
    let _ = app.shutdown().await;
    result
}

/// Resolves a stack selector: exact uuid wins, then exact live title;
/// `None` requires exactly one live stack.
fn resolve_stack(state: &AppState, selector: Option<&str>) -> Result<StackId, CommandError> {
    let live: Vec<&Stack> = state.stacks.values().filter(|s| !s.deleted).collect();
    let Some(selector) = selector else {
        return match live.len() {
            0 => Err(CommandError::NoStacks),
            1 => Ok(live[0].id),
            _ => Err(CommandError::AmbiguousStack),
        };
    };
    if let Ok(uuid) = uuid::Uuid::parse_str(selector) {
        let id = StackId::from_uuid(uuid);
        if live.iter().any(|s| s.id == id) {
            return Ok(id);
        }
    }
    let matches: Vec<StackId> = live
        .iter()
        .filter(|s| s.title == selector)
        .map(|s| s.id)
        .collect();
    match matches.len() {
        0 => Err(CommandError::UnknownStack(selector.to_owned())),
        1 => Ok(matches[0]),
        _ => Err(CommandError::AmbiguousStack),
    }
}

/// The append order for a stack: `max(order) + 1` over the canonical
/// task order, `0` when empty.
fn append_order(state: &AppState, stack: StackId) -> i64 {
    state
        .tasks_in_stack(stack)
        .last()
        .map_or(0, |t| t.order + 1)
}

/// The stack groups of the task list, in board order; an optional
/// selector restricts the listing to one stack.
fn stack_groups(state: &AppState, selector: Option<&str>) -> Result<Vec<StackGroup>, CommandError> {
    let selected = match selector {
        None => None,
        Some(sel) => Some(resolve_stack(state, Some(sel))?),
    };
    let mut stacks: Vec<&Stack> = state
        .stacks
        .values()
        .filter(|s| !s.deleted && selected.is_none_or(|id| s.id == id))
        .collect();
    stacks.sort_by_key(|s| (s.order, s.id));
    Ok(stacks
        .into_iter()
        .map(|s| StackGroup {
            id: s.id.as_uuid().to_string(),
            title: s.title.clone(),
            order: s.order,
            tasks: state
                .tasks_in_stack(s.id)
                .iter()
                .map(|t| task_row(state, t))
                .collect(),
        })
        .collect())
}

/// Lists the bound board's stacks and tasks with the sync footer.
///
/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
pub async fn list(
    config: AppConfig,
    store: Arc<dyn CredentialStore>,
    stack: Option<&str>,
) -> Result<TaskList, CommandError> {
    with_core(config, store, async |app| {
        let state = app.engine.shared_state().load_full();
        Ok(TaskList {
            stacks: stack_groups(&state, stack)?,
            sync: SyncStatusView::new(&state.sync),
        })
    })
    .await
}

/// Creates a task (appended to the target stack).
///
/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
pub async fn add(
    config: AppConfig,
    store: Arc<dyn CredentialStore>,
    title: &str,
    stack: Option<&str>,
) -> Result<MutationReceipt, CommandError> {
    let title = title.to_owned();
    with_core(config, store, async |app| {
        let state = app.engine.shared_state().load_full();
        let stack = resolve_stack(&state, stack)?;
        let order = append_order(&state, stack);
        app.engine
            .execute(StateCommand::CreateTask {
                title: title.clone(),
                stack,
                order,
            })
            .await?;
        let state = app.engine.shared_state().load_full();
        let task = live_task(&state, &title)?;
        Ok(mutation_receipt(&state, task))
    })
    .await
}

/// Completes (or with `undo`, reopens) a task. Repeats are idempotent
/// successes (`NoOp`): the receipt reflects the current state.
///
/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
pub async fn done(
    config: AppConfig,
    store: Arc<dyn CredentialStore>,
    task: &str,
    undo: bool,
) -> Result<MutationReceipt, CommandError> {
    let task_id = parse_task_id(task)?;
    with_core(config, store, async |app| {
        app.engine
            .execute(StateCommand::SetTaskDone {
                id: task_id,
                done: !undo,
            })
            .await?;
        let state = app.engine.shared_state().load_full();
        let task = state
            .tasks
            .get(&task_id)
            .filter(|t| t.is_live())
            .ok_or_else(|| CommandError::UnknownTask(task_id.as_uuid().to_string()))?;
        Ok(mutation_receipt(&state, task))
    })
    .await
}

/// Moves a task to a stack and/or position (one composite intent);
/// default appends to the end of the target stack.
///
/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
pub async fn move_(
    config: AppConfig,
    store: Arc<dyn CredentialStore>,
    task: &str,
    stack: Option<&str>,
    order: Option<i64>,
) -> Result<MutationReceipt, CommandError> {
    let task_id = parse_task_id(task)?;
    with_core(config, store, async |app| {
        let state = app.engine.shared_state().load_full();
        let stack = resolve_stack(&state, stack)?;
        let order = order.unwrap_or_else(|| append_order(&state, stack));
        app.engine
            .execute(StateCommand::MoveTask {
                id: task_id,
                stack,
                order,
            })
            .await?;
        let state = app.engine.shared_state().load_full();
        let task = state
            .tasks
            .get(&task_id)
            .filter(|t| t.is_live())
            .ok_or_else(|| CommandError::UnknownTask(task_id.as_uuid().to_string()))?;
        Ok(mutation_receipt(&state, task))
    })
    .await
}

/// The exact local task selector: an exact uuid, nothing else
/// (scripting-first M1; prefix selectors are backlog).
fn parse_task_id(raw: &str) -> Result<TaskId, CommandError> {
    uuid::Uuid::parse_str(raw)
        .map(TaskId::from_uuid)
        .map_err(|_| CommandError::UnknownTask(raw.to_owned()))
}

/// After a successful create, the task must be present and live in the
/// published state (the reply implies durability).
fn live_task<'a>(
    state: &'a AppState,
    title: &str,
) -> Result<&'a taskboard_domain::Task, CommandError> {
    state
        .tasks
        .values()
        .rev()
        .find(|t| t.is_live() && t.title == title)
        .map_or_else(|| Err(CommandError::UnknownTask(title.to_owned())), Ok)
}

/// The human-readable task list.
#[must_use]
pub fn render_text_list(list: &TaskList) -> String {
    let mut lines = Vec::new();
    for group in &list.stacks {
        lines.push(format!("== {} ==", group.title));
        for task in &group.tasks {
            let mut line = if task.done {
                format!("  [x] {}", task.title)
            } else {
                format!("  [ ] {}", task.title)
            };
            if let Some(due) = task.duedate {
                let _ = write!(line, "  (due {due})");
            }
            lines.push(line);
        }
    }
    lines.push(format!(
        "sync: {} ({} pending)",
        list.sync.phase, list.sync.pending_ops
    ));
    lines.join("\n")
}

/// The human-readable mutation receipt.
#[must_use]
pub fn render_text_receipt(receipt: &MutationReceipt) -> String {
    let state = if receipt.done { "done" } else { "open" };
    format!(
        "{} \"{}\" in {} at position {} ({})",
        receipt.id, receipt.title, receipt.stack_title, receipt.order, state
    )
}
