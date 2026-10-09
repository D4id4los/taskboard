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
/// (`streak >= 1`): `initial * 2^(streak - 1)`, capped at `max`.
#[must_use]
pub fn poll_backoff(streak: u32, initial: Duration, max: Duration) -> Duration {
    if streak == 0 {
        return initial;
    }
    let mut delay = initial;
    for _ in 1..streak {
        match delay.checked_mul(2) {
            Some(doubled) if doubled <= max => delay = doubled,
            _ => return max,
        }
    }
    delay.min(max)
}

#[cfg(test)]
mod tests {
    use super::*;

    const INITIAL: Duration = Duration::from_secs(2);
    const MAX: Duration = Duration::from_secs(60);

    #[test]
    fn backoff_doubles_and_saturates() {
        assert_eq!(poll_backoff(1, INITIAL, MAX), Duration::from_secs(2));
        assert_eq!(poll_backoff(2, INITIAL, MAX), Duration::from_secs(4));
        assert_eq!(poll_backoff(3, INITIAL, MAX), Duration::from_secs(8));
        // 2 * 2^9 would be 1024 s: the cap clamps.
        assert_eq!(poll_backoff(10, INITIAL, MAX), MAX);
        assert_eq!(poll_backoff(1_000, INITIAL, MAX), MAX);
    }

    #[test]
    fn a_zero_streak_never_occurs_but_stays_total() {
        assert_eq!(poll_backoff(0, INITIAL, MAX), INITIAL);
    }

    #[test]
    fn an_initial_above_the_cap_clamps_immediately() {
        assert_eq!(poll_backoff(1, MAX, MAX), MAX);
        assert_eq!(poll_backoff(2, MAX, MAX), MAX);
    }
}
