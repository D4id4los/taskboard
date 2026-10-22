// SPDX-License-Identifier: AGPL-3.0-only
//! `taskboard tasks remind` (design §11): the reminder-rule verb
//! family over the in-process core — set/replace (`--before`), list
//! (`--list`, a pure read), clear (`--clear`), and mute/unmute
//! (`--off`/`--on`). The two-lane shape of every verb family: one-shot
//! wrappers boot the core over [`crate::commands::tasks::with_core`],
//! the `*_on` seams run on a booted session core.

use std::fmt::Write as _;
use std::sync::Arc;

use chrono::TimeDelta;

use taskboard_domain::{AppState, CommandOutcome, ReminderOffset, StateCommand, TaskId};

use crate::commands::CommandError;
use crate::commands::output::{
    ReminderClearReceipt, ReminderMuteReceipt, ReminderRow, ReminderSetReceipt, RemindersList,
};
use crate::config::AppConfig;
use crate::secrets::CredentialStore;

/// Boots the core and runs one remind mode (the one-shot lane).
///
/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
pub async fn set(
    config: AppConfig,
    store: Arc<dyn CredentialStore>,
    task: &str,
    before: Option<TimeDelta>,
) -> Result<ReminderSetReceipt, CommandError> {
    let task = task.to_owned();
    crate::commands::tasks::with_core(config, store, async |app| set_on(app, &task, before).await)
        .await
}

/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
pub async fn clear(
    config: AppConfig,
    store: Arc<dyn CredentialStore>,
    task: &str,
) -> Result<ReminderClearReceipt, CommandError> {
    let task = task.to_owned();
    crate::commands::tasks::with_core(config, store, async |app| clear_on(app, &task).await).await
}

/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
pub async fn mute(
    config: AppConfig,
    store: Arc<dyn CredentialStore>,
    task: &str,
    muted: bool,
) -> Result<ReminderMuteReceipt, CommandError> {
    let task = task.to_owned();
    crate::commands::tasks::with_core(config, store, async |app| mute_on(app, &task, muted).await)
        .await
}

/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
pub async fn list(
    config: AppConfig,
    store: Arc<dyn CredentialStore>,
) -> Result<RemindersList, CommandError> {
    crate::commands::tasks::with_core(config, store, async |app| Ok(list_on(app))).await
}

/// The `tasks remind --before` steps on a booted core (the session
/// seam): resolve the task, resolve the offset (the `--before` value,
/// else the booted config's default), execute the replace, project the
/// fresh rule.
///
/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
///
/// # Panics
///
/// Only via `expect` on the offset construction: a parsed before-duration
/// negates into the domain's sign domain by construction (the same
/// invariant `parse_before` pins).
pub async fn set_on(
    app: &crate::App,
    task: &str,
    before: Option<TimeDelta>,
) -> Result<ReminderSetReceipt, CommandError> {
    let state = app.engine.shared_state().load_full();
    let task_id = crate::commands::tasks::resolve_task(&state, task)?;
    let offset = match before {
        Some(dur) => ReminderOffset::before(-dur)
            .expect("a parsed before-duration negates into the domain's sign domain"),
        None => app.config.reminder_engine_config().defaults.default_before,
    };
    drop(state);
    let outcome = app
        .engine
        .execute(StateCommand::SetReminderRule {
            task: task_id,
            offset,
        })
        .await?;
    let CommandOutcome::ReminderSet { rule } = outcome else {
        return Err(CommandError::UnknownTask(task.to_owned()));
    };
    let state = app.engine.shared_state().load_full();
    let task_title = live_title(&state, task_id)?;
    Ok(ReminderSetReceipt {
        rule_id: rule.as_uuid().to_string(),
        task_id: task_id.as_uuid().to_string(),
        task_title,
        before: offset.text_form(),
    })
}

/// The `tasks remind --clear` steps on a booted core. The receipt's
/// `removed` count comes from the pre-state the selector resolution
/// already loaded, so a `NoOp` repeat reports 0.
///
/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
pub async fn clear_on(app: &crate::App, task: &str) -> Result<ReminderClearReceipt, CommandError> {
    let state = app.engine.shared_state().load_full();
    let task_id = crate::commands::tasks::resolve_task(&state, task)?;
    let pre_count = state
        .reminders
        .rules
        .values()
        .filter(|rule| rule.task == task_id)
        .count();
    drop(state);
    app.engine
        .execute(StateCommand::ClearReminderRules { task: task_id })
        .await?;
    let state = app.engine.shared_state().load_full();
    let task_title = live_title(&state, task_id)?;
    Ok(ReminderClearReceipt {
        task_id: task_id.as_uuid().to_string(),
        task_title,
        removed: pre_count,
    })
}

/// The `tasks remind --off/--on` steps on a booted core. The receipt's
/// boolean is the post-state's mute membership (the outcome carries the
/// same fact; the state read is the projection source).
///
/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
pub async fn mute_on(
    app: &crate::App,
    task: &str,
    muted: bool,
) -> Result<ReminderMuteReceipt, CommandError> {
    let state = app.engine.shared_state().load_full();
    let task_id = crate::commands::tasks::resolve_task(&state, task)?;
    drop(state);
    app.engine
        .execute(StateCommand::SetTaskRemindersMuted {
            task: task_id,
            muted,
        })
        .await?;
    let state = app.engine.shared_state().load_full();
    let task_title = live_title(&state, task_id)?;
    Ok(ReminderMuteReceipt {
        task_id: task_id.as_uuid().to_string(),
        task_title,
        muted: state.reminders.muted.contains(&task_id),
    })
}

/// The `tasks remind --list` steps: a pure read of the published state,
/// no command. Rows in task canonical order, then rule id.
///
/// # Errors
///
/// Never fails today; `Result` keeps the session seam shape.
#[must_use]
pub fn list_on(app: &crate::App) -> RemindersList {
    let state = app.engine.shared_state().load_full();
    RemindersList {
        rules: reminder_rows(&state),
    }
}

/// Projects the state's reminder rules: live tasks in canonical order,
/// each with its rules in id order.
fn reminder_rows(state: &AppState) -> Vec<ReminderRow> {
    let mut live: Vec<&taskboard_domain::Task> =
        state.tasks.values().filter(|t| t.is_live()).collect();
    live.sort_by_key(|t| (t.stack, t.order, t.id));
    let mut rows = Vec::new();
    for task in live {
        for rule in state
            .reminders
            .rules
            .values()
            .filter(|rule| rule.task == task.id)
        {
            rows.push(ReminderRow {
                id: rule.id.as_uuid().to_string(),
                task_id: task.id.as_uuid().to_string(),
                task_title: task.title.clone(),
                before: rule.offset.text_form(),
                created_at: rule.created_at,
                task_muted: state.reminders.muted.contains(&task.id),
            });
        }
    }
    rows
}

/// The title of a live task in the published state (the receipt source;
/// resolution guarantees liveness, the guard guards an engine contract
/// break).
fn live_title(state: &AppState, task_id: TaskId) -> Result<String, CommandError> {
    state
        .tasks
        .get(&task_id)
        .filter(|t| t.is_live())
        .map(|t| t.title.clone())
        .ok_or_else(|| CommandError::UnknownTask(task_id.as_uuid().to_string()))
}

/// The human-readable `--list` rendering.
#[must_use]
pub fn render_text_list(list: &RemindersList) -> String {
    if list.rules.is_empty() {
        return "no reminder rules".to_owned();
    }
    list.rules
        .iter()
        .map(|row| {
            let mut line = format!(
                "{}  {} before due (rule {})",
                row.task_title, row.before, row.id
            );
            if row.task_muted {
                let _ = write!(line, " [muted]");
            }
            line
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The human-readable `--before` receipt.
#[must_use]
pub fn render_text_set(receipt: &ReminderSetReceipt) -> String {
    format!(
        "reminder set for \"{}\" {} before due (rule {})",
        receipt.task_title, receipt.before, receipt.rule_id
    )
}

/// The human-readable `--clear` receipt.
#[must_use]
pub fn render_text_clear(receipt: &ReminderClearReceipt) -> String {
    let noun = if receipt.removed == 1 {
        "reminder rule"
    } else {
        "reminder rules"
    };
    format!(
        "cleared {} {noun} from \"{}\"",
        receipt.removed, receipt.task_title
    )
}

/// The human-readable `--off`/`--on` receipt.
#[must_use]
pub fn render_text_mute(receipt: &ReminderMuteReceipt) -> String {
    let state = if receipt.muted { "muted" } else { "unmuted" };
    format!("reminders {state} for \"{}\"", receipt.task_title)
}

/// One resolved `tasks remind` invocation: the clap flags collapsed to
/// exactly one mode (clap enforces the flag coherence).
#[derive(Debug, Clone)]
pub enum RemindInvocation {
    /// `--list`: enumerate the board's rules (pure read).
    List,
    /// `--clear <task>`: remove all of the task's rules.
    Clear(String),
    /// `--off`/`--on <task>`: mute or unmute the task's firings.
    Mute {
        /// The task selector.
        task: String,
        /// `true` = `--off` (mute).
        muted: bool,
    },
    /// `<task> [--before dur]`: set/replace the single rule.
    Set {
        /// The task selector.
        task: String,
        /// The explicit `--before` value; the config default applies
        /// when `None`.
        before: Option<TimeDelta>,
    },
}

/// Collapses the clap flag tuple into exactly one invocation (clap
/// enforces the coherence: exactly one mode per line, selectors
/// required where used). One definition site for both dispatch lanes.
#[must_use]
pub fn collapse(cmd: crate::cli::TasksCmd) -> Option<RemindInvocation> {
    match cmd {
        crate::cli::TasksCmd::Remind {
            task,
            before,
            list,
            clear,
            off,
            on,
        } => {
            if list {
                Some(RemindInvocation::List)
            } else if clear {
                Some(RemindInvocation::Clear(task.unwrap_or_default()))
            } else if off || on {
                Some(RemindInvocation::Mute {
                    task: task.unwrap_or_default(),
                    muted: off,
                })
            } else {
                Some(RemindInvocation::Set {
                    task: task.unwrap_or_default(),
                    before,
                })
            }
        }
        _ => None,
    }
}

/// The one-shot `tasks remind` dispatch: the clap-flag mode switch over
/// [`super::run_core`], each mode with its own handler and renderer.
pub async fn remind_command(
    io: &mut crate::commands::CommandIo<'_>,
    config_path: Option<std::path::PathBuf>,
    invocation: RemindInvocation,
) -> u8 {
    match invocation {
        RemindInvocation::List => {
            super::run_core(
                io,
                config_path,
                render_text_list,
                |config, store| async move { list(config, store).await },
            )
            .await
        }
        RemindInvocation::Clear(task) => {
            super::run_core(
                io,
                config_path,
                render_text_clear,
                |config, store| async move { clear(config, store, &task).await },
            )
            .await
        }
        RemindInvocation::Mute { task, muted } => {
            super::run_core(
                io,
                config_path,
                render_text_mute,
                move |config, store| async move { mute(config, store, &task, muted).await },
            )
            .await
        }
        RemindInvocation::Set { task, before } => {
            super::run_core(
                io,
                config_path,
                render_text_set,
                move |config, store| async move { set(config, store, &task, before).await },
            )
            .await
        }
    }
}

/// The session-lane mode switch (`dispatch_line`): pure core reads and
/// mutations, no boot. The `--before` default comes from the booted
/// session config ([`set_on`] reads `app.config`).
pub async fn remind_line(
    app: &crate::App,
    io: &mut crate::commands::CommandIo<'_>,
    invocation: RemindInvocation,
) -> u8 {
    match invocation {
        RemindInvocation::List => {
            crate::commands::emit_result(io, Ok(list_on(app)), render_text_list)
        }
        RemindInvocation::Clear(task) => {
            crate::commands::emit_result(io, clear_on(app, &task).await, render_text_clear)
        }
        RemindInvocation::Mute { task, muted } => {
            crate::commands::emit_result(io, mute_on(app, &task, muted).await, render_text_mute)
        }
        RemindInvocation::Set { task, before } => {
            crate::commands::emit_result(io, set_on(app, &task, before).await, render_text_set)
        }
    }
}
