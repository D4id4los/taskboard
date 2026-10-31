// SPDX-License-Identifier: AGPL-3.0-only
//! `taskboard habits` (DR-6/6, D-12): the tracker verb family — the
//! track recipe verbatim: one-shot wrappers over
//! [`super::tasks::with_core`], `*_on` seams on a booted session core,
//! one [`collapse`] definition site consumed by both lanes. The CLI
//! only parses and dispatches: target phrases, the list projection,
//! and the streak render are pure functions; receipts are state truth
//! read back from the published snapshot, never input echoes. The
//! D-C13 law holds at every surface: rows render in stored manual
//! `(order, id)` order; `anchor_time` is a display chip only.

use std::path::PathBuf;
use std::sync::Arc;

use chrono::{DateTime, NaiveDate, Utc};

use taskboard_domain::{
    AppState, CommandOutcome, DayCalendar, HabitChanges, HabitId, HabitLogKey, HabitTarget,
    NonZeroCount, StateCommand, WindowStart, habit_streak_unioned,
};

use crate::cli::{AnchorEdit, HabitsCmd, SeamEdit};
use crate::commands::CommandError;
use crate::commands::output::{
    HabitAddReceipt, HabitAdjustReceipt, HabitDeleteReceipt, HabitListView, HabitLogReceipt,
    HabitRow, HabitUpdateReceipt,
};
use crate::config::AppConfig;
use crate::secrets::CredentialStore;

/// One target flag bag collapsed to its [`HabitTarget`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetPick {
    /// `--daily [N]`.
    Daily(u32),
    /// `--weekly [N]`.
    Weekly(u32),
    /// `--monthly [N]`.
    Monthly(u32),
}

impl TargetPick {
    /// The picked target.
    ///
    /// # Panics
    ///
    /// Only through `expect` on clap's quota parse — the CLI's
    /// `parse_quota_flag` rejects zero before the mint ever sees it.
    #[must_use]
    pub fn target(self) -> HabitTarget {
        let quota = |n: u32| NonZeroCount::new(n).expect("clap's quota parse rejects zero");
        match self {
            Self::Daily(n) => HabitTarget::PerDay(quota(n)),
            Self::Weekly(n) => HabitTarget::PerWeek(quota(n)),
            Self::Monthly(n) => HabitTarget::PerMonth(quota(n)),
        }
    }

    /// Collapses the three mutually-exclusive flags; `None` is the
    /// update-path "no target flag given" (add requires one through
    /// clap's `required_unless_present_any`).
    #[must_use]
    pub fn pick(daily: Option<u32>, weekly: Option<u32>, monthly: Option<u32>) -> Option<Self> {
        match (daily, weekly, monthly) {
            (Some(n), _, _) => Some(Self::Daily(n)),
            (None, Some(n), _) => Some(Self::Weekly(n)),
            (None, None, Some(n)) => Some(Self::Monthly(n)),
            (None, None, None) => None,
        }
    }
}

/// One resolved `habits` invocation: the clap subcommand collapsed to
/// exactly one mode.
#[derive(Debug, Clone)]
pub enum HabitsInvocation {
    /// `add <title> …`.
    Add {
        /// The display title.
        title: String,
        /// The picked target (clap guarantees one).
        target: TargetPick,
        /// The board selector; the single live board when omitted.
        board: Option<taskboard_domain::BoardId>,
        /// The anchor chip.
        at: Option<chrono::NaiveTime>,
    },
    /// `list [--board]`.
    List {
        /// The board selector; the single live board when omitted.
        board: Option<taskboard_domain::BoardId>,
    },
    /// `log <habit>`.
    Log {
        /// The habit selector.
        habit: String,
    },
    /// `adjust <habit> --window --count`.
    Adjust {
        /// The habit selector.
        habit: String,
        /// The window's period-start date.
        window: NaiveDate,
        /// The corrected count.
        count: u32,
    },
    /// `update <habit> …` — the change mask.
    Update {
        /// The habit selector.
        habit: String,
        /// New title.
        title: Option<String>,
        /// New target.
        target: Option<TargetPick>,
        /// The chip's edit.
        at: Option<AnchorEdit>,
        /// The absolute manual-order slot (routes `MoveHabit`).
        order: Option<i64>,
        /// The seam's edit.
        continued_from: Option<SeamEdit>,
    },
    /// `rm <habit>`.
    Rm {
        /// The habit selector.
        habit: String,
    },
}

/// Collapses the clap subcommand into exactly one invocation.
///
/// # Panics
///
/// Only through `expect` on the add target: clap's
/// `required_unless_present_any` group guarantees exactly one target
/// flag, so [`TargetPick::pick`]'s `None` cannot occur through the
/// parse surface.
#[must_use]
pub fn collapse(cmd: HabitsCmd) -> HabitsInvocation {
    match cmd {
        HabitsCmd::Add {
            title,
            daily,
            weekly,
            monthly,
            board,
            at,
        } => HabitsInvocation::Add {
            title,
            target: TargetPick::pick(daily, weekly, monthly)
                .expect("clap requires exactly one target flag on habits add"),
            board,
            at,
        },
        HabitsCmd::List { board } => HabitsInvocation::List { board },
        HabitsCmd::Log { habit } => HabitsInvocation::Log { habit },
        HabitsCmd::Adjust {
            habit,
            window,
            count,
        } => HabitsInvocation::Adjust {
            habit,
            window,
            count,
        },
        HabitsCmd::Update {
            habit,
            title,
            daily,
            weekly,
            monthly,
            at,
            order,
            continued_from,
        } => HabitsInvocation::Update {
            habit,
            title,
            target: TargetPick::pick(daily, weekly, monthly),
            at,
            order,
            continued_from,
        },
        HabitsCmd::Rm { habit } => HabitsInvocation::Rm { habit },
    }
}

/// Boots the core and runs the invocation (the one-shot lane).
///
/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
pub async fn run(
    config: AppConfig,
    store: Arc<dyn CredentialStore>,
    invocation: HabitsInvocation,
) -> Result<HabitOutcome, CommandError> {
    super::tasks::with_core(config, store, async |app| run_on(app, invocation).await).await
}

/// The outcome a habit verb produced: receipts are named so the two
/// dispatch lanes share one renderer switch.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(untagged)]
pub enum HabitOutcome {
    /// `add`.
    Added(HabitAddReceipt),
    /// `list`.
    Listed(HabitListView),
    /// `log`.
    Logged(HabitLogReceipt),
    /// `adjust`.
    Adjusted(HabitAdjustReceipt),
    /// `update`.
    Updated(HabitUpdateReceipt),
    /// `rm`.
    Removed(HabitDeleteReceipt),
}

/// The `habits` steps on a booted core (the session seam).
///
/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
pub async fn run_on(
    app: &crate::App,
    invocation: HabitsInvocation,
) -> Result<HabitOutcome, CommandError> {
    match invocation {
        HabitsInvocation::Add {
            title,
            target,
            board,
            at,
        } => Ok(HabitOutcome::Added(
            add_on(app, &title, target.target(), board, at).await?,
        )),
        HabitsInvocation::List { board } => Ok(HabitOutcome::Listed(list_on(app, board)?)),
        HabitsInvocation::Log { habit } => Ok(HabitOutcome::Logged(log_on(app, &habit).await?)),
        HabitsInvocation::Adjust {
            habit,
            window,
            count,
        } => Ok(HabitOutcome::Adjusted(
            adjust_on(app, &habit, window, count).await?,
        )),
        HabitsInvocation::Update {
            habit,
            title,
            target,
            at,
            order,
            continued_from,
        } => Ok(HabitOutcome::Updated(
            update_on(app, &habit, title, target, at, order, continued_from).await?,
        )),
        HabitsInvocation::Rm { habit } => Ok(HabitOutcome::Removed(rm_on(app, &habit).await?)),
    }
}

/// The `habits add` steps: resolve the board, execute, project the
/// receipt from the post-state — the row is read back from the
/// published snapshot, never an echo of the input.
///
/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
pub async fn add_on(
    app: &crate::App,
    title: &str,
    target: HabitTarget,
    board: Option<taskboard_domain::BoardId>,
    at: Option<chrono::NaiveTime>,
) -> Result<HabitAddReceipt, CommandError> {
    let board = super::resolve_board(app, board)?;
    let CommandOutcome::HabitCreated(habit_id) = app
        .engine
        .execute(StateCommand::CreateHabit {
            board,
            title: title.to_owned(),
            target,
        })
        .await?
    else {
        return Err(outcome_mismatch());
    };
    if let Some(time) = at {
        // Create never mints a chip (D-H7a: the mint path carries no
        // optional fields), so `--at` routes as the immediate sparse
        // edit — no new command arm.
        match app
            .engine
            .execute(StateCommand::UpdateHabit {
                habit: habit_id,
                changes: HabitChanges {
                    anchor_time: Some(Some(time)),
                    ..HabitChanges::default()
                },
            })
            .await?
        {
            CommandOutcome::Applied | CommandOutcome::NoOp => {}
            _ => return Err(outcome_mismatch()),
        }
    }
    let state = app.engine.shared_state().load_full();
    let habit = state.habits.habits.get(&habit_id).cloned();
    Ok(HabitAddReceipt {
        habit: habit.as_ref().map(|h| habit_view(h.id, h)),
    })
}

/// The `habits list` steps: resolve the board, project the canonical
/// rows (stored manual `(order, id)` order — the D-C13 law).
///
/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
pub fn list_on(
    app: &crate::App,
    board: Option<taskboard_domain::BoardId>,
) -> Result<HabitListView, CommandError> {
    let board = super::resolve_board(app, board)?;
    let state = app.engine.shared_state().load_full();
    let day = app.config.resolved_day_calendar();
    Ok(HabitListView {
        habits: habit_rows(&state, board, Utc::now(), day),
    })
}

/// The `habits log` steps: resolve the selector, +1, read the touched
/// row back — the receipt names the window the increment landed in and
/// the post-increment count (a surplus repair is user-visible).
///
/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
///
/// # Panics
///
/// Only via `expect` on the post-execute row read: the outcome names
/// the touched window, so the same-state projection cannot miss.
pub async fn log_on(app: &crate::App, selector: &str) -> Result<HabitLogReceipt, CommandError> {
    let id = resolve_habit(&app.engine.shared_state().load_full(), selector)?;
    match app
        .engine
        .execute(StateCommand::LogHabit { habit: id })
        .await?
    {
        CommandOutcome::HabitLogged {
            habit,
            window,
            count,
        } => {
            let state = app.engine.shared_state().load_full();
            let key = HabitLogKey { habit, window };
            let row = state
                .habits
                .log
                .get(&key)
                .copied()
                .expect("the log outcome names a stored row");
            Ok(HabitLogReceipt {
                habit_id: habit.as_uuid().to_string(),
                window: window.as_date(),
                count: row.count,
                reported: count,
            })
        }
        _ => Err(outcome_mismatch()),
    }
}

/// The `habits adjust` steps: resolve, execute the correction, read
/// the row back. The window travels verbatim — a non-canonical one is
/// the planner's typed `invalid-habit-window` refusal passed through
/// structurally, never a silent rewrite.
///
/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
pub async fn adjust_on(
    app: &crate::App,
    selector: &str,
    window: NaiveDate,
    count: u32,
) -> Result<HabitAdjustReceipt, CommandError> {
    let id = resolve_habit(&app.engine.shared_state().load_full(), selector)?;
    match app
        .engine
        .execute(StateCommand::AdjustHabitLog {
            habit: id,
            window: WindowStart::from_stored(window),
            count,
        })
        .await?
    {
        CommandOutcome::HabitLogAdjusted {
            habit,
            window,
            count,
        } => {
            let state = app.engine.shared_state().load_full();
            let key = HabitLogKey { habit, window };
            let row = state.habits.log.get(&key).copied();
            Ok(HabitAdjustReceipt {
                habit_id: habit.as_uuid().to_string(),
                window: window.as_date(),
                count: row.map_or(count, |entry| entry.count),
                reported: count,
            })
        }
        _ => Err(outcome_mismatch()),
    }
}

/// The `habits update` steps: build the change mask, execute it, then
/// `--order`'s `MoveHabit` when the mask names a reorder. A zero-flag
/// mask (and an unchanged one) is the planner's typed no-op — the
/// receipt carries `changed: false` and the current row.
///
/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
#[allow(clippy::too_many_arguments)] // the change mask IS the verb's shape
pub async fn update_on(
    app: &crate::App,
    selector: &str,
    title: Option<String>,
    target: Option<TargetPick>,
    at: Option<AnchorEdit>,
    order: Option<i64>,
    continued_from: Option<SeamEdit>,
) -> Result<HabitUpdateReceipt, CommandError> {
    let id = resolve_habit(&app.engine.shared_state().load_full(), selector)?;
    let changes = HabitChanges {
        title,
        target: target.map(TargetPick::target),
        anchor_time: at.map(|edit| match edit {
            AnchorEdit::Set(time) => Some(time),
            AnchorEdit::Clear => None,
        }),
        continued_from: continued_from.map(|edit| match edit {
            SeamEdit::Link(series) => Some(series),
            SeamEdit::Unlink => None,
        }),
    };
    let mut mask_hit = false;
    match app
        .engine
        .execute(StateCommand::UpdateHabit { habit: id, changes })
        .await?
    {
        CommandOutcome::Applied => mask_hit = true,
        CommandOutcome::NoOp => {}
        _ => return Err(outcome_mismatch()),
    }
    let mut moved = false;
    if let Some(order) = order {
        match app
            .engine
            .execute(StateCommand::MoveHabit { habit: id, order })
            .await?
        {
            CommandOutcome::Applied => moved = true,
            CommandOutcome::NoOp => {}
            _ => return Err(outcome_mismatch()),
        }
    }
    let state = app.engine.shared_state().load_full();
    let habit = state.habits.habits.get(&id).cloned();
    Ok(HabitUpdateReceipt {
        changed: mask_hit || moved,
        habit: habit.as_ref().map(|h| habit_view(h.id, h)),
    })
}

/// The `habits rm` steps: the tombstone; the log survives and the
/// receipt names the surviving row count (history stays queryable).
///
/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
pub async fn rm_on(app: &crate::App, selector: &str) -> Result<HabitDeleteReceipt, CommandError> {
    let id = resolve_habit(&app.engine.shared_state().load_full(), selector)?;
    let habit_id = id.as_uuid().to_string();
    match app
        .engine
        .execute(StateCommand::DeleteHabit { habit: id })
        .await?
    {
        CommandOutcome::Applied => {
            let state = app.engine.shared_state().load_full();
            let log_rows = state
                .habits
                .log
                .keys()
                .filter(|key| key.habit == id)
                .count();
            Ok(HabitDeleteReceipt {
                habit_id,
                deleted: true,
                log_rows,
            })
        }
        CommandOutcome::NoOp => Ok(HabitDeleteReceipt {
            habit_id,
            deleted: false,
            log_rows: 0,
        }),
        _ => Err(outcome_mismatch()),
    }
}

/// Resolves a habit selector (the `resolve_time_entry` recipe): exact
/// uuid → unique ≥8-char lowercased uuid prefix (a collision names
/// both candidates, typed) → exact live title. A tombstoned habit
/// never resolves (the delete surface is the tombstone's only writer).
///
/// # Errors
///
/// [`CommandError::UnknownHabit`] / [`CommandError::AmbiguousHabit`] —
/// typed, never text-matched in the engine.
pub fn resolve_habit(state: &AppState, selector: &str) -> Result<HabitId, CommandError> {
    let habits = &state.habits.habits;
    if let Ok(uuid) = uuid::Uuid::parse_str(selector) {
        let id = HabitId::from(uuid);
        if habits
            .get(&id)
            .is_some_and(taskboard_domain::Habit::is_live)
        {
            return Ok(id);
        }
    }
    if selector.len() >= 8 {
        let needle = selector.to_lowercase();
        let hits: Vec<HabitId> = habits
            .values()
            .filter(|h| h.is_live() && h.id.as_uuid().to_string().starts_with(&needle))
            .map(|h| h.id)
            .collect();
        match hits.as_slice() {
            [id] => return Ok(*id),
            [] => {}
            [_, ..] => {
                return Err(CommandError::AmbiguousHabit {
                    candidates: hits.iter().map(|id| id.as_uuid().to_string()).collect(),
                });
            }
        }
    }
    let by_title: Vec<HabitId> = habits
        .values()
        .filter(|h| h.is_live() && h.title == selector)
        .map(|h| h.id)
        .collect();
    match by_title.as_slice() {
        [id] => Ok(*id),
        [] => Err(CommandError::UnknownHabit {
            selector: selector.to_owned(),
        }),
        [_, ..] => Err(CommandError::AmbiguousHabit {
            candidates: by_title.iter().map(|id| id.as_uuid().to_string()).collect(),
        }),
    }
}

/// One list/surface row source (pure): the board's **live** habits in
/// stored manual `(order, id)` order (D-C13; `anchor_time` appears
/// only as the chip). `count` is the current window's row (the period
/// floored from `day.today(now)`), `streak` the domain walk unioned
/// through the conversion seam, `met` the restyle flag — the +1
/// button never disables on it.
#[must_use]
pub fn habit_rows(
    state: &AppState,
    board: taskboard_domain::BoardId,
    now: DateTime<Utc>,
    day: DayCalendar,
) -> Vec<HabitRow> {
    let today = day.today(now);
    let mut live: Vec<&taskboard_domain::Habit> = state
        .habits
        .habits
        .values()
        .filter(|h| h.is_live() && h.board == board)
        .collect();
    live.sort_by_key(|h| (h.order, h.id));
    live.iter()
        .map(|habit| {
            let window = WindowStart::of_period(habit.target.kind(), today);
            let key = HabitLogKey {
                habit: habit.id,
                window,
            };
            let count = state.habits.log.get(&key).map_or(0, |e| e.count);
            let quota = habit.target.quota().get();
            HabitRow {
                habit_id: habit.id.as_uuid().to_string(),
                title: habit.title.clone(),
                target_phrase: target_phrase(habit.target),
                count,
                quota,
                streak: habit_streak_unioned(habit, &state.habits.log, &state.tasks, now, day),
                chip: habit.anchor_time,
                met: count >= quota,
            }
        })
        .collect()
}

/// The quota phrase (`3/day`, `1/week`, `12/month`).
#[must_use]
pub fn target_phrase(target: HabitTarget) -> String {
    let unit = match target.kind() {
        taskboard_domain::PeriodKind::Day => "day",
        taskboard_domain::PeriodKind::Week => "week",
        taskboard_domain::PeriodKind::Month => "month",
    };
    format!("{}/{}", target.quota().get(), unit)
}

fn habit_view(id: HabitId, habit: &taskboard_domain::Habit) -> HabitRow {
    HabitRow {
        habit_id: id.as_uuid().to_string(),
        title: habit.title.clone(),
        target_phrase: target_phrase(habit.target),
        count: 0,
        quota: habit.target.quota().get(),
        streak: 0,
        chip: habit.anchor_time,
        met: false,
    }
}

/// The human render of any [`HabitOutcome`].
#[must_use]
pub fn render_text(outcome: &HabitOutcome) -> String {
    match outcome {
        HabitOutcome::Added(receipt) => match &receipt.habit {
            Some(row) => format!(
                "added \"{}\" ({}, {}) chip {}",
                row.title,
                row.habit_id,
                row.target_phrase,
                row.chip
                    .map_or_else(|| "none".to_owned(), |t| t.to_string())
            ),
            None => "added".to_owned(),
        },
        HabitOutcome::Listed(view) => {
            if view.habits.is_empty() {
                return "no habits".to_owned();
            }
            view.habits
                .iter()
                .map(|row| {
                    format!(
                        "{} — {}/{} ({}) streak {}{}{}",
                        row.title,
                        row.count,
                        row.quota,
                        row.target_phrase,
                        row.streak,
                        row.chip.map_or(String::new(), |chip| format!(" [@{chip}]")),
                        if row.met { " met" } else { "" }
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        }
        HabitOutcome::Logged(receipt) => format!(
            "logged 1 on habit {} — window {} now {}",
            receipt.habit_id, receipt.window, receipt.count
        ),
        HabitOutcome::Adjusted(receipt) => format!(
            "habit {} window {} set to {} (stored {})",
            receipt.habit_id, receipt.window, receipt.reported, receipt.count
        ),
        HabitOutcome::Updated(receipt) => {
            let mut line = match &receipt.habit {
                Some(row) => format!("updated \"{}\" [{}]", row.title, row.habit_id),
                None => "updated".to_owned(),
            };
            if !receipt.changed {
                line.push_str(" — nothing to change");
            }
            line
        }
        HabitOutcome::Removed(receipt) => {
            if receipt.deleted {
                format!(
                    "deleted habit {} ({} log rows remain queryable)",
                    receipt.habit_id, receipt.log_rows
                )
            } else {
                format!(
                    "habit {} is already absent (nothing to delete)",
                    receipt.habit_id
                )
            }
        }
    }
}

/// The one-shot `habits` dispatch.
pub async fn habits_command(
    io: &mut crate::commands::CommandIo<'_>,
    config_path: Option<PathBuf>,
    invocation: HabitsInvocation,
) -> u8 {
    crate::commands::run_core(
        io,
        config_path,
        render_text,
        move |config, store| async move { run(config, store, invocation).await },
    )
    .await
}

/// The session-lane dispatch (no boot).
pub async fn habits_line(
    app: &crate::App,
    io: &mut crate::commands::CommandIo<'_>,
    invocation: HabitsInvocation,
) -> u8 {
    crate::commands::emit_result(io, run_on(app, invocation).await, render_text)
}

/// The reply contract pins one outcome per habit command (plus
/// `NoOp`); any other reply is an engine contract bug — surfaced as
/// the generic engine kind, never misreported as a user error.
fn outcome_mismatch() -> CommandError {
    CommandError::Engine(Box::new(taskboard_state::ExecuteError::EngineGone))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone as _;
    use taskboard_domain::{HabitLogEntry, HabitState, PeriodKind, period_start};

    fn ts(secs: i64) -> DateTime<Utc> {
        chrono::Utc.timestamp_opt(secs, 0).unwrap()
    }

    fn day() -> DayCalendar {
        DayCalendar {
            day_start_time: chrono::NaiveTime::MIN,
            offset: chrono::FixedOffset::east_opt(0).unwrap(),
        }
    }

    fn habit(
        raw: u128,
        board: u128,
        order: i64,
        deleted: bool,
        chip: Option<chrono::NaiveTime>,
    ) -> taskboard_domain::Habit {
        taskboard_domain::Habit {
            id: HabitId::from(uuid::Uuid::from_u128(raw)),
            board: taskboard_domain::BoardId::from(uuid::Uuid::from_u128(board)),
            title: format!("habit {raw}"),
            target: HabitTarget::PerDay(NonZeroCount::new(3).unwrap()),
            order,
            anchor_time: chip,
            clocks: taskboard_domain::HabitClocks::all(ts(100)),
            continued_from: None,
            deleted,
        }
    }

    #[test]
    fn collapse_yields_every_subcommand() {
        use clap::Parser;
        let parse = |argv: &[&str]| -> HabitsInvocation {
            let cli = crate::cli::Cli::try_parse_from([vec!["taskboard"], argv.to_vec()].concat())
                .expect("the argv parses");
            let Some(crate::cli::Command::Habits { cmd }) = cli.command else {
                panic!("the argv selects the habits group");
            };
            collapse(cmd)
        };
        assert!(matches!(
            parse(&["habits", "add", "water", "--daily"]),
            HabitsInvocation::Add {
                target: TargetPick::Daily(1),
                ..
            }
        ));
        assert!(matches!(
            parse(&["habits", "add", "water", "--weekly", "4"]),
            HabitsInvocation::Add {
                target: TargetPick::Weekly(4),
                ..
            }
        ));
        assert!(matches!(
            parse(&["habits", "add", "water", "--monthly"]),
            HabitsInvocation::Add {
                target: TargetPick::Monthly(1),
                ..
            }
        ));
        // Exactly-one flag: a second is a clap usage error (exit 2).
        assert!(
            crate::cli::Cli::try_parse_from(vec![
                "taskboard",
                "habits",
                "add",
                "water",
                "--daily",
                "--weekly"
            ])
            .is_err()
        );
        // Zero quota is a parse error, not a mint panic.
        assert!(
            crate::cli::Cli::try_parse_from(vec![
                "taskboard",
                "habits",
                "add",
                "water",
                "--daily",
                "0"
            ])
            .is_err()
        );
        // `--at none` on update clears; a time sets.
        assert!(matches!(
            parse(&["habits", "update", "x", "--at", "none"]),
            HabitsInvocation::Update {
                at: Some(AnchorEdit::Clear),
                ..
            }
        ));
        assert!(matches!(
            parse(&["habits", "update", "x", "--at", "07:30"]),
            HabitsInvocation::Update {
                at: Some(AnchorEdit::Set(_)),
                ..
            }
        ));
        assert!(matches!(
            parse(&["habits", "rm", "x"]),
            HabitsInvocation::Rm { .. }
        ));
        // A zero-flag update still parses (the typed no-op).
        assert!(matches!(
            parse(&["habits", "update", "x"]),
            HabitsInvocation::Update {
                title: None,
                target: None,
                at: None,
                order: None,
                continued_from: None,
                ..
            }
        ));
    }

    #[test]
    fn habit_rows_render_stored_order_and_the_chip_is_never_a_sort_key() {
        let board = taskboard_domain::BoardId::from(uuid::Uuid::from_u128(2));
        let mut state = AppState::default();
        let mut a = habit(1, 2, 5, false, None);
        let mut b = habit(2, 2, 9, false, None);
        a.title = "a".into();
        b.title = "b".into();
        let foreign = habit(3, 3, 0, false, None);
        let dead = habit(4, 2, -1, true, None);
        for h in [b.clone(), dead, foreign, a.clone()] {
            state.habits.habits.insert(h.id, h);
        }
        let now = ts(1_700_000_000);
        let rows = habit_rows(&state, board, now, day());
        assert_eq!(
            rows.iter().map(|r| r.title.as_str()).collect::<Vec<_>>(),
            vec!["a", "b"],
            "stored (order, id) ascending; tombstones and foreign boards never render"
        );
        // The negative pin at the presentation layer: perturbing
        // `anchor_time` changes nothing but the chip column.
        let mut perturbed = state.clone();
        let mut chiped = a.clone();
        chiped.anchor_time = Some(chrono::NaiveTime::from_hms_opt(7, 0, 0).unwrap());
        perturbed.habits.habits.insert(chiped.id, chiped);
        let base_rows = habit_rows(&state, board, now, day());
        let chip_rows = habit_rows(&perturbed, board, now, day());
        assert_eq!(base_rows.len(), chip_rows.len());
        for (b_row, c_row) in base_rows.iter().zip(chip_rows.iter()) {
            assert_eq!(b_row.title, c_row.title, "order untouched by the chip");
            assert_eq!(b_row.count, c_row.count);
            assert_eq!(b_row.streak, c_row.streak);
        }
        assert!(chip_rows[0].chip.is_some(), "the chip renders");
    }

    #[test]
    fn resolve_habit_hits_exact_uuid_then_unique_prefix_then_exact_live_title() {
        let mut state = AppState::default();
        let a = habit(0x1234_5678_0000_0000_0000_0000_0000_0001, 2, 0, false, None);
        let mut b = habit(0xfeed_ba09_0000_0000_0000_0000_0000_0002, 2, 1, false, None);
        b.title = "water".into();
        let (id_a, id_b) = (a.id, b.id);
        state.habits.habits.insert(a.id, a);
        state.habits.habits.insert(b.id, b);
        let full = id_a.as_uuid().to_string();
        assert_eq!(resolve_habit(&state, &full).expect("exact uuid"), id_a);
        assert_eq!(
            resolve_habit(&state, &full[..8]).expect("unique prefix"),
            id_a
        );
        assert_eq!(
            resolve_habit(&state, "water").expect("exact live title"),
            id_b,
            "the title pass reads the *renamed* row"
        );
    }

    #[test]
    fn resolve_habit_names_both_candidates_and_refuses_misses_typedly() {
        let mut state = AppState::default();
        let twin_a = habit(0x1234_0000_0000_0000_0000_0000_0000_0001, 2, 0, false, None);
        let twin_b = habit(0x1234_0000_0000_0000_0000_0000_0000_0002, 2, 1, false, None);
        let uuid_a = twin_a.id.as_uuid();
        let uuid_b = twin_b.id.as_uuid();
        state.habits.habits.insert(twin_a.id, twin_a);
        state.habits.habits.insert(twin_b.id, twin_b);
        let err = resolve_habit(&state, &uuid_a.to_string()[..8]).expect_err("two hits");
        let CommandError::AmbiguousHabit { candidates } = &err else {
            panic!("the shared prefix is typed ambiguous");
        };
        assert_eq!(candidates.len(), 2);
        assert_eq!(err.error_kind(), "ambiguous-habit");
        let err = resolve_habit(&state, "ffffffff").expect_err("no hit");
        assert!(matches!(err, CommandError::UnknownHabit { .. }));
        assert_eq!(err.error_kind(), "unknown-habit");
        let _ = uuid_b;
    }

    #[test]
    fn the_target_phrase_is_the_quota_over_the_unit() {
        assert_eq!(
            target_phrase(HabitTarget::PerDay(NonZeroCount::new(3).unwrap())),
            "3/day"
        );
        assert_eq!(
            target_phrase(HabitTarget::PerWeek(NonZeroCount::new(1).unwrap())),
            "1/week"
        );
        assert_eq!(
            target_phrase(HabitTarget::PerMonth(NonZeroCount::new(12).unwrap())),
            "12/month"
        );
    }

    #[test]
    fn target_pick_collapses_exactly_one_flag_and_none_for_the_update_mask() {
        assert!(matches!(
            TargetPick::pick(Some(2), None, None),
            Some(TargetPick::Daily(2))
        ));
        assert!(matches!(
            TargetPick::pick(None, Some(3), None),
            Some(TargetPick::Weekly(3))
        ));
        assert!(matches!(
            TargetPick::pick(None, None, Some(4)),
            Some(TargetPick::Monthly(4))
        ));
        assert!(
            TargetPick::pick(None, None, None).is_none(),
            "the update mask's untouched target"
        );
    }

    #[test]
    fn period_start_seeds_the_row_source_window() {
        // The row source floors `today` through the domain vocabulary —
        // a weekly habit's row read on a Wednesday reads the Monday key.
        let wednesday = NaiveDate::from_ymd_opt(2026, 10, 28).unwrap();
        assert_eq!(
            period_start(PeriodKind::Week, wednesday),
            NaiveDate::from_ymd_opt(2026, 10, 26).unwrap()
        );
        let _ = HabitLogEntry {
            count: 0,
            clock: ts(0),
        };
        let _ = HabitState::default();
    }
}
