// SPDX-License-Identifier: MIT OR Apache-2.0
//! Time seam.
//!
//! Merge functions never read the clock: time is always *data* passed
//! into them, keeping the policy total, deterministic, and
//! Miri/mutants friendly. Production wiring uses [`SystemClock`]; tests
//! inject a clock returning fixed or advancing logical times. Nothing in
//! this crate calls `Utc::now()` directly. (The identity seam lives in
//! [`crate::idgen`].)

use chrono::{DateTime, Utc};

/// Source of the current wall-clock time.
///
/// Production uses [`SystemClock`]; tests inject a clock returning fixed
/// or advancing logical times. Deterministic by construction.
pub trait Clock: Send + Sync + std::fmt::Debug {
    /// The current instant.
    fn now(&self) -> DateTime<Utc>;
}

/// Production clock: `Utc::now()` via chrono's `clock` feature.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_clock_returns_utc_instant() {
        let before = Utc::now();
        let now = SystemClock.now();
        let after = Utc::now();
        assert!(before <= now && now <= after);
    }
}
