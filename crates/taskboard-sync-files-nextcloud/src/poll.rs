// SPDX-License-Identifier: MIT OR Apache-2.0
//! The cycle-level failure backoff: a verbatim mirror of
//! `taskboard-sync-nextcloud`'s `poll_backoff` (the same doubling
//! curve, cap, and `Retry-After` floor). The Deck copy stays the
//! law-tested home of the policy — this crate deliberately does not
//! depend on the Deck connector crate (SRP: the files transport is
//! not a board connector), so the 15-line pure function is mirrored,
//! not imported; the floor plumbing gets exactly the local tests the
//! chunk plan names.

use std::time::Duration;

/// The wait before the next cycle after `streak` consecutive failures
/// (`streak >= 1`): `initial * 2^(streak - 1)`, capped at `max`, and
/// never below `floor` (the server's advertised `Retry-After`, already
/// clamped to `max` here). `floor` is `Duration::ZERO` when no hint
/// exists.
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

    const INITIAL: Duration = Duration::from_secs(30);
    const MAX: Duration = Duration::from_secs(600);

    #[test]
    fn the_floor_plumbing_pushes_only_between_curve_and_cap() {
        let zero = Duration::ZERO;
        assert_eq!(poll_backoff(1, INITIAL, MAX, zero), INITIAL);
        assert_eq!(
            poll_backoff(1, INITIAL, MAX, Duration::from_secs(45)),
            Duration::from_secs(45),
            "a floor above the curve extends the wait"
        );
        assert_eq!(
            poll_backoff(1, INITIAL, MAX, Duration::from_secs(3_600)),
            MAX,
            "a floor above the cap clamps at the cap"
        );
    }
}
