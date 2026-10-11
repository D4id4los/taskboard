// SPDX-License-Identifier: AGPL-3.0-only
//! `taskboard stacks` (ADR 0009): the minimum stack verb that makes
//! stand-alone usable (`stacks add`); WI-3 owns the rest of the
//! stack/label verbs. Board-less dbs surface the engine's own `NoBoard`
//! rejection (the `no-board` kind).

use std::sync::Arc;

use taskboard_domain::{CommandOutcome, StackId, StateCommand};

use crate::commands::CommandError;
use crate::commands::output::{StackDeleteReceipt, StackReceipt};
use crate::config::AppConfig;
use crate::secrets::CredentialStore;

/// Creates a stack on the live board; `order: None` appends
/// (`max(order) + 1` across the board's live stacks, canonical order).
///
/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
pub async fn add(
    config: AppConfig,
    store: Arc<dyn CredentialStore>,
    title: &str,
    order: Option<i64>,
) -> Result<StackReceipt, CommandError> {
    let title = title.to_owned();
    crate::commands::tasks::with_core(config, store, async |app| {
        let state = app.engine.shared_state().load_full();
        let order = order.unwrap_or_else(|| {
            state
                .stacks
                .values()
                .filter(|s| !s.deleted)
                .map(|s| s.order)
                .max()
                .map_or(0, |max| max + 1)
        });
        drop(state);
        let outcome = app
            .engine
            .execute(StateCommand::CreateStack {
                title: title.clone(),
                order,
            })
            .await?;
        let CommandOutcome::CreatedStack(stack_id) = outcome else {
            return Err(CommandError::Engine(Box::new(
                taskboard_state::ExecuteError::Rejected(taskboard_domain::CommandError::NoBoard),
            )));
        };
        Ok(StackReceipt {
            id: stack_id.as_uuid().to_string(),
            title,
            order,
        })
    })
    .await
}

/// Resolves a stack selector against the published state (shared with
/// the task verbs; the board-less mapping lives in `resolve_target`).
fn resolve(state: &taskboard_domain::AppState, selector: &str) -> Result<StackId, CommandError> {
    crate::commands::tasks::resolve_stack(state, Some(selector))
}

/// Renames a stack. Repeats with the same title are accepted no-ops.
///
/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
pub async fn rename(
    config: AppConfig,
    store: Arc<dyn CredentialStore>,
    stack: &str,
    title: &str,
) -> Result<StackReceipt, CommandError> {
    let stack = stack.to_owned();
    let title = title.to_owned();
    crate::commands::tasks::with_core(config, store, async |app| {
        let state = app.engine.shared_state().load_full();
        let id = resolve(&state, &stack)?;
        drop(state);
        app.engine
            .execute(StateCommand::RenameStack {
                id,
                new_title: title.clone(),
            })
            .await?;
        let state = app.engine.shared_state().load_full();
        let row = state
            .stacks
            .get(&id)
            .filter(|s| !s.deleted)
            .ok_or_else(|| CommandError::UnknownStack(stack))?;
        Ok(StackReceipt {
            id: row.id.as_uuid().to_string(),
            title: row.title.clone(),
            order: row.order,
        })
    })
    .await
}

/// Deletes (tombstones) a stack, cascading to its live tasks. Repeats
/// are idempotent successes reporting the already-deleted snapshot.
///
/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
pub async fn delete(
    config: AppConfig,
    store: Arc<dyn CredentialStore>,
    stack: &str,
) -> Result<StackDeleteReceipt, CommandError> {
    let stack = stack.to_owned();
    crate::commands::tasks::with_core(config, store, async |app| {
        let state = app.engine.shared_state().load_full();
        // Terminal intent: a repeat still resolves its target, so a
        // tombstoned stack stays reachable by exact uuid.
        let id = resolve(&state, &stack).or_else(|err| match err {
            CommandError::UnknownStack(_) => {
                if let Ok(uuid) = uuid::Uuid::parse_str(&stack) {
                    let id = StackId::from_uuid(uuid);
                    if state.stacks.contains_key(&id) {
                        return Ok(id);
                    }
                }
                Err(CommandError::UnknownStack(stack.clone()))
            }
            other => Err(other),
        })?;
        let snapshot = state.stacks.get(&id).map(|row| StackDeleteReceipt {
            id: row.id.as_uuid().to_string(),
            title: row.title.clone(),
            cascaded_tasks: state.tasks_in_stack(id).len(),
        });
        drop(state);
        app.engine.execute(StateCommand::DeleteStack { id }).await?;
        Ok(snapshot.unwrap_or(StackDeleteReceipt {
            id: id.as_uuid().to_string(),
            title: String::new(),
            cascaded_tasks: 0,
        }))
    })
    .await
}

/// Reorders a stack within the board. Repeats with the same order are
/// accepted no-ops.
///
/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
pub async fn move_(
    config: AppConfig,
    store: Arc<dyn CredentialStore>,
    stack: &str,
    order: i64,
) -> Result<StackReceipt, CommandError> {
    let stack = stack.to_owned();
    crate::commands::tasks::with_core(config, store, async |app| {
        let state = app.engine.shared_state().load_full();
        let id = resolve(&state, &stack)?;
        drop(state);
        app.engine
            .execute(StateCommand::MoveStack { id, order })
            .await?;
        let state = app.engine.shared_state().load_full();
        let row = state
            .stacks
            .get(&id)
            .filter(|s| !s.deleted)
            .ok_or_else(|| CommandError::UnknownStack(stack))?;
        Ok(StackReceipt {
            id: row.id.as_uuid().to_string(),
            title: row.title.clone(),
            order: row.order,
        })
    })
    .await
}

/// The human-readable add receipt./// The human-readable add receipt.
#[must_use]
pub fn render_text_add(receipt: &StackReceipt) -> String {
    format!(
        "stack \"{}\" at position {} ({})",
        receipt.title, receipt.order, receipt.id
    )
}

/// The human-readable delete receipt.
#[must_use]
pub fn render_text_delete(receipt: &StackDeleteReceipt) -> String {
    format!(
        "deleted stack \"{}\" ({}) — {} task(s) cascaded",
        receipt.title, receipt.id, receipt.cascaded_tasks
    )
}

#[cfg(test)]
mod tests {
    use crate::commands::output::standalone_note;

    #[test]
    fn the_standalone_note_appears_only_under_standalone() {
        assert!(standalone_note(true, 3).is_some());
        assert!(standalone_note(false, 0).is_none());
    }
}
