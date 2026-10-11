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

use taskboard_domain::{
    AppState, CommandOutcome, LabelId, Stack, StackId, StateCommand, SyncPhase, TaskChanges, TaskId,
};

use crate::commands::CommandError;
use crate::commands::output::{
    MutationReceipt, StackGroup, SyncStatusView, TaskList, mutation_receipt, task_row,
};
use crate::config::AppConfig;
use crate::secrets::CredentialStore;

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
    let standalone = config.sync_backend() == crate::config::SyncBackend::Standalone;
    let app = crate::bootstrap::bootstrap(config, store).await?;
    // A connector-less core can never run a cycle: skip the boot-pull
    // wait entirely (it would only burn the shutdown budget).
    if !standalone {
        wait_for_boot_pull(&app, timeout).await;
    }
    let result = body(&app).await;
    let _ = app.shutdown().await;
    result
}

/// Gives the boot cycle's first pull (a `Syncing` → settled
/// transition) a bounded chance to land before the verb reads the
/// state. Only a cycle *in flight right now* is awaited; a settled
/// hydrated status (the common case) returns immediately — freshness
/// beyond the last sync is `taskboard sync`'s job. A board-less db can
/// only ever skip silently (`NoBoard`), so it returns immediately; a
/// deadline expiry degrades gracefully — offline-first: local data
/// serves the command.
async fn wait_for_boot_pull(app: &crate::App, timeout: Duration) {
    let mut signals = app.engine.subscribe();
    let deadline = tokio::time::Instant::now() + timeout;
    let mut seen_syncing = false;
    loop {
        let state = app.engine.shared_state().load_full();
        match state.sync.phase {
            SyncPhase::Syncing => seen_syncing = true,
            _ if seen_syncing => return,
            // The cycle already settled before we started watching.
            _ if state.sync.last_success.is_some()
                || matches!(
                    state.sync.phase,
                    SyncPhase::Offline | SyncPhase::Failed { .. }
                ) =>
            {
                return;
            }
            // No live board: the cycle can only skip silently.
            _ if !state.boards.values().any(|b| !b.deleted) => return,
            _ => {}
        }
        if tokio::time::Instant::now() >= deadline {
            tracing::warn!("boot pull did not settle in time; serving local data");
            return;
        }
        let _ = tokio::time::timeout(Duration::from_millis(200), signals.recv()).await;
    }
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
/// engine's own `NoBoard` rejection, not `NoStacks`.
fn resolve_target(state: &AppState, selector: Option<&str>) -> Result<StackId, CommandError> {
    let no_board = !state.boards.values().any(|b| !b.deleted);
    resolve_stack(state, selector).map_err(|err| match (err, no_board) {
        (CommandError::NoStacks, true) => CommandError::Engine(Box::new(
            taskboard_state::ExecuteError::Rejected(taskboard_domain::CommandError::NoBoard),
        )),
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
    let task = task.to_owned();
    with_core(config, store, async |app| {
        let state = app.engine.shared_state().load_full();
        let task_id = resolve_task(&state, &task)?;
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
    let task = task.to_owned();
    with_core(config, store, async |app| {
        let state = app.engine.shared_state().load_full();
        let task_id = resolve_task(&state, &task)?;
        let stack = resolve_target(&state, stack)?;
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
    })
    .await
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
    selector_label(config, store, task, label, |t, l| {
        StateCommand::AssignLabel { task: t, label: l }
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
    selector_label(config, store, task, label, |t, l| {
        StateCommand::UnassignLabel { task: t, label: l }
    })
    .await
}

/// The shared label attach/detach body: both verbs compose the two
/// selectors and project the post-state receipt.
async fn selector_label(
    config: AppConfig,
    store: Arc<dyn CredentialStore>,
    task: &str,
    label: &str,
    command: impl Fn(TaskId, LabelId) -> StateCommand,
) -> Result<MutationReceipt, CommandError> {
    let task = task.to_owned();
    let label = label.to_owned();
    with_core(config, store, async |app| {
        let state = app.engine.shared_state().load_full();
        let task_id = resolve_task(&state, &task)?;
        let label_id = crate::commands::labels::resolve_label(&state, &label)?;
        drop(state);
        app.engine.execute(command(task_id, label_id)).await?;
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

/// Exact-uuid match over *all* tasks (tombstones included) — the/// Exact-uuid match over *all* tasks (tombstones included) — the
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

/// Edits a task's fields (sparse changeset; at least one flag required —
/// enforced by clap as a usage error). A repeated identical edit is an
/// accepted no-op (`NoOp`, exit 0).
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
        let state = app.engine.shared_state().load_full();
        let task_id = resolve_task(&state, &task)?;
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
    })
    .await
}

/// Deletes (tombstones) a task. Repeats are idempotent successes: a
/// repeat reports the already-deleted entity's snapshot.
///
/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
pub async fn delete(
    config: AppConfig,
    store: Arc<dyn CredentialStore>,
    task: &str,
) -> Result<crate::commands::output::TaskDeleteReceipt, CommandError> {
    let task = task.to_owned();
    with_core(config, store, async |app| {
        let state = app.engine.shared_state().load_full();
        // Terminal intent: a repeat must still resolve its target, so a
        // tombstoned task stays reachable by exact uuid (the live cascade
        // deliberately hides tombstones from prefix/title selection).
        let task_id = resolve_task(&state, &task).or_else(|err| match err {
            CommandError::UnknownTask(_) => resolve_task_exact_any(&state, &task),
            other => Err(other),
        })?;
        let snapshot = state
            .tasks
            .get(&task_id)
            .map(|t| crate::commands::output::TaskDeleteReceipt {
                id: t.id.as_uuid().to_string(),
                title: t.title.clone(),
            })
            .unwrap_or(crate::commands::output::TaskDeleteReceipt {
                id: task_id.as_uuid().to_string(),
                title: String::new(),
            });
        drop(state);
        app.engine
            .execute(StateCommand::DeleteTask { id: task_id })
            .await?;
        Ok(snapshot)
    })
    .await
}

/// Resolves a task selector (the cascading selector, ADR 0009): exact
/// uuid → unique ≥8-char uuid prefix → exact live-title match. Ambiguity
/// and misses are typed errors carrying the candidate titles. Exact uuid
/// stays first, so existing scripts keep working; a uuid-shaped selector
/// that hits no live task falls through to prefix/title rather than
/// erroring (`UUIDv7` prefixes can collide within about a minute — the honest
/// answer is then the ambiguity error, not a silent mis-target).
fn resolve_task(state: &AppState, selector: &str) -> Result<TaskId, CommandError> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use taskboard_domain::{StackId, TaskClocks, TaskId};
    use uuid::Uuid;

    fn ts(secs: i64) -> chrono::DateTime<chrono::Utc> {
        use chrono::TimeZone as _;
        chrono::Utc.timestamp_opt(secs, 0).unwrap()
    }

    fn task(raw: u128, title: &str, order: i64) -> taskboard_domain::Task {
        taskboard_domain::Task {
            id: TaskId::from(Uuid::from_u128(raw)),
            remote: None,
            title: title.to_owned(),
            description: String::new(),
            duedate: None,
            done: None,
            stack: StackId::from(Uuid::from_u128(2)),
            order,
            labels: std::collections::BTreeSet::default(),
            archived: false,
            deleted: false,
            clocks: TaskClocks {
                title: ts(100),
                description: ts(100),
                duedate: ts(100),
                done: ts(100),
                position: ts(100),
                labels: ts(100),
                archived: ts(100),
                deleted: ts(100),
            },
            remote_seen: None,
        }
    }

    fn state_with(tasks: Vec<taskboard_domain::Task>) -> AppState {
        let mut state = AppState::default();
        for t in tasks {
            state.tasks.insert(t.id, t);
        }
        state
    }

    #[test]
    fn exact_uuid_wins_even_when_a_title_also_matches() {
        let a = task(0x100, "alpha", 0);
        let id = a.id;
        let mut state = state_with(vec![a, task(0x200, "alpha", 1)]);
        state
            .tasks
            .get_mut(&TaskId::from(Uuid::from_u128(0x200)))
            .unwrap()
            .title = "alpha".to_owned();
        assert!(matches!(resolve_task(&state, &id.as_uuid().to_string()), Ok(got) if got == id));
    }

    #[test]
    fn unique_eight_char_prefix_resolves() {
        let a = task(0x1234_0000_0000_0000_0000_0000_0000_0000, "alpha", 0);
        let id = a.id;
        let state = state_with(vec![
            a,
            task(0xabcd_0000_0000_0000_0000_0000_0000_0000, "beta", 1),
        ]);
        let selector = id.as_uuid().to_string()[..8].to_owned();
        assert!(matches!(resolve_task(&state, &selector), Ok(got) if got == id));
    }

    #[test]
    fn colliding_prefix_is_an_ambiguity_error_listing_titles() {
        // Same millisecond of UUIDv7 time ⇒ shared ≥12-char prefixes; the
        // minimum-length shared prefix is engineered via adjacent u128s.
        let a = task(0x0000_0000_0000_00aa, "alpha", 0);
        let b = task(0x0000_0000_0000_00bb, "beta", 1);
        let state = state_with(vec![a, b]);
        // Both hyphenated forms share the leading "00000000-0000-0000"
        // (19 chars ≥ 8): a prefix that matches both must be ambiguous.
        let shared = "00000000";
        let err = resolve_task(&state, shared).expect_err("ambiguous");
        match err {
            CommandError::AmbiguousTask { candidates } => {
                assert_eq!(candidates.len(), 2);
                assert!(candidates.contains(&"alpha".to_owned()));
                assert!(candidates.contains(&"beta".to_owned()));
            }
            other => panic!("expected AmbiguousTask, got {other:?}"),
        }
    }

    #[test]
    fn exact_title_resolves_and_duplicates_are_ambiguous() {
        let a = task(0x100, "alpha", 0);
        let id = a.id;
        assert!(
            matches!(resolve_task(&state_with(vec![a]), "alpha"), Ok(got) if got == id),
            "unique title resolves"
        );
        let err = resolve_task(
            &state_with(vec![task(0x100, "alpha", 0), task(0x200, "alpha", 1)]),
            "alpha",
        )
        .expect_err("duplicate titles ambiguous");
        assert!(
            matches!(err, CommandError::AmbiguousTask { ref candidates } if candidates.len() == 2)
        );
    }

    #[test]
    fn unknown_selector_is_typed_and_tombstones_are_invisible() {
        assert!(matches!(
            resolve_task(&state_with(vec![task(0x100, "alpha", 0)]), "nope"),
            Err(CommandError::UnknownTask(sel)) if sel == "nope"
        ));
        let mut dead = task(0x100, "alpha", 0);
        dead.deleted = true;
        assert!(
            matches!(
                resolve_task(&state_with(vec![dead]), "alpha"),
                Err(CommandError::UnknownTask(sel)) if sel == "alpha"
            ),
            "a tombstoned task is not a title hit"
        );
    }

    #[test]
    fn a_uuid_shaped_selector_that_hits_nothing_falls_through() {
        // A full uuid of a task that does not exist must not error out of
        // the cascade: the fall-through keeps prefix/title working.
        let a = task(0x100, "00001111", 0);
        let id = a.id;
        let state = state_with(vec![a]);
        let missing = Uuid::from_u128(0xdead_beef).to_string();
        assert!(matches!(
            resolve_task(&state, &missing),
            Err(CommandError::UnknownTask(_))
        ));
        assert!(matches!(resolve_task(&state, "00001111"), Ok(got) if got == id));
    }
}
