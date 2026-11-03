// SPDX-License-Identifier: MIT OR Apache-2.0
//! Focus-mode block durations ([`FocusTiming`], issue #496): injected
//! configuration data consumed by the app-layer focus-session runtime
//! and the ui-slint concentration overlay.
//!
//! The type carries **no serde impl** — it is never persisted and never
//! roamed (the `ReminderDefaults` precedent): config stores raw integer
//! minutes and mints through [`FocusTiming::try_new`] at the config-load
//! trust boundary, so a zero-length block can never enter a consumer.

use chrono::TimeDelta;

/// Focus-mode block durations. Private fields; the fallible mint is
/// the config-load trust boundary — both the work block and the respite
/// block are strictly positive whole-second durations (a zero-length
/// block would fire its transition instantly and forever).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FocusTiming {
    work: TimeDelta,
    respite: TimeDelta,
}

/// The focus-timing predicate failed, naming the offending field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum InvalidFocusTiming {
    /// The work block duration is not strictly positive whole seconds.
    #[error("focus work duration must be a positive whole number of seconds")]
    Work,
    /// The respite (break) block duration is not strictly positive
    /// whole seconds.
    #[error("focus respite duration must be a positive whole number of seconds")]
    Respite,
}

impl FocusTiming {
    /// The mint: accepts the timing only when both durations are
    /// strictly positive and whole-second (the
    /// [`crate::tracking::WorkThreshold`] rules — every consumer
    /// derives deadlines by adding these to entry starts, and a
    /// sub-second component would re-cross at a moved instant).
    ///
    /// # Errors
    ///
    /// [`InvalidFocusTiming::Work`] / [`InvalidFocusTiming::Respite`]
    /// when the respective duration is not strictly positive or carries
    /// a sub-second component.
    pub fn try_new(work: TimeDelta, respite: TimeDelta) -> Result<Self, InvalidFocusTiming> {
        let valid = |dur: TimeDelta| dur > TimeDelta::zero() && dur.subsec_nanos() == 0;
        if !valid(work) {
            return Err(InvalidFocusTiming::Work);
        }
        if !valid(respite) {
            return Err(InvalidFocusTiming::Respite);
        }
        Ok(Self { work, respite })
    }

    /// The validated work-block duration (strictly positive always).
    #[must_use]
    pub const fn work(self) -> TimeDelta {
        self.work
    }

    /// The validated respite (break) block duration (strictly positive
    /// always).
    #[must_use]
    pub const fn respite(self) -> TimeDelta {
        self.respite
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn focus_timing_mints_positive_whole_second_durations() {
        let timing = FocusTiming::try_new(Duration::minutes(25), Duration::minutes(5))
            .expect("positive whole-second durations mint");
        assert_eq!(timing.work(), Duration::minutes(25));
        assert_eq!(timing.respite(), Duration::minutes(5));
    }

    #[test]
    fn zero_or_negative_work_or_respite_rejected_typed() {
        for (work, respite, expected) in [
            (
                Duration::zero(),
                Duration::minutes(5),
                InvalidFocusTiming::Work,
            ),
            (
                Duration::minutes(-25),
                Duration::minutes(5),
                InvalidFocusTiming::Work,
            ),
            (
                Duration::minutes(25),
                Duration::zero(),
                InvalidFocusTiming::Respite,
            ),
            (
                Duration::minutes(25),
                Duration::seconds(-5),
                InvalidFocusTiming::Respite,
            ),
        ] {
            let err = FocusTiming::try_new(work, respite)
                .expect_err("a non-positive duration never mints");
            assert_eq!(err, expected);
        }
    }

    #[test]
    fn sub_second_duration_rejected_typed() {
        let err = FocusTiming::try_new(
            Duration::seconds(25) + Duration::milliseconds(500),
            Duration::minutes(5),
        )
        .expect_err("a sub-second duration never mints");
        assert_eq!(err, InvalidFocusTiming::Work);
    }

    #[test]
    fn focus_timing_accessors_round_trip() {
        let timing = FocusTiming::try_new(Duration::seconds(1500), Duration::seconds(300))
            .expect("positive whole-second durations mint");
        assert_eq!(timing.work().num_seconds(), 1500);
        assert_eq!(timing.respite().num_seconds(), 300);
    }
}
