// SPDX-License-Identifier: AGPL-3.0-only
//! `taskboard tasks` (ADR 0009): the task verbs over the in-process
//! core. Every verb boots the core (one pull+push cycle rides along by
//! design), resolves selectors against the published state, executes a
//! domain command, prints the projection, and shuts the core down. The
//! `execute` reply means *durable locally* (outbox semantics); the
//! guarantee is "pushed by the next `sync`/daemon run".

use std::fmt::Write as _;
use std::sync::Arc;
use std::time::Duration;

use chrono::{TimeDelta, Utc};
use taskboard_domain::{
    AppState, CommandOutcome, LabelId, Stack, StackId, StateCommand, TaskChanges, TaskId,
};

use crate::commands::CommandError;
use crate::commands::output::{
    DueContext, JournalReceipt, JournalVerb, MutationReceipt, StackGroup, SyncStatusView, TaskList,
    mutation_receipt, task_row,
};
use crate::config::AppConfig;
use crate::secrets::CredentialStore;

/// The `tasks list` due-soon horizon: a fixed named constant, not a
/// config knob (the desktop strip owns its own value; a CLI constant is
/// not worth a config surface).
pub(crate) const DUE_SOON_HORIZON: TimeDelta = match TimeDelta::try_seconds(86_400) {
    Some(d) => d,
    None => panic!("24h is representable"),
};

/// Boots the core, gives the boot cycle's first pull a bounded chance to
/// land, runs `body`, and always shuts the core down.
///
/// Honesty note: the wait only helps when the cycle is *in flight at
/// command time* (a `Syncing` phase). A bound db that has ever synced
/// hydrates a settled status, so the wait returns immediately and the
/// verb serves the hydration snapshot — remote data newer than the last
/// sync shows up only after `taskboard sync` or the daemon. The boot
/// cycle's push still drains the outbox during the patient shutdown
/// join; durability is local the moment `execute` replies.
pub(crate) async fn with_core<T>(
    config: AppConfig,
    store: Arc<dyn CredentialStore>,
    body: impl AsyncFnOnce(&crate::App) -> Result<T, CommandError>,
) -> Result<T, CommandError> {
    let timeout = Duration::from_secs(config.app.shutdown_timeout_secs.max(1));
    let standalone = crate::connectors::configured_connectors(&config)
        .map_err(CommandError::from)?
        .is_empty();
    let sink = crate::bootstrap::resolve_notifier_sink(&config);
    let app = crate::bootstrap::bootstrap(config, store, sink).await?;
    // A connector-less core can never run a cycle: skip the boot-pull
    // wait entirely (it would only burn the shutdown budget).
    if !standalone {
        crate::bootstrap::wait_for_boot_pull(&app.engine, timeout).await;
    }
    let result = body(&app).await;
    let _ = app.shutdown().await;
    result
}

/// Resolves a stack selector: exact uuid wins, then exact live title;
/// `None` requires exactly one live stack.
pub(crate) fn resolve_stack(
    state: &AppState,
    selector: Option<&str>,
) -> Result<StackId, CommandError> {
    let mut live: Vec<&Stack> = state.stacks.values().filter(|s| !s.deleted).collect();
    live.sort_by_key(|s| (s.order, s.id));
    let candidates = || live.iter().map(|s| s.title.clone()).collect();
    let Some(selector) = selector else {
        return match live.len() {
            0 => Err(CommandError::NoStacks),
            1 => Ok(live[0].id),
            _ => Err(CommandError::AmbiguousStack {
                candidates: candidates(),
            }),
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
        _ => Err(CommandError::AmbiguousStack {
            candidates: candidates(),
        }),
    }
}

/// Resolves the *target* stack of a mutation: a board-less db has no
/// stacks by construction, and the honest typed answer there is the
/// sanctioned zero-live-boards refusal, not `NoStacks`.
fn resolve_target(state: &AppState, selector: Option<&str>) -> Result<StackId, CommandError> {
    let no_board = !state.boards.values().any(|b| !b.deleted);
    resolve_stack(state, selector).map_err(|err| match (err, no_board) {
        (CommandError::NoStacks, true) => CommandError::AmbiguousBoard { candidates: vec![] },
        (err, _) => err,
    })
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
    let now = Utc::now();
    let due = DueContext::compute(state, now);
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
            // The anchor card is hidden from list surfaces (DDF-5, the
            // presentation-layer exact-title filter); every derived
            // count follows because the filter sits at the row source.
            tasks: state
                .tasks_in_stack(s.id)
                .iter()
                .filter(|t| t.title != taskboard_domain::ANCHOR_CARD_TITLE)
                .map(|t| task_row(state, t, &due))
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
    with_core(config, store, async |app| list_on(app, stack)).await
}

/// The `tasks list` steps on a booted core (the REPL/session seam; the
/// one-shot wrapper owns the boot and the shutdown).
/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
pub fn list_on(app: &crate::App, stack: Option<&str>) -> Result<TaskList, CommandError> {
    let state = app.engine.shared_state().load_full();
    Ok(TaskList {
        stacks: stack_groups(&state, stack)?,
        sync: SyncStatusView::new(&state.sync.aggregate()),
    })
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
    let stack = stack.map(str::to_owned);
    with_core(config, store, async |app| {
        add_on(app, title, stack.as_deref()).await
    })
    .await
}

/// The `tasks add` steps on a booted core (the REPL/session seam).
pub(crate) async fn add_on(
    app: &crate::App,
    title: String,
    stack: Option<&str>,
) -> Result<MutationReceipt, CommandError> {
    let state = app.engine.shared_state().load_full();
    // The board-directed default (chunk 10 D10, the deliberate
    // landing change): with `--stack` omitted the verb resolves the
    // board, not the stack — exactly one live board ⇒ its Backlog
    // (created on demand via `CaptureTask`); zero live boards keeps
    // the typed refusal; multiple boards are the typed ambiguity
    // naming the candidates.
    if stack.is_none() {
        let live_boards: Vec<&taskboard_domain::Board> = state
            .boards
            .values()
            .filter(|b| !b.deleted && !b.archived)
            .collect();
        match live_boards.len() {
            1 => {
                let board = live_boards[0].id;
                let selector = board.as_uuid().to_string();
                drop(state);
                return crate::commands::capture::capture_on(app, title, Some(selector.as_str()))
                    .await;
            }
            0 => return Err(CommandError::AmbiguousBoard { candidates: vec![] }),
            _ => {
                return Err(CommandError::AmbiguousBoard {
                    candidates: live_boards
                        .iter()
                        .map(|b| b.id.as_uuid().to_string())
                        .collect(),
                });
            }
        }
    }
    let stack = resolve_target(&state, stack)?;
    let order = append_order(&state, stack);
    let outcome = app
        .engine
        .execute(StateCommand::CreateTask {
            title: title.clone(),
            stack,
            order,
        })
        .await?;
    // The engine's create reply carries the exact id — never
    // re-derive it by scanning titles (duplicate titles would
    // mis-attribute the receipt).
    let CommandOutcome::CreatedTask(task_id) = outcome else {
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
    let task = task.to_owned();
    with_core(config, store, async |app| done_on(app, &task, undo).await).await
}

/// The `tasks done` steps on a booted core (the REPL/session seam); the
/// selector resolves against the booted state (ADR 0009 cascade).
pub(crate) async fn done_on(
    app: &crate::App,
    task: &str,
    undo: bool,
) -> Result<MutationReceipt, CommandError> {
    let state = app.engine.shared_state().load_full();
    let task_id = resolve_task(&state, task)?;
    drop(state);
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
    board: Option<&str>,
    order: Option<i64>,
) -> Result<MutationReceipt, CommandError> {
    let task = task.to_owned();
    let stack = stack.map(str::to_owned);
    let board = board.map(str::to_owned);
    with_core(config, store, async |app| {
        move_on(app, &task, stack.as_deref(), board.as_deref(), order).await
    })
    .await
}

/// The `tasks move` steps on a booted core (the REPL/session seam).
pub(crate) async fn move_on(
    app: &crate::App,
    task: &str,
    stack: Option<&str>,
    board: Option<&str>,
    order: Option<i64>,
) -> Result<MutationReceipt, CommandError> {
    let state = app.engine.shared_state().load_full();
    let task_id = resolve_task(&state, task)?;
    // The optional board pre-filter (chunk 10 D10, D-V5's CLI-triage
    // detail): stack titles colliding across boards scope through the
    // named board; ambiguity without the filter errors as before.
    let stack = match (stack, board) {
        (Some(stack), Some(board)) => {
            let board = crate::commands::resolve_live_board(&state, board)?;
            let scoped: Vec<&Stack> = state
                .stacks
                .values()
                .filter(|s| !s.deleted && s.board == board)
                .collect();
            if let Ok(uuid) = uuid::Uuid::parse_str(stack) {
                let id = StackId::from_uuid(uuid);
                scoped
                    .iter()
                    .find(|s| s.id == id)
                    .map(|s| s.id)
                    .ok_or(CommandError::UnknownStack(stack.to_owned()))?
            } else {
                let matches: Vec<StackId> = scoped
                    .iter()
                    .filter(|s| s.title == stack)
                    .map(|s| s.id)
                    .collect();
                match matches.len() {
                    0 => Err(CommandError::UnknownStack(stack.to_owned()))?,
                    1 => matches[0],
                    // Even scoped, duplicate titles within one board
                    // stay ambiguous — the typed error names the
                    // colliding stacks' titles.
                    _ => {
                        let titles: Vec<String> = matches
                            .iter()
                            .filter_map(|id| state.stacks.get(id).map(|s| s.title.clone()))
                            .collect();
                        return Err(CommandError::AmbiguousStack { candidates: titles });
                    }
                }
            }
        }
        (stack, None) => resolve_target(&state, stack)?,
        // `--board` without `--stack` is a usage error at the clap
        // level by convention; typed here for the REPL's direct line.
        (None, Some(board)) => {
            let _ = crate::commands::resolve_live_board(&state, board)?;
            return Err(CommandError::AmbiguousStack { candidates: vec![] });
        }
    };
    let order = order.unwrap_or_else(|| append_order(&state, stack));
    drop(state);
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
}

/// Attaches a label to a task (selector composition: `resolve_task` +
/// `resolve_label` → `AssignLabel`). Assigning an already-assigned label
/// is an accepted no-op.
///
/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
pub async fn label(
    config: AppConfig,
    store: Arc<dyn CredentialStore>,
    task: &str,
    label: &str,
) -> Result<MutationReceipt, CommandError> {
    let task = task.to_owned();
    let label = label.to_owned();
    with_core(config, store, async |app| {
        label_on(app, &task, &label, |t, l| StateCommand::AssignLabel {
            task: t,
            label: l,
        })
        .await
    })
    .await
}

/// Detaches a label from a task. Unassigning an absent assignment is an
/// accepted no-op.
///
/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
pub async fn unlabel(
    config: AppConfig,
    store: Arc<dyn CredentialStore>,
    task: &str,
    label: &str,
) -> Result<MutationReceipt, CommandError> {
    let task = task.to_owned();
    let label = label.to_owned();
    with_core(config, store, async |app| {
        label_on(app, &task, &label, |t, l| StateCommand::UnassignLabel {
            task: t,
            label: l,
        })
        .await
    })
    .await
}

/// The label attach/detach steps on a booted core (the REPL/session
/// seam): both verbs compose the two selectors and project the
/// post-state receipt.
pub(crate) async fn label_on(
    app: &crate::App,
    task: &str,
    label: &str,
    command: impl Fn(TaskId, LabelId) -> StateCommand,
) -> Result<MutationReceipt, CommandError> {
    let state = app.engine.shared_state().load_full();
    let task_id = resolve_task(&state, task)?;
    let label_id = crate::commands::labels::resolve_label(&state, label)?;
    drop(state);
    app.engine.execute(command(task_id, label_id)).await?;
    let state = app.engine.shared_state().load_full();
    let task = state
        .tasks
        .get(&task_id)
        .filter(|t| t.is_live())
        .ok_or_else(|| CommandError::UnknownTask(task_id.as_uuid().to_string()))?;
    Ok(mutation_receipt(&state, task))
}

/// Edits a task's fields (sparse changeset). The CLI refuses the
/// zero-flag invocation at the parse boundary (the `task-edit-flags`
/// clap group); an empty changeset arriving programmatically is still
/// a typed no-op (`no_op`, exit 0), and a repeated identical edit is
/// likewise an accepted no-op (`NoOp`, exit 0).
///
/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
pub async fn edit(
    config: AppConfig,
    store: Arc<dyn CredentialStore>,
    task: &str,
    changes: TaskChanges,
) -> Result<MutationReceipt, CommandError> {
    let task = task.to_owned();
    with_core(config, store, async |app| {
        edit_on(app, &task, changes).await
    })
    .await
}

/// The `tasks edit` steps on a booted core (the REPL/session seam).
pub(crate) async fn edit_on(
    app: &crate::App,
    task: &str,
    changes: TaskChanges,
) -> Result<MutationReceipt, CommandError> {
    let state = app.engine.shared_state().load_full();
    let task_id = resolve_task(&state, task)?;
    // The anchor-title guard (chunk 4's D16): a rename of the app-data
    // anchor is refused before the engine sees it. Re-asserting the
    // identical title stays a no-op edit (the engine's no-op path).
    if let Some(new_title) = &changes.title
        && state
            .tasks
            .get(&task_id)
            .is_some_and(|task| task.title == taskboard_domain::ANCHOR_CARD_TITLE)
        && new_title != taskboard_domain::ANCHOR_CARD_TITLE
    {
        return Err(CommandError::AnchorTaskReserved);
    }
    drop(state);
    app.engine
        .execute(StateCommand::UpdateTask {
            id: task_id,
            changes,
        })
        .await?;
    let state = app.engine.shared_state().load_full();
    let task = state
        .tasks
        .get(&task_id)
        .filter(|t| t.is_live())
        .ok_or_else(|| CommandError::UnknownTask(task_id.as_uuid().to_string()))?;
    Ok(mutation_receipt(&state, task))
}

/// Deletes (tombstones) a task. Repeats are idempotent successes: a
/// repeat reports the already-deleted entity's snapshot.
///
/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
///
/// # Panics
///
/// Never in practice: resolution guarantees the snapshot lookup hits
/// (`expect` guards an engine contract break, not user input).
pub async fn delete(
    config: AppConfig,
    store: Arc<dyn CredentialStore>,
    task: &str,
) -> Result<crate::commands::output::TaskDeleteReceipt, CommandError> {
    let task = task.to_owned();
    with_core(config, store, async |app| delete_on(app, &task).await).await
}

/// The `tasks delete` steps on a booted core (the REPL/session seam).
pub(crate) async fn delete_on(
    app: &crate::App,
    task: &str,
) -> Result<crate::commands::output::TaskDeleteReceipt, CommandError> {
    let state = app.engine.shared_state().load_full();
    // Terminal intent: a repeat must still resolve its target, so a
    // tombstoned task stays reachable by exact uuid (the live cascade
    // deliberately hides tombstones from prefix/title selection).
    let task_id = resolve_task(&state, task).or_else(|err| match err {
        CommandError::UnknownTask(_) => resolve_task_exact_any(&state, task),
        other => Err(other),
    })?;
    // The anchor-title guard (D16): deleting the app-data anchor is
    // refused typedly — exact-uuid-any resolution included, which is
    // exactly the accident the guard exists for.
    if state
        .tasks
        .get(&task_id)
        .is_some_and(|task| task.title == taskboard_domain::ANCHOR_CARD_TITLE)
    {
        return Err(CommandError::AnchorTaskReserved);
    }
    // Resolution guarantees the row exists (the live cascade or the
    // exact-uuid-any fallback both hit `contains_key`).
    let snapshot = state
        .tasks
        .get(&task_id)
        .map(|t| crate::commands::output::TaskDeleteReceipt {
            id: t.id.as_uuid().to_string(),
            title: t.title.clone(),
        })
        .expect("resolved task exists");
    drop(state);
    app.engine
        .execute(StateCommand::DeleteTask { id: task_id })
        .await?;
    Ok(snapshot)
}

/// Resolves a task selector (the cascading selector, ADR 0009): exact
/// uuid → unique ≥8-char uuid prefix → exact live-title match. Ambiguity
/// and misses are typed errors carrying the candidate titles. Exact uuid
/// stays first, so existing scripts keep working; a uuid-shaped selector
/// that hits no live task falls through to prefix/title rather than
/// erroring (`UUIDv7` prefixes can collide within about a minute — the honest
/// answer is then the ambiguity error, not a silent mis-target).
pub(crate) fn resolve_task(state: &AppState, selector: &str) -> Result<TaskId, CommandError> {
    let mut live: Vec<&taskboard_domain::Task> =
        state.tasks.values().filter(|t| t.is_live()).collect();
    live.sort_by_key(|t| (t.stack, t.order, t.id));
    if let Ok(uuid) = uuid::Uuid::parse_str(selector) {
        let id = TaskId::from_uuid(uuid);
        if live.iter().any(|t| t.id == id) {
            return Ok(id);
        }
    }
    let candidates = |matched: &[&taskboard_domain::Task]| -> Vec<String> {
        matched.iter().map(|t| t.title.clone()).collect()
    };
    if selector.len() >= 8 {
        let needle = selector.to_lowercase();
        let prefix_hits: Vec<&taskboard_domain::Task> = live
            .iter()
            .filter(|t| t.id.as_uuid().to_string().starts_with(&needle))
            .copied()
            .collect();
        match prefix_hits.len() {
            1 => return Ok(prefix_hits[0].id),
            n if n > 1 => {
                return Err(CommandError::AmbiguousTask {
                    candidates: candidates(&prefix_hits),
                });
            }
            _ => {}
        }
    }
    let title_hits: Vec<&taskboard_domain::Task> = live
        .iter()
        .filter(|t| t.title == selector)
        .copied()
        .collect();
    match title_hits.len() {
        0 => Err(CommandError::UnknownTask(selector.to_owned())),
        1 => Ok(title_hits[0].id),
        _ => Err(CommandError::AmbiguousTask {
            candidates: candidates(&title_hits),
        }),
    }
}

/// Exact-uuid match over *all* tasks (tombstones included) — the
/// delete-repeat affordance.
fn resolve_task_exact_any(state: &AppState, selector: &str) -> Result<TaskId, CommandError> {
    if let Ok(uuid) = uuid::Uuid::parse_str(selector) {
        let id = TaskId::from_uuid(uuid);
        if state.tasks.contains_key(&id) {
            return Ok(id);
        }
    }
    Err(CommandError::UnknownTask(selector.to_owned()))
}

/// The human-readable task list; under the resolved `standalone` backend
/// an informational footer names the outbox depth (presentation-only —
/// the JSON payload is unchanged).
#[must_use]
pub fn render_text_list(list: &TaskList, standalone: bool) -> String {
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
                // Banner glyphs (documented in the verb help): `!`
                // overdue, `~` due within DUE_SOON_HORIZON.
                match task.due_marker {
                    Some(crate::commands::output::DueMarker::Overdue) => {
                        let _ = write!(line, " !");
                    }
                    Some(crate::commands::output::DueMarker::DueSoon) => {
                        let _ = write!(line, " ~");
                    }
                    None => {}
                }
            }
            // The field-family segments: each omitted when absent, the
            // canonical domain text forms (never re-spelled here).
            if let Some(scheduled) = task.scheduled {
                let _ = write!(line, "  (starts {scheduled})");
            }
            if let Some(effort) = task.effort {
                let _ = write!(
                    line,
                    "  {}",
                    taskboard_domain::Effort::new(effort).text_form()
                );
            }
            if let Some(importance) = &task.importance {
                let _ = write!(line, "  {importance}");
            }
            if let Some(urgency) = &task.urgency {
                let _ = write!(line, "  {urgency}");
            }
            lines.push(line);
        }
    }
    lines.push(format!(
        "sync: {} ({} pending)",
        list.sync.phase, list.sync.pending_ops
    ));
    if let Some(note) = crate::commands::output::standalone_note(standalone, list.sync.pending_ops)
    {
        lines.push(note);
    }
    lines.join("\n")
}

/// The human-readable mutation receipt.
#[must_use]
pub fn render_text_receipt(receipt: &MutationReceipt) -> String {
    let state = if receipt.done { "done" } else { "open" };
    let mut line = format!(
        "{} \"{}\" in {} at position {} ({})",
        receipt.id, receipt.title, receipt.stack_title, receipt.order, state
    );
    if !receipt.labels.is_empty() {
        let _ = write!(line, " [{}]", receipt.labels.join(", "));
    }
    line
}

/// The human-readable undo/redo receipt: the label when the published
/// window still carries it, the bare entry uuid otherwise (text is UX,
/// never asserted).
#[must_use]
pub fn render_text_undo(receipt: &JournalReceipt) -> String {
    let verb = match receipt.verb {
        JournalVerb::Undo => "undone",
        JournalVerb::Redo => "redone",
    };
    match &receipt.label {
        Some(label) => format!("{verb} \"{label}\" ({})", receipt.entry),
        None => format!("{verb} {}", receipt.entry),
    }
}

/// Undoes the last change (the journal line head, ADR 0019); the
/// engine's typed refusals (`NothingToUndo`, `TargetNotUndoable`, …)
/// pass through untouched.
///
/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
pub async fn undo(
    config: AppConfig,
    store: Arc<dyn CredentialStore>,
) -> Result<JournalReceipt, CommandError> {
    with_core(config, store, async |app| undo_on(app).await).await
}

/// Redoes the last undone change (the head's newest child, ADR 0019).
///
/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
pub async fn redo(
    config: AppConfig,
    store: Arc<dyn CredentialStore>,
) -> Result<JournalReceipt, CommandError> {
    with_core(config, store, async |app| redo_on(app).await).await
}

/// The receipt for a journal verb: the outcome's entry id plus the
/// best-effort label from the published `UndoView.recent` window (the
/// just-touched entry is the most recently appended one, so it is
/// in-window in every reachable v1 case — the `Option` keeps the type
/// total without coupling to window arithmetic).
fn journal_receipt(
    app: &crate::App,
    verb: JournalVerb,
    entry: taskboard_domain::JournalId,
) -> JournalReceipt {
    let state = app.engine.shared_state().load_full();
    let label = state
        .undo
        .recent
        .iter()
        .find(|l| l.id == entry)
        .map(|l| l.label.clone());
    JournalReceipt {
        verb,
        entry: entry.as_uuid().to_string(),
        label,
    }
}

/// The `tasks undo` step on a booted core (the REPL/session seam).
///
/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
pub(crate) async fn undo_on(app: &crate::App) -> Result<JournalReceipt, CommandError> {
    match app.engine.execute(StateCommand::Undo).await? {
        taskboard_domain::CommandOutcome::Undone { entry } => {
            Ok(journal_receipt(app, JournalVerb::Undo, entry))
        }
        _ => unreachable!("Undo plans into Undone and typed refusals only"),
    }
}

/// The `tasks redo` step on a booted core (the REPL/session seam).
///
/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
pub(crate) async fn redo_on(app: &crate::App) -> Result<JournalReceipt, CommandError> {
    match app.engine.execute(StateCommand::Redo).await? {
        taskboard_domain::CommandOutcome::Redone { entry } => {
            Ok(journal_receipt(app, JournalVerb::Redo, entry))
        }
        _ => unreachable!("Redo plans into Redone and typed refusals only"),
    }
}
