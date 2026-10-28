// SPDX-License-Identifier: AGPL-3.0-only
//! One signal policy for the process (ADR 0011's amendment, design
//! D-0), stated once here and consumed surface by surface:
//!
//! > **SIGINT means "cancel the current operation". External
//! > termination (SIGTERM/SIGHUP) means "restore the terminal, then
//! > end gracefully". Every ladder ends in a force rung. Death by
//! > keyboard is reserved for the explicit exits (`exit`/`quit`/Ctrl-D)
//! > and the force rungs.**
//!
//! The counting/ordering logic exists exactly once, as the pure
//! [`Ladder`] state machine below: signals arrive as [`Event::Signal`],
//! acknowledgements as [`Event::Progress`], and the machine answers
//! [`Action::Initiate`] (first rung: cancel / graceful),
//! [`Action::Escalate`] (a middle rung; the daemon's salvage), or
//! [`Action::Force`] (restore + immediate exit). The rung count is
//! surface-configured — the REPL session uses `Ladder::new(0)` per
//! signal family (cancel/force, restore-graceful/force); the daemon's
//! three-rung shape (graceful/salvage/hard, design D-6) is
//! `Ladder::new(1)` of the *same* machine. Kind-agnostic by
//! construction: the daemon wiring may feed one merged instance for
//! its mixed-kind first rung; the REPL keeps one instance per family
//! so the two rung-1 actions never cross-count.
//!
//! The force semantics encode design D-2's condition exactly: a second
//! signal forces *only while the first is not yet acknowledged* —
//! [`Event::Progress`] is the acknowledgement and de-arms the ladder.
//!
//! Orphan completion (D-0/D-2, documented not hidden): cancelling a
//! dispatching line drops the line's future, but an engine command
//! already delivered to the state actor still commits — the next line
//! observes its effect. The cancelled envelope's detail text names
//! this; it is never hidden behind a pretend-rollback.

use std::sync::Arc;

use tokio::sync::watch;

/// A signal family member the watcher listened to. Unix-only kinds are
/// constructed only under `cfg(unix)` (the watcher's cfg-gated
/// streams); the type itself is total so the machine and its tests are
/// platform-independent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signal {
    /// SIGINT / console Ctrl-C: the cancel family.
    Interrupt,
    /// SIGTERM: the external-termination family.
    Terminate,
    /// SIGHUP: the external-termination family.
    Hangup,
}

impl Signal {
    /// The conventional exit code of a default-disposition death by
    /// this signal: `128 + signum` (130/143/129).
    #[must_use]
    pub fn exit_code(self) -> i32 {
        match self {
            Self::Interrupt => 130,
            Self::Terminate => 143,
            Self::Hangup => 129,
        }
    }
}

/// One input to the ladder: a signal arrived, or a prior rung's
/// graceful work completed (cancellation acknowledged / the loop
/// ended) — the de-arm event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Event {
    Signal(Signal),
    Progress,
    /// The rung-1 trigger that is not a signal: `daemon stop` answered
    /// over the control lane. It arms the ladder exactly as a first
    /// signal does — a later signal during a stop-initiated drain is
    /// the *second* trigger, not a re-initiation — but it never
    /// records a kind (design D-6: the forcing code must always name
    /// a real signal; a stop can never fire rung 2/3).
    StopRequest,
}

/// The ladder's verdict: begin the first-rung action, begin a middle
/// rung's action, or force (restore + immediate exit at
/// [`Signal::exit_code`] of [`Ladder::last_signal`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Action {
    Initiate,
    Escalate,
    Force,
}

#[derive(Debug)]
enum State {
    Fresh,
    Armed { rung: u8 },
    Forced,
}

/// The per-family rung counter (design D-1): fresh → armed rungs →
/// forced, with [`Event::Progress`] de-arming to fresh. Pure; the
/// watcher owns the side effects.
#[derive(Debug)]
pub(crate) struct Ladder {
    middle_rungs: u8,
    state: State,
    last: Option<Signal>,
}

impl Ladder {
    /// A ladder with `middle_rungs` escalate rungs between the first
    /// (`Initiate`) and the last (`Force`): the force rung fires on
    /// the `middle_rungs + 2`-th unacknowledged signal.
    #[must_use]
    pub(crate) fn new(middle_rungs: u8) -> Self {
        Self {
            middle_rungs,
            state: State::Fresh,
            last: None,
        }
    }

    /// The total transition function. Every event is answered; a
    /// `None` means "count/acknowledge only, no action".
    pub(crate) fn on(&mut self, event: Event) -> Option<Action> {
        match (&self.state, event) {
            // Everything past the force rung is inert (no action, no
            // state change — the recorded kind stays for the exit code),
            // a fresh ladder's acknowledgement is a no-op too, and a
            // stop request on an already-armed ladder is idempotent
            // (the drain is already starting; nothing re-triggers).
            (State::Forced, _)
            | (State::Fresh, Event::Progress)
            | (State::Armed { .. }, Event::StopRequest) => None,
            // The control-lane trigger (design D-6): counts as trigger
            // #1 but records no kind — the forcing code must always
            // name a real signal, and a stop can never fire rung 2/3.
            (State::Fresh, Event::StopRequest) => {
                self.state = State::Armed { rung: 1 };
                Some(Action::Initiate)
            }
            (State::Fresh, Event::Signal(signal)) => {
                self.last = Some(signal);
                self.state = State::Armed { rung: 1 };
                Some(Action::Initiate)
            }
            // The acknowledgement: de-arm without acting.
            (State::Armed { .. }, Event::Progress) => {
                self.state = State::Fresh;
                None
            }
            (State::Armed { rung }, Event::Signal(signal)) => {
                self.last = Some(signal);
                if *rung < 1 + self.middle_rungs {
                    self.state = State::Armed { rung: rung + 1 };
                    Some(Action::Escalate)
                } else {
                    self.state = State::Forced;
                    Some(Action::Force)
                }
            }
        }
    }

    /// The last signal the ladder counted — the watcher reads it for
    /// the force exit code.
    #[must_use]
    pub(crate) fn last_signal(&self) -> Option<Signal> {
        self.last
    }
}

// ---------------------------------------------------------------------------
// The watcher — thin glue (design D-1/D-9): correctness above the side
// effects is the pure machine's; the side effects themselves are pinned
// end-to-end by the pty tier (restore-before-exit ordering, force
// codes). Installed exactly once per process surface, on production
// paths only (D-7): the hermetic suites consume injected watches and
// race no real signal.
// ---------------------------------------------------------------------------

/// The terminal state as it was when the session started, shared
/// between the tty guard's drop and the watcher's force/termination
/// restores (idempotent: restoring the saved state twice is a no-op).
/// The mutex is share-across-tasks machinery, not state-machinery
/// contention — the payload is written once at session start.
#[cfg(unix)]
pub(crate) type SavedTty = Arc<std::sync::Mutex<Option<nix::sys::termios::Termios>>>;

/// Non-unix line editing runs no termios line discipline: nothing to
/// share.
#[cfg(not(unix))]
pub(crate) type SavedTty = Arc<()>;

/// Restores the saved line discipline (`TCSADRAIN`). One restore body,
/// two callers: the tty guard's `Drop` and the watcher. Idempotent;
/// errors ignored (SIGHUP's restore fails harmlessly on the dead tty).
#[cfg(unix)]
pub(crate) fn restore_tty(saved: &SavedTty) {
    if let Ok(saved) = saved.lock()
        && let Some(saved) = saved.as_ref()
    {
        let _ = nix::sys::termios::tcsetattr(
            std::io::stdin(),
            nix::sys::termios::SetArg::TCSADRAIN,
            saved,
        );
    }
}

/// Non-unix: nothing to restore.
#[cfg(not(unix))]
pub(crate) fn restore_tty(_saved: &SavedTty) {}

/// Consumes a watch change, or waits forever when the sender is gone.
/// A dropped peer (a dead watcher) must never fire an arm — it would
/// cancel or kill a session out of nowhere, and the `Err`-spinning
/// alternative would busy-loop. One home for the dead-peer policy:
/// both consumers (the REPL loop and the one-shot arms) import it from
/// here.
pub async fn changed_or_never<T>(rx: &mut tokio::sync::watch::Receiver<T>) {
    loop {
        match rx.changed().await {
            Ok(()) => return,
            Err(_) => std::future::pending::<()>().await,
        }
    }
}

/// Installs the one-shot CLI surface's SIGINT watcher (design D-5
/// item 1): the interrupt family only — unix `SIGINT`, Windows
/// console Ctrl-C. The termination family is *not* installed here: the
/// one-shot never touches the tty (non-ceremony verbs run no raw
/// mode), so SIGTERM/SIGHUP keep their default disposition on this
/// surface. Drives one [`Ladder::new(0)`] with **no ack half** — the
/// one-shot's cancellation ends the process, so nothing ever
/// de-arms the ladder and the second SIGINT inside the exit window
/// forces at [`Signal::Interrupt`]'s exit code (130) without the
/// envelope print (D-1's "the force rung is always armed behind the
/// first signal", zero extra machinery).
///
/// Returns the cancel watch (SIGINT epochs: one increment per
/// unacknowledged cancel). The watcher-side side effects: send the
/// epoch on `Initiate`, `std::process::exit` on `Force` (no tty
/// restore — nothing was ever saved). A failed registration degrades
/// to never-fires (the `wait_for_signal` precedent; the dead-peer
/// half is [`changed_or_never`]).
///
/// Orphan completion (D-0, documented not hidden): cancelling a
/// relayed request drops the *wait*; a command already delivered to
/// the daemon still completes — the cancelled envelope's detail text
/// names this.
///
/// Handlers are never on test paths (D-7): production one-shot paths
/// only; hermetic tests inject watches and race no real signal.
#[allow(clippy::too_many_lines)] // one feeder block per cfg'd stream, kept flat
#[must_use]
pub fn install_oneshot_watcher() -> tokio::sync::watch::Receiver<u64> {
    let (cancel_tx, cancel) = tokio::sync::watch::channel(0_u64);

    // The SIGINT family feeder (the session watcher's SIGINT half, the
    // termination streams deliberately absent — see the doc). A failed
    // registration spawns no feeder: the returned watch's sender drops
    // when this fn returns, and `changed_or_never` turns that into an
    // arm that can never fire.
    #[cfg(unix)]
    if let Ok(mut interrupt) =
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())
    {
        tokio::spawn(async move {
            let mut ladder = Ladder::new(0);
            let mut epoch: u64 = 0;
            while interrupt.recv().await.is_some() {
                match ladder.on(Event::Signal(Signal::Interrupt)) {
                    Some(Action::Initiate) => {
                        epoch += 1;
                        let _ = cancel_tx.send(epoch);
                    }
                    Some(Action::Force) => {
                        // No tty restore: the one-shot never saved one.
                        // Flushing std streams is safe (no raw mode).
                        tracing::warn!(
                            code = Signal::Interrupt.exit_code(),
                            "the one-shot signal force rung fired"
                        );
                        std::process::exit(Signal::Interrupt.exit_code());
                    }
                    _ => {}
                }
            }
        });
    }
    #[cfg(not(unix))]
    tokio::spawn(async move {
        let mut ladder = Ladder::new(0);
        let mut epoch: u64 = 0;
        if tokio::signal::ctrl_c().await.is_ok() {
            match ladder.on(Event::Signal(Signal::Interrupt)) {
                Some(Action::Initiate) => {
                    epoch += 1;
                    let _ = cancel_tx.send(epoch);
                }
                _ => {}
            }
        }
        // Windows' one-shot ctrl_c bridge delivers one notification;
        // the ladder never de-arms, and the console's own second-^C
        // disposition is the force rung's world.
    });

    cancel
}

/// Installs the interactive session's signal watcher (design D-1/D-7):
/// the chunk's only signal installation, spawned by `run_session` on
/// the production path. Returns the loop-side watch halves the session
/// loop consumes. The watcher owns one [`Ladder::new(0)`] per signal
/// family — the SIGINT family (cancel → force 130) and, on unix, the
/// external-termination family (restore-then-graceful → force
/// `128 + signum`) — and the two side effects that must never be
/// duplicated: the tty restore (before anything else on the
/// termination family and before every forced exit) and
/// `std::process::exit` on the force rung.
///
/// A stream whose registration fails degrades to "never fires" (the
/// `wait_for_signal` precedent); with no streams at all the watcher
/// simply ends — the session keeps today's no-signal world.
///
/// Windows: no termination analog reaches a normal process (console
/// close is death); the ctrl-C listener is the whole story, and the
/// second ^C keeps the console's default disposition.
#[allow(clippy::too_many_lines)] // one feeder block per cfg'd signal stream, kept flat
pub(crate) fn install_session_watcher(saved: SavedTty) -> crate::repl::SessionSignals {
    let (cancel_tx, cancel) = tokio::sync::watch::channel(0_u64);
    let (ack, ack_rx) = tokio::sync::watch::channel(0_u64);
    let (term_tx, terminate) = tokio::sync::watch::channel(());
    let (pump_tx, mut pump_rx) = tokio::sync::mpsc::unbounded_channel::<Event>();

    // The external-termination family feeders (unix only; D-7's
    // cfg discipline). A failed registration spawns no feeder: that
    // signal keeps its default disposition.
    #[cfg(unix)]
    for (kind, signal) in [
        (
            tokio::signal::unix::SignalKind::terminate(),
            Signal::Terminate,
        ),
        (tokio::signal::unix::SignalKind::hangup(), Signal::Hangup),
    ] {
        if let Ok(mut stream) = tokio::signal::unix::signal(kind) {
            let pump = pump_tx.clone();
            tokio::spawn(async move {
                while stream.recv().await.is_some() {
                    if pump.send(Event::Signal(signal)).is_err() {
                        return;
                    }
                }
            });
        }
    }

    // The SIGINT family feeder: tokio's unix Signal (cancel-safe
    // recv in its own task); on Windows the one-shot ctrl_c bridge.
    #[cfg(unix)]
    if let Ok(mut interrupt) =
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())
    {
        let pump = pump_tx.clone();
        tokio::spawn(async move {
            while interrupt.recv().await.is_some() {
                if pump.send(Event::Signal(Signal::Interrupt)).is_err() {
                    return;
                }
            }
        });
    }
    #[cfg(not(unix))]
    {
        let pump = pump_tx.clone();
        tokio::spawn(async move {
            if tokio::signal::ctrl_c().await.is_ok() {
                let _ = pump.send(Event::Signal(Signal::Interrupt));
            }
        });
    }

    // The machine + side effects. `pump_rx` ends only when every
    // feeder is gone (nothing is installed / all registrations
    // failed): the watcher ends with it — never busy, never acting.
    tokio::spawn(async move {
        let mut interrupt_ladder = Ladder::new(0);
        #[cfg(unix)]
        let mut terminate_ladder = Ladder::new(0);
        let mut epoch: u64 = 0;
        let mut ack_rx = ack_rx;
        loop {
            let event = tokio::select! {
                event = pump_rx.recv() => match event {
                    Some(event) => event,
                    None => return,
                },
                // The loop acknowledged a cancellation: de-arm the
                // SIGINT ladder (no Progress exists for the
                // termination family — its graceful end is exit 0).
                changed = ack_rx.changed() => {
                    if changed.is_err() {
                        // The loop is gone (session ending); nothing
                        // left to cancel or de-arm.
                        return;
                    }
                    Event::Progress
                }
            };
            let action = match event {
                Event::Progress => interrupt_ladder.on(Event::Progress),
                // The session feeders never emit this (it is the
                // lifecycle wiring's control-lane event) — but the
                // match must stay total over the machine's alphabet.
                Event::StopRequest => continue,
                Event::Signal(Signal::Interrupt) => interrupt_ladder.on(event),
                #[cfg(unix)]
                Event::Signal(_) => terminate_ladder.on(event),
                #[cfg(not(unix))]
                Event::Signal(_) => continue,
            };
            match action {
                Some(Action::Initiate) => {
                    if matches!(event, Event::Signal(Signal::Interrupt)) {
                        epoch += 1;
                        let _ = cancel_tx.send(epoch);
                    } else {
                        // Terminate/Hangup: restore FIRST (D-4 — the
                        // pane is sane from the first moment of
                        // shutdown), then hand the loop its graceful
                        // Ctrl-D-shaped end.
                        restore_tty(&saved);
                        let _ = term_tx.send(());
                    }
                }
                Some(Action::Escalate) => unreachable!("the session ladders have no middle rungs"),
                Some(Action::Force) => {
                    // Restore the pane, then leave at 128+signum. The
                    // crate forbids `unsafe`, so this is
                    // `std::process::exit` (the daemon tail's
                    // precedent): the explicit restore above already
                    // ran, so flushing std streams is safe.
                    #[cfg(unix)]
                    let code = terminate_ladder
                        .last_signal()
                        .or_else(|| interrupt_ladder.last_signal())
                        .map_or(130, Signal::exit_code);
                    #[cfg(not(unix))]
                    let code = 130;
                    tracing::warn!(code, "the signal force rung fired");
                    restore_tty(&saved);
                    std::process::exit(code);
                }
                None => {}
            }
        }
    });
    crate::repl::SessionSignals {
        cancel,
        ack,
        terminate,
    }
}

/// The watch halves the lifecycle tail consumes and feeds (design
/// D-1/D-6): the three rungs travel out as `begin_drain` (rung 1) and
/// `escalate` (rung 2, carrying the forcing signal — its
/// [`Signal::exit_code`] names the code), while the tail feeds
/// `stop_request` (the `daemon stop` control lane arms rung 1) and
/// `progress` (the drain future resolved — the de-arm that keeps the
/// exit epilogue from flipping a clean 0 into a forced code). Dead
/// senders mean *never act* ([`changed_or_never`] semantics), so the
/// watcher survives to rung 3 even after its tail is gone.
pub struct LifecycleSignals {
    /// Rung 1 (signal-initiated): the tail's serve select listens.
    pub begin_drain: watch::Receiver<()>,
    /// Rung 2: carries the forcing signal kind.
    pub escalate: watch::Receiver<Signal>,
    /// Fed by the tail's `daemon stop` arm (rung 1 via the machine).
    pub stop_request: watch::Sender<()>,
    /// Fed by the tail when the drain future resolves (de-arm).
    pub progress: watch::Sender<()>,
}

/// Installs the daemon/kiosk/GUI lifecycle's signal watcher (design
/// D-1/D-6): one merged [`Ladder::new(1)`] — the mixed-kind row is
/// this wiring's contract — over the interrupt + termination streams
/// (unix; Windows: the single ctrl-C bridge delivering one
/// [`Signal::Interrupt`], then ending — the console's own second-^C
/// disposition is the force world) and the two side channels
/// ([`LifecycleSignals::stop_request`]/[`LifecycleSignals::progress`])
/// consumed in the same select. Actions: `Initiate` →
/// `begin_drain`; `Escalate` → `escalate` with the forcing signal;
/// `Force` → `std::process::exit(last_signal.exit_code())` — no tty
/// restore (this surface never saves one; the one-shot precedent;
/// `std::process::exit`, not `_exit`, under the crate-wide
/// `forbid(unsafe_code)` — the rungs differ in the salvage attempt,
/// not the exit mechanism).
///
/// A failed stream registration spawns no feeder (the
/// `wait_for_signal` degrade precedent); a dead side-channel sender
/// makes that arm never fire ([`changed_or_never`]) instead of
/// killing the watcher. Handlers are never on test paths (D-7):
/// production lifecycle entries install this immediately after their
/// core boot returns — a signal during the boot itself keeps the
/// default disposition, and an install-at-top would swallow a
/// boot-phase ^C into an `Initiate` nobody listens to yet.
#[allow(clippy::too_many_lines)] // one feeder block per cfg'd stream, kept flat
#[must_use]
pub fn install_lifecycle_watcher() -> LifecycleSignals {
    let (begin_drain_tx, begin_drain) = tokio::sync::watch::channel(());
    let (escalate_tx, escalate) = tokio::sync::watch::channel(Signal::Terminate);
    let (stop_tx, stop_rx) = tokio::sync::watch::channel(());
    let (progress_tx, progress_rx) = tokio::sync::watch::channel(());
    let (pump_tx, mut pump_rx) = tokio::sync::mpsc::unbounded_channel::<Event>();

    // The feeders: unix interrupt + terminate streams pump into one
    // mpsc (the session watcher's shape); SIGHUP/SIGQUIT are not
    // installed — they keep their kernel defaults on this surface.
    #[cfg(unix)]
    for (kind, signal) in [
        (
            tokio::signal::unix::SignalKind::interrupt(),
            Signal::Interrupt,
        ),
        (
            tokio::signal::unix::SignalKind::terminate(),
            Signal::Terminate,
        ),
    ] {
        if let Ok(mut stream) = tokio::signal::unix::signal(kind) {
            let pump = pump_tx.clone();
            tokio::spawn(async move {
                while stream.recv().await.is_some() {
                    if pump.send(Event::Signal(signal)).is_err() {
                        return;
                    }
                }
            });
        }
    }
    #[cfg(not(unix))]
    {
        let pump = pump_tx.clone();
        tokio::spawn(async move {
            if tokio::signal::ctrl_c().await.is_ok() {
                let _ = pump.send(Event::Signal(Signal::Interrupt));
            }
        });
    }

    // The machine + side effects. `pump_rx` ends only when every
    // feeder is gone (all registrations failed): the watcher ends with
    // it — never busy, never acting. Dead side senders park their arm
    // forever (never act), so a dying tail cannot fire an arm.
    tokio::spawn(async move {
        let mut ladder = Ladder::new(1);
        let mut stop_rx = stop_rx;
        let mut progress_rx = progress_rx;
        loop {
            let event = tokio::select! {
                event = pump_rx.recv() => match event {
                    Some(event) => event,
                    None => return,
                },
                changed = stop_rx.changed() => {
                    if changed.is_err() {
                        std::future::pending::<()>().await;
                    }
                    Event::StopRequest
                }
                changed = progress_rx.changed() => {
                    if changed.is_err() {
                        std::future::pending::<()>().await;
                    }
                    Event::Progress
                }
            };
            match ladder.on(event) {
                Some(Action::Initiate) => {
                    let _ = begin_drain_tx.send(());
                }
                Some(Action::Escalate) => {
                    // The escalation is always a real signal's work
                    // (StopRequest never escalates), so the kind is
                    // recorded and names the exit code.
                    if let Some(forcing) = ladder.last_signal() {
                        let _ = escalate_tx.send(forcing);
                    }
                }
                Some(Action::Force) => {
                    let code = ladder.last_signal().map_or(130, Signal::exit_code);
                    tracing::warn!(code, "the daemon signal force rung fired");
                    std::process::exit(code);
                }
                None => {}
            }
        }
    });

    LifecycleSignals {
        begin_drain,
        escalate,
        stop_request: stop_tx,
        progress: progress_tx,
    }
}

#[cfg(test)]
mod tests {
    use super::{Action, Event, Ladder, Signal};

    fn sig(s: Signal) -> Event {
        Event::Signal(s)
    }

    /// The REPL config (`new(0)`), SIGINT family: press → cancel,
    /// second press unacknowledged → force 130, progress de-arms.
    #[test]
    fn repl_config_is_initiate_then_force_and_progress_de_arms() {
        let mut ladder = Ladder::new(0);
        assert_eq!(ladder.on(sig(Signal::Interrupt)), Some(Action::Initiate));
        assert_eq!(ladder.last_signal(), Some(Signal::Interrupt));
        // The acknowledgement (the loop drained/acked the cancel) de-arms.
        assert_eq!(ladder.on(Event::Progress), None);
        assert_eq!(ladder.last_signal(), Some(Signal::Interrupt));
        // Fresh again: the next press is a cancel, not a force.
        assert_eq!(ladder.on(sig(Signal::Interrupt)), Some(Action::Initiate));
        // Unacknowledged second press: force, then the ladder is inert.
        assert_eq!(ladder.on(sig(Signal::Interrupt)), Some(Action::Force));
        assert_eq!(ladder.on(sig(Signal::Interrupt)), None);
        assert_eq!(ladder.on(Event::Progress), None);
    }

    /// The REPL termination family: restore-graceful first, a second
    /// signal during the drain forces at the family's own exit code.
    #[test]
    fn repl_termination_family_restores_then_forces_at_128_plus_signum() {
        let mut ladder = Ladder::new(0);
        assert_eq!(ladder.on(sig(Signal::Terminate)), Some(Action::Initiate));
        assert_eq!(ladder.on(sig(Signal::Hangup)), Some(Action::Force));
        assert_eq!(ladder.last_signal().map(Signal::exit_code), Some(129));
    }

    /// The daemon config (`new(1)`, design D-6's three rungs):
    /// graceful → salvage → force.
    #[test]
    fn daemon_config_graceful_salvage_force() {
        let mut ladder = Ladder::new(1);
        assert_eq!(ladder.on(sig(Signal::Terminate)), Some(Action::Initiate));
        assert_eq!(ladder.on(sig(Signal::Terminate)), Some(Action::Escalate));
        assert_eq!(ladder.on(sig(Signal::Terminate)), Some(Action::Force));
        assert_eq!(ladder.on(sig(Signal::Terminate)), None);
    }

    /// T1 (design D-6, §10 Q1/Q2): `daemon stop` arms rung 1 exactly
    /// like a first signal but records no kind; re-arming is
    /// idempotent, and a forced ladder stays inert to it.
    #[test]
    fn stop_request_arms_rung_1_without_a_kind() {
        let mut ladder = Ladder::new(1);
        assert_eq!(ladder.on(Event::StopRequest), Some(Action::Initiate));
        assert_eq!(ladder.last_signal(), None, "no kind is recorded");
        assert_eq!(ladder.on(Event::StopRequest), None, "idempotent");
        assert_eq!(ladder.last_signal(), None);
        // The stop cannot escalate or force; only a real signal can.
        assert_eq!(ladder.on(sig(Signal::Interrupt)), Some(Action::Escalate));
        assert_eq!(ladder.last_signal(), Some(Signal::Interrupt));
        assert_eq!(ladder.on(Event::StopRequest), None, "inert past Force");
        assert_eq!(ladder.on(Event::Progress), None);
        assert_eq!(ladder.on(Event::StopRequest), Some(Action::Initiate));
        assert_eq!(
            ladder.last_signal(),
            Some(Signal::Interrupt),
            "the recorded kind survives de-arms"
        );
    }

    /// T2 (design D-6): stop-initiated drain + one signal escalates at
    /// the signal's kind; a third trigger forces at that kind's code.
    #[test]
    fn stop_then_signal_escalates_then_forces_at_the_signal_code() {
        let mut ladder = Ladder::new(1);
        assert_eq!(ladder.on(Event::StopRequest), Some(Action::Initiate));
        assert_eq!(ladder.on(sig(Signal::Interrupt)), Some(Action::Escalate));
        assert_eq!(ladder.last_signal().map(Signal::exit_code), Some(130));
        assert_eq!(ladder.on(sig(Signal::Terminate)), Some(Action::Force));
        assert_eq!(ladder.last_signal().map(Signal::exit_code), Some(143));
    }

    /// The mixed-kind row (D-6): kinds count together on one merged
    /// instance — the machine is family-agnostic; the wiring decides.
    #[test]
    fn mixed_kinds_count_together_on_one_instance() {
        let mut ladder = Ladder::new(1);
        assert_eq!(ladder.on(sig(Signal::Interrupt)), Some(Action::Initiate));
        assert_eq!(ladder.on(sig(Signal::Terminate)), Some(Action::Escalate));
        assert_eq!(ladder.last_signal(), Some(Signal::Terminate));
        assert_eq!(ladder.on(sig(Signal::Hangup)), Some(Action::Force));
        assert_eq!(ladder.last_signal().map(Signal::exit_code), Some(129));
    }

    /// Exhaustive invariant sweep over every event sequence of length
    /// ≤ 4 over {`Signal::Interrupt`, `Signal::Terminate`, `Progress`,
    /// `StopRequest`}, for both rung configurations (4⁴ = 256 sequences
    /// × 2 configs). The invariants — not a mirror implementation:
    /// (a) `Force` is answered only to the (2+middle)-th consecutive
    ///     unacknowledged signal;
    /// (b) `Forced` ladders never act again;
    /// (c) `Progress` never acts and always re-arms to `Initiate`-next;
    /// (d) `last_signal` is always the most recently fed signal kind;
    /// (e) exit codes are 128+signum for the recorded kind;
    /// (f) `StopRequest` acts only off `Fresh` (then exactly as trigger
    ///     #1: it counts toward the rung but never escalates, never
    ///     forces, never records a kind).
    #[test]
    #[allow(clippy::too_many_lines)] // the invariant bodies are the test
    fn every_short_sequence_satisfies_the_ladder_invariants() {
        let alphabet = [
            sig(Signal::Interrupt),
            sig(Signal::Terminate),
            Event::Progress,
            Event::StopRequest,
        ];
        for middle in [0u8, 1] {
            // Odometer generation over all sequences of length 1..=4
            // (the empty sequence's behavior is fresh-ladder table
            // territory, pinned by the hand tables above).
            let mut sequences: Vec<Vec<Event>> = Vec::new();
            for len in 1..=4usize {
                let total = alphabet
                    .len()
                    .pow(u32::try_from(len).expect("short lengths"));
                for code in 0..total {
                    let mut seq = Vec::with_capacity(len);
                    let mut rem = code;
                    for _ in 0..len {
                        seq.push(alphabet[rem % 4]);
                        rem /= 4;
                    }
                    sequences.push(seq);
                }
            }

            for seq in &sequences {
                let mut ladder = Ladder::new(middle);
                let mut unacked = 0u32;
                let mut forced = false;
                let mut last_kind: Option<Signal> = None;
                for (step, event) in seq.iter().enumerate() {
                    let action = ladder.on(*event);
                    match event {
                        Event::Signal(s) => {
                            last_kind = Some(*s);
                            if forced {
                                assert_eq!(
                                    action, None,
                                    "acted after Force: seq {seq:?} step {step}"
                                );
                            } else {
                                unacked += 1;
                                match action {
                                    Some(Action::Initiate) => {
                                        assert_eq!(
                                            unacked, 1,
                                            "Initiate off the first rung: seq {seq:?} step {step}"
                                        );
                                    }
                                    Some(Action::Escalate) => {
                                        assert!(
                                            middle > 0 && unacked == 2,
                                            "Escalate only on a middle rung: seq {seq:?} step {step}"
                                        );
                                    }
                                    Some(Action::Force) => {
                                        assert_eq!(
                                            unacked,
                                            u32::from(middle) + 2,
                                            "Force only at the last rung: seq {seq:?} step {step}"
                                        );
                                        assert_eq!(
                                            ladder.last_signal(),
                                            Some(*s),
                                            "force code names the forcing signal"
                                        );
                                        assert_eq!(
                                            ladder.last_signal().map(Signal::exit_code),
                                            Some(s.exit_code()),
                                            "exit code is 128+signum"
                                        );
                                        forced = true;
                                    }
                                    None => {}
                                }
                            }
                        }
                        Event::Progress => {
                            assert_eq!(
                                action, None,
                                "Progress never acts: seq {seq:?} step {step}"
                            );
                            if !forced {
                                unacked = 0;
                                assert_eq!(
                                    ladder.last_signal(),
                                    last_kind,
                                    "the recorded kind survives the de-arm"
                                );
                            }
                        }
                        Event::StopRequest => {
                            if forced {
                                assert_eq!(
                                    action, None,
                                    "acted after Force: seq {seq:?} step {step}"
                                );
                            } else if unacked == 0 {
                                assert_eq!(
                                    action,
                                    Some(Action::Initiate),
                                    "StopRequest arms exactly as trigger #1: seq {seq:?} step {step}"
                                );
                                unacked = 1;
                                assert_eq!(
                                    ladder.last_signal(),
                                    last_kind,
                                    "StopRequest records no kind: seq {seq:?} step {step}"
                                );
                            } else {
                                assert_eq!(
                                    action, None,
                                    "StopRequest never escalates or forces: seq {seq:?} step {step}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}
