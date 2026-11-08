// SPDX-License-Identifier: MIT OR Apache-2.0
//! The deadline-driven firing journeys (GUI review round 5, chunk 6,
//! D2): the engine's wake is `min(next due instant, now + tick)` — a
//! reminder fires **at** its due instant after boot and after every
//! due-changing mutation, never at the next tick. The tick stays the
//! fallback sweep (and the anti-busy-loop clamp), live-adjustable via
//! the settings apply. Paused tokio time throughout; the injected clock
//! and tokio time move in lockstep (the reminder suite's primitive).
//!
//! The suites use a 10 s cadence with due instants off the tick grid —
//! under the fixed-cadence loop these journeys would fire one tick
//! late, so each timing assertion is exactly the deadline behavior.
#![forbid(unsafe_code)]

use std::sync::Arc;
use std::time::Duration;

use taskboard_domain::persistence::{PersistedState, TaskRepository};
use taskboard_domain::test_support::{CountingIds, InMemoryRepository};
use taskboard_domain::{
    AdhocWhen, CommandOutcome, DeckFeatures, Habit, HabitClocks, HabitLogEntry, HabitLogKey,
    HabitState, HabitTarget, Insistence, NonZeroCount, NotifierMessage, PeriodKind, ReminderOffset,
    RemoteBoard, RemoteBoardId, RemoteCardId, RemoteCardRef, RemoteStack, RemoteStackId,
    RemoteStackRef, RemoteTask, SettingsSnapshot, StateCommand, SyncCommand, SyncReport,
    SystemEvent, TickSecs, WindowStart,
};
use taskboard_state::{DayRollConfig, EngineHandle, spawn_state_engine};

mod common;

use common::{
    FixedClock, FlakyRepository, T0, seed_bound_board_state, seed_stack_with_due_task,
    test_validators, ts,
};

/// The task map's single task (the `seeded` shape).
fn the_task(handle: &EngineHandle) -> taskboard_domain::Task {
    handle
        .shared_state()
        .load()
        .tasks
        .values()
        .next()
        .cloned()
        .expect("one seeded task")
}

fn enabled(tick_secs: u64) -> SettingsSnapshot {
    SettingsSnapshot {
        reminders_enabled: true,
        tick_secs: TickSecs::new(tick_secs).expect("nonzero"),
        reminder_before: ReminderOffset::at_due(),
        ..SettingsSnapshot::default()
    }
}

fn utc() -> chrono::FixedOffset {
    chrono::FixedOffset::east_opt(0).expect("zero offset")
}

fn anchored_habit_met_today() -> (Habit, HabitState) {
    let habit = Habit {
        id: taskboard_domain::HabitId::from_uuid(uuid::Uuid::from_u128(900)),
        board: taskboard_domain::BoardId::from_uuid(uuid::Uuid::from_u128(1)),
        title: "habit".into(),
        target: HabitTarget::PerDay(NonZeroCount::new(1).expect("nonzero")),
        order: 0,
        anchor_time: Some(chrono::NaiveTime::from_hms_opt(9, 0, 0).expect("09:00")),
        clocks: HabitClocks::all(ts(T0)),
        continued_from: None,
        deleted: false,
    };
    let mut habits = HabitState::default();
    habits.habits.insert(habit.id, habit.clone());
    habits.log.insert(
        HabitLogKey {
            habit: habit.id,
            window: WindowStart::of_period(PeriodKind::Day, ts(T0).date_naive()),
        },
        HabitLogEntry {
            count: 1,
            clock: ts(T0),
        },
    );
    (habit, habits)
}

fn seeded_boundary_habit() -> PersistedState {
    let mut state = seed_stack_with_due_task(None);
    let (_, habits) = anchored_habit_met_today();
    state.habits = habits;
    state
}

struct Harness {
    handle: EngineHandle,
    clock: Arc<FixedClock>,
    notifications: tokio::sync::mpsc::Receiver<NotifierMessage>,
    report_tx: tokio::sync::mpsc::Sender<SyncReport>,
}

async fn spawn(
    repo: Arc<dyn TaskRepository>,
    settings: SettingsSnapshot,
    day_roll: DayRollConfig,
) -> Harness {
    let clock = FixedClock::new(T0);
    let (sync_out, _sync_in) = tokio::sync::mpsc::channel::<SyncCommand>(8);
    let (report_tx, report_rx) = tokio::sync::mpsc::channel::<SyncReport>(8);
    let (system_tx, boot_rx) = tokio::sync::broadcast::channel::<SystemEvent>(8);
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
        None,
    )
    .await
    .expect("engine boots");
    Harness {
        handle,
        clock,
        notifications,
        report_tx,
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

// -- the journeys ---------------------------------------------------------

/// Boot: an undelivered reminder from before the reboot pins the
/// deadline to `now` — it fires on the first loop pass, with zero
/// advanced time.
#[tokio::test(start_paused = true)]
async fn the_boot_tick_fires_a_past_due_without_any_advance() {
    let mut h = spawn(
        Arc::new(InMemoryRepository::with_state(seed_stack_with_due_task(
            Some(T0 - 3_600),
        ))),
        enabled(10),
        DayRollConfig::disabled(),
    )
    .await;
    settle().await;
    assert_eq!(
        fired(&mut h.notifications),
        1,
        "the past-due default fired on the first loop pass"
    );
}

/// A due edit at an arbitrary offset: the firing lands exactly at the
/// new due instant, not at the next tick of the 10 s cadence.
#[tokio::test(start_paused = true)]
async fn a_due_edit_fires_at_its_instant_not_the_next_tick() {
    let mut h = spawn(
        Arc::new(InMemoryRepository::with_state(seed_stack_with_due_task(
            None,
        ))),
        enabled(10),
        DayRollConfig::disabled(),
    )
    .await;
    settle().await;
    let id = the_task(&h.handle).id;
    h.handle
        .execute(StateCommand::UpdateTask {
            id,
            changes: taskboard_domain::TaskChanges {
                duedate: Some(Some(ts(T0 + 32))),
                ..Default::default()
            },
        })
        .await
        .expect("edited");
    advance_both(&h.clock, 31).await;
    settle().await;
    assert_eq!(fired(&mut h.notifications), 0, "quiet before the instant");
    advance_both(&h.clock, 1).await;
    settle().await;
    assert_eq!(
        fired(&mut h.notifications),
        1,
        "fired at the due instant (the old cadence would fire at +40)"
    );
}

/// An `At` rule added at an arbitrary instant fires exactly there.
#[tokio::test(start_paused = true)]
async fn a_rule_add_fires_at_its_instant_not_the_next_tick() {
    let mut h = spawn(
        Arc::new(InMemoryRepository::with_state(seed_stack_with_due_task(
            None,
        ))),
        enabled(10),
        DayRollConfig::disabled(),
    )
    .await;
    settle().await;
    let id = the_task(&h.handle).id;
    let outcome = h
        .handle
        .execute(StateCommand::AddReminderRule {
            task: id,
            offset: ReminderOffset::at_due(),
            at: Some(AdhocWhen::At(ts(T0 + 23))),
            insistence: Insistence::Normal,
        })
        .await
        .expect("accepted");
    assert!(
        matches!(
            outcome,
            CommandOutcome::ReminderRuleAdded { added: true, .. }
        ),
        "the rule landed"
    );
    advance_both(&h.clock, 22).await;
    settle().await;
    assert_eq!(fired(&mut h.notifications), 0, "quiet before the instant");
    advance_both(&h.clock, 1).await;
    settle().await;
    assert_eq!(
        fired(&mut h.notifications),
        1,
        "fired at the rule instant (the old cadence would fire at +30)"
    );
}

/// Undoing a due edit restores the earlier due instant, and the wake
/// follows it back — the firing lands at the restored instant, not at
/// the moved one.
#[tokio::test(start_paused = true)]
async fn an_undo_restores_the_deadline_with_the_due() {
    let mut h = spawn(
        Arc::new(InMemoryRepository::with_state(seed_stack_with_due_task(
            Some(T0 + 37),
        ))),
        enabled(10),
        DayRollConfig::disabled(),
    )
    .await;
    settle().await;
    let id = the_task(&h.handle).id;
    h.handle
        .execute(StateCommand::UpdateTask {
            id,
            changes: taskboard_domain::TaskChanges {
                duedate: Some(Some(ts(T0 + 137))),
                ..Default::default()
            },
        })
        .await
        .expect("edited");
    let outcome = h.handle.execute(StateCommand::Undo).await.expect("undone");
    assert!(matches!(outcome, CommandOutcome::Undone { .. }));
    advance_both(&h.clock, 36).await;
    settle().await;
    assert_eq!(
        fired(&mut h.notifications),
        0,
        "quiet before the restored instant"
    );
    advance_both(&h.clock, 1).await;
    settle().await;
    assert_eq!(
        fired(&mut h.notifications),
        1,
        "fired at the restored due instant, not the moved one"
    );
}

/// Unbinding the board strands the ops but keeps the reminder deadline
/// armed: a due edit after the unbind still fires at its instant.
#[tokio::test(start_paused = true)]
async fn an_unbind_keeps_the_deadline_armed() {
    let mut h = spawn(
        Arc::new(InMemoryRepository::with_state(seed_bound_board_state(
            "board",
        ))),
        enabled(10),
        DayRollConfig::disabled(),
    )
    .await;
    settle().await;
    let board_id = *h
        .handle
        .shared_state()
        .load()
        .boards
        .keys()
        .next()
        .expect("board");
    let outcome = h
        .handle
        .execute(StateCommand::UnbindBoard { board: board_id })
        .await
        .expect("accepted");
    assert!(matches!(outcome, CommandOutcome::Unbound { .. }));
    // The bound-board seed has no stacks or tasks: create both, then
    // edit the task's due.
    let board_id_2 = *h
        .handle
        .shared_state()
        .load()
        .boards
        .keys()
        .next()
        .expect("board");
    let taskboard_domain::CommandOutcome::CreatedStack(stack) = h
        .handle
        .execute(StateCommand::CreateStack {
            board: board_id_2,
            title: "backlog".into(),
            order: 1,
        })
        .await
        .expect("created")
    else {
        panic!("wrong receipt");
    };
    let outcome = h
        .handle
        .execute(StateCommand::CreateTask {
            title: "bound task".into(),
            stack,
            order: 1,
        })
        .await
        .expect("created");
    let taskboard_domain::CommandOutcome::CreatedTask(id) = outcome else {
        panic!("wrong receipt");
    };
    h.handle
        .execute(StateCommand::UpdateTask {
            id,
            changes: taskboard_domain::TaskChanges {
                duedate: Some(Some(ts(T0 + 20))),
                ..Default::default()
            },
        })
        .await
        .expect("edited");
    advance_both(&h.clock, 19).await;
    settle().await;
    assert_eq!(fired(&mut h.notifications), 0, "quiet before the instant");
    advance_both(&h.clock, 1).await;
    settle().await;
    assert_eq!(fired(&mut h.notifications), 1, "fired at the due instant");
}

/// Cancelling an alarm removes its deadline: nothing fires at the
/// cancelled alarm's instant through many cadence ticks.
#[tokio::test(start_paused = true)]
async fn an_alarm_cancel_kills_the_deadline() {
    let mut h = spawn(
        Arc::new(InMemoryRepository::with_state(seed_stack_with_due_task(
            None,
        ))),
        enabled(10),
        DayRollConfig::disabled(),
    )
    .await;
    settle().await;
    let CommandOutcome::AdhocAlarmCreated { alarm } = h
        .handle
        .execute(StateCommand::CreateAdhocAlarm {
            label: "stand up".into(),
            when: AdhocWhen::At(ts(T0 + 25)),
            insistence: Insistence::Normal,
        })
        .await
        .expect("created")
    else {
        panic!("wrong receipt");
    };
    advance_both(&h.clock, 5).await;
    settle().await;
    let outcome = h
        .handle
        .execute(StateCommand::CancelAdhocAlarm { alarm })
        .await
        .expect("accepted");
    assert_eq!(outcome, CommandOutcome::AdhocAlarmCancelled { alarm });
    advance_both(&h.clock, 60).await;
    settle().await;
    assert_eq!(
        fired(&mut h.notifications),
        0,
        "the cancelled alarm never fired"
    );
}

/// A settings change that moves the effective occurrence moves the
/// deadline: the `default_before` widening fires the implicit reminder
/// at the new effective instant, not at the original due.
#[tokio::test(start_paused = true)]
async fn a_settings_change_moves_the_deadline() {
    let mut h = spawn(
        Arc::new(InMemoryRepository::with_state(seed_stack_with_due_task(
            Some(T0 + 105),
        ))),
        enabled(10),
        DayRollConfig::disabled(),
    )
    .await;
    settle().await;
    let outcome = h
        .handle
        .execute(StateCommand::UpdateSettings {
            settings: SettingsSnapshot {
                reminder_before: ReminderOffset::before(
                    chrono::TimeDelta::try_seconds(-30).expect("in range"),
                )
                .expect("nonpositive"),
                ..enabled(10)
            },
        })
        .await
        .expect("applied");
    assert_eq!(outcome, CommandOutcome::SettingsApplied);
    advance_both(&h.clock, 69).await;
    settle().await;
    assert_eq!(fired(&mut h.notifications), 0, "quiet before the instant");
    advance_both(&h.clock, 6).await;
    settle().await;
    assert_eq!(
        fired(&mut h.notifications),
        1,
        "fired at the moved effective instant (due − 30 s)"
    );
}

/// A sync report that lands a past-due due date re-arms from the
/// ingest: the merged task's reminder fires without waiting out the
/// cadence.
#[tokio::test(start_paused = true)]
async fn a_sync_report_re_arms_and_fires() {
    let mut h = spawn(
        Arc::new(InMemoryRepository::with_state(seed_bound_board_state(
            "board",
        ))),
        enabled(10),
        DayRollConfig::disabled(),
    )
    .await;
    settle().await;
    h.report_tx
        .send(SyncReport::Completed {
            app_data: taskboard_domain::AppDataEvidence::Absent,
            duration: Duration::ZERO,
            connector: taskboard_domain::test_support::test_connector(),
            snapshot: Box::new(taskboard_domain::RemoteBoardSnapshot::Deck(
                taskboard_domain::DeckBoardSnapshot {
                    board: RemoteBoard {
                        id: RemoteBoardId::deck(42),
                        title: "board".into(),
                        color: "0000ff".into(),
                        archived: false,
                        deleted_at: None,
                        last_modified: ts(T0),
                    },
                    stacks: vec![RemoteStack {
                        id: RemoteStackRef {
                            board: RemoteBoardId::deck(42),
                            stack: RemoteStackId::deck(500),
                        },
                        title: "backlog".into(),
                        order: 1,
                        archived: false,
                        deleted_at: None,
                        last_modified: ts(T0),
                    }],
                    tasks: vec![RemoteTask {
                        id: RemoteCardRef {
                            board: RemoteBoardId::deck(42),
                            stack: RemoteStackId::deck(500),
                            card: RemoteCardId::deck(600),
                        },
                        title: "remote due".into(),
                        description: String::new(),
                        duedate: Some(ts(T0 - 60)),
                        scheduled: None,
                        done: None,
                        stack: RemoteStackId::deck(500),
                        order: 2,
                        labels: std::collections::BTreeSet::new(),
                        archived: false,
                        last_modified: ts(T0 + 1),
                        created_at: ts(T0 + 1),
                    }],
                    labels: vec![],
                },
            )),
            validators: test_validators(),
            pushes: vec![],
            read_at: ts(T0 + 1),
            features: DeckFeatures::CONSERVATIVE,
        })
        .await
        .expect("report channel open");
    advance_both(&h.clock, 1).await;
    settle().await;
    assert_eq!(
        fired(&mut h.notifications),
        1,
        "the merged past-due fired on the report re-arm"
    );
}

/// The clamp: a permanently-unresolvable deadline (every delivery
/// persist fails) degrades to the tick cadence — bounded wakes, never a
/// spin. A ceiling on garbage input (AGENTS §5): the pathology is the
/// failing storage, not a success bound.
#[tokio::test(start_paused = true)]
async fn a_stuck_deadline_degrades_to_tick_cadence_never_spins() {
    let h = spawn(
        Arc::new(FlakyRepository::new(
            u32::MAX,
            seed_stack_with_due_task(Some(T0 - 3_600)),
        )),
        enabled(1),
        DayRollConfig::disabled(),
    )
    .await;
    settle().await;
    let boot_ticks = h.handle.metrics().ticks;
    for _ in 0..12 {
        advance_both(&h.clock, 1).await;
    }
    let ticks = h.handle.metrics().ticks - boot_ticks;
    assert!(
        (1..=15).contains(&ticks),
        "twelve cadence seconds serviced a bounded number of wakes, got {ticks}"
    );
}

/// The day-boundary floor: an anchored habit keeps an extra wake at the
/// window roll even when the cadence is an hour — the tracker pass's
/// quota gate rolls at the boundary, and the wake must be there.
#[tokio::test(start_paused = true)]
async fn the_day_boundary_wakes_a_deadline_armed_engine() {
    let h = spawn(
        Arc::new(InMemoryRepository::with_state(seeded_boundary_habit())),
        enabled(3_600),
        DayRollConfig {
            enabled: false,
            offset: utc(),
        },
    )
    .await;
    settle().await;
    let boot_ticks = h.handle.metrics().ticks;
    // Midnight UTC is T0 + 14 h; the 1 h cadence never lands there on
    // its own.
    advance_both(&h.clock, 50_399).await;
    settle().await;
    assert_eq!(
        h.handle.metrics().ticks - boot_ticks,
        1,
        "no wake crossed the pre-boundary window except the boot tick"
    );
    advance_both(&h.clock, 2).await;
    settle().await;
    assert_eq!(
        h.handle.metrics().ticks - boot_ticks,
        2,
        "the boundary candidate woke the engine at the window roll"
    );
}

/// The `ReminderRuleAdded` dedup survivor (`added: false` — zero
/// actions planned) is a no-op re-arm: the deadline stays where it was.
#[tokio::test(start_paused = true)]
async fn a_dedup_rule_add_does_not_disturb_the_deadline() {
    let mut h = spawn(
        Arc::new(InMemoryRepository::with_state(seed_stack_with_due_task(
            Some(T0 + 40),
        ))),
        enabled(10),
        DayRollConfig::disabled(),
    )
    .await;
    settle().await;
    let id = the_task(&h.handle).id;
    let first = h
        .handle
        .execute(StateCommand::AddReminderRule {
            task: id,
            offset: ReminderOffset::at_due(),
            at: None,
            insistence: Insistence::Normal,
        })
        .await
        .expect("accepted");
    let CommandOutcome::ReminderRuleAdded { rule, added: true } = first else {
        panic!("wrong receipt");
    };
    let second = h
        .handle
        .execute(StateCommand::AddReminderRule {
            task: id,
            offset: ReminderOffset::at_due(),
            at: None,
            insistence: Insistence::Normal,
        })
        .await
        .expect("accepted");
    let CommandOutcome::ReminderRuleAdded {
        rule: survivor,
        added: false,
    } = second
    else {
        panic!("wrong receipt");
    };
    assert_eq!(survivor, rule, "the dedup survivor is the first rule");
    advance_both(&h.clock, 39).await;
    settle().await;
    assert_eq!(fired(&mut h.notifications), 0, "quiet before the instant");
    advance_both(&h.clock, 1).await;
    settle().await;
    assert_eq!(
        fired(&mut h.notifications),
        1,
        "the reminder still fires at its due instant"
    );
}
