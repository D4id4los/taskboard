// SPDX-License-Identifier: AGPL-3.0-only
//! `taskboard tasks repeat` (DR-6/4, origin §3.7's verbs): set a
//! recurring-series rule (`--daily`/`--weekly`/`--monthly`, with
//! optional `--until`/`--count` bounds) or stop repeating (`--off`) —
//! both riding [`StateCommand::UpdateTask`]'s `recur` field family; no
//! new `StateCommand` exists for this surface. The two-lane shape of
//! every verb family: one-shot wrappers boot the core over
//! [`crate::commands::tasks::with_core`], the `*_on` seams run on a
//! booted session core.

use std::sync::Arc;

use chrono::{Datelike as _, Utc};

use taskboard_domain::{
    DayOfMonth, EveryWeeks, MonthlyAnchor, RecurBound, RecurPattern, RecurRule, SeriesId,
    StateCommand, TaskChanges, WeekdaySet, next_occurrence, series_reference_date,
};

use crate::cli::{MonthlyFlag, RepeatArgs, TasksCmd, WeekdaysFlag};
use crate::commands::CommandError;
use crate::commands::output::{RecurRuleReceipt, RepeatReceipt};
use crate::config::AppConfig;
use crate::secrets::CredentialStore;

/// One resolved `tasks repeat` invocation: the clap flags collapsed to
/// exactly one mode (clap enforces the coherence). The bare-flag
/// defaults (`--weekly`/`--monthly` without a value) stay `Option`
/// through the collapse — they need the resolved task and the
/// calendar, which exist only handler-side.
#[derive(Debug, Clone)]
pub enum RepeatInvocation {
    /// `--off <task>`: clear the rule; the head stays an ordinary task.
    Off {
        /// The task selector.
        task: String,
    },
    /// A pattern (exactly one, clap-enforced) plus an optional bound.
    Set {
        /// The task selector.
        task: String,
        /// The interval grammar.
        pattern: PatternSpec,
        /// Where the series stops.
        bound: BoundSpec,
    },
}

/// The collapsed pattern: the explicit flags; the bare-flag defaults
/// remain `None` for handler-side resolution against the reference
/// date.
#[derive(Debug, Clone)]
pub enum PatternSpec {
    /// Every N days.
    Daily(taskboard_domain::EveryDays),
    /// On the given weekdays of every `interval`-th week;
    /// `on: None` = the task's own weekday.
    Weekly {
        /// The week interval.
        interval: EveryWeeks,
        /// The explicit weekday set, if any.
        on: Option<WeekdaySet>,
    },
    /// Monthly; `on: None` = the task's own day-of-month.
    Monthly {
        /// The explicit anchor, if any.
        on: Option<MonthlyAnchor>,
    },
}

/// The collapsed bound.
#[derive(Debug, Clone)]
pub enum BoundSpec {
    /// No bound.
    None,
    /// No member after this inclusive local date.
    Until(chrono::NaiveDate),
    /// At most N completed occurrences (engine-evaluated).
    Count(taskboard_domain::OccurrenceCount),
}

/// Collapses the clap flag tuple into exactly one invocation (clap
/// enforces the coherence: exactly one mode, bounds require the
/// pattern). One definition site for both dispatch lanes.
///
/// # Panics
///
/// Only via `EveryWeeks::new(1)`'s `expect`: 1 is in the mint's domain
/// by construction (the `--weekly` default interval).
#[must_use]
pub fn collapse(cmd: TasksCmd) -> Option<RepeatInvocation> {
    let args = match cmd {
        TasksCmd::Repeat(args) => args,
        // Exhaustive without a wildcard so the let-else rewrite lint
        // stays quiet (a let-else cannot destructure here).
        TasksCmd::List { .. }
        | TasksCmd::Add { .. }
        | TasksCmd::Done { .. }
        | TasksCmd::Move { .. }
        | TasksCmd::Edit { .. }
        | TasksCmd::Delete { .. }
        | TasksCmd::Label { .. }
        | TasksCmd::Unlabel { .. }
        | TasksCmd::Remind { .. } => return None,
    };
    let RepeatArgs {
        task,
        mode,
        every_weeks,
        until,
        count,
    } = args;
    if mode.off {
        return Some(RepeatInvocation::Off { task });
    }
    let pattern = if let Some(daily) = mode.daily {
        PatternSpec::Daily(daily)
    } else if let Some(weekly) = mode.weekly {
        PatternSpec::Weekly {
            interval: every_weeks.unwrap_or_else(
                // `--weekly` without `--every-weeks` is every week; 1 is
                // in the mint's domain by construction.
                || EveryWeeks::new(1).expect("1 is a valid week interval"),
            ),
            on: match weekly {
                WeekdaysFlag::TaskWeekday => None,
                WeekdaysFlag::Days(days) => Some(days),
            },
        }
    } else {
        // The group guarantees one of the four arms; the last pattern
        // flag is the only survivor here.
        PatternSpec::Monthly {
            on: match mode.monthly? {
                MonthlyFlag::TaskDay => None,
                MonthlyFlag::Anchor(anchor) => Some(anchor),
            },
        }
    };
    let bound = match (until, count) {
        (Some(date), _) => BoundSpec::Until(date),
        (_, Some(n)) => BoundSpec::Count(n),
        (None, None) => BoundSpec::None,
    };
    Some(RepeatInvocation::Set {
        task,
        pattern,
        bound,
    })
}

/// Boots the core and runs one repeat mode (the one-shot lane).
///
/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
pub async fn repeat(
    config: AppConfig,
    store: Arc<dyn CredentialStore>,
    invocation: RepeatInvocation,
) -> Result<RepeatReceipt, CommandError> {
    crate::commands::tasks::with_core(config, store, async |app| repeat_on(app, invocation).await)
        .await
}

/// The `tasks repeat` steps on a booted core (the session seam):
/// resolve the task, derive the anchor through the booted config's day
/// calendar (scheduled → duedate → window-today; never a raw-UTC date
/// treated as local), assemble the rule, and ride `UpdateTask`.
///
/// Guards (app-layer by ruling — the engine never learns of them):
/// setting a pattern on a **done** task is refused typed (`--off` works
/// on any live task — clearing a dead rule is honest cleanup); archived
/// tasks are live and repeatable.
///
/// Re-repeat semantics (D-S4): a task already carrying a rule keeps
/// `series` and `continued_from` (the user is re-scheduling the
/// series, not founding a new one — a minted id would reset the
/// engine's `Count` history); a plain task mints a fresh `SeriesId`
/// app-side. The anchor is never touched.
///
/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
///
/// # Panics
///
/// Only via the bare-default `expect`s: a single weekday is never an
/// empty set, and a real date's day number is always `1..=31`.
pub async fn repeat_on(
    app: &crate::App,
    invocation: RepeatInvocation,
) -> Result<RepeatReceipt, CommandError> {
    let now = Utc::now();
    let state = app.engine.shared_state().load_full();
    let (selector, task_id) = match &invocation {
        RepeatInvocation::Off { task } | RepeatInvocation::Set { task, .. } => (
            task.clone(),
            crate::commands::tasks::resolve_task(&state, task)?,
        ),
    };
    let task = state
        .tasks
        .get(&task_id)
        .filter(|t| t.is_live())
        .ok_or_else(|| CommandError::UnknownTask(selector.clone()))?;
    let recur = assemble_rule(
        &invocation,
        task,
        &selector,
        app.config.day_calendar().calendar,
        now,
    )?;
    drop(state);
    app.engine
        .execute(StateCommand::UpdateTask {
            id: task_id,
            changes: TaskChanges {
                recur: Some(recur),
                ..TaskChanges::default()
            },
        })
        .await?;
    // Post-state truth is what the receipt reports (the mute-receipt
    // precedent): a repeated `--off` that found nothing to clear is an
    // accepted same-value write.
    let state = app.engine.shared_state().load_full();
    let task = state
        .tasks
        .get(&task_id)
        .filter(|t| t.is_live())
        .ok_or_else(|| CommandError::UnknownTask(task_id.as_uuid().to_string()))?;
    Ok(repeat_receipt(
        task,
        task_id,
        app.config.day_calendar().calendar,
        now,
    ))
}

/// The `Set` arm's rule assembly: the ruled anchor chain through the
/// day calendar (scheduled → duedate → window-today; never a raw-UTC
/// date treated as local), the D-S4 series-identity carry, the
/// bare-flag defaults against the reference date, and the bound. `--off`
/// carries no rule (`None`).
///
/// # Errors
///
/// [`CommandError::TaskDone`] when a pattern targets a done head —
/// the guard lives app-layer by ruling, the engine never learns of it.
///
/// # Panics
///
/// Only via the bare-default `expect`s: a single weekday is never an
/// empty set, and a real date's day number is always `1..=31`.
fn assemble_rule(
    invocation: &RepeatInvocation,
    task: &taskboard_domain::Task,
    selector: &str,
    day: taskboard_domain::DayCalendar,
    now: chrono::DateTime<Utc>,
) -> Result<Option<RecurRule>, CommandError> {
    let RepeatInvocation::Set { pattern, bound, .. } = invocation else {
        return Ok(None);
    };
    if !task.is_open() {
        return Err(CommandError::TaskDone {
            selector: selector.to_owned(),
        });
    }
    let anchor = series_reference_date(task, day, now);
    let (series, continued_from) = if let Some(r) = task.recur.as_ref() {
        (r.series, r.continued_from)
    } else {
        (SeriesId::from_uuid(uuid::Uuid::now_v7()), None)
    };
    let pattern = match pattern {
        PatternSpec::Daily(every) => RecurPattern::Daily { every: *every },
        PatternSpec::Weekly { interval, on } => RecurPattern::Weekly {
            interval: *interval,
            on: match on {
                Some(days) => days.clone(),
                None => {
                    WeekdaySet::new([anchor.weekday()]).expect("one weekday is a non-empty set")
                }
            },
        },
        PatternSpec::Monthly { on } => RecurPattern::Monthly {
            on: match on {
                Some(anchor) => *anchor,
                None => MonthlyAnchor::OnDay(
                    DayOfMonth::new(anchor.day()).expect("a real date's day number is 1..=31"),
                ),
            },
        },
    };
    let bound = match bound {
        BoundSpec::None => RecurBound::None,
        BoundSpec::Until(date) => RecurBound::Until(*date),
        BoundSpec::Count(n) => RecurBound::Count(*n),
    };
    Ok(Some(RecurRule {
        series,
        anchor,
        pattern,
        bound,
        continued_from,
    }))
}

/// The post-state receipt projection: the rule's canonical text forms
/// with `next` from the domain's next-occurrence law.
fn repeat_receipt(
    task: &taskboard_domain::Task,
    task_id: taskboard_domain::TaskId,
    day: taskboard_domain::DayCalendar,
    now: chrono::DateTime<Utc>,
) -> RepeatReceipt {
    let rule = task.recur.clone().map(|rule| {
        let next = next_occurrence(&rule, task, day, now);
        RecurRuleReceipt {
            series: rule.series.as_uuid().to_string(),
            anchor: rule.anchor,
            pattern: rule.pattern.text_form(),
            bound: rule.bound.text_form(),
            next: next.date,
        }
    });
    RepeatReceipt {
        task_id: task_id.as_uuid().to_string(),
        task_title: task.title.clone(),
        rule,
    }
}

/// The human-readable `tasks repeat` receipt: the set arm names
/// pattern, bound and next member; the cleared arm names the stop.
/// UX text — never asserted (AGENTS §5).
#[must_use]
pub fn render_text_repeat(receipt: &RepeatReceipt) -> String {
    match &receipt.rule {
        Some(rule) => {
            let next = rule.next.map_or_else(
                || "the series' last occurrence".to_owned(),
                |d| d.to_string(),
            );
            let bound = if rule.bound.is_empty() {
                String::new()
            } else {
                format!(" ({})", rule.bound)
            };
            format!(
                "\"{}\" repeats {}{} — next {} (series {})",
                receipt.task_title, rule.pattern, bound, next, rule.series
            )
        }
        None => format!("\"{}\" no longer repeats", receipt.task_title),
    }
}

/// The one-shot `tasks repeat` dispatch: the collapsed mode over
/// [`super::run_core`].
pub async fn repeat_command(
    io: &mut crate::commands::CommandIo<'_>,
    config_path: Option<std::path::PathBuf>,
    invocation: RepeatInvocation,
) -> u8 {
    super::run_core(
        io,
        config_path,
        render_text_repeat,
        move |config, store| async move { repeat(config, store, invocation).await },
    )
    .await
}

/// The session-lane mode switch (`dispatch_line`): no boot, the
/// calendar comes from the booted session config.
pub async fn repeat_line(
    app: &crate::App,
    io: &mut crate::commands::CommandIo<'_>,
    invocation: RepeatInvocation,
) -> u8 {
    crate::commands::emit_result(io, repeat_on(app, invocation).await, render_text_repeat)
}
