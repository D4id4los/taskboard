// SPDX-License-Identifier: AGPL-3.0-only
//! The pomodoro focus-session runtime (issues #496, #600): one
//! app-layer state-machine task beside the escalation driver that
//! keeps a focus-mode session moving — a work block hands off to a
//! break, a break ends into the next focus block (the chain knob) or
//! into idle, and each runtime-driven boundary hands one typed event
//! to the injected [`FocusBoundarySink`].
//!
//! **The session IS the open entry.** The runtime's owned bookkeeping
//! is minimal by ruling (D-FF2): one slot —
//! [`FocusMemory::decided_long`], the long-break *decision* the
//! runtime itself made, which closed-entry history cannot recover when
//! the break closes early. The chain *position* is never stored: it is
//! re-derived from published entry history by the domain walk
//! (`taskboard_domain::focus::focus_work_position`) at every step, so
//! a restart mid-cycle re-derives the same position and the
//! double-count bug class does not exist. The deadline anchors on the
//! entry's `start`, never on runtime-remembered instants — a foreign
//! or replacement entry re-anchors the session for free.
//!
//! **The session drives only the landed tracking verbs.** Transitions
//! dispatch `StateCommand::StopTracking` and
//! `StateCommand::StartTracking { task, kind }` — nothing else. The
//! two-verb sequence is forced by the landed kind-blind no-op, now in
//! *both* directions: the focus→break hand-off (stop → start
//! `Respite`) and the break→focus auto-resume (stop → start `Work`).
//! The auto-resume fires from the break-end *receipt*, never from
//! observing idle state — an externally stopped break parks the
//! runtime in idle forever (never re-open, never chain; D-F16).
//!
//! **Boundary notifications ride the receipts.** Each completed
//! transition hands one typed [`FocusBoundary`] to the sink *after*
//! its receipt confirms it — at most one delivery per real transition,
//! even across the signal-spam window. The sink is infallible at the
//! call site by contract: implementations log-and-drop failures (the
//! escalation `apply` discipline). The OS plumbing lives in
//! `notifications.rs` (`mount_focus_boundaries`); this module owns the
//! seam and the gentle chime's volume constant.
//!
//! Fences (the escalation fence, verbatim in spirit): no engine change,
//! no new channel topology, no `SystemEvent`/`EngineSignal` variant —
//! the runtime reads the published-state `ArcSwap`, parks on the
//! existing `EngineSignal` broadcast, and arms a private
//! `tokio::time::sleep` only while a phase is live. Tokio time is the
//! *wakeup* mechanism, never a time *source*: every due computation
//! reads the injected [`Clock`] (no `Utc::now`, no `Instant::elapsed`).

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use tokio::sync::broadcast;

use taskboard_domain::focus::{FocusTiming, focus_break_is_long, focus_work_position};
use taskboard_domain::{
    AppState, Clock, CommandOutcome, EngineSignal, TimeEntryKind, TrackingState, ids::TaskId,
    messages::StateCommand,
};
use taskboard_state::EngineHandle;

/// The boundary chime's volume: the escalation ramp's floor
/// ([`crate::escalation::CHIME_LO`]) — one volume vocabulary, one
/// audio stream owner (D-4). A named constant, never config (the
/// escalation fence).
pub(crate) const FOCUS_CHIME_VOLUME: f32 = crate::escalation::CHIME_LO;

/// The focus runtime's owned bookkeeping (issue #600, D-FF2): one slot
/// holding the long-break *decision* the runtime itself made, which
/// closed-entry history cannot recover when the break closes early.
/// Chain position is never stored — it is re-derived by the domain
/// walk at every step. Deliberately **not** recovered across restart:
/// [`FocusMemory::default()`] at spawn, and the walk's duration
/// inference is the recovery rule (recorded leniency — after a
/// restart, an externally-aborted long break whose duration undercuts
/// `long_break` is inferred short, so the cycle may run long by up to
/// `long_break_every − 1` blocks until the next real long break).
#[derive(Debug, Default, PartialEq)]
pub(crate) struct FocusMemory {
    /// The id of the break entry the runtime *decided* was the cycle's
    /// long break. One slot, overwritten at the next long-break
    /// opening — an older decision is unreachable once a nearer
    /// boundary exists.
    decided_long: Option<taskboard_domain::ids::TimeEntryId>,
}

/// One runtime-driven focus-mode boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FocusBoundary {
    /// A focus block ended; the break opened (`long` per the cadence).
    BreakBegan { long: bool },
    /// A break ended and the next focus block auto-started (chain on).
    FocusResumed,
    /// A break ended into idle (chain off).
    SessionIdle,
}

/// Where boundary events go. Infallible at the call site by
/// contract: implementations log-and-drop failures, never propagate
/// (the escalation `apply` discipline — one lost toast never kills
/// the runtime).
pub(crate) trait FocusBoundarySink: Send + Sync + std::fmt::Debug {
    fn boundary<'a>(
        &'a self,
        event: &'a FocusBoundary,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>>;
}

/// One focus-mode action the runtime is due to take.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FocusAction {
    /// The work block is over: hand off to the break (the two-verb
    /// sequence — stop the work entry, open the respite entry). `long`
    /// per the cadence (the walk: `position % long_break_every == 0`).
    OpenBreak {
        /// The task the break entry tracks (the work entry's task).
        task: TaskId,
        /// Whether the break to open is the cycle's long break.
        long: bool,
    },
    /// The break is over: close it; with the chain knob on, start the
    /// next focus block on the same task (two verbs). The chain fires
    /// from this action's receipts, never from idle observation.
    EndBreak {
        /// The task the break entry tracked (the chain resumes it).
        task: TaskId,
        /// Whether to auto-start the next focus block (the chain knob).
        chain_next: bool,
    },
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

/// The whole state machine: one pure function over the published
/// tracking state, the timings, the chain knob, now, and the runtime's
/// owned memory (read-only — the driver owns the write at the
/// receipts). The open pick is the caller-independent `newest_open()`
/// (D-T8: a transient double-open is governed by the newest entry
/// exactly as every other reader).
///
/// | Observed open entry | Phase | Deadline | Due (`deadline ≤ now`) |
/// |---|---|---|---|
/// | `None` | Idle | — | no actions, `next_due: None` (park) |
/// | `Some(e)`, kind `Work` | Focus | `e.start + work` | `OpenBreak { long: position % every == 0 }` |
/// | `Some(e)`, kind `Respite` | Break | `e.start + (long? long_break : respite)` | `EndBreak { chain_next: chain }` |
#[must_use]
pub(crate) fn focus_step(
    tracking: &TrackingState,
    timing: &FocusTiming,
    chain: bool,
    now: DateTime<Utc>,
    memory: &FocusMemory,
) -> FocusStep {
    let Some(entry) = tracking.newest_open() else {
        return FocusStep {
            actions: Vec::new(),
            next_due: None,
        };
    };
    let entries = &tracking.entries;
    let (block, action) = match entry.kind {
        TimeEntryKind::Work => (
            timing.work(),
            Some(FocusAction::OpenBreak {
                task: entry.task,
                long: focus_work_position(entries, entry, timing, memory.decided_long)
                    .is_multiple_of(timing.long_break_every()),
            }),
        ),
        TimeEntryKind::Respite => {
            let long = focus_break_is_long(entries, entry, timing, memory.decided_long);
            (
                if long {
                    timing.long_break()
                } else {
                    timing.respite()
                },
                Some(FocusAction::EndBreak {
                    task: entry.task,
                    chain_next: chain,
                }),
            )
        }
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
/// [`EngineHandle::execute`] and completed transitions'
/// [`FocusBoundary`] events through the sink. Exits when the engine's
/// broadcast closes (the escalation semantics: `Lagged` counts as
/// change).
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
    chain: bool,
    boundaries: Arc<dyn FocusBoundarySink>,
    engine: EngineHandle,
    mut signals: broadcast::Receiver<EngineSignal>,
    clock: Arc<dyn Clock>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        tracing::debug!("focus: runtime started");
        // The runtime's first owned memory (D-FF2): empty at spawn —
        // the walk's duration inference is the restart recovery rule.
        let mut memory = FocusMemory::default();
        loop {
            let now = clock.now();
            let snapshot = state.load();
            let step = focus_step(&snapshot.tracking, &timing, chain, now, &memory);
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
                    FocusAction::OpenBreak { task, long } => {
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
                            Ok(CommandOutcome::TrackingStarted { entry, .. }) => {
                                // The receipt's stamped entry is the
                                // break's anchor datum (publish-order
                                // race: never re-read state here).
                                if long {
                                    memory.decided_long = Some(entry.id);
                                }
                                boundaries
                                    .boundary(&FocusBoundary::BreakBegan { long })
                                    .await;
                                wake = Some(
                                    entry.start
                                        + if long {
                                            timing.long_break()
                                        } else {
                                            timing.respite()
                                        },
                                );
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
                    FocusAction::EndBreak { task, chain_next } => {
                        match engine.execute(StateCommand::StopTracking).await {
                            Ok(CommandOutcome::TrackingStopped { .. }) => {
                                if chain_next {
                                    // The auto-resume: part of the
                                    // break-end act, fired from the
                                    // receipt while the break's task
                                    // is still held — never from
                                    // idle observation.
                                    match engine
                                        .execute(StateCommand::StartTracking {
                                            task,
                                            kind: TimeEntryKind::Work,
                                        })
                                        .await
                                    {
                                        Ok(CommandOutcome::TrackingStarted { entry, .. }) => {
                                            boundaries.boundary(&FocusBoundary::FocusResumed).await;
                                            wake = Some(entry.start + timing.work());
                                        }
                                        Ok(outcome) => {
                                            tracing::warn!(
                                                ?outcome,
                                                "focus: unexpected chain-start receipt"
                                            );
                                        }
                                        Err(err) => {
                                            tracing::warn!(
                                                %err,
                                                "focus: chained start failed; the chain act is dropped"
                                            );
                                        }
                                    }
                                } else {
                                    boundaries.boundary(&FocusBoundary::SessionIdle).await;
                                }
                            }
                            // A state surprise (nothing was open):
                            // abort the chain act — never start a
                            // focus block on nothing; re-derive from
                            // published state at the next signal.
                            Ok(outcome) => {
                                tracing::warn!(
                                    ?outcome,
                                    "focus: break-end stop answered unexpectedly; chain act aborted"
                                );
                            }
                            Err(err) => {
                                tracing::warn!(%err, "focus: break-end stop failed");
                            }
                        }
                    }
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
/// rides the same gate. The `chain`/`notify` knobs never affect the
/// decision (only the act).
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
    use taskboard_domain::TimeEntry;
    use taskboard_domain::command::CommandOutcome;
    use taskboard_domain::test_support::{CountingIds, InMemoryRepository};

    const T0: DateTime<Utc> =
        chrono::DateTime::<Utc>::from_timestamp(1_792_000_000, 0).expect("valid instant");

    fn m(mins: i64) -> TimeDelta {
        TimeDelta::minutes(mins)
    }

    fn timing() -> FocusTiming {
        FocusTiming::try_new(
            TimeDelta::minutes(25),
            TimeDelta::minutes(5),
            TimeDelta::minutes(15),
            4,
        )
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

    fn tracking_of(entries: &[TimeEntry]) -> TrackingState {
        TrackingState {
            entries: entries.iter().map(|e| (e.id, e.clone())).collect(),
        }
    }

    fn step_of(
        entries: &[TimeEntry],
        chain: bool,
        now: DateTime<Utc>,
        memory: &FocusMemory,
    ) -> FocusStep {
        focus_step(&tracking_of(entries), &timing(), chain, now, memory)
    }

    // -- pure focus_step ------------------------------------------------

    #[test]
    fn idle_parks_with_no_actions() {
        let step = focus_step(
            &TrackingState::default(),
            &timing(),
            false,
            T0,
            &FocusMemory::default(),
        );
        assert_eq!(step.actions, [] as [FocusAction; 0]);
        assert_eq!(step.next_due, None);
    }

    #[test]
    fn work_block_arms_its_deadline_and_fires_open_break_at_the_due() {
        let e = entry(TimeEntryKind::Work, T0);
        let memory = FocusMemory::default();
        let early = step_of(
            std::slice::from_ref(&e),
            false,
            T0 + TimeDelta::minutes(24) + TimeDelta::seconds(59),
            &memory,
        );
        assert!(early.actions.is_empty(), "one second early: nothing due");
        assert_eq!(early.next_due, Some(T0 + TimeDelta::minutes(25)));

        let due = step_of(
            std::slice::from_ref(&e),
            false,
            T0 + TimeDelta::minutes(25),
            &memory,
        );
        assert_eq!(
            due.actions,
            vec![FocusAction::OpenBreak {
                task: e.task,
                long: false
            }],
            "at the due instant: exactly the hand-off, short at position 1"
        );
        assert_eq!(due.next_due, None);
    }

    #[test]
    fn respite_block_arms_its_deadline_and_fires_end_break_at_the_due() {
        let e = entry(TimeEntryKind::Respite, T0);
        let memory = FocusMemory::default();
        let early = step_of(
            std::slice::from_ref(&e),
            false,
            T0 + TimeDelta::minutes(4),
            &memory,
        );
        assert_eq!(early.actions, [] as [FocusAction; 0]);
        assert_eq!(early.next_due, Some(T0 + TimeDelta::minutes(5)));

        let due = step_of(
            std::slice::from_ref(&e),
            false,
            T0 + TimeDelta::minutes(5),
            &memory,
        );
        assert_eq!(
            due.actions,
            vec![FocusAction::EndBreak {
                task: e.task,
                chain_next: false
            }]
        );
        assert_eq!(due.next_due, None);
    }

    #[test]
    fn end_break_carries_the_chain_decision() {
        let e = entry(TimeEntryKind::Respite, T0);
        let memory = FocusMemory::default();
        let due = step_of(
            std::slice::from_ref(&e),
            true,
            T0 + TimeDelta::minutes(5),
            &memory,
        );
        assert_eq!(
            due.actions,
            vec![FocusAction::EndBreak {
                task: e.task,
                chain_next: true
            }],
            "chain on: the break end carries the auto-resume decision"
        );
        // Chain off parks: the EndBreak is still due (the break ends),
        // only the chain flag differs.
        let due_off = step_of(
            std::slice::from_ref(&e),
            false,
            T0 + TimeDelta::minutes(5),
            &memory,
        );
        assert_eq!(
            due_off.actions,
            vec![FocusAction::EndBreak {
                task: e.task,
                chain_next: false
            }]
        );
    }

    #[test]
    fn the_handoff_names_the_long_break_at_the_cadence() {
        // Seeded history: three completed works (short breaks between)
        // + an open fourth work ⇒ position 4 ⇒ `long: true`.
        let task = TaskId::from_uuid(uuid::Uuid::now_v7());
        let mk = |kind: TimeEntryKind, start: DateTime<Utc>, end: Option<DateTime<Utc>>| {
            let mut e = entry(kind, start);
            e.task = task;
            e.end = end;
            e
        };
        let history = vec![
            mk(TimeEntryKind::Work, T0, Some(T0 + m(25))),
            mk(TimeEntryKind::Respite, T0 + m(25), Some(T0 + m(30))),
            mk(TimeEntryKind::Work, T0 + m(30), Some(T0 + m(55))),
            mk(TimeEntryKind::Respite, T0 + m(55), Some(T0 + m(60))),
            mk(TimeEntryKind::Work, T0 + m(60), Some(T0 + m(85))),
            mk(TimeEntryKind::Respite, T0 + m(85), Some(T0 + m(90))),
        ];
        let memory = FocusMemory::default();

        // Position 1–3 hands off short; position 4 hands off long.
        for (position, start) in [
            (1u32, T0),
            (2, T0 + m(30)),
            (3, T0 + m(60)),
            (4, T0 + m(90)),
        ] {
            let mut entries = history.clone();
            entries.push(mk(TimeEntryKind::Work, start, None));
            let step = step_of(&entries, false, start + m(25), &memory);
            assert_eq!(
                step.actions,
                vec![FocusAction::OpenBreak {
                    task,
                    long: position == 4
                }],
                "position {position} hands off long = {}",
                position == 4
            );
        }

        // The long break the cadence names arms its own length: the
        // fourth work closed at 115m, the cadence break opens there.
        let mut entries = history.clone();
        entries.push(mk(TimeEntryKind::Work, T0 + m(90), Some(T0 + m(115))));
        entries.push(mk(TimeEntryKind::Respite, T0 + m(115), None));
        let step = step_of(&entries, false, T0 + m(115), &memory);
        assert_eq!(step.next_due, Some(T0 + m(115) + TimeDelta::minutes(15)));
    }

    #[test]
    fn an_open_long_break_arms_the_long_deadline_and_the_cold_one_the_short() {
        let memory = FocusMemory::default();
        // Cold break: position 0 ⇒ short length.
        let cold = entry(TimeEntryKind::Respite, T0);
        let step = step_of(std::slice::from_ref(&cold), false, T0, &memory);
        assert_eq!(step.next_due, Some(T0 + TimeDelta::minutes(5)));
        // Cadence-position 4 break: long length.
        let task = TaskId::from_uuid(uuid::Uuid::now_v7());
        let mut hist = Vec::new();
        let starts = [
            (T0, T0 + m(25), TimeEntryKind::Work),
            (T0 + m(25), T0 + m(30), TimeEntryKind::Respite),
            (T0 + m(30), T0 + m(55), TimeEntryKind::Work),
            (T0 + m(55), T0 + m(60), TimeEntryKind::Respite),
            (T0 + m(60), T0 + m(85), TimeEntryKind::Work),
            (T0 + m(85), T0 + m(90), TimeEntryKind::Respite),
            (T0 + m(90), T0 + m(115), TimeEntryKind::Work),
        ];
        for (s, e, k) in starts {
            let mut row = entry(k, s);
            row.task = task;
            row.end = Some(e);
            hist.push(row);
        }
        let long_open = entry(TimeEntryKind::Respite, T0 + m(115));
        hist.push(long_open.clone());
        let step = step_of(&hist, false, T0 + m(115), &memory);
        assert_eq!(
            step.next_due,
            Some(T0 + m(115) + TimeDelta::minutes(15)),
            "the cadence break arms the long length"
        );
        let due = step_of(&hist, false, T0 + m(115) + TimeDelta::minutes(15), &memory);
        assert_eq!(
            due.actions,
            vec![FocusAction::EndBreak {
                task: long_open.task,
                chain_next: false
            }]
        );
    }

    #[test]
    fn the_newest_open_entry_governs_the_phase() {
        let old = entry(TimeEntryKind::Work, T0);
        let newer = entry(TimeEntryKind::Respite, T0 + TimeDelta::minutes(1));
        let memory = FocusMemory::default();
        let state = tracking_of(&[old, newer.clone()]);
        let step = focus_step(
            &state,
            &timing(),
            false,
            T0 + TimeDelta::minutes(3),
            &memory,
        );
        assert!(step.actions.is_empty(), "the newer respite is not due yet");
        assert_eq!(step.next_due, Some(newer.start + TimeDelta::minutes(5)));
    }

    #[test]
    fn foreign_work_entry_re_anchors() {
        // The deadline anchors on the entry's start: a later-started
        // foreign work entry moves the deadline, forgetting the old one.
        let foreign = entry(TimeEntryKind::Work, T0 + TimeDelta::minutes(10));
        let memory = FocusMemory::default();
        let step = step_of(
            std::slice::from_ref(&foreign),
            false,
            T0 + TimeDelta::minutes(25),
            &memory,
        );
        assert!(step.actions.is_empty(), "not due at the *old* deadline");
        assert_eq!(step.next_due, Some(T0 + TimeDelta::minutes(35)));
    }

    #[test]
    fn a_stale_entry_reconciles_once_at_the_due() {
        // Long past due at boot: exactly one reconciling action, no
        // cascade semantics in the step (the driver fires it at engine
        // now — never backdated).
        let stale = entry(TimeEntryKind::Work, T0 - TimeDelta::minutes(75));
        let memory = FocusMemory::default();
        let step = step_of(std::slice::from_ref(&stale), false, T0, &memory);
        assert_eq!(step.actions.len(), 1);
        assert_eq!(
            step.actions[0],
            FocusAction::OpenBreak {
                task: stale.task,
                long: false
            }
        );
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

    #[test]
    fn the_boundary_chime_volume_is_the_escalation_floor() {
        assert_eq!(
            FOCUS_CHIME_VOLUME,
            crate::escalation::CHIME_LO,
            "one volume vocabulary: the gentle boundary chime sits at the escalation floor"
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

    /// The journeys' boundary sink: records every typed event behind a
    /// Mutex, shared with the test through its Arc.
    #[derive(Debug, Default)]
    struct RecordingSink {
        events: std::sync::Mutex<Vec<FocusBoundary>>,
    }

    impl FocusBoundarySink for RecordingSink {
        fn boundary<'a>(
            &'a self,
            event: &'a FocusBoundary,
        ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
            Box::pin(async move {
                self.events.lock().expect("sink lock").push(event.clone());
            })
        }
    }

    impl RecordingSink {
        fn recorded(&self) -> Vec<FocusBoundary> {
            self.events.lock().expect("sink lock").clone()
        }
    }

    fn recording_sink() -> Arc<RecordingSink> {
        Arc::new(RecordingSink::default())
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
            taskboard_domain::SettingsSnapshot::default(),
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

    async fn start_work(engine: &EngineHandle, task: TaskId) {
        engine
            .execute(StateCommand::StartTracking {
                task,
                kind: TimeEntryKind::Work,
            })
            .await
            .expect("start works");
        tokio::time::advance(std::time::Duration::from_secs(1)).await;
        tokio::task::yield_now().await;
    }

    /// Drives `n` full work blocks with the chain off (the user stops
    /// the break and starts each next block), returning after the nth
    /// break has opened.
    async fn drive_blocks(engine: &EngineHandle, clock: &FakeClock, task: TaskId, n: u32) {
        start_work(engine, task).await;
        for i in 0..n {
            tick(
                clock,
                clock.now() + TimeDelta::minutes(25) + TimeDelta::seconds(5),
            )
            .await;
            if i + 1 < n {
                // The user ends the break seconds after it opens (a
                // short respite — a 25-minute one would be inferred
                // long by the walk) and starts the next block.
                tick(clock, clock.now() + TimeDelta::seconds(1)).await;
                engine
                    .execute(StateCommand::StopTracking)
                    .await
                    .expect("stop works");
                tokio::time::advance(std::time::Duration::from_secs(1)).await;
                tokio::task::yield_now().await;
                start_work(engine, task).await;
            }
        }
    }

    #[tokio::test(start_paused = true)]
    async fn a_focus_session_hands_off_to_the_break_at_the_deadline() {
        let (engine, state, clock) = harness().await;
        let timing = timing();
        let sink = recording_sink();
        let task = live_task(&engine).await;
        let runtime = spawn_focus_runtime(
            Arc::clone(&state),
            timing,
            false,
            sink.clone(),
            engine.clone(),
            engine.subscribe(),
            clock.clone() as Arc<dyn Clock>,
        );

        // The user starts the focus block (never the runtime's act).
        start_work(&engine, task).await;
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
        assert_eq!(
            sink.recorded(),
            vec![FocusBoundary::BreakBegan { long: false }],
            "the boundary rode the receipt"
        );

        runtime.abort();
        let _ = runtime.await;
    }

    #[tokio::test(start_paused = true)]
    async fn the_break_ends_into_idle_at_its_deadline() {
        let (engine, state, clock) = harness().await;
        let timing = timing();
        let sink = recording_sink();
        let task = live_task(&engine).await;
        let runtime = spawn_focus_runtime(
            Arc::clone(&state),
            timing,
            false,
            sink.clone(),
            engine.clone(),
            engine.subscribe(),
            clock.clone() as Arc<dyn Clock>,
        );
        start_work(&engine, task).await;
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
        assert_eq!(
            sink.recorded(),
            vec![
                FocusBoundary::BreakBegan { long: false },
                FocusBoundary::SessionIdle
            ],
            "break began, then the session went idle"
        );

        runtime.abort();
        let _ = runtime.await;
    }

    #[tokio::test(start_paused = true)]
    async fn an_externally_stopped_session_is_never_reopened() {
        let (engine, state, clock) = harness().await;
        let timing = timing();
        let sink = recording_sink();
        let task = live_task(&engine).await;
        let runtime = spawn_focus_runtime(
            Arc::clone(&state),
            timing,
            false,
            sink.clone(),
            engine.clone(),
            engine.subscribe(),
            clock.clone() as Arc<dyn Clock>,
        );
        start_work(&engine, task).await;

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
        assert!(
            sink.recorded().is_empty(),
            "no boundary fired: the transition was the user's, not the runtime's"
        );

        runtime.abort();
        let _ = runtime.await;
    }

    #[tokio::test(start_paused = true)]
    async fn a_foreign_start_re_anchors_the_session() {
        let (engine, state, clock) = harness().await;
        let timing = timing();
        let sink = recording_sink();
        let task_a = live_task(&engine).await;
        let task_b = live_task(&engine).await;
        let runtime = spawn_focus_runtime(
            Arc::clone(&state),
            timing,
            false,
            sink.clone(),
            engine.clone(),
            engine.subscribe(),
            clock.clone() as Arc<dyn Clock>,
        );
        start_work(&engine, task_a).await;

        // Mid-focus the user starts task B (single-global intent: A
        // closes, B opens). The new anchor is B's start.
        tick(&clock, T0 + TimeDelta::minutes(10)).await;
        start_work(&engine, task_b).await;

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
        assert_eq!(
            sink.recorded(),
            vec![FocusBoundary::BreakBegan { long: false }],
            "one boundary, for B's break only"
        );

        runtime.abort();
        let _ = runtime.await;
    }

    #[tokio::test(start_paused = true)]
    async fn a_manual_respite_derives_the_break_phase() {
        let (engine, state, clock) = harness().await;
        let timing = timing();
        let sink = recording_sink();
        let task = live_task(&engine).await;
        let runtime = spawn_focus_runtime(
            Arc::clone(&state),
            timing,
            false,
            sink.clone(),
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
        assert_eq!(
            sink.recorded(),
            vec![FocusBoundary::SessionIdle],
            "a manual respite's end is a runtime-driven boundary too"
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
            taskboard_domain::SettingsSnapshot::default(),
            taskboard_state::DayRollConfig::disabled(),
        )
        .await
        .expect("engine boots");
        let _ = report_tx;
        let published = engine.shared_state();
        let sink = recording_sink();
        let runtime = spawn_focus_runtime(
            Arc::clone(&published),
            timing(),
            false,
            sink.clone(),
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
        assert_eq!(
            sink.recorded(),
            vec![
                FocusBoundary::BreakBegan { long: false },
                FocusBoundary::SessionIdle
            ]
        );

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
        let sink = recording_sink();
        let task = live_task(&engine).await;
        let runtime = spawn_focus_runtime(
            Arc::clone(&state),
            timing,
            false,
            sink.clone(),
            engine.clone(),
            engine.subscribe(),
            clock.clone() as Arc<dyn Clock>,
        );
        start_work(&engine, task).await;

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
        assert_eq!(
            sink.recorded(),
            vec![FocusBoundary::BreakBegan { long: false }],
            "delivery rides the receipts: at most one per real transition"
        );

        runtime.abort();
        let _ = runtime.await;
    }

    #[tokio::test(start_paused = true)]
    async fn the_fourth_break_is_long_and_runs_the_long_length() {
        let (engine, state, clock) = harness().await;
        let timing = timing();
        let sink = recording_sink();
        let task = live_task(&engine).await;
        let runtime = spawn_focus_runtime(
            Arc::clone(&state),
            timing,
            false,
            sink.clone(),
            engine.clone(),
            engine.subscribe(),
            clock.clone() as Arc<dyn Clock>,
        );

        // Four blocks, the user starting each next one (chain off).
        drive_blocks(&engine, &clock, task, 4).await;

        // The fourth break is open and arms the long length.
        let snapshot = state.load();
        let open = open_entries(&snapshot);
        assert_eq!(open.len(), 1, "the fourth break is open");
        assert_eq!(open[0].kind, TimeEntryKind::Respite);
        let long_break_start = open[0].start;
        // Three short hand-offs and the long one — no idle events yet:
        // the helper's user stops end the breaks externally (a user
        // stop is never a runtime boundary).
        assert_eq!(
            sink.recorded(),
            vec![
                FocusBoundary::BreakBegan { long: false },
                FocusBoundary::BreakBegan { long: false },
                FocusBoundary::BreakBegan { long: false },
                FocusBoundary::BreakBegan { long: true },
            ],
            "three short boundaries, then the long one — no idle event yet"
        );

        // The long break runs the long length, ending into idle there.
        tick(
            &clock,
            long_break_start + TimeDelta::minutes(15) + TimeDelta::seconds(5),
        )
        .await;
        assert!(
            open_entries(&state.load()).is_empty(),
            "the long break ended"
        );
        assert_eq!(
            sink.recorded().last(),
            Some(&FocusBoundary::SessionIdle),
            "the long break ends into idle like any chain-off break"
        );

        runtime.abort();
        let _ = runtime.await;
    }

    #[tokio::test(start_paused = true)]
    async fn chain_on_resumes_focus_after_every_break() {
        let (engine, state, clock) = harness().await;
        let timing = timing();
        let sink = recording_sink();
        let task = live_task(&engine).await;
        let runtime = spawn_focus_runtime(
            Arc::clone(&state),
            timing,
            true,
            sink.clone(),
            engine.clone(),
            engine.subscribe(),
            clock.clone() as Arc<dyn Clock>,
        );
        start_work(&engine, task).await;

        // Block 1 → short break → auto-resume → block 2 → … → block 4
        // → the long break → auto-resume. One tick per boundary, in
        // sequence: the chain alternates indefinitely.
        for block in 1..=4u32 {
            // The work block hands off at its deadline.
            tick(
                &clock,
                clock.now() + TimeDelta::minutes(25) + TimeDelta::seconds(5),
            )
            .await;
            let snapshot = state.load();
            let open = open_entries(&snapshot);
            assert_eq!(open.len(), 1, "block {block}'s break is open");
            assert_eq!(open[0].kind, TimeEntryKind::Respite);
            let expect_long = block == 4;
            // The break ends and the next focus block auto-starts on
            // the same task.
            let break_len = if expect_long { 15 } else { 5 };
            tick(
                &clock,
                clock.now() + TimeDelta::minutes(break_len) + TimeDelta::seconds(5),
            )
            .await;
            let snapshot = state.load();
            let open = open_entries(&snapshot);
            assert_eq!(open.len(), 1, "the chained block auto-started");
            assert_eq!(open[0].kind, TimeEntryKind::Work, "the chain resumed work");
            assert_eq!(open[0].task, task, "the chain resumes the same task");
            let resumed_start = open[0].start;
            assert!(
                (resumed_start - clock.now()).num_seconds().abs() <= 5,
                "the resumed block anchors at engine now"
            );
        }

        let expected = [
            FocusBoundary::BreakBegan { long: false },
            FocusBoundary::FocusResumed,
            FocusBoundary::BreakBegan { long: false },
            FocusBoundary::FocusResumed,
            FocusBoundary::BreakBegan { long: false },
            FocusBoundary::FocusResumed,
            FocusBoundary::BreakBegan { long: true },
            FocusBoundary::FocusResumed,
        ];
        assert_eq!(
            sink.recorded(),
            expected,
            "one event per transition, in order"
        );

        runtime.abort();
        let _ = runtime.await;
    }

    #[tokio::test(start_paused = true)]
    async fn an_externally_stopped_break_never_chains() {
        let (engine, state, clock) = harness().await;
        let timing = timing();
        let sink = recording_sink();
        let task = live_task(&engine).await;
        let runtime = spawn_focus_runtime(
            Arc::clone(&state),
            timing,
            true,
            sink.clone(),
            engine.clone(),
            engine.subscribe(),
            clock.clone() as Arc<dyn Clock>,
        );
        start_work(&engine, task).await;

        // The hand-off opens the break…
        tick(&clock, T0 + TimeDelta::minutes(25) + TimeDelta::seconds(5)).await;
        assert_eq!(
            open_entries(&state.load())[0].kind,
            TimeEntryKind::Respite,
            "the break is open"
        );

        // …and the user stops it mid-break. The entry is truth: the
        // runtime parks in idle forever — never re-open, never chain.
        engine
            .execute(StateCommand::StopTracking)
            .await
            .expect("stop works");
        tokio::time::advance(std::time::Duration::from_secs(1)).await;
        tokio::task::yield_now().await;
        tick(&clock, T0 + TimeDelta::minutes(120)).await;
        assert!(
            open_entries(&state.load()).is_empty(),
            "an externally stopped break parks in idle"
        );
        assert_eq!(
            sink.recorded(),
            vec![FocusBoundary::BreakBegan { long: false }],
            "no FocusResumed ever delivered: the chain fires from the break-end receipt, not idle observation"
        );

        runtime.abort();
        let _ = runtime.await;
    }

    #[tokio::test(start_paused = true)]
    async fn a_foreign_start_during_a_break_supersedes_the_pending_chain() {
        let (engine, state, clock) = harness().await;
        let timing = timing();
        let sink = recording_sink();
        let task_a = live_task(&engine).await;
        let task_b = live_task(&engine).await;
        let runtime = spawn_focus_runtime(
            Arc::clone(&state),
            timing,
            true,
            sink.clone(),
            engine.clone(),
            engine.subscribe(),
            clock.clone() as Arc<dyn Clock>,
        );
        start_work(&engine, task_a).await;
        tick(&clock, T0 + TimeDelta::minutes(25) + TimeDelta::seconds(5)).await;
        assert_eq!(open_entries(&state.load())[0].kind, TimeEntryKind::Respite);

        // Mid-break the user starts task B: A's break closes (the
        // single-global start), B opens; the pending break-end + chain
        // act is forgotten and B re-anchors.
        tick(&clock, T0 + TimeDelta::minutes(27)).await;
        start_work(&engine, task_b).await;

        // The break's original deadline passes: nothing fires.
        tick(&clock, T0 + TimeDelta::minutes(30) + TimeDelta::seconds(5)).await;
        let snapshot = state.load();
        let open = open_entries(&snapshot);
        assert_eq!(open.len(), 1, "only B's work entry is open");
        assert_eq!(open[0].task, task_b);
        assert_eq!(open[0].kind, TimeEntryKind::Work, "no chain act fired");

        // B re-anchors: B's break opens at B's own deadline.
        tick(&clock, T0 + TimeDelta::minutes(52) + TimeDelta::seconds(5)).await;
        let snapshot = state.load();
        let open = open_entries(&snapshot);
        assert_eq!(open.len(), 1);
        assert_eq!(open[0].task, task_b, "the break re-anchored to B");
        assert_eq!(open[0].kind, TimeEntryKind::Respite);

        // No duplicate events: A's break began before the foreign
        // start, B's break began at its own deadline — and no
        // FocusResumed: the superseded chain act delivered nothing.
        assert_eq!(
            sink.recorded(),
            vec![
                FocusBoundary::BreakBegan { long: false },
                FocusBoundary::BreakBegan { long: false },
            ],
            "two honest boundaries, zero chain events"
        );

        runtime.abort();
        let _ = runtime.await;
    }

    #[allow(clippy::too_many_lines)] // the seed fixture dominates the count
    #[tokio::test(start_paused = true)]
    async fn restart_mid_cycle_recovers_the_position_from_history() {
        // Two closed W–R pairs in the repo (the daemon restarted
        // mid-cycle), then the runtime spawns and the user starts a
        // work block: the first hand-off is SHORT (position 3, not 5 —
        // the no-double-count pin) and the *fourth* block's break is
        // long.
        let clock = Arc::new(FakeClock::default());
        clock.set(T0);
        let epoch = taskboard_domain::BoardClocks::epoch();
        let task = taskboard_domain::entities::Task {
            id: TaskId::from_uuid(uuid::Uuid::now_v7()),
            remote: None,
            title: "cycle history".into(),
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
        let mk = |kind, start, end: Option<DateTime<Utc>>| {
            let mut e = entry(kind, start);
            e.task = task_id;
            e.task_title = task.title.clone();
            e.end = end;
            e
        };
        // Two closed W–R pairs ending right at the restart instant
        // (inside the gap tolerance — a cycle the walk can recover).
        let history = [
            mk(TimeEntryKind::Work, T0 - m(50), Some(T0 - m(25))),
            mk(TimeEntryKind::Respite, T0 - m(25), Some(T0 - m(20))),
            mk(TimeEntryKind::Work, T0 - m(20), Some(T0 - m(1))),
            mk(TimeEntryKind::Respite, T0 - m(1), Some(T0)),
        ];
        let state_seed = taskboard_domain::PersistedState {
            tasks: [(task.id, task)].into_iter().collect(),
            tracking: taskboard_domain::TrackingState {
                entries: history.iter().map(|e| (e.id, e.clone())).collect(),
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
            taskboard_domain::SettingsSnapshot::default(),
            taskboard_state::DayRollConfig::disabled(),
        )
        .await
        .expect("engine boots");
        let _ = report_tx;
        let published = engine.shared_state();
        let sink = recording_sink();
        let runtime = spawn_focus_runtime(
            Arc::clone(&published),
            timing(),
            false,
            sink.clone(),
            engine.clone(),
            engine.subscribe(),
            clock.clone() as Arc<dyn Clock>,
        );

        // The restarted session: the user's third work block of the
        // cycle. Position 3 ⇒ short break (a double-counting runtime
        // would call this position 5 and hand off long).
        start_work(&engine, task_id).await;
        tick(&clock, T0 + TimeDelta::minutes(25) + TimeDelta::seconds(5)).await;
        assert_eq!(
            sink.recorded(),
            vec![FocusBoundary::BreakBegan { long: false }],
            "position 3: the first hand-off after the restart is short"
        );

        // The user starts the fourth block: end the break one second
        // in (short respite, no zero-length tie) and start work.
        tick(&clock, T0 + TimeDelta::minutes(25) + TimeDelta::seconds(6)).await;
        engine
            .execute(StateCommand::StopTracking)
            .await
            .expect("stop works");
        tokio::time::advance(std::time::Duration::from_secs(1)).await;
        tokio::task::yield_now().await;
        start_work(&engine, task_id).await;
        tick(
            &clock,
            clock.now() + TimeDelta::minutes(25) + TimeDelta::seconds(5),
        )
        .await;
        assert_eq!(
            sink.recorded().last(),
            Some(&FocusBoundary::BreakBegan { long: true }),
            "position 4 lands the long break: the walk recovered the position"
        );

        runtime.abort();
        let _ = runtime.await;
    }

    #[tokio::test(start_paused = true)]
    async fn the_owned_decision_survives_an_early_external_long_break_close() {
        // The runtime opens the long break, the user stops it two
        // minutes in and starts the next block: within the process the
        // cycle reset still holds (the inference-only walk would
        // misplace it — the memory-vs-history pin, journey form).
        let (engine, state, clock) = harness().await;
        let timing = timing();
        let sink = recording_sink();
        let task = live_task(&engine).await;
        let runtime = spawn_focus_runtime(
            Arc::clone(&state),
            timing,
            true,
            sink.clone(),
            engine.clone(),
            engine.subscribe(),
            clock.clone() as Arc<dyn Clock>,
        );

        // Four blocks of the first cycle (chain keeps it moving).
        start_work(&engine, task).await;
        for _ in 0..4 {
            tick(
                &clock,
                clock.now() + TimeDelta::minutes(25) + TimeDelta::seconds(5),
            )
            .await;
            let len = if sink.recorded().last() == Some(&FocusBoundary::BreakBegan { long: true }) {
                15
            } else {
                5
            };
            tick(&clock, clock.now() + TimeDelta::minutes(len)).await;
        }
        assert_eq!(
            sink.recorded().last(),
            Some(&FocusBoundary::FocusResumed),
            "the long break chained into cycle two"
        );

        // Cycle two, block one, then the user interrupts: stop the
        // (short) break two minutes in and start the next block.
        tick(
            &clock,
            clock.now() + TimeDelta::minutes(25) + TimeDelta::seconds(5),
        )
        .await;
        assert_eq!(
            sink.recorded().last(),
            Some(&FocusBoundary::BreakBegan { long: false })
        );
        tick(&clock, clock.now() + TimeDelta::minutes(2)).await;
        engine
            .execute(StateCommand::StopTracking)
            .await
            .expect("stop works");
        tokio::time::advance(std::time::Duration::from_secs(1)).await;
        tokio::task::yield_now().await;
        start_work(&engine, task).await;

        // Cycle two's block two: position 2 ⇒ the break is SHORT (the
        // anchor kept the reset; an inference-only walk would see the
        // aborted long-looking-short respite as crossed and still give
        // position 2 — so pin the *next* long break instead: it lands
        // at cycle two's fourth block, not the third).
        let mut long_at_block: Option<u32> = None;
        for block in 2..=4u32 {
            tick(
                &clock,
                clock.now() + TimeDelta::minutes(25) + TimeDelta::seconds(5),
            )
            .await;
            let last = sink.recorded().last().cloned();
            if last == Some(FocusBoundary::BreakBegan { long: true }) {
                long_at_block = Some(block);
                break;
            }
            tick(&clock, clock.now() + TimeDelta::minutes(5)).await;
        }
        assert_eq!(
            long_at_block,
            Some(4),
            "the anchor keeps the reset: the next long break is cycle two's fourth block"
        );

        runtime.abort();
        let _ = runtime.await;
    }

    #[allow(clippy::too_many_lines)] // the seed fixture dominates the count
    #[tokio::test(start_paused = true)]
    async fn a_stale_break_reconciles_once_with_the_chain() {
        // Seed a repo with an open Respite long past its deadline,
        // spawn the runtime with the chain on: exactly one stop + one
        // chained Work start at engine now — no cascade.
        let clock = Arc::new(FakeClock::default());
        clock.set(T0);
        let epoch = taskboard_domain::BoardClocks::epoch();
        let task = taskboard_domain::entities::Task {
            id: TaskId::from_uuid(uuid::Uuid::now_v7()),
            remote: None,
            title: "stale break's task".into(),
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
        let mut stale = entry(TimeEntryKind::Respite, T0 - TimeDelta::hours(2));
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
            taskboard_domain::SettingsSnapshot::default(),
            taskboard_state::DayRollConfig::disabled(),
        )
        .await
        .expect("engine boots");
        let _ = report_tx;
        let published = engine.shared_state();
        let sink = recording_sink();
        let runtime = spawn_focus_runtime(
            Arc::clone(&published),
            timing(),
            true,
            sink.clone(),
            engine.clone(),
            engine.subscribe(),
            clock.clone() as Arc<dyn Clock>,
        );
        tokio::time::advance(std::time::Duration::from_secs(1)).await;
        tokio::task::yield_now().await;

        // One reconciling act at engine now: the stale break closed,
        // one chained Work entry open, anchored at engine now.
        let snapshot = published.load();
        let open = open_entries(&snapshot);
        assert_eq!(open.len(), 1, "exactly the chained work entry is open");
        assert_eq!(open[0].task, task_id);
        assert_eq!(open[0].kind, TimeEntryKind::Work);
        assert_eq!(open[0].start, T0, "no backdating: anchored at engine now");
        let respites = snapshot
            .tracking
            .entries
            .values()
            .filter(|e| e.kind == TimeEntryKind::Respite)
            .count();
        assert_eq!(respites, 1, "the stale break closed, none re-opened");
        assert_eq!(
            sink.recorded(),
            vec![FocusBoundary::FocusResumed],
            "one chained transition, one event"
        );

        // No cascade: the chained block runs its normal cadence.
        tick(&clock, T0 + TimeDelta::minutes(25) + TimeDelta::seconds(5)).await;
        assert_eq!(
            open_entries(&published.load())[0].kind,
            TimeEntryKind::Respite
        );
        assert_eq!(
            sink.recorded(),
            vec![
                FocusBoundary::FocusResumed,
                FocusBoundary::BreakBegan { long: false }
            ]
        );

        runtime.abort();
        let _ = runtime.await;
    }

    #[tokio::test(start_paused = true)]
    async fn boundary_notifications_fire_once_per_transition() {
        // The signal-spam tick pattern across a hand-off AND a chain
        // resume: deliveries == the real transition count, never more
        // (correctness, not a ceiling).
        let (engine, state, clock) = harness().await;
        let timing = timing();
        let sink = recording_sink();
        let task = live_task(&engine).await;
        let runtime = spawn_focus_runtime(
            Arc::clone(&state),
            timing,
            true,
            sink.clone(),
            engine.clone(),
            engine.subscribe(),
            clock.clone() as Arc<dyn Clock>,
        );
        start_work(&engine, task).await;

        // Spam signal wakes across both boundary instants.
        for i in 0..8 {
            tick(&clock, T0 + TimeDelta::minutes(25) + TimeDelta::seconds(i)).await;
        }
        for i in 0..8 {
            tick(&clock, T0 + TimeDelta::minutes(30) + TimeDelta::seconds(i)).await;
        }
        let events = sink.recorded();
        assert_eq!(
            events,
            vec![
                FocusBoundary::BreakBegan { long: false },
                FocusBoundary::FocusResumed
            ],
            "one delivery per real transition across the spam window"
        );

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
                taskboard_domain::SettingsSnapshot::default(),
                taskboard_state::DayRollConfig::disabled(),
            )
            .await
            .expect("engine boots");
            let _ = report_tx;
            let _ = system_tx;
            engine
        };
        let runtime = spawn_focus_runtime(
            state,
            timing(),
            false,
            recording_sink(),
            engine,
            rx,
            clock as Arc<dyn Clock>,
        );
        tokio::time::timeout(std::time::Duration::from_secs(1), runtime)
            .await
            .expect("the runtime exits on Closed")
            .expect("clean exit");
    }
}
