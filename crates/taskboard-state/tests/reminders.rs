// SPDX-License-Identifier: MIT OR Apache-2.0
//! Reminder-tick routing suite (R-series): the engine's fourth select!
//! arm — fire, at-most-once, snooze re-fire, drain silence, closed
//! notifier, storage-failure-fires-nothing, quiet tick, the auto-advance
//! guard, and mute routing.
//!
//! Routing + timing only; the firing *semantics* are pinned in the
//! domain's pure suite. Every test runs on a paused runtime. The
//! injected clock — which decides *what* fires — moves only through the
//! lockstep helper; the poll helpers advance tokio time (which decides
//! *when* the arm wakes) in 1 ms turns, just enough to let
//! already-triggered work land. The arm uses the timer wheel, so the
//! suite is Miri-gated (the existing suites' "Miri-clean, no timers"
//! claim is untouched: their helpers all spawn with reminders disabled).

use std::sync::Arc;
use std::time::Duration;

use taskboard_domain::NotifierMessage;
use taskboard_domain::idgen::IdGenerator as _;
use taskboard_domain::persistence::{
    BoxFuture, PersistedState, PersistenceAction, RepositoryError, TaskRepository,
};
use taskboard_domain::test_support::{CountingIds, InMemoryRepository};
use taskboard_domain::{
    AdhocWhen, AppState, Board, CommandOutcome, EngineSignal, Notification, ReminderAnchor,
    ReminderDefaults, ReminderOffset, ReminderRef, ReminderSnoozeKey, Stack, StackClocks,
    StateCommand, SyncCommand, SyncReport, SystemEvent, Task, TaskClocks,
};
use taskboard_state::{EngineHandle, ReminderEngineConfig, spawn_state_engine};

mod common;

use common::{FixedClock, T0, ts};

/// Fails the first `failures` applies, then delegates (the engine.rs
/// `FlakyRepository`, local to this suite's durable-then-notify pin).
#[derive(Debug)]
struct FlakyRepository {
    inner: InMemoryRepository,
    remaining_failures: std::sync::atomic::AtomicU32,
}

impl FlakyRepository {
    fn new(failures: u32, state: PersistedState) -> Self {
        Self {
            inner: InMemoryRepository::with_state(state),
            remaining_failures: std::sync::atomic::AtomicU32::new(failures),
        }
    }
}

impl TaskRepository for FlakyRepository {
    fn load(&self) -> BoxFuture<'_, Result<PersistedState, RepositoryError>> {
        self.inner.load()
    }

    fn apply(&self, actions: Vec<PersistenceAction>) -> BoxFuture<'_, Result<(), RepositoryError>> {
        Box::pin(async move {
            let should_fail = self
                .remaining_failures
                .try_update(
                    std::sync::atomic::Ordering::SeqCst,
                    std::sync::atomic::Ordering::SeqCst,
                    |n| (n > 0).then(|| n - 1),
                )
                .is_ok();
            if should_fail {
                return Err(RepositoryError::Unavailable);
            }
            self.inner.apply(actions).await
        })
    }
}

/// The enabled tick config used by the R-series: a 1 s cadence (many
/// ticks per advance window) and an at-due implicit default.
fn tick_config() -> ReminderEngineConfig {
    ReminderEngineConfig {
        enabled: true,
        tick: Duration::from_secs(1),
        defaults: ReminderDefaults {
            default_before: ReminderOffset::at_due(),
        },
    }
}

/// A live board + stack + one live open task with the given due date.
fn seeded_task_state(duedate_secs: i64) -> (PersistedState, taskboard_domain::TaskId) {
    let ids = CountingIds::new();
    let board = Board {
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
    };
    let stack = Stack {
        id: ids.new_stack_id(),
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
    let task = Task {
        id: ids.new_task_id(),
        remote: None,
        title: "due task".into(),
        description: String::new(),
        duedate: Some(ts(duedate_secs)),
        scheduled: None,
        done: None,
        stack: stack.id,
        order: 1,
        labels: std::collections::BTreeSet::new(),
        effort: None,
        importance: None,
        urgency: None,
        recur: None,

        archived: false,
        deleted: false,
        clocks: TaskClocks {
            title: ts(T0),
            description: ts(T0),
            duedate: ts(T0),
            scheduled: ts(T0),
            done: ts(T0),
            position: ts(T0),
            labels: ts(T0),
            effort: ts(T0),
            importance: ts(T0),
            urgency: ts(T0),
            recur: ts(T0),

            archived: ts(T0),
            deleted: ts(T0),
        },
        remote_seen: None,
        remote_baseline: None,
    };
    let task_id = task.id;
    let mut state = PersistedState::default();
    state.boards.insert(board.id, board);
    state.stacks.insert(stack.id, stack);
    state.tasks.insert(task.id, task);
    (state, task_id)
}

struct Spawner {
    repo: Arc<dyn TaskRepository>,
    clock: Arc<FixedClock>,
    reminders: ReminderEngineConfig,
}

impl Default for Spawner {
    fn default() -> Self {
        let (state, _) = seeded_task_state(T0 - 3_600); // past due by an hour
        Self {
            repo: Arc::new(InMemoryRepository::with_state(state)),
            clock: FixedClock::new(T0),
            reminders: tick_config(),
        }
    }
}

struct Harness {
    handle: EngineHandle,
    clock: Arc<FixedClock>,
    system_tx: tokio::sync::broadcast::Sender<SystemEvent>,
    notifications: tokio::sync::mpsc::Receiver<NotifierMessage>,
}

async fn spawn(spawner: Spawner) -> Harness {
    let (sync_out, _sync_in) = tokio::sync::mpsc::channel::<SyncCommand>(8);
    let (_report_tx, report_rx) = tokio::sync::mpsc::channel::<SyncReport>(8);
    let (system_tx, system_rx) = tokio::sync::broadcast::channel::<SystemEvent>(8);
    let (notifier, notifications) = tokio::sync::mpsc::channel::<NotifierMessage>(64);
    let (handle, _join) = spawn_state_engine(
        spawner.repo.clone(),
        Arc::new(CountingIds::new()),
        spawner.clock.clone(),
        sync_out,
        report_rx,
        system_tx.clone(),
        system_rx,
        notifier,
        spawner.reminders,
    )
    .await
    .expect("engine boots");
    Harness {
        handle,
        clock: spawner.clock,
        system_tx,
        notifications,
    }
}

async fn harness() -> Harness {
    spawn(Spawner::default()).await
}

/// One scheduler turn: the spawned engine's first poll runs the
/// immediate first tick against the un-advanced injected clock.
async fn settle_boot_tick() {
    tokio::task::yield_now().await;
}

/// The lockstep primitive: the injected clock (what the evaluator sees)
/// and tokio time (when the arm wakes) move together, so every firing is
/// observable at a tick that can see it.
async fn advance_both(clock: &FixedClock, secs: i64) {
    clock.advance(secs);
    tokio::time::advance(Duration::from_secs(u64::try_from(secs).expect("positive"))).await;
}

/// Polls `predicate` against the shared state, advancing paused time in
/// 1 ms turns — the sanctioned primitive both yields to the engine and
/// keeps the virtual 60 s deadline a real bound (a failing test exits,
/// never spins).
async fn eventually(handle: &EngineHandle, pred: impl Fn(&AppState) -> bool) {
    tokio::time::timeout(Duration::from_secs(60), async {
        loop {
            if pred(&handle.shared_state().load_full()) {
                return;
            }
            tokio::time::advance(Duration::from_millis(1)).await;
        }
    })
    .await
    .expect("predicate satisfied within the deadline");
}

/// Waits until the engine is in drain mode (the `EngineGone` reply is the
/// transition's only observation).
async fn wait_draining(handle: &EngineHandle) {
    tokio::time::timeout(Duration::from_secs(60), async {
        loop {
            if handle
                .execute(StateCommand::RequestSync)
                .await
                .is_err_and(|err| matches!(err, taskboard_state::ExecuteError::EngineGone))
            {
                return;
            }
            tokio::time::advance(Duration::from_millis(1)).await;
        }
    })
    .await
    .expect("engine entered drain mode within the deadline");
}

fn drain_signals(rx: &mut tokio::sync::broadcast::Receiver<EngineSignal>) -> usize {
    let mut count = 0;
    while rx.try_recv().is_ok() {
        count += 1;
    }
    count
}

fn drain_notifications(rx: &mut tokio::sync::mpsc::Receiver<NotifierMessage>) -> Vec<Notification> {
    // The tick path only ever fires; a `Withdraw` here would mean a
    // dismissal landed where none was commanded.
    let mut drained = Vec::new();
    loop {
        match rx.try_recv() {
            Ok(NotifierMessage::Fire(note)) => drained.push(note),
            Ok(NotifierMessage::Withdraw { .. }) => {
                panic!("unexpected withdraw in a tick-path test")
            }
            Err(_) => break,
        }
    }
    drained
}

#[expect(dead_code)]
fn drain_messages(rx: &mut tokio::sync::mpsc::Receiver<NotifierMessage>) -> Vec<NotifierMessage> {
    let mut out = Vec::new();
    while let Ok(note) = rx.try_recv() {
        out.push(note);
    }
    out
}

// ---------------------------------------------------------------------
// R-series
// ---------------------------------------------------------------------

/// R1: the immediate first tick fires the past-due implicit default —
/// exactly one delivery, one notification carrying the firing triple and
/// the title snapshot (occurrence from the injected clock's view), and
/// one repaint signal.
#[cfg_attr(miri, ignore)] // timer wheel (testing-strategy §10 scope)
#[tokio::test(start_paused = true)]
async fn r1_first_tick_fires_the_past_due_default() {
    let mut h = harness().await;
    let mut signals = h.handle.subscribe();
    settle_boot_tick().await;
    let task_id = h
        .handle
        .shared_state()
        .load_full()
        .tasks
        .keys()
        .next()
        .expect("seeded task")
        .to_owned();

    advance_both(&h.clock, 1).await;
    eventually(&h.handle, |app| !app.reminders.deliveries.is_empty()).await;

    let app = h.handle.shared_state().load_full();
    let delivery = app
        .reminders
        .deliveries
        .iter()
        .next()
        .expect("one delivery");
    assert_eq!(app.reminders.deliveries.len(), 1);
    assert_eq!(delivery.reminder, ReminderRef::Default(task_id));
    assert_eq!(delivery.occurrence, ts(T0 - 3_600));
    assert_eq!(delivery.delivered_at, ts(T0));
    drop(app);

    let notes = drain_notifications(&mut h.notifications);
    assert_eq!(
        notes,
        vec![Notification {
            reminder: ReminderRef::Default(task_id),
            anchor: taskboard_domain::ReminderAnchor::Task(task_id),
            title: "due task".into(),
            occurrence: ts(T0 - 3_600),
        }],
        "exactly one notification with the title snapshot"
    );
    assert_eq!(drain_signals(&mut signals), 1, "the firing repainted");
}

/// R2: after the fire, further ticks are silent — the delivery marker is
/// the at-most-once guard, so advancing across many ticks yields no
/// second delivery and no second notification.
#[cfg_attr(miri, ignore)]
#[tokio::test(start_paused = true)]
async fn r2_settled_occurrence_never_refires() {
    let mut h = harness().await;
    settle_boot_tick().await;
    advance_both(&h.clock, 1).await;
    eventually(&h.handle, |app| !app.reminders.deliveries.is_empty()).await;
    assert_eq!(
        drain_notifications(&mut h.notifications).len(),
        1,
        "exactly the first-fire notification"
    );

    advance_both(&h.clock, 30).await;
    h.handle.flush().await;
    let app = h.handle.shared_state().load_full();
    assert_eq!(
        app.reminders.deliveries.len(),
        1,
        "at-most-once per occurrence across many ticks"
    );
    assert!(
        drain_notifications(&mut h.notifications).is_empty(),
        "no second notification"
    );
}

/// R3: snooze reschedules through the same evaluator — quiet until the
/// target, re-fire at the target with the rescheduled occurrence, settled
/// thereafter.
#[cfg_attr(miri, ignore)]
#[tokio::test(start_paused = true)]
async fn r3_snooze_refires_at_its_target_then_settles() {
    let mut h = harness().await;
    settle_boot_tick().await;
    advance_both(&h.clock, 1).await;
    eventually(&h.handle, |app| !app.reminders.deliveries.is_empty()).await;
    assert_eq!(
        drain_notifications(&mut h.notifications).len(),
        1,
        "exactly the first-fire notification"
    );
    let task_id = h
        .handle
        .shared_state()
        .load_full()
        .tasks
        .keys()
        .next()
        .expect("seeded task")
        .to_owned();

    // The snooze executes at the clock's current instant (T0 + 1 s after
    // the lockstep advance above): the engine stamps target = now + 10 m.
    let target = ts(T0 + 1 + 600);
    let outcome = h
        .handle
        .execute(StateCommand::SnoozeReminder {
            reminder: ReminderRef::Default(task_id),
            occurrence: ts(T0 - 3_600),
            postpone: chrono::TimeDelta::try_minutes(10).unwrap(),
        })
        .await
        .expect("accepted");
    assert_eq!(outcome, CommandOutcome::Applied);
    let app = h.handle.shared_state().load_full();
    assert!(app.reminders.snoozes.contains_key(&ReminderSnoozeKey {
        reminder: ReminderRef::Default(task_id),
        occurrence: ts(T0 - 3_600),
    }));
    drop(app);

    // Just before the target: quiet.
    advance_both(&h.clock, 590).await;
    h.handle.flush().await;
    let app = h.handle.shared_state().load_full();
    assert_eq!(
        app.reminders.deliveries.len(),
        1,
        "still only the base fire"
    );
    drop(app);

    // Past the target: the re-fire carries the rescheduled occurrence.
    advance_both(&h.clock, 20).await;
    eventually(&h.handle, |app| app.reminders.deliveries.len() == 2).await;
    let notes = drain_notifications(&mut h.notifications);
    assert_eq!(notes.len(), 1, "exactly the re-fire notification");
    assert_eq!(notes[0].occurrence, target);
    let app = h.handle.shared_state().load_full();
    assert!(
        app.reminders
            .is_delivered(&ReminderRef::Default(task_id), &target)
    );

    // Settled thereafter.
    advance_both(&h.clock, 60).await;
    h.handle.flush().await;
    let app = h.handle.shared_state().load_full();
    assert_eq!(app.reminders.deliveries.len(), 2, "no third fire");
}

/// R-alarm series (chunk 7, D11): the tick's routing of standalone
/// alarms on the paused clock. Semantics live in the domain suite;
/// these pin the engine-side correlation: the delivery batch lands with
/// the alarm row kept alive until dismissal, notification payloads
/// carry the alarm anchor, and the snoozed ref re-fires at its target.
/// The alarm rows are seeded/minted through the engine's own command
/// path (`CreateAdhocAlarm`) — the staged create-then-advance shape.
/// Re-timed 2026-10-26 (#474): delivery-keyed settlement was the pre-R3
/// contract; DR-1 R3
/// (`.artifacts/plans/2026-10-25-reminder-rule-sync-design-round-plan.md`
/// §4-R3) makes dismissal the settle — a fired alarm row and its
/// delivery survive until the dismissal batch's post-pass prunes them.
/// R9: an alarm fires within one tick of its stamped instant — exactly
/// one delivery, one notification with the `Alarm` anchor and the label,
/// the row and delivery survive on the shared state, and the dismissal
/// batch's post-pass reclaims them.
#[cfg_attr(miri, ignore)] // timer wheel (testing-strategy §10 scope)
#[tokio::test(start_paused = true)]
async fn r9_alarm_fires_within_one_tick_and_survives_until_dismissal() {
    let mut h = spawn(Spawner {
        repo: Arc::new(InMemoryRepository::with_state(PersistedState::default())),
        ..Spawner::default()
    })
    .await;
    settle_boot_tick().await;
    let outcome = h
        .handle
        .execute(StateCommand::CreateAdhocAlarm {
            label: "steep the tea".into(),
            when: AdhocWhen::In(chrono::TimeDelta::try_seconds(30).unwrap()),
        })
        .await
        .expect("accepted");
    let CommandOutcome::AdhocAlarmCreated { alarm: alarm_id } = outcome else {
        panic!("create yields the created receipt");
    };
    let reminder = ReminderRef::Adhoc(alarm_id);

    advance_both(&h.clock, 31).await;
    eventually(&h.handle, |app| app.reminders.deliveries.len() == 1).await;

    // The fired alarm is the live in-app surface: row and delivery both
    // survive (settled is delivered ∧ dismissed, not delivered alone).
    let app = h.handle.shared_state().load_full();
    assert!(
        app.reminders.alarms.contains_key(&alarm_id),
        "the fired alarm row survives until dismissal"
    );
    assert!(app.reminders.is_delivered(&reminder, &ts(T0 + 30)));
    let surface = taskboard_domain::active_notifications(
        &app.tasks,
        &app.reminders,
        &ReminderDefaults::default(),
        ts(T0 + 30),
    );
    assert_eq!(surface.len(), 1, "the fired alarm is active");
    assert_eq!(surface[0].reminder, reminder);
    assert_eq!(surface[0].occurrence, ts(T0 + 30));
    drop(app);

    // The durable-then-notify leg of the tick's delivery batch is R1's
    // subject (the batch path is shared, construction unchanged by the
    // anchor widening). What the alarm arm adds: the notification whose
    // delivery marker is in the shared-semantics fake.
    let notes = drain_notifications(&mut h.notifications);
    assert_eq!(
        notes,
        vec![Notification {
            reminder,
            anchor: ReminderAnchor::Alarm(alarm_id),
            title: "steep the tea".into(),
            occurrence: ts(T0 + 30),
        }],
        "one payload with the alarm anchor and the label"
    );

    // The dismissal settles it: the dismissal batch's post-pass prunes
    // the row with its dependents, and the withdraw lands on the
    // notifier.
    let outcome = h
        .handle
        .execute(StateCommand::DismissReminder {
            reminder,
            occurrence: ts(T0 + 30),
        })
        .await
        .expect("accepted");
    assert_eq!(outcome, CommandOutcome::Applied);
    let NotifierMessage::Withdraw {
        reminder: withdrawn,
        occurrence,
    } = next_withdraw(&mut h.notifications).await
    else {
        unreachable!("next_withdraw only returns Withdraw")
    };
    assert_eq!(withdrawn, reminder);
    assert_eq!(occurrence, ts(T0 + 30));
    eventually(&h.handle, |app| app.reminders.alarms.is_empty()).await;
    let app = h.handle.shared_state().load_full();
    assert!(
        app.reminders.deliveries.is_empty(),
        "the prune cascades the settled delivery out of memory too"
    );
    drop(app);
}

/// R10: a future alarm is quiet — advances below its target leave the
/// row untouched (no delivery, no notification).
#[cfg_attr(miri, ignore)]
#[tokio::test(start_paused = true)]
async fn r10_future_alarm_is_quiet_below_its_target() {
    let mut h = spawn(Spawner {
        repo: Arc::new(InMemoryRepository::with_state(PersistedState::default())),
        ..Spawner::default()
    })
    .await;
    settle_boot_tick().await;
    let CommandOutcome::AdhocAlarmCreated { alarm: alarm_id } = h
        .handle
        .execute(StateCommand::CreateAdhocAlarm {
            label: "wake up".into(),
            when: AdhocWhen::In(chrono::TimeDelta::try_hours(1).unwrap()),
        })
        .await
        .expect("accepted")
    else {
        panic!("create yields the created receipt");
    };

    advance_both(&h.clock, 120).await;
    h.handle.flush().await;
    let app = h.handle.shared_state().load_full();
    assert_eq!(app.reminders.alarms.len(), 1, "the row survives");
    assert!(app.reminders.alarms.contains_key(&alarm_id));
    assert!(
        app.reminders.deliveries.is_empty(),
        "no delivery below the target"
    );
    drop(app);
    assert_eq!(
        drain_notifications(&mut h.notifications),
        [] as [Notification; 0]
    );
}

/// R11: a snoozed alarm re-fires at its target — the engine-side
/// routing of a snoozed `Adhoc` ref (the chain semantics are the
/// domain suite's P7).
#[cfg_attr(miri, ignore)]
#[tokio::test(start_paused = true)]
async fn r11_snoozed_alarm_refires_at_its_target() {
    let mut h = spawn(Spawner {
        repo: Arc::new(InMemoryRepository::with_state(PersistedState::default())),
        ..Spawner::default()
    })
    .await;
    settle_boot_tick().await;
    let CommandOutcome::AdhocAlarmCreated { alarm: alarm_id } = h
        .handle
        .execute(StateCommand::CreateAdhocAlarm {
            label: "tea".into(),
            when: AdhocWhen::In(chrono::TimeDelta::try_seconds(30).unwrap()),
        })
        .await
        .expect("accepted")
    else {
        panic!("create yields the created receipt");
    };
    let reminder = ReminderRef::Adhoc(alarm_id);
    // Snooze the pending occurrence (typed-legal before the fire): the
    // engine stamps target = now + 10 m (the clock is still at T0).
    let target = ts(T0 + 600);
    let outcome = h
        .handle
        .execute(StateCommand::SnoozeReminder {
            reminder,
            occurrence: ts(T0 + 30),
            postpone: chrono::TimeDelta::try_minutes(10).unwrap(),
        })
        .await
        .expect("accepted");
    assert_eq!(outcome, CommandOutcome::Applied);

    // Past the original stamp: quiet (the chain moved the fire).
    advance_both(&h.clock, 31).await;
    h.handle.flush().await;
    let app = h.handle.shared_state().load_full();
    assert!(
        app.reminders.deliveries.is_empty(),
        "quiet below the target"
    );
    assert_eq!(app.reminders.alarms.len(), 1, "still alive");
    drop(app);

    // At the target: fires with the rescheduled occurrence. The row
    // and delivery survive (re-timed 2026-10-26 (#474): DR-1 R3 makes
    // dismissal the settle; the persistence half of the old pin is
    // intact — the prune still lands in the same post-pass batch, now
    // keyed on delivered ∧ dismissed).
    advance_both(&h.clock, 600).await;
    eventually(&h.handle, |app| app.reminders.deliveries.len() == 1).await;
    let notes = drain_notifications(&mut h.notifications);
    assert_eq!(notes.len(), 1, "exactly the re-fire notification");
    assert_eq!(notes[0].occurrence, target);
    assert_eq!(notes[0].anchor, ReminderAnchor::Alarm(alarm_id));
    let app = h.handle.shared_state().load_full();
    assert!(
        app.reminders.alarms.contains_key(&alarm_id),
        "the fired alarm row survives until dismissal"
    );
    drop(app);

    // The dismissal batch's post-pass prunes the settled alarm.
    let outcome = h
        .handle
        .execute(StateCommand::DismissReminder {
            reminder,
            occurrence: target,
        })
        .await
        .expect("accepted");
    assert_eq!(outcome, CommandOutcome::Applied);
    eventually(&h.handle, |app| {
        app.reminders.alarms.is_empty() && app.reminders.deliveries.is_empty()
    })
    .await;
}

/// R12: snoozing a FIRED alarm resolves and moves the awaiting key —
/// re-timed 2026-10-26 (#474): between fire and dismissal the row (and
/// its snooze affordance) live, so `SnoozeReminder` on the fired
/// occurrence is accepted, the chain moves the next fire to the
/// rescheduled target, and the final dismissal batch's post-pass
/// reclaims the row.
#[cfg_attr(miri, ignore)]
#[tokio::test(start_paused = true)]
async fn r12_snooze_on_a_fired_alarm_moves_the_awaiting_key() {
    let mut h = spawn(Spawner {
        repo: Arc::new(InMemoryRepository::with_state(PersistedState::default())),
        ..Spawner::default()
    })
    .await;
    settle_boot_tick().await;
    let CommandOutcome::AdhocAlarmCreated { alarm: alarm_id } = h
        .handle
        .execute(StateCommand::CreateAdhocAlarm {
            label: "tea".into(),
            when: AdhocWhen::In(chrono::TimeDelta::try_seconds(30).unwrap()),
        })
        .await
        .expect("accepted")
    else {
        panic!("create yields the created receipt");
    };
    let reminder = ReminderRef::Adhoc(alarm_id);

    // Fire at T0 + 30.
    advance_both(&h.clock, 31).await;
    eventually(&h.handle, |app| app.reminders.deliveries.len() == 1).await;
    assert_eq!(drain_notifications(&mut h.notifications).len(), 1);

    // Snooze the FIRED occurrence: the engine stamps target =
    // now + 10 m (the clock is at T0 + 31), accepted against the live row.
    let target = ts(T0 + 31 + 600);
    let outcome = h
        .handle
        .execute(StateCommand::SnoozeReminder {
            reminder,
            occurrence: ts(T0 + 30),
            postpone: chrono::TimeDelta::try_minutes(10).unwrap(),
        })
        .await
        .expect("accepted");
    assert_eq!(outcome, CommandOutcome::Applied);

    // Quiet below the target: still exactly the first delivery.
    advance_both(&h.clock, 560).await;
    h.handle.flush().await;
    let app = h.handle.shared_state().load_full();
    assert_eq!(app.reminders.deliveries.len(), 1, "quiet below the target");
    assert!(app.reminders.alarms.contains_key(&alarm_id));
    drop(app);

    // The re-fire carries the rescheduled occurrence; the row survives.
    advance_both(&h.clock, 60).await;
    eventually(&h.handle, |app| app.reminders.deliveries.len() == 2).await;
    let notes = drain_notifications(&mut h.notifications);
    assert_eq!(notes.len(), 1, "exactly the re-fire notification");
    assert_eq!(notes[0].occurrence, target);
    let app = h.handle.shared_state().load_full();
    assert!(app.reminders.alarms.contains_key(&alarm_id));
    drop(app);

    // The dismissal settles it: pruned by the same batch's post-pass.
    let outcome = h
        .handle
        .execute(StateCommand::DismissReminder {
            reminder,
            occurrence: target,
        })
        .await
        .expect("accepted");
    assert_eq!(outcome, CommandOutcome::Applied);
    eventually(&h.handle, |app| {
        app.reminders.alarms.is_empty() && app.reminders.deliveries.is_empty()
    })
    .await;
}

/// R4: drain disarms the timer — a due reminder that arrives during the
/// shutdown drain never fires (not merely ignored; the sleep future is
/// never constructed again).
#[cfg_attr(miri, ignore)]
#[tokio::test(start_paused = true)]
async fn r4_drain_silences_the_timer() {
    // Due in the future: the first (immediate) tick must NOT fire it, so
    // the test isolates the drain window.
    let (state, _) = seeded_task_state(T0 + 3_600);
    let mut h = spawn(Spawner {
        repo: Arc::new(InMemoryRepository::with_state(state)),
        ..Spawner::default()
    })
    .await;
    h.system_tx.send(SystemEvent::Shutdown).expect("broadcast");
    wait_draining(&h.handle).await;

    advance_both(&h.clock, 7_200).await;
    h.handle.flush().await;
    let app = h.handle.shared_state().load_full();
    assert!(
        app.reminders.deliveries.is_empty(),
        "the disarmed timer never fires the due reminder"
    );
    assert_eq!(
        drain_notifications(&mut h.notifications),
        [] as [Notification; 0]
    );
}

/// R5: a closed notifier channel does not block the delivery — the
/// marker lands, the engine keeps serving commands (headless configs are
/// legal).
#[cfg_attr(miri, ignore)]
#[tokio::test(start_paused = true)]
async fn r5_closed_notifier_channel_still_delivers() {
    let h = harness().await;
    drop(h.notifications); // the sink consumer is gone before any tick

    advance_both(&h.clock, 1).await;
    eventually(&h.handle, |app| !app.reminders.deliveries.is_empty()).await;

    // The engine is alive and answering.
    let outcome = h
        .handle
        .execute(StateCommand::RequestSync)
        .await
        .expect("engine still answers");
    assert!(matches!(
        outcome,
        CommandOutcome::NoOp | CommandOutcome::SyncRequested
    ));
}

/// R6: durable-then-notify, end to end — a failed delivery batch fires
/// nothing (no marker, no notification), surfaces the edge-triggered
/// `StorageFailed` broadcast, and the next tick fires normally once the
/// injector is exhausted.
#[cfg_attr(miri, ignore)]
#[tokio::test(start_paused = true)]
async fn r6_storage_failure_fires_nothing_and_recovers() {
    let (state, _) = seeded_task_state(T0 - 3_600);
    let repo = Arc::new(FlakyRepository::new(1, state));
    let mut h = spawn(Spawner {
        repo: repo.clone() as Arc<dyn TaskRepository>,
        ..Spawner::default()
    })
    .await;
    let mut events = h.system_tx.subscribe();
    settle_boot_tick().await;
    h.handle.flush().await;
    let app = h.handle.shared_state().load_full();
    assert!(
        app.reminders.deliveries.is_empty(),
        "a failed batch must not record the delivery"
    );
    drop(app);
    assert_eq!(
        drain_notifications(&mut h.notifications),
        [] as [Notification; 0]
    );
    let mut fired = 0;
    while matches!(events.try_recv(), Ok(SystemEvent::StorageFailed)) {
        fired += 1;
    }
    assert_eq!(
        fired, 1,
        "one edge-triggered StorageFailed, not one per tick"
    );

    // The injector is exhausted: the next tick fires normally.
    advance_both(&h.clock, 2).await;
    h.handle.flush().await;
    eventually(&h.handle, |app| !app.reminders.deliveries.is_empty()).await;
    let notes = drain_notifications(&mut h.notifications);
    assert_eq!(notes.len(), 1, "the recovery tick notifies exactly once");
}

/// R7: a quiet tick is invisible — no due reminders means no publish and
/// no repaint signal (a tick is a no-op, not a repaint).
#[cfg_attr(miri, ignore)]
#[tokio::test(start_paused = true)]
async fn r7_quiet_ticks_are_invisible() {
    // Due in the future: every tick in the window is quiet.
    let (state, _) = seeded_task_state(T0 + 3_600);
    let h = spawn(Spawner {
        repo: Arc::new(InMemoryRepository::with_state(state)),
        ..Spawner::default()
    })
    .await;
    let mut signals = h.handle.subscribe();

    advance_both(&h.clock, 120).await;
    h.handle.flush().await;
    assert_eq!(
        drain_signals(&mut signals),
        0,
        "quiet ticks publish and signal nothing"
    );
}

/// R8 — the mandatory auto-advance guard: a *disabled* engine over a
/// past-due task performs zero timer work. On this paused runtime,
/// advancing far beyond many tick intervals must produce no delivery, no
/// notification, and no signal — if anyone flips a helper default to
/// enabled, this test fails loudly (the paused suites rely on the arm
/// not existing).
#[cfg_attr(miri, ignore)]
#[tokio::test(start_paused = true)]
async fn r8_disabled_engine_never_ticks_under_auto_advance() {
    let mut h = spawn(Spawner {
        reminders: ReminderEngineConfig::disabled(),
        ..Spawner::default()
    })
    .await;
    let mut signals = h.handle.subscribe();

    // Far beyond many (hypothetical) 1 s ticks; the auto-advance also
    // fast-forwards every idle await — a live timer would fire here.
    advance_both(&h.clock, 3_600).await;
    h.handle.flush().await;

    let app = h.handle.shared_state().load_full();
    assert!(app.reminders.deliveries.is_empty());
    assert_eq!(
        drain_notifications(&mut h.notifications),
        [] as [Notification; 0]
    );
    assert_eq!(drain_signals(&mut signals), 0);
}

/// R9: mute routing — the command lands (membership visible) and the
/// muted task never fires, even far past its due date.
#[cfg_attr(miri, ignore)]
#[tokio::test(start_paused = true)]
async fn r9_muted_task_never_fires() {
    // Due in the future so the first tick cannot fire before the mute.
    let (state, task_id) = seeded_task_state(T0 + 60);
    let mut h = spawn(Spawner {
        repo: Arc::new(InMemoryRepository::with_state(state)),
        ..Spawner::default()
    })
    .await;

    let outcome = h
        .handle
        .execute(StateCommand::SetTaskRemindersMuted {
            task: task_id,
            muted: true,
        })
        .await
        .expect("accepted");
    assert_eq!(
        outcome,
        CommandOutcome::ReminderMuted {
            task: task_id,
            muted: true
        }
    );
    let app = h.handle.shared_state().load_full();
    assert!(app.reminders.muted.contains(&task_id));
    drop(app);

    advance_both(&h.clock, 120).await;
    h.handle.flush().await;
    let app = h.handle.shared_state().load_full();
    assert!(
        app.reminders.deliveries.is_empty(),
        "a muted task contributes no firings"
    );
    assert_eq!(
        drain_notifications(&mut h.notifications),
        [] as [Notification; 0]
    );
}

// ---- The withdraw direction (R3, design D-10) ---------------------------

/// Waits for the next `Withdraw` on the notifier, advancing paused time
/// in 1 ms turns — the suite's deadline discipline. A command's receipt
/// implies its withdrawals are already in the channel (sent before the
/// reply), so this only ever waits on the boot tick's racing fire.
async fn next_withdraw(rx: &mut tokio::sync::mpsc::Receiver<NotifierMessage>) -> NotifierMessage {
    tokio::time::timeout(Duration::from_secs(60), async {
        loop {
            match rx.try_recv() {
                Ok(message @ NotifierMessage::Withdraw { .. }) => return message,
                Ok(_) => {}
                Err(_) => tokio::time::advance(Duration::from_millis(1)).await,
            }
        }
    })
    .await
    .expect("withdraw within the deadline")
}

/// The local `DismissReminder` lands one best-effort `Withdraw` on the
/// notifier channel, and the accepted no-op repeat lands nothing.
#[tokio::test(start_paused = true)]
async fn local_dismiss_withdraws_the_toast_once() {
    let mut h = harness().await;
    let task_id = {
        let app = h.handle.shared_state().load_full();
        *app.tasks.keys().next().expect("seeded task")
    };
    let occurrence = {
        let app = h.handle.shared_state().load_full();
        app.tasks
            .get(&task_id)
            .and_then(|t| t.duedate)
            .expect("seeded duedate")
    };
    settle_boot_tick().await;

    h.handle
        .execute(StateCommand::DismissReminder {
            reminder: ReminderRef::Default(task_id),
            occurrence,
        })
        .await
        .expect("accepted");
    h.handle.flush().await;
    // The command's receipt implies the withdraw is already in the
    // channel (sent before the reply); a pending boot-tick `Fire` may
    // sit ahead of it in the FIFO.
    let NotifierMessage::Withdraw {
        reminder,
        occurrence: withdrawn,
    } = next_withdraw(&mut h.notifications).await
    else {
        unreachable!("next_withdraw only returns Withdraw")
    };
    assert_eq!(reminder, ReminderRef::Default(task_id));
    assert_eq!(withdrawn, occurrence);

    // The repeat is the accepted no-op: no second withdraw (any pending
    // boot-tick fire is a `Fire`, never a `Withdraw`).
    h.handle
        .execute(StateCommand::DismissReminder {
            reminder: ReminderRef::Default(task_id),
            occurrence,
        })
        .await
        .expect("accepted no-op");
    h.handle.flush().await;
    while let Ok(message) = h.notifications.try_recv() {
        assert!(
            !matches!(message, NotifierMessage::Withdraw { .. }),
            "the no-op repeat withdraws nothing"
        );
    }
}

/// Mark Done's planner-emitted dismissal rows (the reopen hazard, R3.5.5)
/// withdraw through the same derivation — one `Withdraw` per active
/// occurrence, emitted from the applied batch.
#[tokio::test(start_paused = true)]
async fn mark_done_dismissals_withdraw_too() {
    let mut h = harness().await;
    let task_id = {
        let app = h.handle.shared_state().load_full();
        *app.tasks.keys().next().expect("seeded task")
    };
    settle_boot_tick().await;
    // The occurrence must have FIRED on this device before it is active
    // (the surface is fired ∧ not dismissed): one tick past the due
    // instant, exactly one fire notification.
    advance_both(&h.clock, 60).await;
    let fired = tokio::time::timeout(Duration::from_secs(60), async {
        loop {
            let drained = drain_notifications(&mut h.notifications);
            if !drained.is_empty() {
                return drained;
            }
            tokio::time::advance(Duration::from_millis(1)).await;
        }
    })
    .await
    .expect("the boot fire within the deadline");
    assert_eq!(fired.len(), 1, "the tick fires the overdue occurrence");

    h.handle
        .execute(StateCommand::SetTaskDone {
            id: task_id,
            done: true,
        })
        .await
        .expect("accepted");
    h.handle.flush().await;
    let NotifierMessage::Withdraw { reminder, .. } = next_withdraw(&mut h.notifications).await
    else {
        unreachable!("next_withdraw only returns Withdraw")
    };
    assert_eq!(reminder, ReminderRef::Default(task_id));
    while let Ok(message) = h.notifications.try_recv() {
        assert!(
            !matches!(message, NotifierMessage::Withdraw { .. }),
            "exactly one withdraw for the single active occurrence"
        );
    }
}
