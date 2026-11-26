// SPDX-License-Identifier: AGPL-3.0-only
//! The reminder sound semantics journey tests (#1438): the kind →
//! sound contract pinned at the `ChimeSink` seam — once-on-firing for
//! every `Insistence::Normal` firing (US-16, US-17, US-19), the
//! continuous-until-dismissed `Alarm` ladder regression pin (US-18),
//! and the kiosk `DisabledChime` mount (US-20). All assertions are
//! typed call-log facts on fakes; the clock is the injected
//! `ManualClock` under tokio's paused time (no sleeps, no wall reads).

use std::sync::Arc;

use chrono::{DateTime, TimeDelta, Utc};
use tokio::sync::broadcast;

use taskboard_app::config::AppMode;
use taskboard_app::escalation::{
    self, AudioError, CHIME_EVERY, CHIME_FIRST, CHIME_HI, CHIME_LO, ChimeSink,
    ONE_SHOT_CHIME_VOLUME,
};
use taskboard_app::notifications::{NotificationSink, NotifyError, SinkKind};
use taskboard_domain::{EngineSignal, Insistence, Notification, ReminderRef};

const T0: DateTime<Utc> =
    chrono::DateTime::<Utc>::from_timestamp(1_792_000_000, 0).expect("valid instant");

fn utc() -> chrono::FixedOffset {
    chrono::FixedOffset::east_opt(0).expect("zero offset")
}

// -- the fakes ---------------------------------------------------------

/// The recording chime fake (the #1307 seam): a mutex Vec of requested
/// volumes — the typed trigger record, never audio output.
#[derive(Default)]
struct RecordingChime {
    calls: std::sync::Mutex<Vec<f32>>,
}

impl std::fmt::Debug for RecordingChime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RecordingChime")
    }
}

impl ChimeSink for RecordingChime {
    fn chime(&self, volume: f32) -> Result<(), AudioError> {
        self.calls.lock().expect("chime records").push(volume);
        Ok(())
    }
}

/// The recording notification sink: ordered `(deliver | withdraw,
/// reminder, occurrence)` records — the toast channel's observable.
#[derive(Default)]
struct FakeSink {
    records: std::sync::Mutex<Vec<(&'static str, ReminderRef, DateTime<Utc>)>>,
}

impl std::fmt::Debug for FakeSink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("FakeSink")
    }
}

impl NotificationSink for FakeSink {
    fn deliver<'a>(
        &'a self,
        notification: &'a Notification,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), NotifyError>> + Send + 'a>>
    {
        Box::pin(async move {
            self.records.lock().expect("sink records").push((
                "deliver",
                notification.reminder,
                notification.occurrence,
            ));
            Ok(())
        })
    }

    fn withdraw<'a>(
        &'a self,
        reminder: &'a ReminderRef,
        occurrence: &'a DateTime<Utc>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), NotifyError>> + Send + 'a>>
    {
        Box::pin(async move {
            self.records
                .lock()
                .expect("sink records")
                .push(("withdraw", *reminder, *occurrence));
            Ok(())
        })
    }
}

fn chime_calls(chime: &RecordingChime) -> Vec<f32> {
    chime.calls.lock().expect("chime records").clone()
}

fn deliver_count(sink: &FakeSink) -> usize {
    sink.records
        .lock()
        .expect("sink records")
        .iter()
        .filter(|(kind, _, _)| *kind == "deliver")
        .count()
}

// -- the fixtures ------------------------------------------------------

/// A delivered, un-dismissed `Normal` ad-hoc timer (an
/// [`ReminderRef::Adhoc`] alarm row with `Insistence::Normal`).
fn timer_state() -> taskboard_domain::AppState {
    let id = taskboard_domain::AdhocAlarmId::from_uuid(uuid::Uuid::now_v7());
    let mut state = taskboard_domain::AppState::default();
    state.reminders.alarms.insert(
        id,
        taskboard_domain::AdhocAlarm {
            insistence: Insistence::Normal,
            id,
            label: "tea".to_owned(),
            fire_at: T0,
            created_at: T0 - TimeDelta::hours(1),
        },
    );
    state
        .reminders
        .deliveries
        .insert(taskboard_domain::ReminderDelivery {
            reminder: ReminderRef::Adhoc(id),
            occurrence: T0,
            delivered_at: T0,
        });
    state
}

/// A delivered, un-dismissed break-anchored `Normal` reminder: one
/// open work span from `T0 - 2h` against a 1 h break threshold crosses
/// at exactly `T0 - 1h`, and the delivery receipt settles that
/// occurrence.
fn break_state() -> taskboard_domain::AppState {
    let rule_id = taskboard_domain::BreakRuleId::from_uuid(uuid::Uuid::now_v7());
    let board = taskboard_domain::BoardId::from_uuid(uuid::Uuid::now_v7());
    let entry_id = taskboard_domain::TimeEntryId::from_uuid(uuid::Uuid::now_v7());
    let task_id = taskboard_domain::TaskId::from_uuid(uuid::Uuid::now_v7());
    let start = T0 - TimeDelta::hours(2);
    let occurrence = start + TimeDelta::hours(1); // the threshold crossing
    let mut state = taskboard_domain::AppState::default();
    state.tracking.entries.insert(
        entry_id,
        taskboard_domain::TimeEntry {
            id: entry_id,
            task: task_id,
            task_title: "focus".to_owned(),
            start,
            end: None,
            created_at: start,
            updated_at: start,
            kind: taskboard_domain::TimeEntryKind::Work,
            by: None,
        },
    );
    state.reminders.break_rules.insert(
        rule_id,
        taskboard_domain::BreakRule {
            id: rule_id,
            board,
            label: "take a break".to_owned(),
            threshold: taskboard_domain::WorkThreshold::try_new(TimeDelta::hours(1))
                .expect("whole-second threshold"),
            insistence: Insistence::Normal,
            created_at: start,
            by: None,
        },
    );
    state
        .reminders
        .deliveries
        .insert(taskboard_domain::ReminderDelivery {
            reminder: ReminderRef::Break(rule_id),
            occurrence,
            delivered_at: occurrence,
        });
    state
}

/// A delivered, un-dismissed task-anchored `Normal` reminder (an At
/// rule on a live, open, due-dated task), optionally muted — the
/// #1153 mute fact rides the task-shaped `reminders.muted` set, which
/// the domain derivation itself gates on.
fn task_rule_state(muted: bool) -> taskboard_domain::AppState {
    let task_id = taskboard_domain::TaskId::from_uuid(uuid::Uuid::now_v7());
    let rule_id = taskboard_domain::ReminderRuleId::from_uuid(uuid::Uuid::now_v7());
    let stack = taskboard_domain::StackId::from_uuid(uuid::Uuid::now_v7());
    let mut state = taskboard_domain::AppState::default();
    state.tasks.insert(
        task_id,
        taskboard_domain::Task {
            id: task_id,
            remote: None,
            title: "file the report".to_owned(),
            description: String::new(),
            duedate: Some(T0 + TimeDelta::hours(4)),
            scheduled: None,
            done: None,
            stack,
            order: 0,
            label_memberships: std::collections::BTreeMap::new(),
            effort: None,
            importance: None,
            urgency: None,
            recur: None,
            kind: taskboard_domain::entities::TaskKind::Standard,
            archived: false,
            deleted: false,
            clocks: taskboard_domain::TaskClocks {
                title: T0 - TimeDelta::hours(2),
                description: T0 - TimeDelta::hours(2),
                duedate: T0 - TimeDelta::hours(2),
                scheduled: T0 - TimeDelta::hours(2),
                done: T0 - TimeDelta::hours(2),
                position: T0 - TimeDelta::hours(2),
                effort: T0 - TimeDelta::hours(2),
                importance: T0 - TimeDelta::hours(2),
                urgency: T0 - TimeDelta::hours(2),
                recur: T0 - TimeDelta::hours(2),
                kind: T0 - TimeDelta::hours(2),
                archived: T0 - TimeDelta::hours(2),
                deleted: T0 - TimeDelta::hours(2),
            },
            authors: taskboard_domain::entities::TaskAuthors::default(),
            remote_seen: None,
            remote_baseline: None,
        },
    );
    state.reminders.rules.insert(
        rule_id,
        taskboard_domain::ReminderRule {
            id: rule_id,
            task: task_id,
            kind: taskboard_domain::ReminderKind::At(
                taskboard_domain::AtInstant::whole_second(T0).expect("whole-second instant"),
            ),
            insistence: Insistence::Normal,
            created_at: T0 - TimeDelta::hours(1),
            updated_at: T0 - TimeDelta::hours(1),
            by: None,
        },
    );
    state
        .reminders
        .deliveries
        .insert(taskboard_domain::ReminderDelivery {
            reminder: ReminderRef::Rule(rule_id),
            occurrence: T0,
            delivered_at: T0,
        });
    if muted {
        state.reminders.muted.insert(task_id);
    }
    state
}

/// An awaiting `Insistence::Alarm` ad-hoc alarm (the ladder's subject).
fn alarm_state() -> taskboard_domain::AppState {
    let id = taskboard_domain::AdhocAlarmId::from_uuid(uuid::Uuid::now_v7());
    let mut state = taskboard_domain::AppState::default();
    state.reminders.alarms.insert(
        id,
        taskboard_domain::AdhocAlarm {
            insistence: Insistence::Alarm,
            id,
            label: "hard stop".to_owned(),
            fire_at: T0,
            created_at: T0 - TimeDelta::hours(1),
        },
    );
    state
        .reminders
        .deliveries
        .insert(taskboard_domain::ReminderDelivery {
            reminder: ReminderRef::Adhoc(id),
            occurrence: T0,
            delivered_at: T0,
        });
    state
}

/// Spawns the driver over `state` with the given chime seam and a
/// recording toast sink; returns (state handle, signal sender, sink,
/// chime, clock, join handle).
#[allow(clippy::type_complexity)]
fn driver(
    state: taskboard_domain::AppState,
    chime: Arc<dyn ChimeSink>,
) -> (
    Arc<arc_swap::ArcSwap<taskboard_domain::AppState>>,
    broadcast::Sender<EngineSignal>,
    Arc<FakeSink>,
    Arc<taskboard_domain::test_support::ManualClock>,
    tokio::task::JoinHandle<()>,
) {
    let state = Arc::new(arc_swap::ArcSwap::from_pointee(state));
    let (signals_tx, _keepalive) = broadcast::channel::<EngineSignal>(64);
    let sink = Arc::new(FakeSink::default());
    let clock = Arc::new(taskboard_domain::test_support::ManualClock::default());
    clock.set(T0);
    let join = escalation::spawn_escalation_driver(
        Arc::clone(&state),
        utc(),
        Arc::clone(&sink) as Arc<dyn NotificationSink>,
        chime,
        signals_tx.subscribe(),
        clock.clone(),
    );
    (state, signals_tx, sink, clock, join)
}

/// Wakes the driver on a state change and lets one paused-time tick
/// elapse (the deterministic wake, never a sleep).
async fn wake(signals_tx: &broadcast::Sender<EngineSignal>) {
    signals_tx.send(EngineSignal::StateUpdated).expect("signal");
    tokio::time::advance(std::time::Duration::from_secs(1)).await;
    tokio::task::yield_now().await;
}

// -- US-16 -------------------------------------------------------------

/// US-16/S1 (AC-1): a break-anchored `Insistence::Normal` reminder
/// fires exactly one chime at its t₀, at the shared one-shot volume,
/// with the toast delivery unchanged.
#[tokio::test(start_paused = true)]
async fn us16_s1_normal_reminder_chimes_once_at_fire() {
    let occurrence = T0 - TimeDelta::hours(1); // the break fixture's crossing
    let state = break_state();
    let chime = Arc::new(RecordingChime::default());
    let (_state, signals_tx, sink, clock, join) =
        driver(state, Arc::clone(&chime) as Arc<dyn ChimeSink>);
    clock.set(occurrence);
    wake(&signals_tx).await;

    let calls = chime_calls(&chime);
    assert_eq!(calls.len(), 1, "exactly one chime at t₀: {calls:?}");
    assert_eq!(
        calls[0], ONE_SHOT_CHIME_VOLUME,
        "the shared one-shot volume (journey D-2)"
    );
    // The driver's own toast channel stays untouched for Normal
    // firings — the fire toast itself is the engine's E-20 delivery,
    // never a driver action (no re-toast is created for a Normal
    // firing).
    assert_eq!(
        deliver_count(&sink),
        0,
        "no re-toast rides the one-shot channel"
    );
    join.abort();
    let _ = join.await;
}

/// US-16/S2 (AC-2): no new firing, no further chime — once-on-firing,
/// no cadence, no ramp.
#[tokio::test(start_paused = true)]
async fn us16_s2_no_repeat_after_the_firing_tick() {
    let occurrence = T0 - TimeDelta::hours(1);
    let state = break_state();
    let chime = Arc::new(RecordingChime::default());
    let (_state, signals_tx, _sink, clock, join) =
        driver(state, Arc::clone(&chime) as Arc<dyn ChimeSink>);
    clock.set(occurrence);
    wake(&signals_tx).await;
    assert_eq!(chime_calls(&chime).len(), 1, "the t₀ chime");

    // Minutes of cadence time with no new firing: silent.
    clock.set(occurrence + TimeDelta::minutes(10));
    tokio::time::advance(std::time::Duration::from_secs(600)).await;
    tokio::task::yield_now().await;
    assert_eq!(
        chime_calls(&chime).len(),
        1,
        "no repetition after the firing tick"
    );
    join.abort();
    let _ = join.await;
}

/// US-16/S1 escape (AC-3): a degraded sink (the open failed at boot)
/// chimes silently and the toast still fires — degrade-to-visual.
#[tokio::test(start_paused = true)]
async fn us16_s3_degraded_sink_still_delivers_the_toast() {
    // The failing open: the typed degrade path mints DisabledChime.
    let chime = escalation::mount_chime(true, AppMode::Desktop, SinkKind::Desktop, || {
        Err(AudioError::Open(rodio_error_string()))
    });
    let occurrence = T0 - TimeDelta::hours(1);
    let state = break_state();
    let (_state, signals_tx, sink, clock, join) = driver(state, chime);
    clock.set(occurrence);
    wake(&signals_tx).await;

    // The degraded sink cannot fail the caller; the driver's toast
    // channel is unchanged (the fire toast is the engine's E-20
    // delivery, outside this seam) and no chime error escapes.
    assert_eq!(
        deliver_count(&sink),
        0,
        "no re-toast rides the one-shot channel"
    );
    join.abort();
    let _ = join.await;
}

/// The typed open-failure class the mount seam feeds (the rodio cause
/// string rides along; no rodio type leaks past this file).
fn rodio_error_string() -> String {
    "no default audio device".to_owned()
}

// -- US-17 -------------------------------------------------------------

/// US-17/S1 (AC-1): a muted reminder's firing makes no chime call (the
/// mute fact gates the domain derivation, so the awaiting set is
/// empty and the driver has nothing to chime).
#[tokio::test(start_paused = true)]
async fn us17_s1_muted_reminder_makes_no_chime_call() {
    let state = task_rule_state(true);
    let chime = Arc::new(RecordingChime::default());
    let (_state, signals_tx, _sink, clock, join) =
        driver(state, Arc::clone(&chime) as Arc<dyn ChimeSink>);
    clock.set(T0 + TimeDelta::minutes(1));
    wake(&signals_tx).await;
    tokio::time::advance(std::time::Duration::from_secs(60)).await;
    tokio::task::yield_now().await;
    assert!(
        chime_calls(&chime).is_empty(),
        "a muted reminder stays silent"
    );

    join.abort();
    let _ = join.await;
}

/// US-17/S2 (AC-2): the same reminder unmuted chimes exactly once at
/// its (still awaiting, receipt-bearing) occurrence.
#[tokio::test(start_paused = true)]
async fn us17_s2_unmuted_reminder_chimes_again() {
    let chime = Arc::new(RecordingChime::default());
    let (state, signals_tx, _sink, clock, join) = driver(
        task_rule_state(true),
        Arc::clone(&chime) as Arc<dyn ChimeSink>,
    );
    // Muted: silent.
    clock.set(T0 + TimeDelta::minutes(1));
    wake(&signals_tx).await;
    assert!(chime_calls(&chime).is_empty(), "muted stays silent");

    // Unmute: the same published occurrence re-enters the awaiting
    // derivation; the one-shot fires once.
    clock.set(T0 + TimeDelta::minutes(2));
    state.store(Arc::new(task_rule_state(false)));
    wake(&signals_tx).await;
    let calls = chime_calls(&chime);
    assert_eq!(calls.len(), 1, "unmute restores the chime: {calls:?}");
    assert_eq!(calls[0], ONE_SHOT_CHIME_VOLUME);
    join.abort();
    let _ = join.await;
}

// -- US-18 -------------------------------------------------------------

/// US-18/S1 (AC-1): the Alarm ladder regression pin — chime at
/// t₀+1 min, then every 30 s, volume ramping [`CHIME_LO`]→[`CHIME_HI`],
/// continuing while the alarm awaits.
#[tokio::test(start_paused = true)]
async fn us18_s1_alarm_ladder_continues_until_dismissed() {
    let state = alarm_state();
    let chime = Arc::new(RecordingChime::default());
    let (_state, signals_tx, _sink, clock, join) =
        driver(state, Arc::clone(&chime) as Arc<dyn ChimeSink>);

    // Arm, then walk the ladder: the opener at +1 min, cadence at
    // +1:30 and +2 min.
    clock.set(T0);
    wake(&signals_tx).await;
    assert!(chime_calls(&chime).is_empty(), "nothing before the opener");
    for (delta, _label) in [
        (CHIME_FIRST, "opener"),
        (CHIME_FIRST + CHIME_EVERY, "second"),
        (CHIME_FIRST + CHIME_EVERY * 2, "third"),
    ] {
        clock.set(T0 + delta);
        tokio::time::advance(std::time::Duration::from_secs(
            u64::try_from(delta.num_seconds()).expect("positive delta"),
        ))
        .await;
        tokio::task::yield_now().await;
    }
    let calls = chime_calls(&chime);
    assert_eq!(calls.len(), 3, "the landed ladder cadence: {calls:?}");
    // The ramp: rising volumes, inside [CHIME_LO, CHIME_HI].
    assert!(calls[0] >= CHIME_LO && calls[0] < CHIME_HI);
    assert!(calls[1] > calls[0], "the ramp rises: {calls:?}");
    assert!(calls[2] > calls[1], "the ramp rises: {calls:?}");
    assert!(*calls.last().expect("three calls") <= CHIME_HI);
    // The ladder keeps going while the alarm awaits.
    clock.set(T0 + CHIME_FIRST + CHIME_EVERY * 3);
    tokio::time::advance(std::time::Duration::from_secs(240)).await;
    tokio::task::yield_now().await;
    assert_eq!(
        chime_calls(&chime).len(),
        4,
        "continuous until dismissed — the fourth rung"
    );
    join.abort();
    let _ = join.await;
}

/// US-18/S2 (AC-2): the alarm dismissed — the firing leaves the
/// awaiting set and the next ticks make no further chime call.
#[tokio::test(start_paused = true)]
async fn us18_s2_dismissed_alarm_stops_the_ladder() {
    let chime = Arc::new(RecordingChime::default());
    let (state, signals_tx, _sink, clock, join) =
        driver(alarm_state(), Arc::clone(&chime) as Arc<dyn ChimeSink>);
    clock.set(T0 + CHIME_FIRST);
    wake(&signals_tx).await;
    let before = chime_calls(&chime).len();
    assert!(before >= 1, "the ladder opened");

    // Dismiss (the notification consumed): state back to empty.
    clock.set(T0 + CHIME_FIRST + CHIME_EVERY);
    state.store(Arc::new(taskboard_domain::AppState::default()));
    wake(&signals_tx).await;
    tokio::time::advance(std::time::Duration::from_secs(300)).await;
    tokio::task::yield_now().await;
    assert_eq!(
        chime_calls(&chime).len(),
        before,
        "no further chime after the dismissal"
    );
    join.abort();
    let _ = join.await;
}

// -- US-19 -------------------------------------------------------------

/// US-19/S1 (AC-1): the timer one-shot regression pin — exactly one
/// chime at completion t₀, no ladder.
#[tokio::test(start_paused = true)]
async fn us19_s1_timer_chime_stays_one_shot() {
    let state = timer_state();
    let chime = Arc::new(RecordingChime::default());
    let (_state, signals_tx, sink, clock, join) =
        driver(state, Arc::clone(&chime) as Arc<dyn ChimeSink>);
    clock.set(T0);
    wake(&signals_tx).await;

    let calls = chime_calls(&chime);
    assert_eq!(calls.len(), 1, "exactly one completion chime: {calls:?}");
    assert_eq!(calls[0], ONE_SHOT_CHIME_VOLUME);
    assert_eq!(
        deliver_count(&sink),
        0,
        "the one-shot channel is chime-only — no re-toast ladder"
    );
    clock.set(T0 + TimeDelta::minutes(5));
    tokio::time::advance(std::time::Duration::from_secs(300)).await;
    tokio::task::yield_now().await;
    assert_eq!(chime_calls(&chime).len(), 1, "no ladder follows");
    join.abort();
    let _ = join.await;
}

// -- US-20 -------------------------------------------------------------

/// US-20/S1 (AC-1): mode = Kiosk mounts the `DisabledChime` no-op —
/// the mount gate skips audio construction entirely (the open closure
/// is never called) and the returned sink cannot fail.
#[test]
fn us20_s1_kiosk_mounts_the_disabled_chime() {
    let chime = escalation::mount_chime(false, AppMode::Kiosk, SinkKind::Log, || {
        panic!("the kiosk gate must never construct an audio sink");
    });
    // The structural posture: the no-op answers Ok for any volume —
    // no rodio construction, no open attempt, silent by design.
    for volume in [0.0, CHIME_LO, ONE_SHOT_CHIME_VOLUME, CHIME_HI] {
        assert!(
            chime.chime(volume).is_ok(),
            "the mounted no-op never fails the caller at {volume}"
        );
    }
    // And the open-failure degrade at mount is the same shape.
    let degraded = escalation::mount_chime(true, AppMode::Desktop, SinkKind::Desktop, || {
        Err(AudioError::Open(rodio_error_string()))
    });
    assert!(degraded.chime(1.0).is_ok());
}
