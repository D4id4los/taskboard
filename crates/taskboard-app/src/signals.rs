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

/// A signal family member the watcher listened to. Unix-only kinds are
/// constructed only under `cfg(unix)` (the watcher's cfg-gated
/// streams); the type itself is total so the machine and its tests are
/// platform-independent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // wired by the session watcher (lands later in this chunk)
pub(crate) enum Signal {
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
    #[allow(dead_code)] // wired by the session watcher
    pub(crate) fn exit_code(self) -> i32 {
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
#[allow(dead_code)] // wired by the session watcher
pub(crate) enum Event {
    Signal(Signal),
    Progress,
}

/// The ladder's verdict: begin the first-rung action, begin a middle
/// rung's action, or force (restore + immediate exit at
/// [`Signal::exit_code`] of [`Ladder::last_signal`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // wired by the session watcher
pub(crate) enum Action {
    Initiate,
    Escalate,
    Force,
}

#[derive(Debug)]
#[allow(dead_code)] // wired by the session watcher
enum State {
    Fresh,
    Armed { rung: u8 },
    Forced,
}

/// The per-family rung counter (design D-1): fresh → armed rungs →
/// forced, with [`Event::Progress`] de-arming to fresh. Pure; the
/// watcher owns the side effects.
#[derive(Debug)]
#[allow(dead_code)] // wired by the session watcher
pub(crate) struct Ladder {
    middle_rungs: u8,
    state: State,
    last: Option<Signal>,
}

#[allow(dead_code)] // wired by the session watcher
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
            // and a fresh ladder's acknowledgement is a no-op too.
            (State::Forced, _) | (State::Fresh, Event::Progress) => None,
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
    /// ≤ 4 over {Signal(Interrupt), Signal(Terminate), Progress}, for
    /// both rung configurations (3⁴ = 81 sequences × 2 configs). The
    /// invariants — not a mirror implementation:
    /// (a) `Force` is answered only to the (2+middle)-th consecutive
    ///     unacknowledged signal;
    /// (b) `Forced` ladders never act again;
    /// (c) `Progress` never acts and always re-arms to `Initiate`-next;
    /// (d) `last_signal` is always the most recently fed signal kind;
    /// (e) exit codes are 128+signum for the recorded kind.
    #[test]
    fn every_short_sequence_satisfies_the_ladder_invariants() {
        let alphabet = [
            sig(Signal::Interrupt),
            sig(Signal::Terminate),
            Event::Progress,
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
                        seq.push(alphabet[rem % 3]);
                        rem /= 3;
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
                    }
                }
            }
        }
    }
}
