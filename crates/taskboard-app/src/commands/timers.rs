// SPDX-License-Identifier: AGPL-3.0-only
//! `taskboard timers` (D-R9): the standalone alarm verb family over the
//! in-process core — add (`--in`/`--at`), list (a pure read), cancel
//! (uuid/prefix/label-prefix selector). The two-lane shape of every
//! verb family: one-shot wrappers boot the core over
//! [`crate::commands::tasks::with_core`], the `*_on` seams run on a
//! booted session core. Alarms are device-local: no sync lane anywhere.

use std::sync::Arc;

use chrono::{DateTime, Utc};

use taskboard_domain::{AdhocAlarmId, AppState, CommandOutcome, StateCommand};

use crate::commands::CommandError;
use crate::commands::output::{TimerAddReceipt, TimerCancelReceipt, TimerRow, TimersList};
use crate::config::AppConfig;
use crate::secrets::CredentialStore;

/// Boots the core and creates one alarm (the one-shot lane).
///
/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
pub async fn add(
    config: AppConfig,
    store: Arc<dyn CredentialStore>,
    label: &str,
    when: taskboard_domain::AdhocWhen,
    insistence: taskboard_domain::Insistence,
) -> Result<TimerAddReceipt, CommandError> {
    let label = label.to_owned();
    crate::commands::tasks::with_core(config, store, async |app| {
        add_on(app, &label, when, insistence).await
    })
    .await
}

/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
pub async fn cancel(
    config: AppConfig,
    store: Arc<dyn CredentialStore>,
    selector: &str,
) -> Result<TimerCancelReceipt, CommandError> {
    let selector = selector.to_owned();
    crate::commands::tasks::with_core(config, store, async |app| cancel_on(app, &selector).await)
        .await
}

/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
pub async fn list(
    config: AppConfig,
    store: Arc<dyn CredentialStore>,
) -> Result<TimersList, CommandError> {
    crate::commands::tasks::with_core(config, store, async |app| Ok(list_on(app))).await
}

/// The `timers add` steps on a booted core (the session seam): execute
/// the create, project the receipt from the outcome + post-state — the
/// stamped `In` value is state truth (`fire_at` read back from the
/// alarms map, not an input echo; the `ReminderAddReceipt` projection
/// pattern).
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
    when: taskboard_domain::AdhocWhen,
    insistence: taskboard_domain::Insistence,
) -> Result<TimerAddReceipt, CommandError> {
    let outcome = app
        .engine
        .execute(StateCommand::CreateAdhocAlarm {
            label: label.to_owned(),
            when,
            insistence,
        })
        .await?;
    let CommandOutcome::AdhocAlarmCreated { alarm } = outcome else {
        return Err(CommandError::UnknownAdhocAlarm(label.to_owned()));
    };
    let state = app.engine.shared_state().load_full();
    let (fire_at, insistence) = state
        .reminders
        .alarms
        .get(&alarm)
        .map(|a| (a.fire_at, a.insistence))
        .expect("the create outcome names the minted alarm");
    Ok(TimerAddReceipt {
        alarm_id: alarm.as_uuid().to_string(),
        label: label.to_owned(),
        fire_at,
        insistence,
    })
}

/// The `timers cancel` steps on a booted core: resolve the selector,
/// project the **pre-execute** row (the row is gone post-execute — the
/// `TaskDeleteReceipt` pre-state precedent), execute the cancel.
///
/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
///
/// # Panics
///
/// Only via `expect` on the pre-execute row read: resolution guarantees
/// the alarm exists in the same state snapshot the projection reads.
pub async fn cancel_on(
    app: &crate::App,
    selector: &str,
) -> Result<TimerCancelReceipt, CommandError> {
    let state = app.engine.shared_state().load_full();
    let alarm_id = resolve_alarm(&state, selector)?;
    let label = state
        .reminders
        .alarms
        .get(&alarm_id)
        .map(|a| a.label.clone())
        .expect("resolution guarantees the alarm exists");
    drop(state);
    app.engine
        .execute(StateCommand::CancelAdhocAlarm { alarm: alarm_id })
        .await?;
    Ok(TimerCancelReceipt {
        alarm_id: alarm_id.as_uuid().to_string(),
        label,
    })
}

/// Resolves an alarm selector against the alarms map (the
/// `resolve_rule` shape): exact alarm uuid → unique ≥8-char lowercased
/// uuid prefix ([`CommandError::AmbiguousAdhocAlarm`] on multi-hit) →
/// unique case-folded label prefix (full-label equality is the
/// zero-length-suffix case; [`CommandError::AmbiguousAdhocAlarm`] on
/// multi-hit). A uuid-shaped miss falls through to the label pass.
/// Label matching is `starts_with` — a prefix, not a substring.
///
/// # Errors
///
/// [`CommandError::UnknownAdhocAlarm`] / [`CommandError::AmbiguousAdhocAlarm`]
/// — typed, never text-matched in the engine.
pub fn resolve_alarm(state: &AppState, selector: &str) -> Result<AdhocAlarmId, CommandError> {
    let alarms = &state.reminders.alarms;
    if let Ok(uuid) = uuid::Uuid::parse_str(selector) {
        let id = AdhocAlarmId::from_uuid(uuid);
        if alarms.contains_key(&id) {
            return Ok(id);
        }
    }
    if selector.len() >= 8 {
        let needle = selector.to_lowercase();
        let hits: Vec<AdhocAlarmId> = alarms
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
    let hits: Vec<AdhocAlarmId> = alarms
        .values()
        .filter(|alarm| alarm.label.to_lowercase().starts_with(&needle))
        .map(|alarm| alarm.id)
        .collect();
    match hits.as_slice() {
        [id] => Ok(*id),
        [] => Err(CommandError::UnknownAdhocAlarm(selector.to_owned())),
        [_, ..] => Err(CommandError::AmbiguousAdhocAlarm {
            candidates: hits.iter().map(|id| id.as_uuid().to_string()).collect(),
        }),
    }
}

/// The `timers list` steps: a pure read of the published state, no
/// command. Rows in canonical `(fire_at, id)` order; `effective_at` is
/// the chain-walked fire instant (`taskboard_domain::alarm_occurrence`).
/// `now` is read at the command layer — the app boundary may read
/// clocks; the domain stays pure.
#[must_use]
pub fn list_on(app: &crate::App) -> TimersList {
    let state = app.engine.shared_state().load_full();
    TimersList {
        timers: timer_rows(&state, chrono::Utc::now()),
    }
}

/// Projects the alarms map onto list rows, canonical `(fire_at, id)`.
#[must_use]
pub fn timer_rows(state: &AppState, now: DateTime<Utc>) -> Vec<TimerRow> {
    let mut alarms: Vec<&taskboard_domain::AdhocAlarm> = state.reminders.alarms.values().collect();
    alarms.sort_by_key(|a| (a.fire_at, a.id));
    alarms
        .iter()
        .filter_map(|alarm| {
            let effective_at = taskboard_domain::alarm_occurrence(&state.reminders, alarm.id)?;
            let delta = effective_at.signed_duration_since(now);
            let remaining = (delta > chrono::TimeDelta::zero()).then_some(delta);
            Some(TimerRow {
                alarm_id: alarm.id.as_uuid().to_string(),
                label: alarm.label.clone(),
                fire_at: alarm.fire_at,
                effective_at,
                overdue: effective_at <= now,
                remaining,
                insistence: alarm.insistence,
            })
        })
        .collect()
}

/// The human-readable `list` rendering.
#[must_use]
pub fn render_text_list(list: &TimersList) -> String {
    use std::fmt::Write as _;
    if list.timers.is_empty() {
        return "no timers".to_owned();
    }
    list.timers
        .iter()
        .map(|row| {
            let mut line = format!(
                "{}  fires at {} (alarm {})",
                row.label,
                row.effective_at.to_rfc3339(),
                row.alarm_id
            );
            if row.insistence == taskboard_domain::Insistence::Alarm {
                let _ = write!(line, " [alarm]");
            }
            if row.overdue {
                let _ = write!(line, " [overdue]");
            }
            line
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The human-readable `add` receipt (the stamped value, not an echo).
#[must_use]
pub fn render_text_add(receipt: &TimerAddReceipt) -> String {
    use std::fmt::Write as _;
    let mut line = format!(
        "timer \"{}\" set for {} (alarm {})",
        receipt.label,
        receipt.fire_at.to_rfc3339(),
        receipt.alarm_id
    );
    if receipt.insistence == taskboard_domain::Insistence::Alarm {
        let _ = write!(line, " [alarm]");
    }
    line
}

/// The human-readable `cancel` receipt.
#[must_use]
pub fn render_text_cancel(receipt: &TimerCancelReceipt) -> String {
    format!(
        "cancelled timer \"{}\" ({})",
        receipt.label, receipt.alarm_id
    )
}

/// One resolved `timers` invocation: the clap subcommand collapsed to
/// exactly one mode.
#[derive(Debug, Clone)]
pub enum TimersInvocation {
    /// `add <label> --in/--at`.
    Add {
        /// The notification label.
        label: String,
        /// The timing flag pair (clap enforces exactly one).
        when: taskboard_domain::AdhocWhen,
        /// The explicit `--insistence` value; `Normal` when `None`.
        insistence: taskboard_domain::Insistence,
    },
    /// `list` (pure read).
    List,
    /// `cancel <selector>`.
    Cancel(String),
}

/// Collapses the clap subcommand into exactly one invocation. One
/// definition site for both dispatch lanes.
#[must_use]
pub fn collapse(cmd: crate::cli::TimersCmd) -> Option<TimersInvocation> {
    match cmd {
        crate::cli::TimersCmd::Add {
            timer_label: label,
            in_,
            at,
            insistence,
        } => {
            let when = match (in_, at) {
                (Some(dur), _) => taskboard_domain::AdhocWhen::In(dur),
                (None, Some(at)) => taskboard_domain::AdhocWhen::At(at),
                // Unreachable through clap (a required group); the
                // `None` keeps `collapse` total for direct calls.
                (None, None) => return None,
            };
            Some(TimersInvocation::Add {
                label,
                when,
                insistence: insistence.unwrap_or_default(),
            })
        }
        crate::cli::TimersCmd::List => Some(TimersInvocation::List),
        crate::cli::TimersCmd::Cancel { selector } => Some(TimersInvocation::Cancel(selector)),
    }
}

/// The one-shot `timers` dispatch: the mode switch over
/// [`super::run_core`], each mode with its own handler and renderer.
pub async fn timers_command(
    io: &mut crate::commands::CommandIo<'_>,
    config_path: Option<std::path::PathBuf>,
    invocation: TimersInvocation,
) -> u8 {
    match invocation {
        TimersInvocation::List => {
            super::run_core(
                io,
                config_path,
                render_text_list,
                |config, store| async move { list(config, store).await },
            )
            .await
        }
        TimersInvocation::Cancel(selector) => {
            super::run_core(
                io,
                config_path,
                render_text_cancel,
                move |config, store| async move { cancel(config, store, &selector).await },
            )
            .await
        }
        TimersInvocation::Add {
            label,
            when,
            insistence,
        } => super::run_core(
            io,
            config_path,
            render_text_add,
            move |config, store| async move { add(config, store, &label, when, insistence).await },
        )
        .await,
    }
}

/// The session-lane mode switch (`dispatch_line`): pure core reads and
/// mutations, no boot.
pub async fn timers_line(
    app: &crate::App,
    io: &mut crate::commands::CommandIo<'_>,
    invocation: TimersInvocation,
) -> u8 {
    match invocation {
        TimersInvocation::List => {
            crate::commands::emit_result(io, Ok(list_on(app)), render_text_list)
        }
        TimersInvocation::Cancel(selector) => {
            crate::commands::emit_result(io, cancel_on(app, &selector).await, render_text_cancel)
        }
        TimersInvocation::Add {
            label,
            when,
            insistence,
        } => crate::commands::emit_result(
            io,
            add_on(app, &label, when, insistence).await,
            render_text_add,
        ),
    }
}
