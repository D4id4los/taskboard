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
use taskboard_state::{EngineHandle, ReminderEngineConfig, spawn_state_engine};

mod common;

use common::{FixedClock, T0, seed_board_state};

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
        ReminderEngineConfig::disabled(),
        taskboard_state::DayRollConfig::disabled(),
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
    let clock = FixedClock::new(T0);
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
    clock.advance(60);
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
    clock.advance(60);
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
    clock.advance(60);
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
    let clock = FixedClock::new(T0);
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
    clock.advance(3_600);
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
    let clock = FixedClock::new(T0);
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
