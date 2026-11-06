// SPDX-License-Identifier: MIT OR Apache-2.0
//! The live-settings journeys (GUI review round 5, chunk 5): the engine
//! applies `UpdateSettings` wholesale, publishes and announces it, and
//! every tick-arm read — cadence, enabled flag, implicit-reminder
//! defaults, the day calendar — is a live read, never a spawn capture.
//! Paused tokio time throughout; the injected clock and tokio time move
//! in lockstep (the reminder suite's primitive).

#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use taskboard_domain::idgen::IdGenerator as _;
use taskboard_domain::persistence::{PersistedState, TaskRepository};
use taskboard_domain::test_support::{CountingIds, InMemoryRepository};
use taskboard_domain::{
    AppState, Board, CommandOutcome, EngineSignal, NotifierMessage, ReminderOffset,
    SettingsSnapshot, Stack, StackClocks, StateCommand, SyncCommand, SyncReport, SyncStateReader,
    SystemEvent, Task, TaskClocks, TickSecs,
};
use taskboard_state::{DayRollConfig, EngineHandle, spawn_state_engine};

mod common;

use common::{CountingRepository, FixedClock, T0, ts};

// -- fixtures ------------------------------------------------------------

fn board() -> Board {
    let ids = CountingIds::new();
    Board {
        id: ids.new_board_id(),
        remote: None,
        title: "board".into(),
        color: taskboard_domain::Color::new("0000ff"),
        archived: false,
        deleted: false,
        clocks: taskboard_domain::BoardClocks {
            title: taskboard_domain::BoardClocks::epoch(),
            color: taskboard_domain::BoardClocks::epoch(),
            archived: taskboard_domain::BoardClocks::epoch(),
            deleted: taskboard_domain::BoardClocks::epoch(),
        },
        remote_seen: None,
    }
}

fn task(id: u128, stack: taskboard_domain::StackId, duedate: Option<i64>) -> Task {
    Task {
        id: taskboard_domain::TaskId::from_uuid(uuid::Uuid::from_u128(id)),
        remote: None,
        title: "task".into(),
        description: String::new(),
        duedate: duedate.map(ts),
        scheduled: None,
        done: None,
        stack,
        order: 1,
        label_memberships: BTreeMap::new(),
        effort: None,
        importance: None,
        urgency: None,
        recur: None,
        kind: taskboard_domain::entities::TaskKind::Standard,
        archived: false,
        deleted: false,
        clocks: TaskClocks {
            title: ts(T0),
            description: ts(T0),
            duedate: ts(T0),
            scheduled: ts(T0),
            done: ts(T0),
            position: ts(T0),
            effort: ts(T0),
            importance: ts(T0),
            urgency: ts(T0),
            recur: ts(T0),
            kind: ts(T0),
            archived: ts(T0),
            deleted: ts(T0),
        },
        remote_seen: None,
        remote_baseline: None,
    }
}

/// A live board + stack + one open task due at `duedate_secs`.
fn seeded(duedate_secs: Option<i64>) -> PersistedState {
    let board = board();
    let stack = Stack {
        id: taskboard_domain::StackId::from_uuid(uuid::Uuid::from_u128(500)),
        remote: None,
        board: board.id,
        title: "backlog".into(),
        order: 1,
        archived: false,
        deleted: false,
        clocks: StackClocks {
            title: ts(T0),
            order: ts(T0),
            deleted: ts(T0),
        },
        remote_seen: None,
    };
    let task = task(600, stack.id, duedate_secs);
    let mut state = PersistedState::default();
    state.boards.insert(board.id, board);
    state.stacks.insert(stack.id, stack);
    state.tasks.insert(task.id, task);
    state
}

fn enabled(tick_secs: u64) -> SettingsSnapshot {
    SettingsSnapshot {
        reminders_enabled: true,
        tick_secs: TickSecs::new(tick_secs).expect("nonzero"),
        reminder_before: ReminderOffset::at_due(),
        ..SettingsSnapshot::default()
    }
}

fn utc_roll() -> DayRollConfig {
    DayRollConfig {
        enabled: true,
        offset: chrono::FixedOffset::east_opt(0).expect("zero offset"),
    }
}

struct Harness {
    handle: EngineHandle,
    clock: Arc<FixedClock>,
    system_tx: tokio::sync::broadcast::Sender<SystemEvent>,
    system_rx: tokio::sync::broadcast::Receiver<SystemEvent>,
    notifications: tokio::sync::mpsc::Receiver<NotifierMessage>,
}

async fn spawn(
    repo: Arc<dyn TaskRepository>,
    clock: Arc<FixedClock>,
    settings: SettingsSnapshot,
    day_roll: DayRollConfig,
) -> Harness {
    let (sync_out, _sync_in) = tokio::sync::mpsc::channel::<SyncCommand>(8);
    let (_report_tx, report_rx) = tokio::sync::mpsc::channel::<SyncReport>(8);
    let (system_tx, boot_rx) = tokio::sync::broadcast::channel::<SystemEvent>(8);
    // Subscribed before the spawn: the announcement cannot be missed.
    let system_rx = system_tx.subscribe();
    let (notifier, notifications) = tokio::sync::mpsc::channel::<NotifierMessage>(64);
    let (handle, _join) = spawn_state_engine(
        repo,
        Arc::new(CountingIds::new()),
        clock.clone(),
        sync_out,
        report_rx,
        system_tx.clone(),
        boot_rx,
        notifier,
        settings,
        day_roll,
    )
    .await
    .expect("engine boots");
    Harness {
        handle,
        clock,
        system_tx,
        system_rx,
        notifications,
    }
}

/// The lockstep primitive: the injected clock (what the evaluator sees)
/// and tokio time (when the arm wakes) move together.
async fn advance_both(clock: &FixedClock, secs: i64) {
    clock.advance(secs);
    tokio::time::advance(Duration::from_secs(u64::try_from(secs).expect("positive"))).await;
}

/// A short settle: enough virtual turns for a due wake to run its arm.
async fn settle() {
    for _ in 0..5 {
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_millis(1)).await;
    }
}

async fn apply(handle: &EngineHandle, settings: SettingsSnapshot) {
    let outcome = handle
        .execute(StateCommand::UpdateSettings { settings })
        .await
        .expect("applied");
    assert_eq!(outcome, CommandOutcome::SettingsApplied);
}

fn published(handle: &EngineHandle) -> Arc<AppState> {
    handle.shared_state().load_full()
}

/// The firings observed so far (drains the notifier without waiting).
fn fired(notifications: &mut tokio::sync::mpsc::Receiver<NotifierMessage>) -> usize {
    let mut n = 0;
    while let Ok(message) = notifications.try_recv() {
        if matches!(message, NotifierMessage::Fire(_)) {
            n += 1;
        }
    }
    n
}

// -- the apply journey ---------------------------------------------------

#[tokio::test(start_paused = true)]
async fn the_boot_seed_is_published_with_the_hydrated_state() {
    let seed = SettingsSnapshot {
        day_start_time: chrono::NaiveTime::from_hms_opt(5, 0, 0).unwrap(),
        ..SettingsSnapshot::default()
    };
    let h = spawn(
        Arc::new(InMemoryRepository::with_state(seeded(None))),
        FixedClock::new(T0),
        seed.clone(),
        DayRollConfig::disabled(),
    )
    .await;
    assert_eq!(published(&h.handle).settings, seed);
    assert_eq!(h.handle.read_settings(), seed);
}

#[tokio::test(start_paused = true)]
async fn update_settings_replaces_the_snapshot_wholesale() {
    let h = spawn(
        Arc::new(InMemoryRepository::with_state(seeded(None))),
        FixedClock::new(T0),
        SettingsSnapshot::default(),
        DayRollConfig::disabled(),
    )
    .await;
    let first = SettingsSnapshot {
        due_soon_days: taskboard_domain::DueSoonDays::new(7).unwrap(),
        focus_chain: true,
        ..SettingsSnapshot::default()
    };
    apply(&h.handle, first.clone()).await;
    assert_eq!(published(&h.handle).settings, first);

    // A second snapshot replaces, never merges: the first's
    // `focus_chain` does not survive into the second.
    let second = SettingsSnapshot {
        poll_interval_secs: TickSecs::new(3).unwrap(),
        ..SettingsSnapshot::default()
    };
    apply(&h.handle, second.clone()).await;
    let now = published(&h.handle);
    assert_eq!(now.settings, second);
    assert!(!now.settings.focus_chain);
    // The sync actor's facet reads the same published value.
    assert_eq!(h.handle.read_settings(), second);
    // Memory-only: nothing entered the persisted shape and the
    // mutation stamp never moved.
    assert_eq!(now.last_updated, None);
}

#[tokio::test(start_paused = true)]
async fn update_settings_publishes_a_signal_and_rings_the_system_doorbell() {
    let mut h = spawn(
        Arc::new(InMemoryRepository::with_state(seeded(None))),
        FixedClock::new(T0),
        SettingsSnapshot::default(),
        DayRollConfig::disabled(),
    )
    .await;
    let mut signals = h.handle.subscribe();
    apply(&h.handle, enabled(4)).await;
    assert_eq!(
        signals.try_recv().expect("the repaint signal fired"),
        EngineSignal::StateUpdated
    );
    let event = tokio::time::timeout(Duration::from_millis(200), h.system_rx.recv())
        .await
        .expect("an event arrived")
        .expect("channel open");
    assert_eq!(event, SystemEvent::SettingsChanged);
    let _keepalive = h.system_tx;
}

// -- the tick arm's live reads ---------------------------------------------

#[tokio::test(start_paused = true)]
async fn a_tick_cadence_change_re_arms_at_the_next_wake() {
    // Reminders enabled at a 60 s cadence; nothing due at boot.
    let repo = Arc::new(InMemoryRepository::with_state(seeded(None)));
    let mut h = spawn(
        repo,
        FixedClock::new(T0),
        enabled(60),
        DayRollConfig::disabled(),
    )
    .await;
    settle().await;
    assert_eq!(fired(&mut h.notifications), 0);

    // A past-due task created mid-cadence: its implicit reminder is
    // due but the next wake is ~60 s away.
    let stack = *published(&h.handle).stacks.keys().next().expect("stack");
    h.handle
        .execute(StateCommand::CreateTask {
            title: "late".into(),
            stack,
            order: 2,
        })
        .await
        .expect("created");
    let created = published(&h.handle)
        .tasks
        .values()
        .find(|t| t.title == "late")
        .expect("the task")
        .id;
    h.handle
        .execute(StateCommand::UpdateTask {
            id: created,
            changes: taskboard_domain::TaskChanges {
                duedate: Some(Some(ts(T0 - 60))),
                ..Default::default()
            },
        })
        .await
        .expect("edited");
    advance_both(&h.clock, 10).await;
    settle().await;
    assert_eq!(
        fired(&mut h.notifications),
        0,
        "the old cadence has not woken"
    );

    // The cadence drops to 5 s: the wake re-arms from the apply, so
    // the firing lands within one *new* tick — not at the old 60 s.
    apply(&h.handle, enabled(5)).await;
    advance_both(&h.clock, 6).await;
    settle().await;
    assert_eq!(
        fired(&mut h.notifications),
        1,
        "fired within the new cadence"
    );
}

#[tokio::test(start_paused = true)]
async fn a_live_disable_stops_the_tick_and_a_re_enable_resumes_it() {
    // Past-due task: the first (immediate) tick fires it.
    let repo = Arc::new(InMemoryRepository::with_state(seeded(Some(T0 - 3_600))));
    let mut h = spawn(
        repo,
        FixedClock::new(T0),
        enabled(1),
        DayRollConfig::disabled(),
    )
    .await;
    settle().await;
    assert_eq!(fired(&mut h.notifications), 1);

    // Disabled live; a second past-due task appears; many old ticks
    // pass — nothing fires.
    apply(
        &h.handle,
        SettingsSnapshot {
            reminders_enabled: false,
            ..enabled(1)
        },
    )
    .await;
    let stack = *published(&h.handle).stacks.keys().next().expect("stack");
    h.handle
        .execute(StateCommand::CreateTask {
            title: "late".into(),
            stack,
            order: 2,
        })
        .await
        .expect("created");
    let created = published(&h.handle)
        .tasks
        .values()
        .find(|t| t.title == "late")
        .expect("the task")
        .id;
    h.handle
        .execute(StateCommand::UpdateTask {
            id: created,
            changes: taskboard_domain::TaskChanges {
                duedate: Some(Some(ts(T0 - 60))),
                ..Default::default()
            },
        })
        .await
        .expect("edited");
    for _ in 0..5 {
        advance_both(&h.clock, 1).await;
        settle().await;
    }
    assert_eq!(
        fired(&mut h.notifications),
        0,
        "a disabled engine fires nothing"
    );

    // Re-enabled live: the next wake fires the waiting reminder.
    apply(&h.handle, enabled(1)).await;
    advance_both(&h.clock, 2).await;
    settle().await;
    assert_eq!(fired(&mut h.notifications), 1, "the re-enable resumes");
}

#[tokio::test(start_paused = true)]
async fn the_implicit_reminder_reads_the_live_defaults() {
    // The task is due in 5 min; with an at-due default nothing fires.
    let repo = Arc::new(InMemoryRepository::with_state(seeded(Some(T0 + 300))));
    let mut h = spawn(
        repo,
        FixedClock::new(T0),
        enabled(1),
        DayRollConfig::disabled(),
    )
    .await;
    settle().await;
    advance_both(&h.clock, 1).await;
    settle().await;
    assert_eq!(fired(&mut h.notifications), 0);

    // A 10-minute "before" default makes the same task due *now*: the
    // planner's implicit reminder carries the new offset at the next
    // tick — no spawn-time copy survives.
    apply(
        &h.handle,
        SettingsSnapshot {
            reminder_before: ReminderOffset::before(chrono::TimeDelta::minutes(-10)).unwrap(),
            ..enabled(1)
        },
    )
    .await;
    advance_both(&h.clock, 2).await;
    settle().await;
    assert_eq!(fired(&mut h.notifications), 1);
}

// -- the day calendar's live read --------------------------------------------

fn daily_head(day_secs: i64) -> PersistedState {
    let mut state = seeded(None);
    let head = state.tasks.values_mut().next().expect("the task");
    head.scheduled = Some(ts(day_secs));
    head.recur = Some(taskboard_domain::RecurRule {
        series: taskboard_domain::SeriesId::from_uuid(uuid::Uuid::from_u128(9_000)),
        anchor: chrono::NaiveDate::from_ymd_opt(2026, 6, 1).unwrap(),
        pattern: taskboard_domain::RecurPattern::Daily {
            every: taskboard_domain::recur::EveryDays::new(1).unwrap(),
        },
        bound: taskboard_domain::RecurBound::None,
        continued_from: None,
        paused_until: None,
    });
    state
}

fn head_scheduled(app: &AppState) -> Option<chrono::DateTime<chrono::Utc>> {
    app.tasks.values().next().expect("the head").scheduled
}

#[tokio::test(start_paused = true)]
async fn a_day_start_move_never_re_rolls_an_already_rolled_date() {
    // 2026-06-15 06:00 UTC; the head sits at yesterday 09:00 and rolls
    // to today 09:00 on the first shared wake.
    const DAY0: i64 = 1_781_481_600;
    let now = DAY0 + 6 * 3_600;
    let repo = Arc::new(CountingRepository::new(daily_head(DAY0 - 86_400 + 32_400)));
    let h = spawn(
        repo.clone(),
        FixedClock::new(now),
        SettingsSnapshot::default(),
        utc_roll(),
    )
    .await;
    settle().await;
    let rolled = Some(ts(DAY0 + 32_400));
    assert_eq!(head_scheduled(&published(&h.handle)), rolled);
    let applies_after_roll = repo.applies();

    // A day start of 05:00 keeps "today" the same date (06:00 ≥
    // 05:00): the watermark's fast path — no batch.
    apply(
        &h.handle,
        SettingsSnapshot {
            day_start_time: chrono::NaiveTime::from_hms_opt(5, 0, 0).unwrap(),
            ..SettingsSnapshot::default()
        },
    )
    .await;
    advance_both(&h.clock, 31).await;
    settle().await;
    assert_eq!(head_scheduled(&published(&h.handle)), rolled);
    assert_eq!(repo.applies(), applies_after_roll);

    // A day start of 07:00 moves "today" back to yesterday's window
    // and a return to 00:00 moves it forward again: neither re-rolls
    // the already-rolled head — the stale predicate is idempotent, so
    // the moves are watermark bookkeeping only.
    for hour in [7, 0] {
        apply(
            &h.handle,
            SettingsSnapshot {
                day_start_time: chrono::NaiveTime::from_hms_opt(hour, 0, 0).unwrap(),
                ..SettingsSnapshot::default()
            },
        )
        .await;
        advance_both(&h.clock, 31).await;
        settle().await;
        assert_eq!(head_scheduled(&published(&h.handle)), rolled);
        assert_eq!(repo.applies(), applies_after_roll);
    }
}
