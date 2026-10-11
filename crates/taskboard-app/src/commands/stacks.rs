// SPDX-License-Identifier: AGPL-3.0-only
//! `taskboard stacks` (ADR 0009): the minimum stack verb that makes
//! stand-alone usable (`stacks add`); WI-3 owns the rest of the
//! stack/label verbs. Board-less dbs surface the engine's own `NoBoard`
//! rejection (the `no-board` kind).

use std::sync::Arc;

use taskboard_domain::{CommandOutcome, StateCommand};

use crate::commands::CommandError;
use crate::commands::output::StackReceipt;
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

/// The human-readable add receipt.
#[must_use]
pub fn render_text_add(receipt: &StackReceipt) -> String {
    format!(
        "created stack \"{}\" at position {} ({})",
        receipt.title, receipt.order, receipt.id
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
