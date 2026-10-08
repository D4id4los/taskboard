// SPDX-License-Identifier: MIT OR Apache-2.0
//! Pure retry-backoff policy for the Deck client.
//!
//! The policy is a pure function of the retry index so it can be
//! property-tested and advanced deterministically under paused tokio time.

use std::time::Duration;

/// Doubling backoff: 500 ms, 1 s, 2 s, ... capped at [`BackoffPolicy::MAX_DELAY`],
/// with a bounded total attempt count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackoffPolicy {
    max_attempts: u32,
    initial_delay: Duration,
    max_delay: Duration,
}

impl BackoffPolicy {
    /// Default policy: 3 attempts total (initial request + 2 retries),
    /// starting at 500 ms and capped at 8 s.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            max_attempts: 3,
            initial_delay: Duration::from_millis(500),
            max_delay: Duration::from_secs(8),
        }
    }

    /// Total number of attempts, including the first request.
    #[must_use]
    pub const fn max_attempts(&self) -> u32 {
        self.max_attempts
    }

    /// Delay before the retry with the given zero-based index.
    ///
    /// Returns `None` once the attempt budget is exhausted. Delays double
    /// per retry and never exceed [`Self::max_delay`].
    #[must_use]
    pub fn delay(&self, retry_index: u32) -> Option<Duration> {
        if retry_index + 1 >= self.max_attempts {
            return None;
        }
        Some(delay_at(self.initial_delay, self.max_delay, retry_index))
    }
}

impl Default for BackoffPolicy {
    fn default() -> Self {
        Self::new()
    }
}

/// Doubles the base delay `retry_index` times, saturating at `max_delay`.
fn delay_at(base: Duration, max: Duration, retry_index: u32) -> Duration {
    let mut d = base;
    let mut i = 0;
    while i < retry_index {
        match d.checked_mul(2) {
            Some(doubled) if doubled <= max => d = doubled,
            _ => return max,
        }
        i += 1;
    }
    d
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn default_sequence_doubles_until_cap() {
        let p = BackoffPolicy::new();
        assert_eq!(p.delay(0), Some(Duration::from_millis(500)));
        assert_eq!(p.delay(1), Some(Duration::from_secs(1)));
        assert_eq!(p.delay(2), None, "3 attempts => 2 retries max");
    }

    #[test]
    fn delays_are_strictly_increasing_until_cap() {
        let p = BackoffPolicy::new();
        let mut prev = Duration::ZERO;
        for i in 0..20 {
            let Some(d) = p.delay(i) else {
                assert!(i >= p.max_attempts() - 1);
                break;
            };
            assert!(d > prev, "delay {d:?} at index {i} not above {prev:?}");
            assert!(d <= Duration::from_secs(8));
            prev = d;
        }
    }

    #[test]
    fn long_retry_run_saturates_at_cap() {
        let p = BackoffPolicy {
            max_attempts: 20,
            ..BackoffPolicy::new()
        };
        // 500 ms * 2^9 would be 256 s; the cap must clamp it to 8 s.
        assert_eq!(p.delay(9), Some(Duration::from_secs(8)));
    }

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(256))]

        #[test]
        fn prop_delays_positive_increasing_capped(retry_index in 0u32..64) {
            let p = BackoffPolicy::new();
            if let Some(d) = p.delay(retry_index) {
                prop_assert!(d > Duration::ZERO);
                prop_assert!(d <= Duration::from_secs(8));
                if retry_index > 0
                    && let Some(prev) = p.delay(retry_index - 1)
                {
                    prop_assert!(d >= prev, "delays must be non-decreasing");
                }
            } else {
                prop_assert!(retry_index + 1 >= p.max_attempts());
            }
        }
    }
}
