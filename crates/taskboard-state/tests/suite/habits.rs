// SPDX-License-Identifier: MIT OR Apache-2.0
// Environment gate, not a weakened assertion: this suite drives the live
// tokio engine (channels, scheduling), which testing_strategy §10 scopes
// out of Miri. The transition logic it routes to is Miri-covered in the
// domain suites and in `core.rs`.
#![cfg_attr(miri, allow(dead_code))]
//! Engine tier for the habit verbs (DR-6/5, the tracking.rs template):
//! create → log → adjust → delete journeys over the live engine with
//! published-snapshot assertions (memory ≡ disk), the log outliving the
//! tombstone, and the crash-resume leg — a fresh engine over the same
//! repository sees the log rows (persisted intent). `FixedClock` advances
//! drive all stamps; oneshot replies are the completion signals; no
//! sleeps, no wall-clock assumptions. The engine routes habit commands
//! generically (no habit-specific actor code exists to test) and its
//! UTC-midnight test calendar makes T0's window-date 1970-01-01.

use std::sync::Arc;
use std::time::Duration;

use taskboard_domain::test_support::{CountingIds, InMemoryRepository};
use taskboard_domain::{
    CommandOutcome, HabitLogKey, NotifierMessage, StateCommand, SyncCommand, SystemEvent,
    WindowStart,
};
use taskboard_state::{EngineHandle, ExecuteError, spawn_state_engine};

use crate::common;

use crate::common::{FixedClock, T0, seed_board_state};

struct Harness {
    handle: EngineHandle,
    repo: Arc<InMemoryRepository>,
    system_tx: tokio::sync::broadcast::Sender<SystemEvent>,
    join: tokio::task::JoinHandle<()>,
}

async fn spawn_over(repo: Arc<InMemoryRepository>, clock: Arc<FixedClock>) -> Harness {
    let (sync_out, _sync_in) = tokio::sync::mpsc::channel::<SyncCommand>(8);
    let (_report_tx, report_rx) = tokio::sync::mpsc::channel::<taskboard_domain::SyncReport>(8);
    let (system_tx, system_rx) = tokio::sync::broadcast::channel::<SystemEvent>(8);
    let (notifier, _notifications) = tokio::sync::mpsc::channel::<NotifierMessage>(64);
    let (handle, join) = spawn_state_engine(
        repo.clone(),
        Arc::new(CountingIds::new()),
        clock.clone(),
        sync_out,
        report_rx,
        system_tx.clone(),
        system_rx,
        notifier,
        taskboard_domain::SettingsSnapshot::default(),
        taskboard_state::DayRollConfig::disabled(),
        None,
    )
    .await
    .expect("engine boots");
    Harness {
        handle,
        repo,
        system_tx,
        join,
    }
}

impl Harness {
    fn board_id(&self) -> taskboard_domain::BoardId {
        *self
            .handle
            .shared_state()
            .load()
            .boards
            .keys()
            .next()
            .expect("the fixture has exactly one board")
    }

    /// The published habits section (cheap deep clone per read).
    fn published(&self) -> taskboard_domain::HabitState {
        self.handle.shared_state().load().habits.clone()
    }

    /// Graceful shutdown: the drain starts on `Shutdown`, the join
    /// completes within a paused-time deadline (the engine.rs idiom).
    async fn shutdown(self) {
        self.system_tx
            .send(SystemEvent::Shutdown)
            .expect("broadcast");
        tokio::time::timeout(Duration::from_secs(60), self.join)
            .await
            .expect("engine exits after drain")
            .expect("clean exit");
    }
}

/// T0's canonical window under the engine's UTC-midnight calendar.
fn window(day_offset: i64) -> WindowStart {
    WindowStart::of_period(
        taskboard_domain::PeriodKind::Day,
        chrono::NaiveDate::from_ymd_opt(1970, 1, 1).expect("epoch date")
            + chrono::Days::new(u64::try_from(day_offset).expect("non-negative")),
    )
}

fn key(habit: taskboard_domain::HabitId, day_offset: i64) -> HabitLogKey {
    HabitLogKey {
        habit,
        window: window(day_offset),
    }
}

/// H1: the full journey — create mints and publishes, each log is one
/// increment the receipt reports, adjust rewrites the window audited,
/// delete tombstones while the log survives (published *and* on disk),
/// and the repeat delete is the accepted no-op.
#[tokio::test]
#[allow(clippy::too_many_lines)] // one journey; the leg order IS the assertion

async fn h1_habit_journey_create_log_adjust_delete() {
    let repo = Arc::new(InMemoryRepository::with_state(seed_board_state("kiosk")));
    let clock = Arc::new(FixedClock::from_secs(T0));
    let h = spawn_over(repo, clock.clone()).await;

    // Create.
    let board = h.board_id();
    let CommandOutcome::HabitCreated(habit) = h
        .handle
        .execute(StateCommand::CreateHabit {
            board,
            title: "meditate".into(),
            target: taskboard_domain::HabitTarget::PerDay(
                taskboard_domain::NonZeroCount::new(3).unwrap(),
            ),
        })
        .await
        .expect("accepted")
    else {
        panic!("wrong receipt");
    };
    let published = h.published();
    let row = &published.habits[&habit];
    assert_eq!(row.title, "meditate");
    assert_eq!(row.order, 0, "append on an empty board");
    assert_eq!(row.clocks.title, common::ts(T0));
    assert_eq!(row.continued_from, None, "create never mints a link");

    // Log twice, one clock advance apart: counts 1 then 2.
    let CommandOutcome::HabitLogged {
        window: w1,
        count: 1,
        ..
    } = h
        .handle
        .execute(StateCommand::LogHabit { habit })
        .await
        .expect("accepted")
    else {
        panic!("wrong receipt");
    };
    assert_eq!(w1, window(0), "T0's window-date under UTC midnight");
    clock.advance_secs(60);
    let CommandOutcome::HabitLogged { count: count2, .. } = h
        .handle
        .execute(StateCommand::LogHabit { habit })
        .await
        .expect("accepted")
    else {
        panic!("wrong receipt");
    };
    assert_eq!(count2, 2);
    let published = h.published();
    assert_eq!(published.log[&key(habit, 0)].count, 2);
    // Memory ≡ disk at the receipt.
    let persisted = h.repo.snapshot();
    assert_eq!(persisted.habits, published, "memory ≡ disk: habits");

    // Adjust: the audited correction rewrites the window.
    clock.advance_secs(60);
    let CommandOutcome::HabitLogAdjusted {
        window: w3,
        count: 5,
        ..
    } = h
        .handle
        .execute(StateCommand::AdjustHabitLog {
            habit,
            window: window(0),
            count: 5,
        })
        .await
        .expect("accepted")
    else {
        panic!("wrong receipt");
    };
    assert_eq!(w3, window(0));
    let published = h.published();
    assert_eq!(published.log[&key(habit, 0)].count, 5);
    assert_eq!(
        published.log[&key(habit, 0)].clock,
        common::ts(T0 + 120),
        "the correction's fresh stamp"
    );
    assert_eq!(h.repo.snapshot().habits, published);

    // Delete: the tombstone lands, the log survives everywhere.
    clock.advance_secs(60);
    let outcome = h
        .handle
        .execute(StateCommand::DeleteHabit { habit })
        .await
        .expect("accepted");
    assert_eq!(outcome, CommandOutcome::Applied);
    let published = h.published();
    assert!(published.habits[&habit].deleted, "tombstoned");
    assert_eq!(
        published.habits[&habit].clocks.deleted,
        common::ts(T0 + 180)
    );
    assert_eq!(
        published.log[&key(habit, 0)].count,
        5,
        "history outlives the tracker"
    );
    let persisted = h.repo.snapshot();
    assert_eq!(persisted.habits, published, "memory ≡ disk: habits");
    assert!(
        persisted.habits.log.contains_key(&key(habit, 0)),
        "the log row is durable"
    );

    // The repeat is the accepted no-op; nothing persisted.
    let before = h.repo.snapshot();
    let repeat = h.handle.execute(StateCommand::DeleteHabit { habit }).await;
    match repeat {
        Ok(CommandOutcome::NoOp) => {}
        other => panic!("expected NoOp, got {other:?}"),
    }
    assert_eq!(h.repo.snapshot(), before, "the no-op persisted nothing");
    h.shutdown().await;
}

/// H2: the crash-resume leg — engine 1 creates, logs, and adjusts, then
/// shuts down; engine 2 boots over the same repository and hydrates the
/// habit and its log rows intact (the log rows are the intent).
#[tokio::test]
async fn h2_fresh_engine_over_the_same_repo_sees_the_log() {
    let repo = Arc::new(InMemoryRepository::with_state(seed_board_state("kiosk")));
    let clock = Arc::new(FixedClock::from_secs(T0));
    let engine1 = spawn_over(repo.clone(), clock.clone()).await;
    let board = engine1.board_id();
    let CommandOutcome::HabitCreated(habit) = engine1
        .handle
        .execute(StateCommand::CreateHabit {
            board,
            title: "stretch".into(),
            target: taskboard_domain::HabitTarget::PerWeek(
                taskboard_domain::NonZeroCount::new(2).unwrap(),
            ),
        })
        .await
        .expect("accepted")
    else {
        panic!("wrong receipt");
    };
    let CommandOutcome::HabitLogged {
        window, count: 1, ..
    } = engine1
        .handle
        .execute(StateCommand::LogHabit { habit })
        .await
        .expect("accepted")
    else {
        panic!("wrong receipt");
    };
    assert_eq!(
        window.as_date(),
        chrono::NaiveDate::from_ymd_opt(1969, 12, 29).expect("that week's Monday"),
        "a weekly habit counts in its ISO week's window"
    );
    engine1.shutdown().await;
    let logged_window = window;

    // A "fresh process": a new engine over the same repository.
    clock.advance_secs(3_600);
    let engine2 = spawn_over(repo.clone(), clock.clone()).await;
    let published = engine2.published();
    assert_eq!(published.habits.len(), 1, "the habit hydrated");
    assert_eq!(published.habits[&habit].title, "stretch");
    let resumed_key = HabitLogKey {
        habit,
        window: logged_window,
    };
    assert_eq!(published.log[&resumed_key].count, 1, "the log row intact");
    // And commandable across the restart: the next increment continues
    // the same window's count.
    let CommandOutcome::HabitLogged { count: 2, .. } = engine2
        .handle
        .execute(StateCommand::LogHabit { habit })
        .await
        .expect("accepted")
    else {
        panic!("wrong receipt");
    };
    assert_eq!(engine2.repo.snapshot().habits.log[&resumed_key].count, 2);
    engine2.shutdown().await;
}

/// H3: rejections route through the engine typed — an unknown habit id
/// is a typed error, nothing is persisted.
#[tokio::test]
async fn h3_unknown_habit_rejects_typed_and_persists_nothing() {
    let repo = Arc::new(InMemoryRepository::with_state(seed_board_state("kiosk")));
    let clock = Arc::new(FixedClock::from_secs(T0));
    let h = spawn_over(repo.clone(), clock.clone()).await;
    let stranger = taskboard_domain::HabitId::from(uuid::Uuid::from_u128(654_321));
    let before = h.repo.snapshot();
    let outcome = h
        .handle
        .execute(StateCommand::LogHabit { habit: stranger })
        .await;
    match outcome {
        Err(taskboard_state::ExecuteError::Rejected(
            taskboard_domain::CommandError::UnknownHabit(got),
        )) => assert_eq!(got, stranger),
        other => panic!("expected typed rejection, got {other:?}"),
    }
    assert_eq!(h.repo.snapshot(), before, "nothing persisted");
    h.shutdown().await;
}

/// US-6/S1 (J1, chunk 4): the `UpdateHabit` seam round-trips both
/// additive fields through the live engine — the description and the
/// anchor day store, publish, and persist; the canonical window stays
/// ISO-Monday (a weekly habit's period start is untouched by the day),
/// and a kind-mismatched variant is accepted (the domain's documented
/// inertness — no planner policing).
#[tokio::test]
async fn us6_s1_update_habit_stores_the_day_inside_the_canonical_window() {
    let repo = Arc::new(InMemoryRepository::with_state(seed_board_state("kiosk")));
    let clock = Arc::new(FixedClock::from_secs(T0));
    let h = spawn_over(repo.clone(), clock.clone()).await;
    let board = h.board_id();
    let CommandOutcome::HabitCreated(habit) = h
        .handle
        .execute(StateCommand::CreateHabit {
            board,
            title: "run".into(),
            target: taskboard_domain::HabitTarget::PerWeek(
                taskboard_domain::NonZeroCount::new(2).unwrap(),
            ),
        })
        .await
        .expect("create")
    else {
        panic!("create names the habit");
    };
    let wednesday = taskboard_domain::habit::AnchorDay::Weekday(chrono::Weekday::Wed);
    match h
        .handle
        .execute(StateCommand::UpdateHabit {
            habit,
            changes: taskboard_domain::HabitChanges {
                description: Some("weekly long run".into()),
                anchor_day: Some(Some(wednesday)),
                ..taskboard_domain::HabitChanges::default()
            },
        })
        .await
        .expect("update")
    {
        CommandOutcome::Applied => {}
        other => panic!("the edit applies, got {other:?}"),
    }
    let stored = h.published().habits[&habit].clone();
    assert_eq!(stored.description, "weekly long run");
    assert_eq!(stored.anchor_day, Some(wednesday));

    // The window stays canonical: the weekly period start of any date
    // is its Monday, before and after the day was stored.
    let mid_week = chrono::NaiveDate::from_ymd_opt(2026, 10, 28).unwrap(); // a Wednesday
    assert_eq!(
        taskboard_domain::habit::period_start(taskboard_domain::PeriodKind::Week, mid_week),
        chrono::NaiveDate::from_ymd_opt(2026, 10, 26).unwrap()
    );
    // The nudge day IS that Wednesday inside the Monday window (the
    // constructive law the reminder engine consumes — chunk 10 rides).
    assert_eq!(
        taskboard_domain::habit::nudge_day(
            &stored,
            WindowStart::of_period(taskboard_domain::PeriodKind::Week, mid_week),
        ),
        mid_week
    );

    // A kind-mismatched variant (MonthDay on a weekly habit) is
    // accepted — inert at read time, never a planner refusal.
    let month_day = taskboard_domain::habit::AnchorDay::MonthDay(
        taskboard_domain::habit::MonthDay::new(17).expect("17"),
    );
    match h
        .handle
        .execute(StateCommand::UpdateHabit {
            habit,
            changes: taskboard_domain::HabitChanges {
                anchor_day: Some(Some(month_day)),
                ..taskboard_domain::HabitChanges::default()
            },
        })
        .await
        .expect("update")
    {
        CommandOutcome::Applied => {}
        other => panic!("the inert edit applies, got {other:?}"),
    }
    assert_eq!(h.published().habits[&habit].anchor_day, Some(month_day));

    // Persisted intent: a fresh engine over the same repository sees
    // both fields.
    let before = h.published();
    h.shutdown().await;
    let clock = Arc::new(FixedClock::from_secs(T0));
    let h2 = spawn_over(repo, clock).await;
    assert_eq!(h2.published(), before, "memory ≡ disk across restarts");
    h2.shutdown().await;
}

/// US-5 AC-1 (J1, H4 D5): the editor's changeset maps every field
/// through one sparse `UpdateHabit` — title, description set AND
/// clear, quota (a period-kind change carrying the now-mismatched day
/// inert), `anchor_time` set AND clear — while every untouched field's
/// LWW clock stays byte-equal (the sparse discipline: `None` =
/// untouched, clocks preserved).
#[tokio::test]
#[allow(clippy::too_many_lines)] // the journey reads best as one scenario
async fn us5_editor_changeset_maps_each_field_and_preserves_untouched_clocks() {
    let repo = Arc::new(InMemoryRepository::with_state(seed_board_state("kiosk")));
    let clock = Arc::new(FixedClock::from_secs(T0));
    let h = spawn_over(repo.clone(), clock.clone()).await;
    let board = h.board_id();
    let CommandOutcome::HabitCreated(habit) = h
        .handle
        .execute(StateCommand::CreateHabit {
            board,
            title: "run".into(),
            target: taskboard_domain::HabitTarget::PerWeek(
                taskboard_domain::NonZeroCount::new(2).unwrap(),
            ),
        })
        .await
        .expect("create")
    else {
        panic!("create names the habit");
    };
    let before = h.published().habits[&habit].clone();

    // Every D5 row in one sparse changeset.
    let wednesday = taskboard_domain::habit::AnchorDay::Weekday(chrono::Weekday::Wed);
    clock.advance_secs(60);
    match h
        .handle
        .execute(StateCommand::UpdateHabit {
            habit,
            changes: taskboard_domain::HabitChanges {
                title: Some("long run".into()),
                description: Some("slow and far".into()),
                target: Some(taskboard_domain::HabitTarget::PerWeek(
                    taskboard_domain::NonZeroCount::new(3).unwrap(),
                )),
                anchor_time: Some(Some(chrono::NaiveTime::from_hms_opt(7, 30, 0).unwrap())),
                anchor_day: Some(Some(wednesday)),
                scheduled_days: Some(
                    taskboard_domain::habit::WeekdaySet::new([
                        chrono::Weekday::Mon,
                        chrono::Weekday::Wed,
                        chrono::Weekday::Fri,
                    ])
                    .expect("three days"),
                ),
                reminder: Some(Some(taskboard_domain::habit::HabitReminderOverride {
                    base: taskboard_domain::habit::NudgeBase::PeriodStart,
                    offset_days: 1,
                    time: Some(chrono::NaiveTime::from_hms_opt(21, 0, 0).unwrap()),
                    snooze: taskboard_domain::habit::HabitSnooze::Hours(3),
                })),
                continued_from: None,
            },
        })
        .await
        .expect("update")
    {
        CommandOutcome::Applied => {}
        other => panic!("the edit applies, got {other:?}"),
    }
    let after = h.published().habits[&habit].clone();
    assert_eq!(after.title, "long run");
    assert_eq!(after.description, "slow and far");
    assert_eq!(
        after.target,
        taskboard_domain::HabitTarget::PerWeek(taskboard_domain::NonZeroCount::new(3).unwrap())
    );
    assert_eq!(
        after.anchor_time,
        Some(chrono::NaiveTime::from_hms_opt(7, 30, 0).unwrap())
    );
    assert_eq!(after.anchor_day, Some(wednesday));
    assert_eq!(
        after.scheduled_days,
        Some(
            taskboard_domain::habit::WeekdaySet::new([
                chrono::Weekday::Mon,
                chrono::Weekday::Wed,
                chrono::Weekday::Fri,
            ])
            .expect("three days")
        )
    );
    assert!(after.reminder.is_some());
    // Every touched field's clock moved (LWW stamps ride the injected
    // clock), the untouched ones stay byte-equal.
    assert_ne!(after.clocks.title, before.clocks.title);
    assert_ne!(after.clocks.target, before.clocks.target);
    assert_ne!(after.clocks.description, before.clocks.description);
    assert_ne!(after.clocks.anchor_time, before.clocks.anchor_time);
    assert_ne!(after.clocks.anchor_day, before.clocks.anchor_day);
    assert_ne!(after.clocks.scheduled_days, before.clocks.scheduled_days);
    assert_ne!(after.clocks.reminder, before.clocks.reminder);
    // The sparse discipline's other half: nothing else churns.
    assert_eq!(
        after.clocks.order, before.clocks.order,
        "the order slot is reorder-only"
    );
    assert_eq!(after.clocks.deleted, before.clocks.deleted);
    assert_eq!(
        after.continued_from, before.continued_from,
        "the conversion seam is not editor data"
    );

    // `Some("")` clears the description; a kind change to PerDay
    // carries the now-mismatched day inert (no silent destructive
    // clearing).
    clock.advance_secs(60);
    match h
        .handle
        .execute(StateCommand::UpdateHabit {
            habit,
            changes: taskboard_domain::HabitChanges {
                description: Some(String::new()),
                target: Some(taskboard_domain::HabitTarget::PerDay(
                    taskboard_domain::NonZeroCount::new(1).unwrap(),
                )),
                ..taskboard_domain::HabitChanges::default()
            },
        })
        .await
        .expect("update")
    {
        CommandOutcome::Applied => {}
        other => panic!("the clear applies, got {other:?}"),
    }
    let cleared = h.published().habits[&habit].clone();
    assert_eq!(cleared.description, "");
    assert_eq!(
        cleared.target,
        taskboard_domain::HabitTarget::PerDay(taskboard_domain::NonZeroCount::new(1).unwrap())
    );
    assert_eq!(
        cleared.anchor_day,
        Some(wednesday),
        "the mismatched day is carried, never silently dropped"
    );

    // `Some(None)` clears the double-Option fields.
    clock.advance_secs(60);
    match h
        .handle
        .execute(StateCommand::UpdateHabit {
            habit,
            changes: taskboard_domain::HabitChanges {
                anchor_time: Some(None),
                anchor_day: Some(None),
                reminder: Some(None),
                ..taskboard_domain::HabitChanges::default()
            },
        })
        .await
        .expect("update")
    {
        CommandOutcome::Applied => {}
        other => panic!("the clear applies, got {other:?}"),
    }
    let cleared_both = h.published().habits[&habit].clone();
    assert_eq!(cleared_both.anchor_time, None);
    assert_eq!(cleared_both.anchor_day, None);
    assert_eq!(
        cleared_both.reminder, None,
        "the clear leg reverts the override"
    );

    // Persisted intent: a fresh engine over the same repository sees
    // the final shape.
    let expected = h.published();
    h.shutdown().await;
    let h2 = spawn_over(repo, Arc::new(FixedClock::from_secs(T0))).await;
    assert_eq!(h2.published(), expected, "memory ≡ disk across restarts");
    h2.shutdown().await;
}

// ---- HF1: skip + pause (#955, #958; plan 2026-11-21-habit-skip-pause) --

/// US-1/S2 (engine leg): `SkipHabit` routes the exact typed mark onto the
/// current canonical window's log row — count preserved, receipt names
/// the window — and the repeat is the accepted no-op (no clock churn).
/// `UnskipHabit` (the undo inverse) clears the mark and keeps the count.
#[tokio::test(start_paused = true)]
async fn skip_habit_routes_the_exact_command_and_the_repeat_no_ops() {
    let repo = Arc::new(InMemoryRepository::with_state(seed_board_state("kiosk")));
    let clock = Arc::new(FixedClock::from_secs(T0));
    let h = spawn_over(repo, clock.clone()).await;
    let board = h.board_id();
    let CommandOutcome::HabitCreated(habit) = h
        .handle
        .execute(StateCommand::CreateHabit {
            board,
            title: "meditate".into(),
            target: taskboard_domain::HabitTarget::PerDay(
                taskboard_domain::NonZeroCount::new(3).unwrap(),
            ),
        })
        .await
        .expect("accepted")
    else {
        panic!("wrong receipt");
    };
    // Log once so the skip must preserve the count.
    clock.advance_secs(60);
    let _ = h
        .handle
        .execute(StateCommand::LogHabit { habit })
        .await
        .expect("accepted");
    clock.advance_secs(60);
    let CommandOutcome::HabitSkipped {
        window: skipped, ..
    } = h
        .handle
        .execute(StateCommand::SkipHabit { habit })
        .await
        .expect("accepted")
    else {
        panic!("wrong receipt");
    };
    assert_eq!(skipped, window(0), "the current canonical window");
    let published = h.published();
    let row = &published.log[&key(habit, 0)];
    assert!(row.skip, "the typed mark landed");
    assert_eq!(row.count, 1, "the count is untouched by the mark");
    assert_eq!(h.repo.snapshot().habits, published, "memory ≡ disk");

    // The repeat is the accepted no-op; nothing persisted.
    clock.advance_secs(60);
    let before = h.repo.snapshot();
    let repeat = h.handle.execute(StateCommand::SkipHabit { habit }).await;
    match repeat {
        Ok(CommandOutcome::NoOp) => {}
        other => panic!("expected NoOp, got {other:?}"),
    }
    assert_eq!(h.repo.snapshot(), before, "the no-op persisted nothing");

    // The undo inverse clears the mark, keeps the count.
    clock.advance_secs(60);
    let _ = h
        .handle
        .execute(StateCommand::UnskipHabit { habit })
        .await
        .expect("accepted");
    let published = h.published();
    assert!(
        !published.log[&key(habit, 0)].skip,
        "the mark cleared (badge hygiene)"
    );
    assert_eq!(published.log[&key(habit, 0)].count, 1);
    // Unskipping an unmarked window is the accepted no-op.
    clock.advance_secs(60);
    let before = h.repo.snapshot();
    let repeat = h.handle.execute(StateCommand::UnskipHabit { habit }).await;
    match repeat {
        Ok(CommandOutcome::NoOp) => {}
        other => panic!("expected NoOp, got {other:?}"),
    }
    assert_eq!(h.repo.snapshot(), before);
    h.shutdown().await;
}

/// US-2/AC-4 + AC-5 (engine leg): a past through-date is the typed
/// `InvalidHabitPause`; a valid pause lands on the row (badge + through
/// date); the unchanged value no-ops; the explicit `None` resume clears
/// for badge hygiene (the `PauseSeries` posture verbatim).
#[tokio::test(start_paused = true)]
async fn pause_round_trips_resume_clears_and_past_date_is_typed_invalid() {
    let repo = Arc::new(InMemoryRepository::with_state(seed_board_state("kiosk")));
    let clock = Arc::new(FixedClock::from_secs(T0));
    let h = spawn_over(repo, clock.clone()).await;
    let board = h.board_id();
    let CommandOutcome::HabitCreated(habit) = h
        .handle
        .execute(StateCommand::CreateHabit {
            board,
            title: "stretch".into(),
            target: taskboard_domain::HabitTarget::PerDay(
                taskboard_domain::NonZeroCount::new(1).unwrap(),
            ),
        })
        .await
        .expect("accepted")
    else {
        panic!("wrong receipt");
    };
    // AC-5: a past-date through-date is the typed rejection.
    let past = chrono::NaiveDate::from_ymd_opt(1969, 12, 31).expect("valid");
    let err = h
        .handle
        .execute(StateCommand::PauseHabit {
            habit,
            until: Some(past),
        })
        .await
        .expect_err("a past pause is rejected");
    assert!(
        matches!(
            err,
            ExecuteError::Rejected(taskboard_domain::CommandError::InvalidHabitPause { .. })
        ),
        "typed invalid-pause: {err:?}"
    );
    // AC-1: a valid pause lands.
    let today = chrono::NaiveDate::from_ymd_opt(1970, 1, 1).expect("valid");
    let outcome = h
        .handle
        .execute(StateCommand::PauseHabit {
            habit,
            until: Some(today),
        })
        .await
        .expect("accepted");
    assert_eq!(outcome, CommandOutcome::Applied);
    let published = h.published();
    assert_eq!(published.habits[&habit].paused_until, Some(today));
    assert_eq!(
        published.habits[&habit].clocks.paused_until,
        common::ts(T0),
        "the pause write clock stamped"
    );
    assert_eq!(h.repo.snapshot().habits, published, "memory ≡ disk");
    // The unchanged value is the accepted no-op (no clock churn).
    clock.advance_secs(60);
    let before = h.repo.snapshot();
    let repeat = h
        .handle
        .execute(StateCommand::PauseHabit {
            habit,
            until: Some(today),
        })
        .await;
    match repeat {
        Ok(CommandOutcome::NoOp) => {}
        other => panic!("expected NoOp, got {other:?}"),
    }
    assert_eq!(h.repo.snapshot(), before);
    // AC-4: the explicit None resume clears for badge hygiene.
    clock.advance_secs(60);
    let _ = h
        .handle
        .execute(StateCommand::PauseHabit { habit, until: None })
        .await
        .expect("accepted");
    let published = h.published();
    assert_eq!(published.habits[&habit].paused_until, None);
    assert_eq!(h.repo.snapshot().habits, published);
    // Resuming an unpaused habit is the accepted no-op.
    clock.advance_secs(60);
    let before = h.repo.snapshot();
    let repeat = h
        .handle
        .execute(StateCommand::PauseHabit { habit, until: None })
        .await;
    match repeat {
        Ok(CommandOutcome::NoOp) => {}
        other => panic!("expected NoOp, got {other:?}"),
    }
    assert_eq!(h.repo.snapshot(), before);
}

// ---- HF4 (#960, #959): schedule + override routes over the live engine --

/// US-5/S2 (`schedule_save_routes_the_exact_update_habit`): the
/// editor's schedule save routes the exact sparse `UpdateHabit {
/// scheduled_days: Some(set) }` — the set stores verbatim, its clock
/// stamps, and the shape persists across a restart (memory ≡ disk).
#[tokio::test]
async fn us5_s2_schedule_save_routes_the_exact_update_habit() {
    let repo = Arc::new(InMemoryRepository::with_state(seed_board_state("kiosk")));
    let clock = Arc::new(FixedClock::from_secs(T0));
    let h = spawn_over(repo.clone(), clock.clone()).await;
    let board = h.board_id();
    let CommandOutcome::HabitCreated(habit) = h
        .handle
        .execute(StateCommand::CreateHabit {
            board,
            title: "run".into(),
            target: taskboard_domain::HabitTarget::PerWeek(
                taskboard_domain::NonZeroCount::new(2).unwrap(),
            ),
        })
        .await
        .expect("create")
    else {
        panic!("create names the habit");
    };
    clock.advance_secs(60);
    let set = taskboard_domain::habit::WeekdaySet::new([
        chrono::Weekday::Mon,
        chrono::Weekday::Wed,
        chrono::Weekday::Fri,
    ])
    .expect("three days");
    match h
        .handle
        .execute(StateCommand::UpdateHabit {
            habit,
            changes: taskboard_domain::HabitChanges {
                scheduled_days: Some(set),
                ..taskboard_domain::HabitChanges::default()
            },
        })
        .await
        .expect("update")
    {
        CommandOutcome::Applied => {}
        other => panic!("the schedule save applies, got {other:?}"),
    }
    let stored = h.published().habits[&habit].clone();
    assert_eq!(stored.scheduled_days, Some(set));
    assert_ne!(
        stored.clocks.scheduled_days, stored.clocks.title,
        "the set stamps its own clock slot"
    );
    // Persisted intent.
    let expected = h.published();
    h.shutdown().await;
    let h2 = spawn_over(repo, Arc::new(FixedClock::from_secs(T0))).await;
    assert_eq!(h2.published(), expected, "memory ≡ disk across restarts");
    h2.shutdown().await;
}

/// US-6/S2 + US-6/S3 (`override_save_routes_the_exact_command`,
/// `clear_leg_reverts_to_the_o1_default`): the override save routes
/// the exact double-Option set and the clear leg (`Some(None)`)
/// reverts the habit to the plain O-1 default — the field goes back
/// to `None`, byte-identical to a habit that never carried one.
#[tokio::test]
async fn us6_s2_override_save_routes_the_exact_command_and_the_clear_leg_reverts() {
    let repo = Arc::new(InMemoryRepository::with_state(seed_board_state("kiosk")));
    let clock = Arc::new(FixedClock::from_secs(T0));
    let h = spawn_over(repo.clone(), clock.clone()).await;
    let board = h.board_id();
    let CommandOutcome::HabitCreated(habit) = h
        .handle
        .execute(StateCommand::CreateHabit {
            board,
            title: "stretch".into(),
            target: taskboard_domain::HabitTarget::PerMonth(
                taskboard_domain::NonZeroCount::new(4).unwrap(),
            ),
        })
        .await
        .expect("create")
    else {
        panic!("create names the habit");
    };
    let ovr = taskboard_domain::habit::HabitReminderOverride {
        base: taskboard_domain::habit::NudgeBase::PeriodStart,
        offset_days: 2,
        time: Some(chrono::NaiveTime::from_hms_opt(7, 45, 0).unwrap()),
        snooze: taskboard_domain::habit::HabitSnooze::Days(1),
    };
    clock.advance_secs(60);
    match h
        .handle
        .execute(StateCommand::UpdateHabit {
            habit,
            changes: taskboard_domain::HabitChanges {
                reminder: Some(Some(ovr)),
                ..taskboard_domain::HabitChanges::default()
            },
        })
        .await
        .expect("update")
    {
        CommandOutcome::Applied => {}
        other => panic!("the override save applies, got {other:?}"),
    }
    assert_eq!(h.published().habits[&habit].reminder, Some(ovr));
    clock.advance_secs(60);
    match h
        .handle
        .execute(StateCommand::UpdateHabit {
            habit,
            changes: taskboard_domain::HabitChanges {
                reminder: Some(None),
                ..taskboard_domain::HabitChanges::default()
            },
        })
        .await
        .expect("update")
    {
        CommandOutcome::Applied => {}
        other => panic!("the clear leg applies, got {other:?}"),
    }
    let reverted = h.published().habits[&habit].clone();
    assert_eq!(reverted.reminder, None, "clear reverts to the O-1 default");
    // A zero-leg repetition stays the typed no-op (no clock churn).
    let before = h.published().habits[&habit].clone();
    clock.advance_secs(60);
    match h
        .handle
        .execute(StateCommand::UpdateHabit {
            habit,
            changes: taskboard_domain::HabitChanges {
                reminder: Some(None),
                ..taskboard_domain::HabitChanges::default()
            },
        })
        .await
        .expect("update")
    {
        CommandOutcome::NoOp => {}
        other => panic!("an unchanged clear is the no-op, got {other:?}"),
    }
    assert_eq!(h.published().habits[&habit].clocks, before.clocks);
    h.shutdown().await;
}

/// HF4 R-6 (`scheduled_days_on_a_daily_habit_is_a_typed_rejection`):
/// the write path refuses the field on a `PerDay` target with the typed
/// rejection — before any plan exists, nothing persists.
#[tokio::test]
async fn us5_scheduled_days_on_a_daily_habit_is_a_typed_rejection() {
    let repo = Arc::new(InMemoryRepository::with_state(seed_board_state("kiosk")));
    let clock = Arc::new(FixedClock::from_secs(T0));
    let h = spawn_over(repo, clock.clone()).await;
    let board = h.board_id();
    let CommandOutcome::HabitCreated(habit) = h
        .handle
        .execute(StateCommand::CreateHabit {
            board,
            title: "daily walk".into(),
            target: taskboard_domain::HabitTarget::PerDay(
                taskboard_domain::NonZeroCount::new(1).unwrap(),
            ),
        })
        .await
        .expect("create")
    else {
        panic!("create names the habit");
    };
    clock.advance_secs(60);
    let before = h.published();
    let outcome = h
        .handle
        .execute(StateCommand::UpdateHabit {
            habit,
            changes: taskboard_domain::HabitChanges {
                scheduled_days: Some(taskboard_domain::habit::WeekdaySet::all()),
                ..taskboard_domain::HabitChanges::default()
            },
        })
        .await;
    match outcome {
        Err(taskboard_state::ExecuteError::Rejected(
            taskboard_domain::CommandError::ScheduledDaysOnDailyHabit(got),
        )) => assert_eq!(got, habit),
        other => panic!("expected the typed R-6 refusal, got {other:?}"),
    }
    assert_eq!(
        h.published(),
        before,
        "the refusal persists nothing (no clock churn, no row)"
    );
    h.shutdown().await;
}
