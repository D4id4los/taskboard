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

use crate::cli::DayEdit;

use crate::cli::{AnchorEdit, HabitsCmd, SeamEdit};
use crate::commands::output::{
    HabitAddReceipt, HabitAdjustReceipt, HabitDeleteReceipt, HabitHistoryView, HabitListView,
    HabitLogReceipt, HabitPauseReceipt, HabitRow, HabitSkipReceipt, HabitStatsView,
    HabitUpdateReceipt, HabitWindowRow, HabitWindowStateView,
};
use crate::commands::{CommandError, outcome_mismatch};
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
        /// The description's immediate sparse edit.
        description: Option<String>,
        /// The anchor day's immediate sparse edit.
        day: Option<DayEdit>,
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
        /// The description's edit (`none` clears via `Some("")`).
        description: Option<String>,
        /// The anchor day's edit.
        day: Option<DayEdit>,
        /// The scheduled-day set (HF4): a set replaces, `all` clears.
        days: Option<taskboard_domain::habit::WeekdaySet>,
        /// The override's edit: `Some(Some(..))` sets, `Some(None)`
        /// clears, `None` = untouched (the `anchor_time` twin).
        reminder: Option<Option<taskboard_domain::habit::HabitReminderOverride>>,
        /// The absolute manual-order slot (routes `MoveHabit`).
        order: Option<i64>,
        /// The `--before` neighbour selector (routes `HabitReorder`,
        /// #1356).
        before: Option<String>,
        /// The `--after` neighbour selector (routes `HabitReorder`,
        /// #1356).
        after: Option<String>,
        /// The seam's edit.
        continued_from: Option<SeamEdit>,
    },
    /// `rm <habit>`.
    Rm {
        /// The habit selector.
        habit: String,
    },
    /// `skip <habit>`.
    Skip {
        /// The habit selector.
        habit: String,
    },
    /// `pause <habit> --until <date>`.
    Pause {
        /// The habit selector.
        habit: String,
        /// The inclusive pause-through date.
        until: NaiveDate,
    },
    /// `resume <habit>`.
    Resume {
        /// The habit selector.
        habit: String,
    },
    /// `restore <habit>` — the #901 restore-family verb's first
    /// surface (tombstone-only resurrect; the log is never touched,
    /// D26).
    Restore {
        /// The habit selector (uuid | ≥8-char uuid prefix;
        /// tombstones resolve, no title leg).
        habit: String,
    },
    /// `history <habit>` (HF3, US-7).
    History {
        /// The habit selector (tombstones match too).
        habit: String,
    },
    /// `stats <habit>` (HF3, US-9).
    Stats {
        /// The habit selector (tombstones match too).
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
            description,
            day,
        } => HabitsInvocation::Add {
            title,
            target: TargetPick::pick(daily, weekly, monthly)
                .expect("clap requires exactly one target flag on habits add"),
            board,
            at,
            description,
            day,
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
            description,
            day,
            days,
            reminder,
            reminder_clear,
            order,
            before,
            after,
            continued_from,
        } => HabitsInvocation::Update {
            habit,
            title,
            target: TargetPick::pick(daily, weekly, monthly),
            at,
            description,
            day,
            days,
            reminder: reminder.map(Some).or(reminder_clear.then_some(None)),
            order,
            before,
            after,
            continued_from,
        },
        HabitsCmd::Rm { habit } => HabitsInvocation::Rm { habit },
        HabitsCmd::Skip { habit } => HabitsInvocation::Skip { habit },
        HabitsCmd::Pause { habit, until } => HabitsInvocation::Pause { habit, until },
        HabitsCmd::Resume { habit } => HabitsInvocation::Resume { habit },
        HabitsCmd::Restore { habit } => HabitsInvocation::Restore { habit },
        HabitsCmd::History { habit } => HabitsInvocation::History { habit },
        HabitsCmd::Stats { habit } => HabitsInvocation::Stats { habit },
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
    /// `restore` — the pinned engine receipt verbatim (`HabitRestored`
    /// or the accepted live-target `NoOp`; the serde-pinned
    /// `CommandOutcome` is the `--json` contract, journey D-3/W-5).
    Restored(CommandOutcome),
    /// `skip`.
    Skipped(HabitSkipReceipt),
    /// `pause` / `resume`.
    Paused(HabitPauseReceipt),
    /// `history` (HF3, US-7).
    History(HabitHistoryView),
    /// `stats` (HF3, US-9).
    Stats(HabitStatsView),
}

/// The `habits skip` steps: resolve, mark the current canonical window
/// skipped, read the streak back (US-1: the receipt shows the streak
/// survived — display data, never an input echo).
///
/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
pub async fn skip_on(app: &crate::App, selector: &str) -> Result<HabitSkipReceipt, CommandError> {
    let id = resolve_habit(&app.engine.shared_state().load_full(), selector)?;
    match app
        .engine
        .execute(StateCommand::SkipHabit { habit: id })
        .await?
    {
        CommandOutcome::HabitSkipped { habit, window } => {
            let state = app.engine.shared_state().load_full();
            let row = state.habits.habits[&habit].clone();
            Ok(HabitSkipReceipt {
                habit_id: habit.as_uuid().to_string(),
                window: window.as_date(),
                streak: taskboard_domain::habit::streak(
                    &row,
                    &state.habits.log,
                    Utc::now(),
                    app.config.resolved_day_calendar(),
                ),
            })
        }
        _ => Err(outcome_mismatch()),
    }
}

/// The `habits pause` steps: resolve, execute the pause (a past-date
/// through-date is the planner's typed `invalid-habit-pause` refusal
/// passed through structurally), read the field back.
///
/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
pub async fn pause_on(
    app: &crate::App,
    selector: &str,
    until: Option<NaiveDate>,
) -> Result<HabitPauseReceipt, CommandError> {
    let id = resolve_habit(&app.engine.shared_state().load_full(), selector)?;
    match app
        .engine
        .execute(StateCommand::PauseHabit { habit: id, until })
        .await?
    {
        CommandOutcome::Applied | CommandOutcome::NoOp => {
            let state = app.engine.shared_state().load_full();
            Ok(HabitPauseReceipt {
                habit_id: id.as_uuid().to_string(),
                until: state.habits.habits[&id].paused_until,
            })
        }
        _ => Err(outcome_mismatch()),
    }
}

/// The `habits resume` steps: the pause's explicit `None` clear leg.
///
/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
pub async fn resume_on(
    app: &crate::App,
    selector: &str,
) -> Result<HabitPauseReceipt, CommandError> {
    pause_on(app, selector, None).await
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
            description,
            day,
        } => {
            let target = target.target();
            validate_day_against_target(day, target)?;
            Ok(HabitOutcome::Added(
                add_on(app, &title, target, board, at, description, day).await?,
            ))
        }
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
            description,
            day,
            days,
            reminder,
            order,
            before,
            after,
            continued_from,
        } => {
            if let Some(pick) = target {
                validate_day_against_target(day, pick.target())?;
            }
            Ok(HabitOutcome::Updated(
                update_on(
                    app,
                    &habit,
                    title,
                    target,
                    at,
                    description,
                    day,
                    order,
                    before,
                    after,
                    continued_from,
                    days,
                    reminder,
                )
                .await?,
            ))
        }
        HabitsInvocation::Rm { habit } => Ok(HabitOutcome::Removed(rm_on(app, &habit).await?)),
        HabitsInvocation::Skip { habit } => Ok(HabitOutcome::Skipped(skip_on(app, &habit).await?)),
        HabitsInvocation::Pause { habit, until } => Ok(HabitOutcome::Paused(
            pause_on(app, &habit, Some(until)).await?,
        )),
        HabitsInvocation::Resume { habit } => {
            Ok(HabitOutcome::Paused(resume_on(app, &habit).await?))
        }
        HabitsInvocation::Restore { habit } => {
            Ok(HabitOutcome::Restored(restore_on(app, &habit).await?))
        }
        HabitsInvocation::History { habit } => Ok(HabitOutcome::History(history_on(app, &habit)?)),
        HabitsInvocation::Stats { habit } => Ok(HabitOutcome::Stats(stats_on(app, &habit)?)),
    }
}

/// The US-6 parse-boundary kind check: a `--day` variant must match
/// the concurrent cadence flag when one is given (a `mon..sun` day on
/// `--monthly`, a `1..31` day on `--weekly`, or any `--day` at all on
/// `--daily` are all refused typedly). Without a concurrent cadence
/// flag the field is accepted — the domain's documented inertness is
/// the answer, and no cross-command lookup exists in the parser.
///
/// # Errors
///
/// [`CommandError::HabitDayKindMismatch`] on any mismatched pairing.
pub fn validate_day_against_target(
    day: Option<DayEdit>,
    target: HabitTarget,
) -> Result<(), CommandError> {
    // Absent or clearing: untouched or unset — every cadence accepts.
    let Some(DayEdit::Set(day)) = day else {
        return Ok(());
    };
    let matched = match (target.kind(), day) {
        (taskboard_domain::PeriodKind::Week, taskboard_domain::habit::AnchorDay::Weekday(_))
        | (taskboard_domain::PeriodKind::Month, taskboard_domain::habit::AnchorDay::MonthDay(_)) => {
            true
        }
        (taskboard_domain::PeriodKind::Week, taskboard_domain::habit::AnchorDay::MonthDay(_))
        | (taskboard_domain::PeriodKind::Month, taskboard_domain::habit::AnchorDay::Weekday(_))
        | (taskboard_domain::PeriodKind::Day, _) => false,
    };
    if matched {
        Ok(())
    } else {
        Err(CommandError::HabitDayKindMismatch)
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
    description: Option<String>,
    day: Option<DayEdit>,
) -> Result<HabitAddReceipt, CommandError> {
    let board = super::resolve_board(&app.engine, board)?;
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
    if at.is_some() || description.is_some() || day.is_some() {
        // Create never mints optional fields (D-H7a: the mint path
        // carries no chips), so `--at`/`--description`/`--day` route
        // as one immediate sparse edit — no new command arm.
        match app
            .engine
            .execute(StateCommand::UpdateHabit {
                habit: habit_id,
                changes: HabitChanges {
                    anchor_time: at.map(Some),
                    description,
                    anchor_day: day.and_then(DayEdit::change),
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
    let board = super::resolve_board(&app.engine, board)?;
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

/// The update verb's move leg (#1356): `--before/--after <habit>`
/// mints the gap index the landed `HabitReorder` grammar consumes —
/// the deterministic relative address over a densely ordered section
/// (the absolute `--order` tie resolves by id luck). The neighbour
/// must live on the habit's board: the reorder grammar is per
/// section. Yields whether a move leg executed (the receipt's
/// `moved` disjunct).
async fn apply_habit_move(
    app: &crate::App,
    id: taskboard_domain::HabitId,
    order: Option<i64>,
    before: Option<String>,
    after: Option<String>,
) -> Result<bool, CommandError> {
    if before.is_some() && after.is_some() {
        return Err(CommandError::MoveNeighbourMismatch {
            selector: "--before and --after are one mode".to_owned(),
        });
    }
    let after_mode = after.is_some();
    match before.or(after) {
        Some(neighbour_sel) => {
            let state = app.engine.shared_state().load_full();
            let neighbour = resolve_habit(&state, &neighbour_sel)?;
            let board = state.habits.habits.get(&id).map(|row| row.board).ok_or(
                CommandError::UnknownHabit {
                    selector: neighbour_sel.clone(),
                },
            )?;
            let neighbour_row = state
                .habits
                .habits
                .get(&neighbour)
                .filter(|row| row.is_live())
                .ok_or(CommandError::UnknownHabit {
                    selector: neighbour_sel.clone(),
                })?;
            if neighbour_row.board != board {
                return Err(CommandError::MoveNeighbourMismatch {
                    selector: neighbour_sel,
                });
            }
            let mut rows: Vec<&taskboard_domain::habit::Habit> = state
                .habits
                .habits
                .values()
                .filter(|row| row.is_live() && row.board == board)
                .collect();
            rows.sort_by_key(|row| (row.order, row.id));
            let index = rows.iter().position(|row| row.id == neighbour).ok_or(
                CommandError::UnknownHabit {
                    selector: neighbour_sel.clone(),
                },
            )?;
            // The engine's gap semantics (`habit_reorder_plan`):
            // `--before` targets the neighbour's gap, `--after` the
            // gap past it — landing on the mover's own edges is the
            // accepted in-place no-op.
            let target = index + usize::from(after_mode);
            drop(state);
            match app
                .engine
                .execute(StateCommand::HabitReorder { habit: id, target })
                .await?
            {
                CommandOutcome::Applied => Ok(true),
                CommandOutcome::NoOp => Ok(false),
                _ => Err(outcome_mismatch()),
            }
        }
        None => match order {
            Some(order) => match app
                .engine
                .execute(StateCommand::MoveHabit { habit: id, order })
                .await?
            {
                CommandOutcome::Applied => Ok(true),
                CommandOutcome::NoOp => Ok(false),
                _ => Err(outcome_mismatch()),
            },
            None => Ok(false),
        },
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
    description: Option<String>,
    day: Option<DayEdit>,
    order: Option<i64>,
    before: Option<String>,
    after: Option<String>,
    continued_from: Option<SeamEdit>,
    days_mask: Option<taskboard_domain::habit::WeekdaySet>,
    reminder_mask: Option<Option<taskboard_domain::habit::HabitReminderOverride>>,
) -> Result<HabitUpdateReceipt, CommandError> {
    let id = resolve_habit(&app.engine.shared_state().load_full(), selector)?;
    let changes = HabitChanges {
        title,
        description,
        target: target.map(TargetPick::target),
        anchor_time: at.map(|edit| match edit {
            AnchorEdit::Set(time) => Some(time),
            AnchorEdit::Clear => None,
        }),
        anchor_day: day.and_then(DayEdit::change),
        scheduled_days: days_mask,
        reminder: reminder_mask,
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
    let moved = apply_habit_move(app, id, order, before, after).await?;
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

/// Resolves a *restore* target: exact uuid → unique ≥8-char uuid
/// prefix, tombstones included, and deliberately **no title leg** —
/// tombstoned rows may share titles, so ids are the restore family's
/// only address (journey D-2).
///
/// # Errors
///
/// [`CommandError::UnknownHabit`] / [`CommandError::AmbiguousHabit`] —
/// typed, never text-matched.
pub(crate) fn resolve_restorable_habit(
    state: &AppState,
    selector: &str,
) -> Result<HabitId, CommandError> {
    let habits = &state.habits.habits;
    if let Ok(uuid) = uuid::Uuid::parse_str(selector) {
        let id = HabitId::from(uuid);
        return if habits.contains_key(&id) {
            Ok(id)
        } else {
            Err(CommandError::UnknownHabit {
                selector: selector.to_owned(),
            })
        };
    }
    if selector.len() >= 8 {
        let needle = selector.to_lowercase();
        let hits: Vec<HabitId> = habits
            .values()
            .filter(|h| h.id.as_uuid().to_string().starts_with(&needle))
            .map(|h| h.id)
            .collect();
        if let [id] = hits.as_slice() {
            return Ok(*id);
        }
        if !hits.is_empty() {
            return Err(CommandError::AmbiguousHabit {
                candidates: hits.iter().map(|id| id.as_uuid().to_string()).collect(),
            });
        }
    }
    Err(CommandError::UnknownHabit {
        selector: selector.to_owned(),
    })
}

/// The `habits restore` steps on a booted core (both dispatch lanes):
/// dispatches [`StateCommand::RestoreHabit`] and returns the engine's
/// pinned receipt verbatim (`HabitRestored`, or `NoOp` for the
/// accepted live-target no-op) — the `--json` payload is the
/// serde-pinned [`CommandOutcome`] itself (journey D-3/W-5).
///
/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
pub(crate) async fn restore_on(
    app: &crate::App,
    selector: &str,
) -> Result<CommandOutcome, CommandError> {
    let state = app.engine.shared_state().load_full();
    let id = resolve_restorable_habit(&state, selector)?;
    drop(state);
    match app
        .engine
        .execute(StateCommand::RestoreHabit { id })
        .await?
    {
        // The pinned receipt family: the restore fact, or the accepted
        // live-target no-op. Anything else is an engine contract break.
        outcome @ (CommandOutcome::HabitRestored { .. } | CommandOutcome::NoOp) => Ok(outcome),
        _ => Err(outcome_mismatch()),
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
                description: habit.description.clone(),
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
        description: habit.description.clone(),
        target_phrase: target_phrase(habit.target),
        count: 0,
        quota: habit.target.quota().get(),
        streak: 0,
        chip: habit.anchor_time,
        met: false,
    }
}

/// The `habits history` render.
#[must_use]
pub fn render_history(view: &HabitHistoryView) -> String {
    if view.windows.is_empty() {
        return format!("history of \"{}\" — no windows yet", view.title);
    }
    let rows: Vec<String> = view
        .windows
        .iter()
        .map(|row| {
            format!(
                "{} {} (count {})",
                row.window,
                row_state_label(row.state),
                row.count
            )
        })
        .collect();
    format!(
        "history of \"{}\" [{}] ({} windows)\n{}",
        view.title,
        view.habit_id,
        view.windows.len(),
        rows.join("\n")
    )
}

/// The `habits stats` render.
#[must_use]
pub fn render_stats(view: &HabitStatsView) -> String {
    let adherence = if view.adherence_empty {
        "no windows yet".to_owned()
    } else {
        format!(
            "{}/{} scheduled",
            view.adherence_met, view.adherence_scheduled
        )
    };
    format!(
        "stats of \"{}\" [{}] — adherence {}, best streak {}, {} window totals",
        view.title,
        view.habit_id,
        adherence,
        view.best_streak,
        view.totals.len()
    )
}

/// The typed state's address label (the variant's own name — an
/// address, never a prose oracle).
#[must_use]
pub const fn row_state_label(state: HabitWindowStateView) -> &'static str {
    match state {
        HabitWindowStateView::Met => "met",
        HabitWindowStateView::Pending => "pending",
        HabitWindowStateView::Skipped => "skipped",
        HabitWindowStateView::Paused => "paused",
        HabitWindowStateView::PreSchedule => "pre_schedule",
        HabitWindowStateView::Missed => "missed",
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
                    // The description column appears only when
                    // non-empty (the additive display law).
                    let description = if row.description.is_empty() {
                        String::new()
                    } else {
                        format!(" — {}", row.description)
                    };
                    format!(
                        "{}{description} — {}/{} ({}) streak {}{}{}",
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
        HabitOutcome::Skipped(receipt) => format!(
            "skipped habit {} — window {} marked (streak {})",
            receipt.habit_id, receipt.window, receipt.streak
        ),
        HabitOutcome::Paused(receipt) => {
            if let Some(until) = receipt.until {
                format!("habit {} paused through {until}", receipt.habit_id)
            } else {
                format!("habit {} resumed", receipt.habit_id)
            }
        }
        HabitOutcome::History(view) => render_history(view),
        HabitOutcome::Stats(view) => render_stats(view),
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
        HabitOutcome::Restored(CommandOutcome::HabitRestored { habit }) => {
            format!("restored habit {} (log intact)", habit.as_uuid())
        }
        HabitOutcome::Restored(CommandOutcome::NoOp) => {
            "habit is already live (nothing to restore)".to_owned()
        }
        HabitOutcome::Restored(_) => "unexpected restore receipt".to_owned(),
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

// ---- History + stats (HF3, US-7/US-9) --------------------------------------

/// The projection verbs' resolver: the same uuid/prefix/title ladder
/// as [`resolve_habit`] but **tombstones match** — the log outlives
/// the tracker (US-7 AC-5), so a deleted habit's history and stats
/// still render. Exact-title candidates consider every stored row.
///
/// # Errors
///
/// [`CommandError::UnknownHabit`] / [`CommandError::AmbiguousHabit`] —
/// typed, never text-matched.
pub fn resolve_habit_with_tombstones(
    state: &AppState,
    selector: &str,
) -> Result<HabitId, CommandError> {
    let habits = &state.habits.habits;
    if let Ok(uuid) = uuid::Uuid::parse_str(selector) {
        let id = HabitId::from(uuid);
        if habits.contains_key(&id) {
            return Ok(id);
        }
    }
    if selector.len() >= 8 {
        let needle = selector.to_lowercase();
        let hits: Vec<HabitId> = habits
            .values()
            .filter(|h| h.id.as_uuid().to_string().starts_with(&needle))
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
        .filter(|h| h.title == selector)
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

/// One canonical window's typed state (the walk's classification,
/// verbatim semantics — met / pending / missed).
fn window_state_view(
    habit: &taskboard_domain::habit::Habit,
    row: Option<&taskboard_domain::HabitLogEntry>,
    window: chrono::NaiveDate,
    now: DateTime<Utc>,
    day: taskboard_domain::DayCalendar,
) -> HabitWindowStateView {
    let kind = habit.target.kind();
    let quota = habit.target.quota().get();
    let count = row.map_or(0, |entry| entry.count);
    if habit
        .paused_until
        .is_some_and(|until| taskboard_domain::habit::pause_covers_window(kind, window, until))
    {
        // Pause coverage first — the walk's composing arm.
        HabitWindowStateView::Paused
    } else if count >= quota {
        HabitWindowStateView::Met
    } else if row.is_some_and(|entry| entry.skip) {
        HabitWindowStateView::Skipped
    } else if taskboard_domain::habit::preschedule_window(
        habit,
        taskboard_domain::habit::WindowStart::of_period(kind, window),
        day.today(now),
    ) {
        HabitWindowStateView::PreSchedule
    } else {
        let grace = chrono::TimeDelta::try_hours(24).expect("24 h is representable");
        let grace_end = taskboard_domain::habit::period_end(kind, window, day) + grace;
        if now >= grace_end {
            HabitWindowStateView::Missed
        } else {
            HabitWindowStateView::Pending
        }
    }
}

/// The `habits history` steps (HF3, US-7): the canonical log windows
/// chronological, typed states. Pure projection — no command.
///
/// # Errors
///
/// [`CommandError`] — typed, see [`resolve_habit_with_tombstones`].
pub fn history_on(app: &crate::App, selector: &str) -> Result<HabitHistoryView, CommandError> {
    let state = app.engine.shared_state().load_full();
    let id = resolve_habit_with_tombstones(&state, selector)?;
    let habit = state
        .habits
        .habits
        .get(&id)
        .ok_or(CommandError::UnknownHabit {
            selector: selector.to_owned(),
        })?;
    let day = app.config.resolved_day_calendar();
    let now = Utc::now();
    let kind = habit.target.kind();
    let windows: Vec<HabitWindowRow> =
        taskboard_domain::habit::statistics_horizon(habit, &state.habits.log, now, day)
            .into_iter()
            .map(|window| {
                let key = taskboard_domain::HabitLogKey {
                    habit: id,
                    window: taskboard_domain::habit::WindowStart::of_period(kind, window),
                };
                let entry = state.habits.log.get(&key);
                HabitWindowRow {
                    window,
                    count: entry.map_or(0, |row| row.count),
                    state: window_state_view(habit, entry, window, now, day),
                }
            })
            .collect();
    Ok(HabitHistoryView {
        habit_id: id.as_uuid().to_string(),
        title: habit.title.clone(),
        windows,
    })
}

/// The `habits stats` steps (HF3, US-9): the domain fns' typed
/// values. Pure projection — no command.
///
/// # Errors
///
/// [`CommandError`] — typed, see [`resolve_habit_with_tombstones`].
pub fn stats_on(app: &crate::App, selector: &str) -> Result<HabitStatsView, CommandError> {
    let state = app.engine.shared_state().load_full();
    let id = resolve_habit_with_tombstones(&state, selector)?;
    stats_projection(&state, id, Utc::now(), app.config.resolved_day_calendar())
}

/// The typed stats projection over a resolved habit (pure — the
/// verb's engine step is only resolve + this).
///
/// # Errors
///
/// [`CommandError::UnknownHabit`] — typed.
pub fn stats_projection(
    state: &AppState,
    id: HabitId,
    now: DateTime<Utc>,
    day: taskboard_domain::DayCalendar,
) -> Result<HabitStatsView, CommandError> {
    let habit = state
        .habits
        .habits
        .get(&id)
        .ok_or(CommandError::UnknownHabit {
            selector: id.as_uuid().to_string(),
        })?;
    let ratio = taskboard_domain::habit::adherence(habit, &state.habits.log, now, day);
    let best = taskboard_domain::habit::best_streak(habit, &state.habits.log, now, day);
    let totals = taskboard_domain::habit::per_period_totals(habit, &state.habits.log);
    Ok(HabitStatsView {
        habit_id: id.as_uuid().to_string(),
        title: habit.title.clone(),
        adherence_empty: ratio.is_empty(),
        adherence_met: ratio.met(),
        adherence_scheduled: ratio.scheduled(),
        best_streak: best,
        totals: totals
            .into_iter()
            .map(|(window, count)| (window.as_date(), count))
            .collect(),
    })
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
            description: String::new(),
            target: HabitTarget::PerDay(NonZeroCount::new(3).unwrap()),
            order,
            anchor_time: chip,
            anchor_day: None,
            scheduled_days: None,
            reminder: None,
            clocks: taskboard_domain::HabitClocks::all(ts(100)),
            continued_from: None,
            deleted,
            by: None,

            paused_until: None,
        }
    }

    // US-9/S2 (HF3, W-10): the stats projection carries the domain
    // fns' typed values — the empty history is the typed empty
    // state, the populated one the met/scheduled pair.
    #[test]
    fn us9_s2_cli_stats_renders_the_typed_values() {
        let mut state = AppState::default();
        let h = habit(11, 1, 0, false, None);
        let mut log = std::collections::BTreeMap::new();
        let today = chrono::NaiveDate::from_ymd_opt(2026, 11, 20).unwrap();
        for (d, count) in [(18, 3_u32), (19, 0_u32)] {
            log.insert(
                taskboard_domain::HabitLogKey {
                    habit: h.id,
                    window: taskboard_domain::habit::WindowStart::of_period(
                        PeriodKind::Day,
                        today
                            - chrono::Duration::days(
                                today
                                    .signed_duration_since(
                                        chrono::NaiveDate::from_ymd_opt(2026, 11, d).unwrap(),
                                    )
                                    .num_days(),
                            ),
                    ),
                },
                HabitLogEntry {
                    count,
                    clock: ts(10),
                    by: None,
                    skip: false,
                },
            );
        }
        state.habits = HabitState {
            habits: std::collections::BTreeMap::from([(h.id, h.clone())]),
            log,
            day_log: std::collections::BTreeMap::default(),
            time_log: std::collections::BTreeMap::default(),
            events: std::collections::BTreeMap::default(),
        };
        // The empty twin: same shape, no rows.
        let empty = habit(12, 1, 0, false, None);
        let empty_id = empty.id;
        state.habits.habits.insert(empty.id, empty);
        let numbers =
            crate::commands::habits::stats_projection(&state, h.id, ts(1_795_440_000), day())
                .unwrap();
        assert_eq!(numbers.adherence_met, 1);
        assert!(numbers.adherence_scheduled >= 2);
        assert!(!numbers.adherence_empty);
        let vacant =
            crate::commands::habits::stats_projection(&state, empty_id, ts(1_795_440_000), day())
                .unwrap();
        assert!(vacant.adherence_empty);
        assert_eq!(vacant.best_streak, 0);
    }

    // US-7 AC-5 (HF3, W-8): a tombstoned habit's log outlives the
    // tracker — the projection verbs resolve it.
    #[test]
    fn us7_tombstoned_history_still_projects() {
        let mut state = AppState::default();
        let h = habit(13, 1, 0, true, None);
        let id = h.id;
        state.habits = HabitState {
            habits: std::collections::BTreeMap::from([(id, h)]),
            log: std::collections::BTreeMap::new(),
            day_log: std::collections::BTreeMap::default(),
            time_log: std::collections::BTreeMap::default(),
            events: std::collections::BTreeMap::default(),
        };
        let resolved =
            resolve_habit_with_tombstones(&state, &format!("{}", id.as_uuid())).expect("resolves");
        assert_eq!(resolved, id);
        assert!(resolve_habit(&state, &format!("{}", id.as_uuid())).is_err());
    }

    #[test]
    #[allow(clippy::too_many_lines)] // one parse sample per mask slot
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
        // `--description` travels as text; `none` records the empty
        // clear value.
        assert!(matches!(
            parse(&["habits", "add", "water", "--daily", "--description", "note"]),
            HabitsInvocation::Add {
                description: Some(desc),
                ..
            } if desc == "note"
        ));
        assert!(matches!(
            parse(&["habits", "update", "x", "--description", "none"]),
            HabitsInvocation::Update {
                description: Some(desc),
                ..
            } if desc.is_empty()
        ));
        // `--day` parses weekday names and month days; `none` clears.
        assert!(matches!(
            parse(&["habits", "update", "x", "--day", "mon"]),
            HabitsInvocation::Update {
                day: Some(crate::cli::DayEdit::Set(
                    taskboard_domain::habit::AnchorDay::Weekday(chrono::Weekday::Mon)
                )),
                ..
            }
        ));
        assert!(matches!(
            parse(&["habits", "add", "water", "--monthly", "--day", "17"]),
            HabitsInvocation::Add {
                day: Some(crate::cli::DayEdit::Set(
                    taskboard_domain::habit::AnchorDay::MonthDay(_)
                )),
                ..
            }
        ));
        assert!(matches!(
            parse(&["habits", "update", "x", "--day", "none"]),
            HabitsInvocation::Update {
                day: Some(crate::cli::DayEdit::Clear),
                ..
            }
        ));
        // Day 0 and day 32 are parse errors (the mint's law surfaced
        // at the CLI boundary), not typed refusals downstream.
        for bad in ["0", "32"] {
            assert!(
                crate::cli::Cli::try_parse_from(vec![
                    "taskboard",
                    "habits",
                    "update",
                    "x",
                    "--day",
                    bad
                ])
                .is_err()
            );
        }
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
                description: None,
                day: None,
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
    fn restore_selector_hits_tombstones_by_uuid_then_unique_prefix() {
        let mut dead = habit(0x1234_5678_0000_0000_0000_0000_0000_0001, 2, 0, false, None);
        dead.deleted = true;
        let id = dead.id;
        let mut state = AppState::default();
        state.habits.habits.insert(dead.id, dead);
        // The tombstone is the restore surface's addressable row.
        assert_eq!(
            resolve_restorable_habit(&state, &id.as_uuid().to_string()).expect("exact uuid"),
            id
        );
        assert_eq!(
            resolve_restorable_habit(&state, &id.as_uuid().to_string()[..8])
                .expect("unique prefix"),
            id
        );
        // No title leg: tombstoned rows may share titles (journey D-2).
        assert!(matches!(
            resolve_restorable_habit(&state, "never-title"),
            Err(CommandError::UnknownHabit { .. })
        ));
        // An unknown full uuid is the typed rejection, never a miss.
        let nil = uuid::Uuid::from_u128(0x999).to_string();
        assert!(matches!(
            resolve_restorable_habit(&state, &nil),
            Err(CommandError::UnknownHabit { selector }) if selector == nil
        ));
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
    fn the_day_kind_gate_refuses_mismatches_and_accepts_matches_and_absent_flags() {
        use taskboard_domain::habit::{AnchorDay, MonthDay};
        let weekly = HabitTarget::PerWeek(NonZeroCount::new(1).unwrap());
        let monthly = HabitTarget::PerMonth(NonZeroCount::new(1).unwrap());
        let daily = HabitTarget::PerDay(NonZeroCount::new(1).unwrap());
        let weekday = DayEdit::Set(AnchorDay::Weekday(chrono::Weekday::Wed));
        let month_day = DayEdit::Set(AnchorDay::MonthDay(MonthDay::new(17).unwrap()));
        // Matched pairings pass.
        assert!(validate_day_against_target(Some(weekday), weekly).is_ok());
        assert!(validate_day_against_target(Some(month_day), monthly).is_ok());
        // Mismatched pairings are the typed refusal.
        let err = validate_day_against_target(Some(weekday), monthly).unwrap_err();
        assert!(matches!(err, CommandError::HabitDayKindMismatch));
        assert_eq!(err.error_kind(), "habit-day-kind-mismatch");
        assert!(matches!(
            validate_day_against_target(Some(month_day), weekly).unwrap_err(),
            CommandError::HabitDayKindMismatch
        ));
        // Any day on a daily habit refuses (a day habit has no inner day).
        assert!(matches!(
            validate_day_against_target(Some(weekday), daily).unwrap_err(),
            CommandError::HabitDayKindMismatch
        ));
        // Absent or clearing day passes for every cadence (the planner's
        // inertness is the answer when the cadence is unknown here).
        assert!(validate_day_against_target(None, weekly).is_ok());
        assert!(validate_day_against_target(Some(DayEdit::Clear), monthly).is_ok());
    }

    #[test]
    fn the_list_row_carries_the_description_field() {
        let board = taskboard_domain::BoardId::from(uuid::Uuid::from_u128(2));
        let mut state = AppState::default();
        let mut quiet = habit(1, 2, 0, false, None);
        let mut noted = habit(2, 2, 1, false, None);
        quiet.description = String::new();
        noted.description = "long runs".into();
        state.habits.habits.insert(quiet.id, quiet.clone());
        state.habits.habits.insert(noted.id, noted.clone());
        let rows = habit_rows(&state, board, ts(1_700_000_000), day());
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].description, "", "the empty description rides empty");
        assert_eq!(rows[1].description, "long runs");
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
            by: None,

            skip: false,
        };
        let _ = HabitState::default();
    }
}

#[cfg(test)]
mod engine_seam_tests {
    use super::*;
    use crate::test_support::standalone_boot_fixture;
    use taskboard_domain::habit::AnchorDay;

    async fn minted_board(
        fixture: &crate::test_support::StandaloneFixture,
    ) -> taskboard_domain::BoardId {
        let CommandOutcome::CreatedBoard(id) = fixture
            .app
            .engine
            .execute(StateCommand::CreateBoard {
                title: "habits".to_owned(),
                color: taskboard_domain::Color::new("0082c9"),
            })
            .await
            .expect("board mints")
        else {
            panic!("CreateBoard always mints");
        };
        id
    }

    fn daily() -> HabitTarget {
        HabitTarget::PerDay(NonZeroCount::new(1).expect("1 is nonzero"))
    }

    /// The plain create: the receipt reads the minted row back from the
    /// published state (never an input echo).
    #[tokio::test]
    async fn add_creates_and_reads_the_row_back() {
        let fixture = standalone_boot_fixture("pred-habit-add").await;
        let board = minted_board(&fixture).await;
        let receipt = add_on(
            &fixture.app,
            "water",
            daily(),
            Some(board),
            None,
            None,
            None,
        )
        .await
        .expect("habits add");
        let row = receipt.habit.expect("the minted row reads back");
        assert_eq!(row.title, "water");
        assert_eq!(row.target_phrase, "1/day");
        assert!(row.chip.is_none());
        let _ = fixture.app.shutdown().await;
    }

    /// The sparse immediate edit: `--at`/`--description`/`--day` land
    /// on the published row — each field-deletion mutant drops exactly
    /// one of these legs.
    #[tokio::test]
    async fn add_routes_the_sparse_edit_onto_the_minted_row() {
        let fixture = standalone_boot_fixture("pred-habit-sparse").await;
        let board = minted_board(&fixture).await;
        let at = chrono::NaiveTime::from_hms_opt(7, 30, 0).expect("07:30");
        let day = DayEdit::Set(AnchorDay::Weekday(chrono::Weekday::Mon));
        let receipt = add_on(
            &fixture.app,
            "run",
            daily(),
            Some(board),
            Some(at),
            Some("morning miles".to_owned()),
            Some(day),
        )
        .await
        .expect("habits add");
        let row = receipt.habit.expect("the edited row reads back");
        assert_eq!(row.chip, Some(at));
        let state = fixture.app.engine.shared_state().load_full();
        let stored = state
            .habits
            .habits
            .values()
            .find(|h| h.title == "run")
            .expect("the habit row");
        assert_eq!(stored.description, "morning miles");
        assert_eq!(
            stored.anchor_day,
            Some(taskboard_domain::habit::AnchorDay::Weekday(
                chrono::Weekday::Mon
            ))
        );
        let _ = fixture.app.shutdown().await;
    }

    /// The `changed` disjunct (plan §3 #13): a field-only edit flips it
    /// via `mask_hit`, a pure reorder flips it via `moved`, and a
    /// vacuous edit keeps it `false` — each leg of the `mask_hit ||
    /// moved` table.
    #[tokio::test]
    async fn update_changed_tracks_the_edit_and_move_disjuncts() {
        let fixture = standalone_boot_fixture("pred-habit-update").await;
        let board = minted_board(&fixture).await;
        let receipt = add_on(
            &fixture.app,
            "water",
            daily(),
            Some(board),
            None,
            None,
            None,
        )
        .await
        .expect("habits add");
        let row = receipt.habit.expect("minted");
        let id = taskboard_domain::HabitId::from(
            uuid::Uuid::parse_str(&row.habit_id).expect("receipt uuid"),
        );
        // Field-only edit ⇒ mask_hit.
        let mask = update_on(
            &fixture.app,
            "water",
            Some("water daily".to_owned()),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .expect("the update lands");
        assert!(mask.changed, "a field edit flips mask_hit");
        // Pure reorder ⇒ moved.
        let reorder = update_on(
            &fixture.app,
            "water daily",
            None,
            None,
            None,
            None,
            None,
            Some(3),
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .expect("the reorder lands");
        assert!(reorder.changed, "a reorder flips moved");
        // Vacuous (the identical title again) ⇒ neither disjunct.
        let vacuous = update_on(
            &fixture.app,
            "water daily",
            Some("water daily".to_owned()),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .expect("the vacuous edit is accepted");
        assert!(!vacuous.changed, "the identical edit is the typed no-op");
        let _ = id;
        let _ = fixture.app.shutdown().await;
    }

    /// The #1356 twin pin: on a densely ordered section, `--after
    /// <adjacent>` places the habit immediately after the neighbour
    /// deterministically — the adjacency asserted from the canonical
    /// `(order, id)` sequence, and a cross-board neighbour refuses
    /// typed (`MoveNeighbourMismatch`).
    #[tokio::test]
    async fn update_after_adjacent_is_deterministic_on_a_dense_section() {
        let fixture = standalone_boot_fixture("pred-habit-after").await;
        let board = minted_board(&fixture).await;
        for title in ["a", "b", "c"] {
            let _ = add_on(&fixture.app, title, daily(), Some(board), None, None, None)
                .await
                .expect("dense habit add");
        }
        let receipt = update_on(
            &fixture.app,
            "a",
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            Some("b".to_owned()),
            None,
            None,
            None,
        )
        .await
        .expect("the dense adjacent move");
        assert!(receipt.changed, "the reorder flips moved");
        let state = fixture.app.engine.shared_state().load_full();
        let mut rows: Vec<&taskboard_domain::habit::Habit> = state
            .habits
            .habits
            .values()
            .filter(|row| row.is_live() && row.board == board)
            .collect();
        rows.sort_by_key(|row| (row.order, row.id));
        let titles: Vec<&str> = rows.iter().map(|row| row.title.as_str()).collect();
        assert_eq!(titles, ["b", "a", "c"], "a lands directly after b");
        drop(state);

        // A neighbour on another board refuses typed.
        let other = {
            let CommandOutcome::CreatedBoard(id) = fixture
                .app
                .engine
                .execute(StateCommand::CreateBoard {
                    title: "other".to_owned(),
                    color: taskboard_domain::Color::new("0082c9"),
                })
                .await
                .expect("board mints")
            else {
                panic!("CreateBoard always mints");
            };
            id
        };
        let _ = add_on(&fixture.app, "x", daily(), Some(other), None, None, None)
            .await
            .expect("the other board's habit");
        let err = update_on(
            &fixture.app,
            "a",
            None,                 // title
            None,                 // target
            None,                 // at
            None,                 // description
            None,                 // day
            None,                 // order
            None,                 // before
            Some("x".to_owned()), // after
            None,                 // continued_from
            None,                 // days
            None,                 // reminder
        )
        .await
        .expect_err("the cross-board neighbour refuses");
        assert!(matches!(err, CommandError::MoveNeighbourMismatch { .. }));
        let _ = fixture.app.shutdown().await;
    }

    /// The tombstone receipt (plan §3 #14): the log survives and the
    /// receipt names the surviving row count; the repeat is the
    /// accepted non-deleted receipt.
    #[tokio::test]
    async fn rm_tombstones_and_the_log_survives() {
        let fixture = standalone_boot_fixture("pred-habit-rm").await;
        let board = minted_board(&fixture).await;
        let receipt = add_on(
            &fixture.app,
            "water",
            daily(),
            Some(board),
            None,
            None,
            None,
        )
        .await
        .expect("habits add");
        let habit_id = receipt.habit.expect("minted").habit_id.clone();
        let _ = log_on(&fixture.app, "water").await.expect("the log lands");
        let removed = rm_on(&fixture.app, "water").await.expect("the rm lands");
        assert!(removed.deleted);
        assert_eq!(removed.log_rows, 1, "the log row survives the tombstone");
        // The repeat is the typed unknown: rm's resolver is live-only
        // (a tombstone never resolves, by uuid or title — the restore
        // family owns tombstone addressing), so the NoOp receipt arm
        // inside `rm_on` is unreachable from the seam (a recorded
        // survivor per plan §4 D4, not a kill target).
        let err = rm_on(&fixture.app, &habit_id)
            .await
            .expect_err("the tombstone no longer resolves");
        assert!(matches!(err, CommandError::UnknownHabit { .. }));
        let _ = fixture.app.shutdown().await;
    }
}
