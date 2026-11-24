// SPDX-License-Identifier: MIT OR Apache-2.0
//! Deterministic [`Clock`] doubles shared across the workspace's test
//! tiers (issue #1264): one frozen instant, one hand-driven cell, one
//! stepping read — the three shapes every local double collapsed onto.
//! Clock-discipline law (`testing_strategy` §6.1–6.2): a fixture never
//! reads the wall clock; it injects one of these.

use std::sync::Mutex;
use std::sync::atomic::{AtomicI64, Ordering};

use chrono::{DateTime, TimeDelta, Utc};

use crate::clock::Clock;

fn instant_from_secs(secs: i64) -> DateTime<Utc> {
    use chrono::TimeZone as _;
    Utc.timestamp_opt(secs, 0).single().expect("in-range stamp")
}

/// A clock pinned at one instant: every read observes the same time.
#[derive(Debug, Clone, Copy)]
pub struct FrozenClock(DateTime<Utc>);

impl FrozenClock {
    /// A clock frozen at `at`.
    #[must_use]
    pub fn new(at: DateTime<Utc>) -> Self {
        Self(at)
    }

    /// A clock frozen at the whole-second stamp `secs`.
    ///
    /// # Panics
    ///
    /// Panics if `secs` is outside chrono's representable range — a
    /// fixture constant bug, not an input condition.
    #[must_use]
    pub fn from_secs(secs: i64) -> Self {
        Self::new(instant_from_secs(secs))
    }

    /// The pinned instant (for fixtures that assert a minted stamp
    /// against the fixture's own constant).
    #[must_use]
    pub fn instant(&self) -> DateTime<Utc> {
        self.0
    }
}

impl Clock for FrozenClock {
    fn now(&self) -> DateTime<Utc> {
        self.0
    }
}

/// A clock the test drives by hand: `set`/`advance` move the single
/// shared cell, so a fixture controls every instant a spawned actor
/// observes — deterministically, never sleeping.
#[derive(Debug)]
pub struct ManualClock(Mutex<DateTime<Utc>>);

impl ManualClock {
    /// A clock starting at `at`.
    #[must_use]
    pub fn new(at: DateTime<Utc>) -> Self {
        Self(Mutex::new(at))
    }

    /// A clock starting at the whole-second stamp `secs`.
    ///
    /// # Panics
    ///
    /// Panics if `secs` is outside chrono's representable range — a
    /// fixture constant bug, not an input condition.
    #[must_use]
    pub fn from_secs(secs: i64) -> Self {
        Self::new(instant_from_secs(secs))
    }

    /// Jump the shared cell to `at`.
    ///
    /// # Panics
    ///
    /// Panics only if the cell's mutex is poisoned — a fixture bug, not
    /// an input condition.
    pub fn set(&self, at: DateTime<Utc>) {
        *self.0.lock().expect("clock mutex") = at;
    }

    /// Jump the shared cell to the whole-second stamp `secs`.
    ///
    /// # Panics
    ///
    /// Panics if `secs` is outside chrono's representable range, or if
    /// the cell's mutex is poisoned — fixture bugs, not input
    /// conditions.
    pub fn set_secs(&self, secs: i64) {
        self.set(instant_from_secs(secs));
    }

    /// Move the shared cell forward by `delta` (negative deltas are
    /// tolerated but a fixture has no reason to reach backwards).
    ///
    /// # Panics
    ///
    /// Panics if the advanced result overflows chrono's range — a
    /// fixture bug, not an input condition.
    pub fn advance(&self, delta: TimeDelta) {
        *self.0.lock().expect("clock mutex") += delta;
    }

    /// Move the shared cell forward by `secs` whole seconds.
    ///
    /// # Panics
    ///
    /// Panics if `secs` is outside chrono's representable range or the
    /// advanced result overflows — a fixture constant bug, not an input
    /// condition.
    pub fn advance_secs(&self, secs: i64) {
        self.advance(TimeDelta::try_seconds(secs).expect("in-range step"));
    }
}

impl Default for ManualClock {
    /// Starts at the Unix epoch; a fixture sets the cell before any
    /// actor observes it.
    fn default() -> Self {
        Self::new(instant_from_secs(0))
    }
}

impl Clock for ManualClock {
    fn now(&self) -> DateTime<Utc> {
        *self.0.lock().expect("clock mutex")
    }
}

/// A clock that advances a fixed whole-second step per read: later
/// stamps always compare newer (freshness predicates, the self-clobber
/// ordering) while staying deterministic.
#[derive(Debug)]
pub struct StepClock {
    next_secs: AtomicI64,
    step_secs: i64,
}

impl StepClock {
    /// A stepping clock from the whole-second stamp `start_secs`,
    /// advancing `step_secs` per read.
    #[must_use]
    pub fn new(start_secs: i64, step_secs: i64) -> Self {
        Self {
            next_secs: AtomicI64::new(start_secs),
            step_secs,
        }
    }

    /// A stepping clock from the instant `start`, advancing `step_secs`
    /// per read.
    #[must_use]
    pub fn at(start: DateTime<Utc>, step_secs: i64) -> Self {
        Self::new(start.timestamp(), step_secs)
    }
}

impl Clock for StepClock {
    fn now(&self) -> DateTime<Utc> {
        let secs = self.next_secs.fetch_add(self.step_secs, Ordering::Relaxed);
        instant_from_secs(secs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // One determinism pin per double, lowest layer (AGENTS §5): the
    // frozen read is constant; the manual cell moves only when driven;
    // the stepping read is strictly ordered.

    #[test]
    fn frozen_clock_returns_the_pinned_instant() {
        let at = instant_from_secs(1_700_000_000);
        let clock = FrozenClock::new(at);
        assert_eq!(clock.now(), at);
        assert_eq!(clock.now(), clock.instant());
    }

    #[test]
    fn manual_clock_moves_only_when_driven() {
        let clock = ManualClock::from_secs(1_000);
        assert_eq!(clock.now(), instant_from_secs(1_000));
        clock.advance_secs(30);
        assert_eq!(clock.now(), instant_from_secs(1_030));
        clock.set_secs(2_000);
        assert_eq!(clock.now(), instant_from_secs(2_000));
        clock.advance(TimeDelta::try_seconds(5).unwrap());
        assert_eq!(clock.now(), instant_from_secs(2_005));
    }

    #[test]
    fn stepping_clock_reads_are_strictly_newer() {
        let clock = StepClock::new(10_000, 30);
        let a = clock.now();
        let b = clock.now();
        assert_eq!(b - a, TimeDelta::try_seconds(30).unwrap());
        assert_eq!(clock.now(), instant_from_secs(10_060));
    }
}
