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

use taskboard_domain::idgen::IdGenerator as _;
use taskboard_domain::persistence::{
    BoxFuture, PersistedState, PersistenceAction, RepositoryError, TaskRepository,
};
use taskboard_domain::test_support::{CountingIds, InMemoryRepository};
use taskboard_domain::{
    AppState, Board, CommandOutcome, EngineSignal, Notification, ReminderDefaults, ReminderOffset,
    ReminderRef, ReminderSnoozeKey, Stack, StackClocks, StateCommand, SyncCommand, SyncReport,
    SystemEvent, Task, TaskClocks,
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
        done: None,
        stack: stack.id,
        order: 1,
        labels: std::collections::BTreeSet::new(),
        archived: false,
        deleted: false,
        clocks: TaskClocks {
            title: ts(T0),
            description: ts(T0),
            duedate: ts(T0),
            done: ts(T0),
            position: ts(T0),
            labels: ts(T0),
            archived: ts(T0),
            deleted: ts(T0),
        },
        remote_seen: None,
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
    notifications: tokio::sync::mpsc::Receiver<Notification>,
}

async fn spawn(spawner: Spawner) -> Harness {
    let (sync_out, _sync_in) = tokio::sync::mpsc::channel::<SyncCommand>(8);
    let (_report_tx, report_rx) = tokio::sync::mpsc::channel::<SyncReport>(8);
    let (system_tx, system_rx) = tokio::sync::broadcast::channel::<SystemEvent>(8);
    let (notifier, notifications) = tokio::sync::mpsc::channel::<Notification>(64);
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

fn drain_notifications(rx: &mut tokio::sync::mpsc::Receiver<Notification>) -> Vec<Notification> {
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
            task: task_id,
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
