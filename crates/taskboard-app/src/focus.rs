// SPDX-License-Identifier: AGPL-3.0-only
//! The pomodoro focus-session runtime (issue #496): one app-layer
//! state-machine task beside the escalation driver that keeps a
//! focus-mode session moving — a work block hands off to a break, a
//! break ends into idle.
//!
//! **The session IS the open entry.** The runtime holds no bookkeeping
//! of its own: the entire machine is the pure [`focus_step`] over the
//! published open entry (`newest_open()`), the injected
//! [`FocusTiming`], and the injected clock's `now`. The deadline
//! anchors on the entry's `start`, never on runtime-remembered
//! instants — a foreign or replacement entry re-anchors the session for
//! free, and a restart mid-session re-derives everything from the
//! rehydrated row (ADR 0017's crash-safety property).
//!
//! **The session drives only the landed tracking verbs.** Transitions
//! dispatch `StateCommand::StopTracking` and
//! `StateCommand::StartTracking { kind: Respite }` — nothing else. A
//! parallel time-tracking path is the bug class the DR-5 fence exists
//! to prevent; starting a focus block is never the runtime's act (the
//! user starts it from any landed surface).
//!
//! The focus→break hand-off is the *two-verb* sequence
//! (`StopTracking` → `StartTracking { kind: Respite }`), awaited to
//! each receipt: the landed `StartTracking` idempotent no-op is
//! task-keyed and kind-blind, so a single kind-switching dispatch
//! while the work entry is open would be silently swallowed and the
//! break would never open.
//!
//! Fences (the escalation fence, verbatim in spirit): no engine change,
//! no new channel topology, no `SystemEvent`/`EngineSignal` variant —
//! the runtime reads the published-state `ArcSwap`, parks on the
//! existing `EngineSignal` broadcast, and arms a private
//! `tokio::time::sleep` only while a phase is live. Tokio time is the
//! *wakeup* mechanism, never a time *source*: every due computation
//! reads the injected [`Clock`] (no `Utc::now`, no `Instant::elapsed`).

use std::sync::Arc;

use chrono::{DateTime, Utc};
use tokio::sync::broadcast;

use taskboard_domain::focus::FocusTiming;
use taskboard_domain::{
    AppState, Clock, EngineSignal, TimeEntry, TimeEntryKind, ids::TaskId, messages::StateCommand,
};
use taskboard_state::EngineHandle;

/// One focus-mode action the runtime is due to take.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FocusAction {
    /// The work block is over: hand off to the break (the two-verb
    /// sequence — stop the work entry, open the respite entry).
    OpenBreak {
        /// The task the break entry tracks (the work entry's task).
        task: TaskId,
    },
    /// The break is over: close it (one verb; the session ends into
    /// idle — no auto-chain of the next focus, the user starts it).
    EndBreak,
}

/// What one `focus_step` evaluation is due to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FocusStep {
    /// The due actions (at most one per step).
    pub(crate) actions: Vec<FocusAction>,
    /// When the next transition is due; `None` ⇒ park on signals with
    /// no timer armed.
    pub(crate) next_due: Option<DateTime<Utc>>,
}

/// The whole state machine: one pure function over the observed open
/// entry, the timings, and now. `open` is the caller's
/// `newest_open()` pick (D-T8: a transient double-open is governed by
/// the newest entry exactly as every other reader).
///
/// | Observed open entry | Phase | Deadline | Due (`deadline ≤ now`) |
/// |---|---|---|---|
/// | `None` | Idle | — | no actions, `next_due: None` (park) |
/// | `Some(e)`, kind `Work` | Focus | `e.start + work` | `OpenBreak` |
/// | `Some(e)`, kind `Respite` | Break | `e.start + respite` | `EndBreak` |
#[must_use]
pub(crate) fn focus_step(
    open: Option<&TimeEntry>,
    timing: &FocusTiming,
    now: DateTime<Utc>,
) -> FocusStep {
    let Some(entry) = open else {
        return FocusStep {
            actions: Vec::new(),
            next_due: None,
        };
    };
    let (block, action) = match entry.kind {
        TimeEntryKind::Work => (
            timing.work(),
            Some(FocusAction::OpenBreak { task: entry.task }),
        ),
        // Break end closes the entry; the session ends into idle.
        TimeEntryKind::Respite => (timing.respite(), Some(FocusAction::EndBreak)),
    };
    let deadline = entry.start + block;
    if deadline <= now {
        FocusStep {
            actions: action.into_iter().collect(),
            next_due: None,
        }
    } else {
        FocusStep {
            actions: Vec::new(),
            next_due: Some(deadline),
        }
    }
}

/// Spawns the focus-session runtime: parks on the engine-signal
/// broadcast with no armed timer while no session is live; wakes on
/// state changes (or its own phase timer, armed only while a phase is
/// live) and steps the pure machine, firing due transitions through
/// [`EngineHandle::execute`]. Exits when the engine's broadcast closes
/// (the escalation semantics: `Lagged` counts as change).
///
/// The post-action wake derives from the command *receipt*, not from a
/// fresh state read — the reply can resolve before the engine's
/// `ArcSwap` store publishes, so a state-derived re-step could fire on
/// stale data. The receipt is a wakeup hint, never truth: every
/// signal-wakened step still re-derives from published state, so a
/// surprise between dispatch and wake is corrected at the next step,
/// never fought.
#[allow(clippy::too_many_lines)] // the park/live/fire loop, one flat shape
#[must_use]
pub(crate) fn spawn_focus_runtime(
    state: Arc<arc_swap::ArcSwap<AppState>>,
    timing: FocusTiming,
    engine: EngineHandle,
    mut signals: broadcast::Receiver<EngineSignal>,
    clock: Arc<dyn Clock>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        tracing::debug!("focus: runtime started");
        loop {
            let now = clock.now();
            let snapshot = state.load();
            let open = snapshot.tracking.newest_open();
            let step = focus_step(open, &timing, now);
            drop(snapshot);
            if step.actions.is_empty() {
                match step.next_due {
                    None => {
                        // Idle: park on the signal lane with NO timer
                        // armed.
                        match signals.recv().await {
                            Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => continue,
                            Err(broadcast::error::RecvError::Closed) => {
                                tracing::debug!("focus: signal channel closed, runtime exiting");
                                return;
                            }
                        }
                    }
                    Some(due) => {
                        // Live, not due: sleep until the deadline,
                        // interruptible by any state change.
                        tokio::select! {
                            () = tokio::time::sleep((due - now).to_std().unwrap_or_default()) => {}
                            r = signals.recv() => match r {
                                Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => {}
                                Err(broadcast::error::RecvError::Closed) => {
                                    tracing::debug!(
                                        "focus: signal channel closed, runtime exiting"
                                    );
                                    return;
                                }
                            },
                        }
                        continue;
                    }
                }
            }
            // Due: fire the actions, each awaited to its receipt.
            let mut wake: Option<DateTime<Utc>> = None;
            for action in step.actions {
                match action {
                    FocusAction::OpenBreak { task } => {
                        // Two verbs (the kind-blind no-op land-mine):
                        // stop the work entry, then open the respite
                        // entry on the same task.
                        match engine.execute(StateCommand::StopTracking).await {
                            Ok(_) => {}
                            Err(err) => {
                                tracing::warn!(%err, "focus: stop-tracking hand-off failed");
                                continue;
                            }
                        }
                        match engine
                            .execute(StateCommand::StartTracking {
                                task,
                                kind: TimeEntryKind::Respite,
                            })
                            .await
                        {
                            Ok(taskboard_domain::CommandOutcome::TrackingStarted {
                                entry, ..
                            }) => {
                                // The receipt's stamped entry is the
                                // break's anchor datum (publish-order
                                // race: never re-read state here).
                                wake = Some(entry.start + timing.respite());
                            }
                            Ok(outcome) => {
                                tracing::warn!(
                                    ?outcome,
                                    "focus: unexpected start-tracking receipt"
                                );
                            }
                            Err(err) => {
                                tracing::warn!(%err, "focus: start-break hand-off failed");
                            }
                        }
                    }
                    FocusAction::EndBreak => match engine.execute(StateCommand::StopTracking).await
                    {
                        // Break end is session end: park (the user
                        // starts the next focus — no auto-chain).
                        Ok(_) => {}
                        Err(err) => {
                            tracing::warn!(%err, "focus: break-end stop failed");
                        }
                    },
                }
            }
            match wake {
                Some(due) => {
                    let now = clock.now();
                    tokio::select! {
                        () = tokio::time::sleep((due - now).to_std().unwrap_or_default()) => {}
                        r = signals.recv() => match r {
                            Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => {}
                            Err(broadcast::error::RecvError::Closed) => {
                                tracing::debug!("focus: signal channel closed, runtime exiting");
                                return;
                            }
                        },
                    }
                }
                None => {
                    // Actions fired without a live next phase (or all
                    // failed): re-derive from published state at the
                    // next signal.
                    match signals.recv().await {
                        Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => {}
                        Err(broadcast::error::RecvError::Closed) => {
                            tracing::debug!("focus: signal channel closed, runtime exiting");
                            return;
                        }
                    }
                }
            }
        }
    })
}

/// The boot-time spawn decision (D-F3/D-F13): the runtime runs only
/// when focus mode is opted in **and** the run mode is not kiosk. A
/// kiosk is a passive appliance — no auto-transitions; its open
/// entries are display data. The overlay (a desktop affordance)
/// rides the same gate.
#[must_use]
pub(crate) fn focus_spawn_decision(
    mode: crate::config::AppMode,
    focus: Option<FocusTiming>,
) -> Option<FocusTiming> {
    if mode == crate::config::AppMode::Kiosk {
        return None;
    }
    focus
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeDelta;
    use std::sync::atomic::{AtomicI64, Ordering};
    use taskboard_domain::command::CommandOutcome;
    use taskboard_domain::test_support::{CountingIds, InMemoryRepository};

    const T0: DateTime<Utc> =
        chrono::DateTime::<Utc>::from_timestamp(1_792_000_000, 0).expect("valid instant");

    fn timing() -> FocusTiming {
        FocusTiming::try_new(TimeDelta::minutes(25), TimeDelta::minutes(5))
            .expect("positive timings")
    }

    fn entry(kind: TimeEntryKind, start: DateTime<Utc>) -> TimeEntry {
        TimeEntry {
            id: taskboard_domain::ids::TimeEntryId::from_uuid(uuid::Uuid::now_v7()),
            task: TaskId::from_uuid(uuid::Uuid::now_v7()),
            task_title: "t".into(),
            start,
            end: None,
            created_at: start,
            updated_at: start,
            kind,
        }
    }

    // -- pure focus_step ------------------------------------------------

    #[test]
    fn idle_parks_with_no_actions() {
        let step = focus_step(None, &timing(), T0);
        assert_eq!(step.actions, [] as [FocusAction; 0]);
        assert_eq!(step.next_due, None);
    }

    #[test]
    fn work_block_arms_its_deadline_and_fires_open_break_at_the_due() {
        let e = entry(TimeEntryKind::Work, T0);
        let early = focus_step(
            Some(&e),
            &timing(),
            T0 + TimeDelta::minutes(24) + TimeDelta::seconds(59),
        );
        assert!(early.actions.is_empty(), "one second early: nothing due");
        assert_eq!(early.next_due, Some(T0 + TimeDelta::minutes(25)));

        let due = focus_step(Some(&e), &timing(), T0 + TimeDelta::minutes(25));
        assert_eq!(
            due.actions,
            vec![FocusAction::OpenBreak { task: e.task }],
            "at the due instant: exactly the hand-off"
        );
        assert_eq!(due.next_due, None);
    }

    #[test]
    fn respite_block_arms_its_deadline_and_fires_end_break_at_the_due() {
        let e = entry(TimeEntryKind::Respite, T0);
        let early = focus_step(Some(&e), &timing(), T0 + TimeDelta::minutes(4));
        assert_eq!(early.actions, [] as [FocusAction; 0]);
        assert_eq!(early.next_due, Some(T0 + TimeDelta::minutes(5)));

        let due = focus_step(Some(&e), &timing(), T0 + TimeDelta::minutes(5));
        assert_eq!(due.actions, vec![FocusAction::EndBreak]);
        assert_eq!(due.next_due, None);
    }

    #[test]
    fn the_newest_open_entry_governs_the_phase() {
        let old = entry(TimeEntryKind::Work, T0);
        let newer = entry(TimeEntryKind::Respite, T0 + TimeDelta::minutes(1));
        let state = taskboard_domain::TrackingState {
            entries: [old.clone(), newer.clone()]
                .into_iter()
                .map(|e| (e.id, e))
                .collect(),
        };
        let open = state.newest_open().expect("an open entry");
        assert_eq!(open.id, newer.id, "the newer row governs");
        let step = focus_step(Some(open), &timing(), T0 + TimeDelta::minutes(3));
        assert!(step.actions.is_empty(), "the newer respite is not due yet");
        assert_eq!(step.next_due, Some(newer.start + TimeDelta::minutes(5)));
    }

    #[test]
    fn foreign_work_entry_re_anchors() {
        // The deadline anchors on the entry's start: a later-started
        // foreign work entry moves the deadline, forgetting the old one.
        let foreign = entry(TimeEntryKind::Work, T0 + TimeDelta::minutes(10));
        let step = focus_step(Some(&foreign), &timing(), T0 + TimeDelta::minutes(25));
        assert!(step.actions.is_empty(), "not due at the *old* deadline");
        assert_eq!(step.next_due, Some(T0 + TimeDelta::minutes(35)));
    }

    #[test]
    fn a_stale_entry_reconciles_once_at_the_due() {
        // Long past due at boot: exactly one reconciling action, no
        // cascade semantics in the step (the driver fires it at engine
        // now — never backdated).
        let stale = entry(TimeEntryKind::Work, T0 - TimeDelta::minutes(75));
        let step = focus_step(Some(&stale), &timing(), T0);
        assert_eq!(step.actions.len(), 1);
        assert_eq!(step.actions[0], FocusAction::OpenBreak { task: stale.task });
    }

    #[test]
    fn the_spawn_decision_excludes_kiosk_and_the_disabled_mode() {
        let t = Some(timing());
        assert_eq!(
            focus_spawn_decision(crate::config::AppMode::Kiosk, t),
            None,
            "kiosk never spawns the runtime"
        );
        assert_eq!(
            focus_spawn_decision(crate::config::AppMode::Desktop, None),
            None,
            "disabled never spawns"
        );
        assert_eq!(
            focus_spawn_decision(crate::config::AppMode::Daemon, None),
            None,
            "disabled never spawns on the daemon either"
        );
        assert!(
            focus_spawn_decision(crate::config::AppMode::Desktop, t).is_some(),
            "desktop + enabled spawns"
        );
        assert!(
            focus_spawn_decision(crate::config::AppMode::Daemon, t).is_some(),
            "daemon + enabled spawns (R-7: the CLI observes via track status)"
        );
    }

    // -- the driver journeys (real engine, paused tokio time) ----------

    /// The fake clock: a nanos-timestamp cell the test drives (the
    /// escalation tests' shape). The *same* Arc feeds the engine and
    /// the runtime, so engine stamps and runtime dues share one time.
    #[derive(Debug, Default)]
    struct FakeClock(AtomicI64);

    impl Clock for FakeClock {
        fn now(&self) -> DateTime<Utc> {
            chrono::DateTime::<Utc>::from_timestamp(self.0.load(Ordering::SeqCst), 0)
                .expect("valid fake instant")
        }
    }

    impl FakeClock {
        fn set(&self, t: DateTime<Utc>) {
            self.0.store(t.timestamp(), Ordering::SeqCst);
        }
    }

    /// The journeys' harness: a real engine over an in-memory repo,
    /// reminder/day-roll disabled so no engine timer interferes, and
    /// the shared fake clock. Returns (engine, state Arc clone, clock).
    async fn harness() -> (
        EngineHandle,
        Arc<arc_swap::ArcSwap<AppState>>,
        Arc<FakeClock>,
    ) {
        let clock = Arc::new(FakeClock::default());
        clock.set(T0);
        let repo: Arc<dyn taskboard_domain::persistence::TaskRepository> =
            Arc::new(InMemoryRepository::default());
        let (sync_out, _sync_in) = tokio::sync::mpsc::channel(8);
        let (report_tx, report_rx) = tokio::sync::mpsc::channel(8);
        let (system_tx, system_rx) = tokio::sync::broadcast::channel(8);
        let (notifier, _notifications) = tokio::sync::mpsc::channel(64);
        let (engine, _engine_join) = taskboard_state::spawn_state_engine(
            repo,
            Arc::new(CountingIds::new()),
            clock.clone() as Arc<dyn Clock>,
            sync_out,
            report_rx,
            system_tx.clone(),
            system_rx,
            notifier,
            taskboard_state::ReminderEngineConfig::disabled(),
            taskboard_state::DayRollConfig::disabled(),
        )
        .await
        .expect("engine boots");
        // Keep a reports sender clone? The engine's reports receiver is
        // permanent; the parked `report_tx` above stays alive in this
        // scope for the engine's lifetime — fine for a journey.
        let _ = report_tx;
        let state = engine.shared_state();
        (engine, state, clock)
    }

    /// The advance-both lockstep: the fake wall clock moves, the
    /// paused tokio timer moves with it.
    async fn tick(clock: &FakeClock, to: DateTime<Utc>) {
        let delta = (to - clock.now()).to_std().expect("positive delta");
        clock.set(to);
        tokio::time::advance(delta).await;
        tokio::task::yield_now().await;
    }

    /// A live task to track: captured into the inbox (never errors on
    /// the happy path).
    async fn live_task(engine: &EngineHandle) -> TaskId {
        match engine
            .execute(StateCommand::CaptureTask {
                title: "focus target".into(),
                board: None,
            })
            .await
            .expect("capture works")
        {
            CommandOutcome::CreatedTask(task) => task,
            outcome => panic!("expected CreatedTask, got {outcome:?}"),
        }
    }

    fn open_entries(state: &AppState) -> Vec<&TimeEntry> {
        let mut open: Vec<&TimeEntry> = state.tracking.open_entries().collect();
        open.sort_by_key(|e| e.start);
        open
    }

    #[tokio::test(start_paused = true)]
    async fn a_focus_session_hands_off_to_the_break_at_the_deadline() {
        let (engine, state, clock) = harness().await;
        let timing = timing();
        let task = live_task(&engine).await;
        let runtime = spawn_focus_runtime(
            Arc::clone(&state),
            timing,
            engine.clone(),
            engine.subscribe(),
            clock.clone() as Arc<dyn Clock>,
        );

        // The user starts the focus block (never the runtime's act).
        engine
            .execute(StateCommand::StartTracking {
                task,
                kind: TimeEntryKind::Work,
            })
            .await
            .expect("start works");
        // The runtime sees the new open entry on its signal wake.
        tokio::time::advance(std::time::Duration::from_secs(1)).await;
        tokio::task::yield_now().await;
        assert_eq!(
            open_entries(&state.load()).len(),
            1,
            "the work entry is open"
        );

        // Advance past the work deadline: the hand-off fires.
        tick(&clock, T0 + TimeDelta::minutes(25) + TimeDelta::seconds(5)).await;
        let snapshot = state.load();
        let open = open_entries(&snapshot);
        assert_eq!(open.len(), 1, "exactly one entry open after the hand-off");
        assert_eq!(
            open[0].kind,
            TimeEntryKind::Respite,
            "the open entry is the break"
        );
        assert_eq!(open[0].task, task, "the break tracks the same task");
        // And the work entry is closed, not deleted.
        let closed_work = state
            .load()
            .tracking
            .entries
            .values()
            .filter(|e| e.kind == TimeEntryKind::Work)
            .all(|e| e.end.is_some());
        assert!(closed_work, "the work entry is closed");

        runtime.abort();
        let _ = runtime.await;
    }

    #[tokio::test(start_paused = true)]
    async fn the_break_ends_into_idle_at_its_deadline() {
        let (engine, state, clock) = harness().await;
        let timing = timing();
        let task = live_task(&engine).await;
        let runtime = spawn_focus_runtime(
            Arc::clone(&state),
            timing,
            engine.clone(),
            engine.subscribe(),
            clock.clone() as Arc<dyn Clock>,
        );
        engine
            .execute(StateCommand::StartTracking {
                task,
                kind: TimeEntryKind::Work,
            })
            .await
            .expect("start works");
        tokio::time::advance(std::time::Duration::from_secs(1)).await;
        tokio::task::yield_now().await;
        tick(&clock, T0 + TimeDelta::minutes(25) + TimeDelta::seconds(5)).await;
        assert_eq!(open_entries(&state.load()).len(), 1, "the break is open");

        tick(&clock, T0 + TimeDelta::minutes(30) + TimeDelta::seconds(5)).await;
        assert!(
            open_entries(&state.load()).is_empty(),
            "the break ended into idle"
        );

        // The runtime parks: further advances change nothing.
        tick(&clock, T0 + TimeDelta::minutes(120)).await;
        assert!(
            open_entries(&state.load()).is_empty(),
            "no auto-chain: idle stays idle"
        );

        runtime.abort();
        let _ = runtime.await;
    }

    #[tokio::test(start_paused = true)]
    async fn an_externally_stopped_session_is_never_reopened() {
        let (engine, state, clock) = harness().await;
        let timing = timing();
        let task = live_task(&engine).await;
        let runtime = spawn_focus_runtime(
            Arc::clone(&state),
            timing,
            engine.clone(),
            engine.subscribe(),
            clock.clone() as Arc<dyn Clock>,
        );
        engine
            .execute(StateCommand::StartTracking {
                task,
                kind: TimeEntryKind::Work,
            })
            .await
            .expect("start works");
        tokio::time::advance(std::time::Duration::from_secs(1)).await;
        tokio::task::yield_now().await;

        // Mid-focus the user stops from any landed surface.
        engine
            .execute(StateCommand::StopTracking)
            .await
            .expect("stop works");
        tokio::time::advance(std::time::Duration::from_secs(1)).await;
        tokio::task::yield_now().await;

        // Advance well past the deadline: the runtime follows state —
        // idle, park, never re-open.
        tick(&clock, T0 + TimeDelta::minutes(60)).await;
        assert!(
            open_entries(&state.load()).is_empty(),
            "the entry is truth: an externally stopped session stays stopped"
        );

        runtime.abort();
        let _ = runtime.await;
    }

    #[tokio::test(start_paused = true)]
    async fn a_foreign_start_re_anchors_the_session() {
        let (engine, state, clock) = harness().await;
        let timing = timing();
        let task_a = live_task(&engine).await;
        let task_b = live_task(&engine).await;
        let runtime = spawn_focus_runtime(
            Arc::clone(&state),
            timing,
            engine.clone(),
            engine.subscribe(),
            clock.clone() as Arc<dyn Clock>,
        );
        engine
            .execute(StateCommand::StartTracking {
                task: task_a,
                kind: TimeEntryKind::Work,
            })
            .await
            .expect("start works");
        tokio::time::advance(std::time::Duration::from_secs(1)).await;
        tokio::task::yield_now().await;

        // Mid-focus the user starts task B (single-global intent: A
        // closes, B opens). The new anchor is B's start.
        tick(&clock, T0 + TimeDelta::minutes(10)).await;
        engine
            .execute(StateCommand::StartTracking {
                task: task_b,
                kind: TimeEntryKind::Work,
            })
            .await
            .expect("start works");
        tokio::time::advance(std::time::Duration::from_secs(1)).await;
        tokio::task::yield_now().await;

        // Task A's original deadline passes: nothing fires — the old
        // deadline is forgotten.
        tick(&clock, T0 + TimeDelta::minutes(26)).await;
        let snapshot = state.load();
        let open = open_entries(&snapshot);
        assert_eq!(open.len(), 1, "still exactly task B's work entry");
        assert_eq!(open[0].task, task_b);
        assert_eq!(open[0].kind, TimeEntryKind::Work, "no premature break");

        // B's own deadline (start 10m + 25m) fires the break on B.
        tick(&clock, T0 + TimeDelta::minutes(36)).await;
        let snapshot = state.load();
        let open = open_entries(&snapshot);
        assert_eq!(open.len(), 1);
        assert_eq!(open[0].task, task_b, "the break re-anchored to B");
        assert_eq!(open[0].kind, TimeEntryKind::Respite);

        runtime.abort();
        let _ = runtime.await;
    }

    #[tokio::test(start_paused = true)]
    async fn a_manual_respite_derives_the_break_phase() {
        let (engine, state, clock) = harness().await;
        let timing = timing();
        let task = live_task(&engine).await;
        let runtime = spawn_focus_runtime(
            Arc::clone(&state),
            timing,
            engine.clone(),
            engine.subscribe(),
            clock.clone() as Arc<dyn Clock>,
        );
        // The user opens a respite manually (the CLI's `--kind respite`).
        engine
            .execute(StateCommand::StartTracking {
                task,
                kind: TimeEntryKind::Respite,
            })
            .await
            .expect("start works");
        tokio::time::advance(std::time::Duration::from_secs(1)).await;
        tokio::task::yield_now().await;

        // Not due at the respite's own deadline, one second before it.
        tick(&clock, T0 + TimeDelta::minutes(4) + TimeDelta::seconds(59)).await;
        assert_eq!(
            open_entries(&state.load()).len(),
            1,
            "the manual respite arms at its own length, not the work length"
        );
        tick(&clock, T0 + TimeDelta::minutes(5) + TimeDelta::seconds(5)).await;
        assert!(
            open_entries(&state.load()).is_empty(),
            "the break ends at its own deadline"
        );

        runtime.abort();
        let _ = runtime.await;
    }

    #[allow(clippy::too_many_lines)] // the seed fixture dominates the count
    #[tokio::test(start_paused = true)]
    async fn a_stale_entry_reconciles_exactly_once_at_boot() {
        // Seed a repo with an open work entry started 3 × work ago, then
        // spawn the runtime: exactly one hand-off fires at engine now —
        // no backdating, no cascade of missed cycles.
        let clock = Arc::new(FakeClock::default());
        clock.set(T0);
        // The stale entry's task must be a live row — StartTracking on
        // an unknown task is a typed rejection (the planner's law).
        let epoch = taskboard_domain::BoardClocks::epoch();
        let task = taskboard_domain::entities::Task {
            id: TaskId::from_uuid(uuid::Uuid::now_v7()),
            remote: None,
            title: "stale session's task".into(),
            description: String::new(),
            duedate: None,
            scheduled: None,
            done: None,
            stack: taskboard_domain::ids::StackId::from_uuid(uuid::Uuid::now_v7()),
            order: 1,
            label_memberships: std::collections::BTreeMap::new(),
            effort: None,
            importance: None,
            urgency: None,
            recur: None,
            kind: taskboard_domain::entities::TaskKind::Standard,
            archived: false,
            deleted: false,
            clocks: taskboard_domain::entities::TaskClocks {
                title: epoch,
                description: epoch,
                duedate: epoch,
                scheduled: epoch,
                done: epoch,
                position: epoch,
                effort: epoch,
                importance: epoch,
                urgency: epoch,
                recur: epoch,
                kind: epoch,
                archived: epoch,
                deleted: epoch,
            },
            remote_seen: None,
            remote_baseline: None,
        };
        let task_id = task.id;
        let mut stale = entry(TimeEntryKind::Work, T0 - TimeDelta::minutes(75));
        stale.task = task_id;
        stale.task_title = task.title.clone();
        let state_seed = taskboard_domain::PersistedState {
            tasks: [(task.id, task)].into_iter().collect(),
            tracking: taskboard_domain::TrackingState {
                entries: [(stale.id, stale)].into_iter().collect(),
            },
            ..taskboard_domain::PersistedState::default()
        };
        let repo: Arc<dyn taskboard_domain::persistence::TaskRepository> =
            Arc::new(InMemoryRepository::with_state(state_seed));
        let (sync_out, _sync_in) = tokio::sync::mpsc::channel(8);
        let (report_tx, report_rx) = tokio::sync::mpsc::channel(8);
        let (system_tx, system_rx) = tokio::sync::broadcast::channel(8);
        let (notifier, _notifications) = tokio::sync::mpsc::channel(64);
        let (engine, _engine_join) = taskboard_state::spawn_state_engine(
            repo,
            Arc::new(CountingIds::new()),
            clock.clone() as Arc<dyn Clock>,
            sync_out,
            report_rx,
            system_tx.clone(),
            system_rx,
            notifier,
            taskboard_state::ReminderEngineConfig::disabled(),
            taskboard_state::DayRollConfig::disabled(),
        )
        .await
        .expect("engine boots");
        let _ = report_tx;
        let published = engine.shared_state();
        let runtime = spawn_focus_runtime(
            Arc::clone(&published),
            timing(),
            engine.clone(),
            engine.subscribe(),
            clock.clone() as Arc<dyn Clock>,
        );
        tokio::time::advance(std::time::Duration::from_secs(1)).await;
        tokio::task::yield_now().await;

        // Exactly one respite entry exists, stamped at engine now (no
        // backdating); the stale entry closed at engine now.
        let state_now = published.load();
        let respite: Vec<&TimeEntry> = state_now
            .tracking
            .entries
            .values()
            .filter(|e| e.kind == TimeEntryKind::Respite)
            .collect();
        assert_eq!(respite.len(), 1, "exactly one break ever opened");
        assert_eq!(
            respite[0].start, T0,
            "the break's anchor is engine now, never backdated"
        );
        let work = state_now
            .tracking
            .entries
            .values()
            .find(|e| e.kind == TimeEntryKind::Work)
            .expect("the stale work row survives");
        assert_eq!(work.end, Some(T0), "the stale entry closed at engine now");

        // The reconciled break then follows the normal cadence.
        tick(&clock, T0 + TimeDelta::minutes(5) + TimeDelta::seconds(5)).await;
        assert!(open_entries(&published.load()).is_empty(), "break ends");

        runtime.abort();
        let _ = runtime.await;
    }

    #[tokio::test(start_paused = true)]
    async fn the_hand_off_cannot_double_open_a_break() {
        // Structural terminal-state pin over the receipt/publish race
        // window: across the hand-off (and an extra signal wake racing
        // it), exactly one Respite row ever exists (correctness, not a
        // ceiling).
        let (engine, state, clock) = harness().await;
        let timing = timing();
        let task = live_task(&engine).await;
        let runtime = spawn_focus_runtime(
            Arc::clone(&state),
            timing,
            engine.clone(),
            engine.subscribe(),
            clock.clone() as Arc<dyn Clock>,
        );
        engine
            .execute(StateCommand::StartTracking {
                task,
                kind: TimeEntryKind::Work,
            })
            .await
            .expect("start works");
        tokio::time::advance(std::time::Duration::from_secs(1)).await;
        tokio::task::yield_now().await;

        // Drive the race window: spam state-change signals while the
        // deadline crosses; every interleaved stop is a no-op against
        // engine truth and the start opens exactly one break.
        for i in 0..8 {
            tick(&clock, T0 + TimeDelta::minutes(25) + TimeDelta::seconds(i)).await;
        }
        let respites = state
            .load()
            .tracking
            .entries
            .values()
            .filter(|e| e.kind == TimeEntryKind::Respite)
            .count();
        assert_eq!(
            respites, 1,
            "exactly one break row exists across the raced hand-off"
        );
        assert_eq!(open_entries(&state.load()).len(), 1);

        runtime.abort();
        let _ = runtime.await;
    }

    #[tokio::test]
    async fn the_runtime_exits_when_the_broadcast_closes() {
        let state = Arc::new(arc_swap::ArcSwap::from_pointee(AppState::default()));
        let clock = Arc::new(FakeClock::default());
        // The only sender died at the binding: the broadcast is already
        // closed, so the runtime's first park answers `Closed` and the
        // task exits (the escalation Closed-exit shape).
        let (_, rx) = broadcast::channel::<EngineSignal>(8);
        let engine = {
            // A closed-inbox handle is unreachable here; the runtime
            // parks before ever touching the engine, so a placeholder
            // sender pair is enough. Build a real engine-free handle by
            // spawning nothing — instead reuse the minimal channel set.
            // The EngineHandle's fields are private, so the journey
            // harness builds one real engine.
            let repo: Arc<dyn taskboard_domain::persistence::TaskRepository> =
                Arc::new(InMemoryRepository::default());
            let (sync_out, _sync_in) = tokio::sync::mpsc::channel(8);
            let (report_tx, report_rx) = tokio::sync::mpsc::channel(8);
            let (system_tx, system_rx) = tokio::sync::broadcast::channel(8);
            let (notifier, _notifications) = tokio::sync::mpsc::channel(64);
            let (engine, _join) = taskboard_state::spawn_state_engine(
                repo,
                Arc::new(CountingIds::new()),
                clock.clone() as Arc<dyn Clock>,
                sync_out,
                report_rx,
                system_tx.clone(),
                system_rx,
                notifier,
                taskboard_state::ReminderEngineConfig::disabled(),
                taskboard_state::DayRollConfig::disabled(),
            )
            .await
            .expect("engine boots");
            let _ = report_tx;
            let _ = system_tx;
            engine
        };
        let runtime = spawn_focus_runtime(state, timing(), engine, rx, clock as Arc<dyn Clock>);
        tokio::time::timeout(std::time::Duration::from_secs(1), runtime)
            .await
            .expect("the runtime exits on Closed")
            .expect("clean exit");
    }
}
