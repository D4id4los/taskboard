// SPDX-License-Identifier: AGPL-3.0-only
//! `taskboard tasks remind` (design §11, D-R1/D-R3): the reminder-rule
//! verb family over the in-process core — add (`--before`, one rule per
//! offset), remove one (`--remove`, by rule uuid/prefix/offset),
//! clear all (`--clear`), list (`--list`, a pure read), and mute/unmute
//! (`--off`/`--on`). The two-lane shape of every verb family: one-shot
//! wrappers boot the core over [`crate::commands::tasks::with_core`],
//! the `*_on` seams run on a booted session core.

use std::fmt::Write as _;
use std::sync::Arc;

use chrono::TimeDelta;

use taskboard_domain::{
    AppState, CommandOutcome, ReminderKind, ReminderOffset, ReminderRuleId, StateCommand, TaskId,
};

use crate::commands::CommandError;
use crate::commands::output::{
    ReminderAddReceipt, ReminderClearReceipt, ReminderMuteReceipt, ReminderRemoveReceipt,
    ReminderRow, RemindersList,
};
use crate::config::AppConfig;
use crate::secrets::CredentialStore;

/// Boots the core and runs one remind mode (the one-shot lane).
///
/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
pub async fn add(
    config: AppConfig,
    store: Arc<dyn CredentialStore>,
    task: &str,
    before: Option<TimeDelta>,
    insistence: taskboard_domain::Insistence,
) -> Result<ReminderAddReceipt, CommandError> {
    let task = task.to_owned();
    crate::commands::tasks::with_core(config, store, async |app| {
        add_on(app, &task, before, insistence).await
    })
    .await
}

/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
pub async fn remove(
    config: AppConfig,
    store: Arc<dyn CredentialStore>,
    task: &str,
    rule: &str,
) -> Result<ReminderRemoveReceipt, CommandError> {
    let task = task.to_owned();
    let rule = rule.to_owned();
    crate::commands::tasks::with_core(config, store, async |app| {
        remove_on(app, &task, &rule).await
    })
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
/// else the booted config's default), execute the add, project the
/// receipt — `added: false` names the dedup survivor, `true` the fresh
/// rule; both project identically from post-state.
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
pub async fn add_on(
    app: &crate::App,
    task: &str,
    before: Option<TimeDelta>,
    insistence: taskboard_domain::Insistence,
) -> Result<ReminderAddReceipt, CommandError> {
    let state = app.engine.shared_state().load_full();
    let task_id = crate::commands::tasks::resolve_task(&state, task)?;
    let offset = match before {
        Some(dur) => ReminderOffset::before(-dur)
            .expect("a parsed before-duration negates into the domain's sign domain"),
        None => app.config.settings_snapshot().reminder_before,
    };
    drop(state);
    let outcome = app
        .engine
        .execute(StateCommand::AddReminderRule {
            task: task_id,
            offset,
            at: None,
            insistence,
        })
        .await?;
    let CommandOutcome::ReminderRuleAdded { rule, added } = outcome else {
        return Err(CommandError::UnknownTask(task.to_owned()));
    };
    let state = app.engine.shared_state().load_full();
    let task_title = live_title(&state, task_id)?;
    let insistence = state
        .reminders
        .rules
        .get(&rule)
        .map(|r| r.insistence)
        .unwrap_or_default();
    Ok(ReminderAddReceipt {
        rule_id: rule.as_uuid().to_string(),
        task_id: task_id.as_uuid().to_string(),
        task_title,
        before: offset.text_form(),
        added,
        insistence,
    })
}

/// The `tasks remind --remove <rule>` steps on a booted core: resolve
/// the task, resolve the rule selector against the task's rules (D9),
/// execute the removal, and project the receipt from the **pre-execute**
/// rule row (the rule is gone post-execute — the `TaskDeleteReceipt`
/// pre-state precedent); the task title projects from post-state.
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
    task: &str,
    rule: &str,
) -> Result<ReminderRemoveReceipt, CommandError> {
    let state = app.engine.shared_state().load_full();
    let task_id = crate::commands::tasks::resolve_task(&state, task)?;
    let rule_id = resolve_rule(&state, task_id, rule)?;
    let before = state
        .reminders
        .rules
        .get(&rule_id)
        .map(rule_when_text)
        .expect("resolution guarantees the rule exists");
    drop(state);
    app.engine
        .execute(StateCommand::RemoveReminderRule { rule: rule_id })
        .await?;
    let state = app.engine.shared_state().load_full();
    let task_title = live_title(&state, task_id)?;
    Ok(ReminderRemoveReceipt {
        rule_id: rule_id.as_uuid().to_string(),
        task_id: task_id.as_uuid().to_string(),
        task_title,
        before,
    })
}

/// Resolves a rule selector against the *target task's* rules (D-R3 — a
/// prefix hitting only another task's rule is a miss, not a candidate):
/// exact uuid → unique ≥8-char uuid prefix (an ambiguous prefix reports
/// the candidates immediately) → the offset text form (unique by the
/// dedup invariant). A uuid-shaped miss falls through, like
/// `resolve_task`.
///
/// # Errors
///
/// [`CommandError::UnknownReminderRule`] / [`CommandError::AmbiguousReminderRule`]
/// — typed, never text-matched in the engine.
pub fn resolve_rule(
    state: &AppState,
    task_id: TaskId,
    selector: &str,
) -> Result<ReminderRuleId, CommandError> {
    let rules: Vec<&taskboard_domain::ReminderRule> = state
        .reminders
        .rules
        .values()
        .filter(|rule| rule.task == task_id)
        .collect();
    if let Ok(uuid) = uuid::Uuid::parse_str(selector) {
        let id = ReminderRuleId::from_uuid(uuid);
        if rules.iter().any(|rule| rule.id == id) {
            return Ok(id);
        }
    }
    if selector.len() >= 8 {
        let needle = selector.to_lowercase();
        let hits: Vec<ReminderRuleId> = rules
            .iter()
            .filter(|rule| rule.id.as_uuid().to_string().starts_with(&needle))
            .map(|rule| rule.id)
            .collect();
        match hits.as_slice() {
            [id] => return Ok(*id),
            [] => {}
            [_, ..] => {
                return Err(CommandError::AmbiguousReminderRule {
                    candidates: hits.iter().map(|id| id.as_uuid().to_string()).collect(),
                });
            }
        }
    }
    if let Some(offset) = ReminderOffset::parse_text_form(selector)
        && let Some(rule) = rules
            .iter()
            .find(|rule| rule.kind == ReminderKind::DueOffset(offset))
    {
        return Ok(rule.id);
    }
    Err(CommandError::UnknownReminderRule(selector.to_owned()))
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
                before: rule_when_text(rule),
                created_at: rule.created_at,
                task_muted: state.reminders.muted.contains(&task.id),
                insistence: rule.insistence,
            });
        }
    }
    rows
}

/// The kind-aware timing text for a rule row (r4 D29): a due-offset
/// rule renders the canonical before-magnitude (`"10m"`); an `At` rule
/// renders its absolute UTC fire instant (the [`AtInstant`] text form —
/// local rendering is a presentation concern the CLI does not own).
fn rule_when_text(rule: &taskboard_domain::ReminderRule) -> String {
    match rule.kind {
        ReminderKind::DueOffset(offset) => offset.text_form(),
        ReminderKind::At(at) => at.text_form(),
    }
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
            if row.insistence == taskboard_domain::Insistence::Alarm {
                let _ = write!(line, " [alarm]");
            }
            if row.task_muted {
                let _ = write!(line, " [muted]");
            }
            line
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The human-readable `--before` receipt (`added: false` renders the
/// dedup hit — no state diff needed, the outcome carries the fact).
#[must_use]
pub fn render_text_add(receipt: &ReminderAddReceipt) -> String {
    let mut line = if receipt.added {
        format!(
            "reminder added for \"{}\" {} before due (rule {})",
            receipt.task_title, receipt.before, receipt.rule_id
        )
    } else {
        format!(
            "a reminder {} before due is already set for \"{}\" (rule {})",
            receipt.before, receipt.task_title, receipt.rule_id
        )
    };
    if receipt.insistence == taskboard_domain::Insistence::Alarm {
        use std::fmt::Write as _;
        let _ = write!(line, " [alarm]");
    }
    line
}

/// The human-readable `--remove` receipt.
#[must_use]
pub fn render_text_remove(receipt: &ReminderRemoveReceipt) -> String {
    format!(
        "removed the reminder {} before due from \"{}\" (rule {})",
        receipt.before, receipt.task_title, receipt.rule_id
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
    /// `--remove <task> <rule>`: remove one of the task's rules.
    Remove {
        /// The task selector.
        task: String,
        /// The rule selector (uuid | ≥8-char prefix | offset text form).
        rule: String,
    },
    /// `--off`/`--on <task>`: mute or unmute the task's firings.
    Mute {
        /// The task selector.
        task: String,
        /// `true` = `--off` (mute).
        muted: bool,
    },
    /// `<task> [--before dur] [--insistence rung]`: add one rule (the
    /// config default applies when `--before` is omitted; the rung
    /// default is `Normal` when `--insistence` is omitted).
    Add {
        /// The task selector.
        task: String,
        /// The explicit `--before` value; the config default applies
        /// when `None`.
        before: Option<TimeDelta>,
        /// The explicit `--insistence` value; `Normal` when `None` (a
        /// create-time-only flag — no "unspecified" state is stored).
        insistence: taskboard_domain::Insistence,
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
            insistence,
            list,
            clear,
            remove,
            off,
            on,
        } => {
            if list {
                Some(RemindInvocation::List)
            } else if clear {
                Some(RemindInvocation::Clear(task.unwrap_or_default()))
            } else if let Some(rule) = remove {
                Some(RemindInvocation::Remove {
                    task: task.unwrap_or_default(),
                    rule,
                })
            } else if off || on {
                Some(RemindInvocation::Mute {
                    task: task.unwrap_or_default(),
                    muted: off,
                })
            } else {
                Some(RemindInvocation::Add {
                    task: task.unwrap_or_default(),
                    before,
                    insistence: insistence.unwrap_or_default(),
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
        RemindInvocation::Remove { task, rule } => {
            super::run_core(
                io,
                config_path,
                render_text_remove,
                move |config, store| async move { remove(config, store, &task, &rule).await },
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
        RemindInvocation::Add {
            task,
            before,
            insistence,
        } => super::run_core(
            io,
            config_path,
            render_text_add,
            move |config, store| async move { add(config, store, &task, before, insistence).await },
        )
        .await,
    }
}

/// The session-lane mode switch (`dispatch_line`): pure core reads and
/// mutations, no boot. The `--before` default comes from the booted
/// session config ([`add_on`] reads `app.config`).
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
        RemindInvocation::Remove { task, rule } => {
            crate::commands::emit_result(io, remove_on(app, &task, &rule).await, render_text_remove)
        }
        RemindInvocation::Mute { task, muted } => {
            crate::commands::emit_result(io, mute_on(app, &task, muted).await, render_text_mute)
        }
        RemindInvocation::Add {
            task,
            before,
            insistence,
        } => crate::commands::emit_result(
            io,
            add_on(app, &task, before, insistence).await,
            render_text_add,
        ),
    }
}

#[cfg(test)]
mod predicate_tests {
    use super::*;
    use taskboard_domain::{Insistence, ReminderKind, ReminderOffset, ReminderRuleId};
    use uuid::Uuid;

    fn rule(raw: u128, task_raw: u128, offset: ReminderOffset) -> taskboard_domain::ReminderRule {
        taskboard_domain::ReminderRule {
            id: ReminderRuleId::from_uuid(Uuid::from_u128(raw)),
            task: TaskId::from_uuid(Uuid::from_u128(task_raw)),
            kind: ReminderKind::DueOffset(offset),
            insistence: Insistence::Normal,
            created_at: crate::test_support::frozen_now(),
            updated_at: crate::test_support::frozen_now(),
            by: None,
        }
    }

    fn rules_state(task_raw: u128, rules: &[taskboard_domain::ReminderRule]) -> AppState {
        let mut state = AppState::default();
        for r in rules {
            state.reminders.rules.insert(r.id, r.clone());
        }
        let _ = task_raw;
        state
    }

    fn offset(mins: i64) -> ReminderOffset {
        ReminderOffset::parse_text_form(&format!("{mins}m")).expect("the offset parses")
    }

    /// The selector ladder over one task's rules: exact uuid, unique
    /// ≥8-char prefix, then the offset text form. The `==` flips make
    /// every positive case resolve a *wrong* rule or nothing — each
    /// positive pin flips (plan §3 #15).
    #[test]
    fn resolve_rule_hits_uuid_then_unique_prefix_then_offset_text() {
        let task: u128 = 0x11;
        let a = rule(0x1234_5678_0000_0000_0000_0000_0000_0001, task, offset(10));
        let id = a.id;
        let state = rules_state(task, &[a, rule(2, task, offset(30))]);
        let full = id.as_uuid().to_string();
        assert_eq!(
            resolve_rule(&state, TaskId::from_uuid(Uuid::from_u128(task)), &full)
                .expect("exact uuid"),
            id
        );
        assert_eq!(
            resolve_rule(&state, TaskId::from_uuid(Uuid::from_u128(task)), &full[..8])
                .expect("unique prefix"),
            id
        );
        assert_eq!(
            resolve_rule(&state, TaskId::from_uuid(Uuid::from_u128(task)), "10m")
                .expect("offset text form"),
            id,
            "the dedup-unique offset text resolves its rule"
        );
    }

    /// A shared prefix within the task's rules is the typed ambiguity
    /// naming both candidates.
    #[test]
    fn resolve_rule_ambiguous_prefix_names_candidates() {
        let task: u128 = 0x11;
        let a = rule(0x1234_5670_0000_0000_0000_0000_0000_0001, task, offset(10));
        let b = rule(0x1234_5670_0000_0000_0000_0000_0000_0002, task, offset(30));
        let state = rules_state(task, &[a, b]);
        let shared =
            ReminderRuleId::from_uuid(Uuid::from_u128(0x1234_5670_0000_0000_0000_0000_0000_0001))
                .as_uuid()
                .to_string();
        let err = resolve_rule(
            &state,
            TaskId::from_uuid(Uuid::from_u128(task)),
            &shared[..8],
        )
        .expect_err("two prefix hits");
        let CommandError::AmbiguousReminderRule { candidates } = &err else {
            panic!("a shared prefix is the typed ambiguity, got {err:?}");
        };
        assert_eq!(candidates.len(), 2);
    }

    /// A prefix hitting only *another task's* rule is a miss, not a
    /// candidate (D-R3's scoping law — the resolution is task-local).
    #[test]
    fn resolve_rule_scopes_to_the_target_task() {
        let task: u128 = 0x11;
        let other: u128 = 0x12;
        let foreign = rule(0x1234_5678_0000_0000_0000_0000_0000_0001, other, offset(10));
        let full = foreign.id.as_uuid().to_string();
        let state = rules_state(task, &[foreign]);
        assert!(matches!(
            resolve_rule(&state, TaskId::from_uuid(Uuid::from_u128(task)), &full),
            Err(CommandError::UnknownReminderRule(_))
        ));
    }

    /// A sub-gate selector skips the prefix pass; a non-matching offset
    /// text and a full miss are the typed unknown (never another
    /// task's rule).
    #[test]
    fn resolve_rule_refuses_misses_typedly() {
        let task: u128 = 0x11;
        let a = rule(0x1234_5678_0000_0000_0000_0000_0000_0001, task, offset(10));
        let state = rules_state(task, &[a]);
        let t = TaskId::from_uuid(Uuid::from_u128(task));
        assert!(matches!(
            resolve_rule(&state, t, "1234"),
            Err(CommandError::UnknownReminderRule(_))
        ));
        assert!(
            matches!(
                resolve_rule(&state, t, "5m"),
                Err(CommandError::UnknownReminderRule(_))
            ),
            "a different offset is not a candidate"
        );
        assert!(matches!(
            resolve_rule(&state, t, "zzz"),
            Err(CommandError::UnknownReminderRule(_))
        ));
    }

    // ---- collapse (the mode switch, plan §3 #16) ---------------------------
    //
    // The `off || on` flip: under `&&` the `off`-alone row falls to
    // unreachable (`Mute{muted: true}` is the only observer), the
    // `on`-alone row collapses to the *wrong* `Add` mode — this table
    // flips both.

    #[allow(clippy::fn_params_excessive_bools)] // the clap flag bag IS the shape under test
    fn remind(
        list: bool,
        clear: bool,
        remove: Option<String>,
        off: bool,
        on: bool,
    ) -> crate::cli::TasksCmd {
        crate::cli::TasksCmd::Remind {
            task: Some("t".to_owned()),
            before: None,
            insistence: None,
            list,
            clear,
            remove,
            off,
            on,
        }
    }

    #[test]
    fn collapse_yields_every_remind_mode() {
        assert!(matches!(
            collapse(remind(true, false, None, false, false)),
            Some(RemindInvocation::List)
        ));
        assert!(matches!(
            collapse(remind(false, true, None, false, false)),
            Some(RemindInvocation::Clear(_))
        ));
        assert!(matches!(
            collapse(remind(false, false, Some("r".to_owned()), false, false)),
            Some(RemindInvocation::Remove { rule, .. }) if rule == "r"
        ));
        assert!(
            matches!(
                collapse(remind(false, false, None, true, false)),
                Some(RemindInvocation::Mute { muted: true, .. })
            ),
            "--off alone mutes"
        );
        assert!(
            matches!(
                collapse(remind(false, false, None, false, true)),
                Some(RemindInvocation::Mute { muted: false, .. })
            ),
            "--on alone unmutes — the || flip-killer"
        );
        assert!(matches!(
            collapse(remind(false, false, None, false, false)),
            Some(RemindInvocation::Add { .. })
        ));
    }

    /// A non-Remind `tasks` subcommand never collapses to a remind mode.
    #[test]
    fn collapse_is_none_for_non_remind_subcommands() {
        assert!(collapse(crate::cli::TasksCmd::List { stack: None }).is_none());
    }
}
