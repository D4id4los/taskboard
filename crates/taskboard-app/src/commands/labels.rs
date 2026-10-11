// SPDX-License-Identifier: AGPL-3.0-only
//! `taskboard labels` (ADR 0009): the label verbs over the in-process
//! core. `list` is the discoverability floor (labels are otherwise
//! invisible until assigned); `add/edit` validate the color strictly at
//! the CLI boundary — unlike board colors there is no publish-time
//! fallback, so a malformed value would strand the outbox op forever.

use std::sync::Arc;

use taskboard_domain::{AppState, CommandOutcome, LabelChanges, LabelId, StateCommand};

pub(crate) use crate::cli::DEFAULT_LABEL_COLOR;
use crate::commands::CommandError;
use crate::commands::output::{LabelReceipt, LabelRow};
use crate::config::AppConfig;
use crate::secrets::CredentialStore;

/// Resolves a label selector: exact uuid, then exact live title (the
/// `resolve_stack` shape; prefixes stay a task-only affordance).
pub(crate) fn resolve_label(state: &AppState, selector: &str) -> Result<LabelId, CommandError> {
    let mut live: Vec<&taskboard_domain::Label> =
        state.labels.values().filter(|l| !l.deleted).collect();
    live.sort_by_key(|l| l.id);
    if let Ok(uuid) = uuid::Uuid::parse_str(selector) {
        let id = LabelId::from_uuid(uuid);
        if live.iter().any(|l| l.id == id) {
            return Ok(id);
        }
    }
    let hits: Vec<&taskboard_domain::Label> = live
        .iter()
        .filter(|l| l.title == selector)
        .copied()
        .collect();
    match hits.len() {
        0 => Err(CommandError::UnknownLabel(selector.to_owned())),
        1 => Ok(hits[0].id),
        _ => Err(CommandError::AmbiguousLabel {
            candidates: hits.iter().map(|l| l.title.clone()).collect(),
        }),
    }
}

/// The live board's labels in board order with their assignment counts.
///
/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
pub async fn list(
    config: AppConfig,
    store: Arc<dyn CredentialStore>,
) -> Result<Vec<LabelRow>, CommandError> {
    crate::commands::tasks::with_core(config, store, async |app| list_on(app)).await
}

/// The `labels list` steps on a booted core (the REPL/session seam).
pub(crate) fn list_on(app: &crate::App) -> Result<Vec<LabelRow>, CommandError> {
    {
        let state = app.engine.shared_state().load_full();
        if !state.boards.values().any(|b| !b.deleted) {
            return Err(CommandError::NoBoard);
        }
        let mut rows: Vec<LabelRow> = state
            .labels
            .values()
            .filter(|l| !l.deleted)
            .map(|l| LabelRow {
                id: l.id.as_uuid().to_string(),
                title: l.title.clone(),
                color: l.color.as_str().to_owned(),
                assigned_tasks: state
                    .tasks
                    .values()
                    .filter(|t| t.is_live() && t.labels.contains(&l.id))
                    .count(),
            })
            .collect();
        rows.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(rows)
    }
}

/// Creates a label (default color when `--color` is omitted — the value
/// parser already normalized six hex digits to lowercase).
///
/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
pub async fn add(
    config: AppConfig,
    store: Arc<dyn CredentialStore>,
    title: &str,
    color: Option<&str>,
) -> Result<LabelReceipt, CommandError> {
    let title = title.to_owned();
    // The clap value parser normalizes to lowercase; the direct-call
    // path (this crate's tests) normalizes here too.
    let color = color.map_or_else(|| DEFAULT_LABEL_COLOR.to_owned(), str::to_lowercase);
    crate::commands::tasks::with_core(config, store, async |app| add_on(app, title, color).await)
        .await
}

/// The `labels add` steps on a booted core (the REPL/session seam);
/// `color` arrives normalized (lowercase hex, default applied).
pub(crate) async fn add_on(
    app: &crate::App,
    title: String,
    color: String,
) -> Result<LabelReceipt, CommandError> {
    {
        let outcome = app
            .engine
            .execute(StateCommand::CreateLabel {
                title: title.clone(),
                color: taskboard_domain::Color::new(color.clone()),
            })
            .await?;
        let CommandOutcome::CreatedLabel(id) = outcome else {
            // The engine's own rejection (e.g. `NoBoard`) already
            // surfaced via `?` above; a different accepted outcome would
            // be an engine contract break, reported as the missing
            // board's kind.
            return Err(CommandError::NoBoard);
        };
        Ok(LabelReceipt {
            id: id.as_uuid().to_string(),
            title,
            color,
        })
    }
}

/// Edits a label's title and/or color. A changeset touching nothing that
/// differs is an accepted no-op.
///
/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
pub async fn edit(
    config: AppConfig,
    store: Arc<dyn CredentialStore>,
    label: &str,
    title: Option<String>,
    color: Option<String>,
) -> Result<LabelReceipt, CommandError> {
    let label = label.to_owned();
    crate::commands::tasks::with_core(config, store, async |app| {
        edit_on(app, &label, title, color).await
    })
    .await
}

/// The `labels edit` steps on a booted core (the REPL/session seam).
pub(crate) async fn edit_on(
    app: &crate::App,
    label: &str,
    title: Option<String>,
    color: Option<String>,
) -> Result<LabelReceipt, CommandError> {
    {
        let state = app.engine.shared_state().load_full();
        let id = resolve_label(&state, label)?;
        drop(state);
        app.engine
            .execute(StateCommand::UpdateLabel {
                id,
                changes: LabelChanges {
                    title,
                    color: color.map(taskboard_domain::Color::new),
                },
            })
            .await?;
        let state = app.engine.shared_state().load_full();
        let row = state
            .labels
            .get(&id)
            .filter(|l| !l.deleted)
            .ok_or_else(|| CommandError::UnknownLabel(label.to_owned()))?;
        Ok(LabelReceipt {
            id: row.id.as_uuid().to_string(),
            title: row.title.clone(),
            color: row.color.as_str().to_owned(),
        })
    }
}

/// Deletes a label. Repeats are idempotent successes.
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
    label: &str,
) -> Result<LabelReceipt, CommandError> {
    let label = label.to_owned();
    crate::commands::tasks::with_core(config, store, async |app| delete_on(app, &label).await).await
}

/// The `labels delete` steps on a booted core (the REPL/session seam).
pub(crate) async fn delete_on(app: &crate::App, label: &str) -> Result<LabelReceipt, CommandError> {
    {
        let state = app.engine.shared_state().load_full();
        // Terminal intent: a repeat still resolves its target, so a
        // tombstoned label stays reachable by exact uuid.
        let id = resolve_label(&state, label).or_else(|err| match err {
            CommandError::UnknownLabel(_) => {
                if let Ok(uuid) = uuid::Uuid::parse_str(label) {
                    let id = LabelId::from_uuid(uuid);
                    if state.labels.contains_key(&id) {
                        return Ok(id);
                    }
                }
                Err(CommandError::UnknownLabel(label.to_owned()))
            }
            other => Err(other),
        })?;
        // Resolution guarantees the row exists (the live cascade or the
        // exact-uuid fallback both hit `contains_key`).
        let snapshot = state
            .labels
            .get(&id)
            .map(|row| LabelReceipt {
                id: row.id.as_uuid().to_string(),
                title: row.title.clone(),
                color: row.color.as_str().to_owned(),
            })
            .expect("resolved label exists");
        drop(state);
        app.engine.execute(StateCommand::DeleteLabel { id }).await?;
        Ok(snapshot)
    }
}

/// The human-readable labels listing.
#[must_use]
pub fn render_text_list(rows: &[LabelRow]) -> String {
    if rows.is_empty() {
        return "no labels".to_owned();
    }
    rows.iter()
        .map(|r| format!("{}\t{}\t[{}]", r.title, r.color, r.assigned_tasks))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The human-readable label receipt.
#[must_use]
pub fn render_text_receipt(receipt: &LabelReceipt) -> String {
    format!(
        "label \"{}\" ({}) [{}]",
        receipt.title, receipt.id, receipt.color
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use taskboard_domain::BoardId;
    use uuid::Uuid;

    fn label(raw: u128, title: &str) -> taskboard_domain::Label {
        taskboard_domain::Label {
            id: LabelId::from(Uuid::from_u128(raw)),
            remote: None,
            board: BoardId::from(Uuid::from_u128(2)),
            title: title.to_owned(),
            color: taskboard_domain::Color::new("31cc7c"),
            deleted: false,
            clocks: taskboard_domain::LabelClocks {
                title: taskboard_domain::BoardClocks::epoch(),
                color: taskboard_domain::BoardClocks::epoch(),
                deleted: taskboard_domain::BoardClocks::epoch(),
            },
            remote_seen: None,
        }
    }

    fn state_with(labels: Vec<taskboard_domain::Label>) -> AppState {
        let mut state = AppState::default();
        for l in labels {
            state.labels.insert(l.id, l);
        }
        state
    }

    #[test]
    fn label_selector_cascade_uuid_then_title() {
        let a = label(0x100, "urgent");
        let id = a.id;
        let state = state_with(vec![a]);
        assert!(matches!(resolve_label(&state, &id.as_uuid().to_string()), Ok(got) if got == id));
        assert!(matches!(resolve_label(&state, "urgent"), Ok(got) if got == id));
        assert!(matches!(
            resolve_label(&state, "nope"),
            Err(CommandError::UnknownLabel(sel)) if sel == "nope"
        ));
        let err = resolve_label(
            &state_with(vec![label(0x100, "dupe"), label(0x200, "dupe")]),
            "dupe",
        )
        .expect_err("ambiguous");
        assert!(
            matches!(err, CommandError::AmbiguousLabel { ref candidates } if candidates.len() == 2)
        );
    }

    #[test]
    fn tombstoned_labels_are_invisible() {
        let mut dead = label(0x100, "gone");
        dead.deleted = true;
        assert!(matches!(
            resolve_label(&state_with(vec![dead]), "gone"),
            Err(CommandError::UnknownLabel(_))
        ));
    }
}
