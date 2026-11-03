// SPDX-License-Identifier: AGPL-3.0-only
//! `taskboard breaks` (#495, the `timers` recipe): the break-reminder
//! rule verb family over the in-process core — add (`--after`, the
//! accumulated-work threshold), list (a pure read), remove
//! (uuid/prefix/label-prefix selector). The two-lane shape of every
//! verb family: one-shot wrappers boot the core over
//! [`crate::commands::tasks::with_core`], the `*_on` seams run on a
//! booted session core. Break rules roam (#588): `add` resolves the
//! board (the `habits add` device) and the rule joins that board's
//! synced `break-rules` section.

use std::sync::Arc;

use taskboard_domain::{AppState, BreakRuleId, CommandOutcome, StateCommand};

use crate::commands::CommandError;
use crate::commands::output::{BreakAddReceipt, BreakRemoveReceipt, BreakRuleRow, BreaksList};
use crate::config::AppConfig;
use crate::secrets::CredentialStore;

/// Boots the core and creates one break rule (the one-shot lane).
///
/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
pub async fn add(
    config: AppConfig,
    store: Arc<dyn CredentialStore>,
    label: &str,
    threshold: taskboard_domain::WorkThreshold,
    insistence: taskboard_domain::Insistence,
) -> Result<BreakAddReceipt, CommandError> {
    let label = label.to_owned();
    crate::commands::tasks::with_core(config, store, async |app| {
        add_on(app, &label, threshold, insistence).await
    })
    .await
}

/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
pub async fn remove(
    config: AppConfig,
    store: Arc<dyn CredentialStore>,
    selector: &str,
) -> Result<BreakRemoveReceipt, CommandError> {
    let selector = selector.to_owned();
    crate::commands::tasks::with_core(config, store, async |app| remove_on(app, &selector).await)
        .await
}

/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
pub async fn list(
    config: AppConfig,
    store: Arc<dyn CredentialStore>,
) -> Result<BreaksList, CommandError> {
    crate::commands::tasks::with_core(config, store, async |app| Ok(list_on(app))).await
}

/// The `breaks add` steps on a booted core (the session seam): execute
/// the create, project the receipt from the outcome + post-state (the
/// `TimerAddReceipt` projection pattern).
///
/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
///
/// # Panics
///
/// Only via `expect` on the post-execute row read: the create outcome
/// names the minted id, so the same-state projection cannot miss.
pub async fn add_on(
    app: &crate::App,
    label: &str,
    threshold: taskboard_domain::WorkThreshold,
    insistence: taskboard_domain::Insistence,
) -> Result<BreakAddReceipt, CommandError> {
    // The board resolves at the surface (D-8): the single live board
    // is the default; an explicit `--board` flag overrides (N.4's
    // flag wiring); zero or several boards share the ambiguity refusal.
    let board = super::resolve_board(app, None)?;
    let outcome = app
        .engine
        .execute(StateCommand::CreateBreakRule {
            board: Some(board),
            label: label.to_owned(),
            threshold,
            insistence,
        })
        .await?;
    let CommandOutcome::BreakRuleCreated { rule } = outcome else {
        return Err(CommandError::Engine(Box::new(
            taskboard_state::ExecuteError::EngineGone,
        )));
    };
    let state = app.engine.shared_state().load_full();
    let stored = state
        .reminders
        .break_rules
        .get(&rule)
        .expect("the create outcome names the minted rule");
    Ok(BreakAddReceipt {
        rule_id: rule.as_uuid().to_string(),
        label: stored.label.clone(),
        threshold_seconds: u64::try_from(stored.threshold.as_duration().num_seconds()).unwrap_or(0),
        threshold: stored.threshold.text_form(),
        insistence: stored.insistence,
    })
}

/// The `breaks remove` steps on a booted core: resolve the selector,
/// project the **pre-execute** row (the row is gone post-execute — the
/// `TimerCancelReceipt` pre-state precedent), execute the removal.
///
/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
///
/// # Panics
///
/// Only via `expect` on the pre-execute row read: resolution guarantees
/// the rule exists in the same state snapshot the projection reads.
pub async fn remove_on(
    app: &crate::App,
    selector: &str,
) -> Result<BreakRemoveReceipt, CommandError> {
    let state = app.engine.shared_state().load_full();
    let rule_id = resolve_rule(&state, selector)?;
    let label = state
        .reminders
        .break_rules
        .get(&rule_id)
        .map(|rule| rule.label.clone())
        .expect("resolution guarantees the rule exists");
    drop(state);
    app.engine
        .execute(StateCommand::DeleteBreakRule { id: rule_id })
        .await?;
    Ok(BreakRemoveReceipt {
        rule_id: rule_id.as_uuid().to_string(),
        label,
    })
}

/// Resolves a break-rule selector against the rules map (the
/// `resolve_alarm` shape verbatim): exact rule uuid → unique ≥8-char
/// lowercased uuid prefix → unique case-folded label prefix.
///
/// # Errors
///
/// [`CommandError::UnknownBreakRule`] / [`CommandError::AmbiguousAdhocAlarm`]
/// — typed, never text-matched in the engine.
pub fn resolve_rule(state: &AppState, selector: &str) -> Result<BreakRuleId, CommandError> {
    let rules = &state.reminders.break_rules;
    if let Ok(uuid) = uuid::Uuid::parse_str(selector) {
        let id = BreakRuleId::from_uuid(uuid);
        if rules.contains_key(&id) {
            return Ok(id);
        }
    }
    if selector.len() >= 8 {
        let needle = selector.to_lowercase();
        let hits: Vec<BreakRuleId> = rules
            .keys()
            .filter(|id| id.as_uuid().to_string().starts_with(&needle))
            .copied()
            .collect();
        match hits.as_slice() {
            [id] => return Ok(*id),
            [] => {}
            [_, ..] => {
                return Err(CommandError::AmbiguousAdhocAlarm {
                    candidates: hits.iter().map(|id| id.as_uuid().to_string()).collect(),
                });
            }
        }
    }
    let needle = selector.to_lowercase();
    let hits: Vec<BreakRuleId> = rules
        .values()
        .filter(|rule| rule.label.to_lowercase().starts_with(&needle))
        .map(|rule| rule.id)
        .collect();
    match hits.as_slice() {
        [id] => Ok(*id),
        [] => Err(CommandError::UnknownBreakRule(selector.to_owned())),
        [_, ..] => Err(CommandError::AmbiguousAdhocAlarm {
            candidates: hits.iter().map(|id| id.as_uuid().to_string()).collect(),
        }),
    }
}

/// The `breaks list` steps: a pure read of the published state, no
/// command. Rows in canonical `(threshold, id)` order.
#[must_use]
pub fn list_on(app: &crate::App) -> BreaksList {
    let state = app.engine.shared_state().load_full();
    BreaksList {
        rules: rule_rows(&state),
    }
}

/// Projects the rules map onto list rows, canonical `(threshold, id)`.
#[must_use]
pub fn rule_rows(state: &AppState) -> Vec<BreakRuleRow> {
    let mut rules: Vec<&taskboard_domain::BreakRule> =
        state.reminders.break_rules.values().collect();
    rules.sort_by_key(|rule| (rule.threshold, rule.id));
    rules
        .iter()
        .map(|rule| BreakRuleRow {
            rule_id: rule.id.as_uuid().to_string(),
            label: rule.label.clone(),
            threshold_seconds: u64::try_from(rule.threshold.as_duration().num_seconds())
                .unwrap_or(0),
            threshold: rule.threshold.text_form(),
            insistence: rule.insistence,
        })
        .collect()
}

/// The human-readable `list` rendering.
#[must_use]
pub fn render_text_list(list: &BreaksList) -> String {
    use std::fmt::Write as _;
    if list.rules.is_empty() {
        return "no break rules".to_owned();
    }
    list.rules
        .iter()
        .map(|row| {
            let mut line = format!(
                "{}  after {} (rule {})",
                row.label, row.threshold, row.rule_id
            );
            if row.insistence == taskboard_domain::Insistence::Alarm {
                let _ = write!(line, " [alarm]");
            }
            line
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The human-readable `add` receipt.
#[must_use]
pub fn render_text_add(receipt: &BreakAddReceipt) -> String {
    use std::fmt::Write as _;
    let mut line = format!(
        "break rule \"{}\" set after {} (rule {})",
        receipt.label, receipt.threshold, receipt.rule_id
    );
    if receipt.insistence == taskboard_domain::Insistence::Alarm {
        let _ = write!(line, " [alarm]");
    }
    line
}

/// The human-readable `remove` receipt.
#[must_use]
pub fn render_text_remove(receipt: &BreakRemoveReceipt) -> String {
    format!(
        "removed break rule \"{}\" ({})",
        receipt.label, receipt.rule_id
    )
}

/// One resolved `breaks` invocation: the clap subcommand collapsed to
/// exactly one mode.
#[derive(Debug, Clone)]
pub enum BreaksInvocation {
    /// `add --after <dur> [--label] [--insistence]`.
    Add {
        /// The notification title.
        label: String,
        /// The accumulated-work threshold.
        threshold: taskboard_domain::WorkThreshold,
        /// The explicit `--insistence` value; `Normal` when `None`.
        insistence: taskboard_domain::Insistence,
    },
    /// `list` (pure read).
    List,
    /// `remove <selector>`.
    Remove(String),
}

/// Collapses the clap subcommand into exactly one invocation. One
/// definition site for both dispatch lanes.
#[must_use]
pub fn collapse(cmd: crate::cli::BreaksCmd) -> Option<BreaksInvocation> {
    match cmd {
        crate::cli::BreaksCmd::Add {
            after,
            label,
            insistence,
        } => Some(BreaksInvocation::Add {
            // The threshold is minted at the clap boundary (the
            // `parse_after_flag` value parser) — `collapse` cannot fail.
            label: label.unwrap_or_else(|| format!("break after {}", after.text_form())),
            threshold: after,
            insistence: insistence.unwrap_or_default(),
        }),
        crate::cli::BreaksCmd::List => Some(BreaksInvocation::List),
        crate::cli::BreaksCmd::Remove { selector } => Some(BreaksInvocation::Remove(selector)),
    }
}

/// The one-shot `breaks` dispatch: the mode switch over
/// [`super::run_core`], each mode with its own handler and renderer.
pub async fn breaks_command(
    io: &mut crate::commands::CommandIo<'_>,
    config_path: Option<std::path::PathBuf>,
    invocation: BreaksInvocation,
) -> u8 {
    match invocation {
        BreaksInvocation::List => {
            super::run_core(
                io,
                config_path,
                render_text_list,
                |config, store| async move { list(config, store).await },
            )
            .await
        }
        BreaksInvocation::Remove(selector) => {
            super::run_core(
                io,
                config_path,
                render_text_remove,
                move |config, store| async move { remove(config, store, &selector).await },
            )
            .await
        }
        BreaksInvocation::Add {
            label,
            threshold,
            insistence,
        } => {
            super::run_core(
                io,
                config_path,
                render_text_add,
                move |config, store| async move {
                    add(config, store, &label, threshold, insistence).await
                },
            )
            .await
        }
    }
}

/// The session-lane mode switch (`dispatch_line`): pure core reads and
/// mutations, no boot.
pub async fn breaks_line(
    app: &crate::App,
    io: &mut crate::commands::CommandIo<'_>,
    invocation: BreaksInvocation,
) -> u8 {
    match invocation {
        BreaksInvocation::List => {
            crate::commands::emit_result(io, Ok(list_on(app)), render_text_list)
        }
        BreaksInvocation::Remove(selector) => {
            crate::commands::emit_result(io, remove_on(app, &selector).await, render_text_remove)
        }
        BreaksInvocation::Add {
            label,
            threshold,
            insistence,
        } => crate::commands::emit_result(
            io,
            add_on(app, &label, threshold, insistence).await,
            render_text_add,
        ),
    }
}
