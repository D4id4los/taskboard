// SPDX-License-Identifier: MIT OR Apache-2.0
//! Pure poll-scheduling policy for the sync actor: the failure backoff.
//!
//! Doubling from the injected initial delay, saturating at the cap — the
//! same shape as the client's [`crate::backoff::BackoffPolicy`], but over
//! cycle-level failure streaks. The actor's waits ride the tokio timer,
//! which `start_paused` tests advance deterministically (no injected
//! sleeper seam needed).

use std::time::Duration;

/// The wait before the next cycle after `streak` consecutive failures
/// (`streak >= 1`): `initial * 2^(streak - 1)`, capped at `max`, and never
/// below `floor` (the server's advertised `Retry-After`, already clamped
/// to `max` here). `floor` is `Duration::ZERO` when no hint exists.
#[must_use]
pub fn poll_backoff(streak: u32, initial: Duration, max: Duration, floor: Duration) -> Duration {
    if streak == 0 {
        return initial;
    }
    let mut delay = initial;
    for _ in 1..streak {
        match delay.checked_mul(2) {
            Some(doubled) if doubled <= max => delay = doubled,
            _ => return max.max(floor.min(max)),
        }
    }
    delay.min(max).max(floor.min(max))
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const INITIAL: Duration = Duration::from_secs(2);
    const MAX: Duration = Duration::from_secs(60);
    const ZERO: Duration = Duration::ZERO;

    #[test]
    fn backoff_doubles_and_saturates() {
        assert_eq!(poll_backoff(1, INITIAL, MAX, ZERO), Duration::from_secs(2));
        assert_eq!(poll_backoff(2, INITIAL, MAX, ZERO), Duration::from_secs(4));
        assert_eq!(poll_backoff(3, INITIAL, MAX, ZERO), Duration::from_secs(8));
        // 2 * 2^9 would be 1024 s: the cap clamps.
        assert_eq!(poll_backoff(10, INITIAL, MAX, ZERO), MAX);
        assert_eq!(poll_backoff(1_000, INITIAL, MAX, ZERO), MAX);
    }

    #[test]
    fn a_zero_streak_never_occurs_but_stays_total() {
        assert_eq!(poll_backoff(0, INITIAL, MAX, ZERO), INITIAL);
    }

    #[test]
    fn an_initial_above_the_cap_clamps_immediately() {
        assert_eq!(poll_backoff(1, MAX, MAX, ZERO), MAX);
        assert_eq!(poll_backoff(2, MAX, MAX, ZERO), MAX);
    }

    #[test]
    fn a_floor_below_the_curve_leaves_the_curve() {
        assert_eq!(
            poll_backoff(1, INITIAL, MAX, Duration::from_secs(1)),
            INITIAL
        );
        assert_eq!(
            poll_backoff(3, INITIAL, MAX, Duration::from_secs(8)),
            Duration::from_secs(8)
        );
    }

    #[test]
    fn a_floor_between_curve_and_cap_pushes_the_deadline_later() {
        assert_eq!(
            poll_backoff(1, INITIAL, MAX, Duration::from_secs(5)),
            Duration::from_secs(5)
        );
    }

    #[test]
    fn a_floor_above_the_cap_clamps_at_the_cap() {
        assert_eq!(poll_backoff(1, INITIAL, MAX, Duration::from_secs(120)), MAX);
    }

    proptest! {
        #[test]
        fn floor_never_lowers_the_curve_and_the_cap_still_ceils(
            streak in 1u32..40,
            initial in 0u64..120,
            max in 0u64..600,
            floor in 0u64..=u64::MAX,
        ) {
            let (initial, max) = (Duration::from_secs(initial), Duration::from_secs(max));
            let floor = Duration::from_nanos(floor);
            let without = poll_backoff(streak, initial, max, ZERO);
            let with = poll_backoff(streak, initial, max, floor);
            prop_assert!(with >= without);
            prop_assert!(with <= max);
        }
    }
}
