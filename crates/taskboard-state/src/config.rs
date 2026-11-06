// SPDX-License-Identifier: MIT OR Apache-2.0
//! Day-roll knobs for the engine's tick arm.
//!
//! The reminder scheduler's knobs (enabled, tick cadence, implicit
//! defaults) no longer live here: since round-5 chunk 5 they are fields
//! of the live [`taskboard_domain::SettingsSnapshot`] the engine owns
//! and re-reads per loop iteration (`spawn_state_engine`'s `settings`
//! argument seeds it; `StateCommand::UpdateSettings` replaces it).

use chrono::FixedOffset;

/// Day-boundary roll knobs (DR-6/2 D-E8). Production boots it `enabled`
/// with the device's fixed offset — there is deliberately **no
/// user-facing switch**: the roll is the recurring-series feature's core
/// materialization, and a kiosk with reminders off still needs its
/// series to appear (anti-configurability, D-C8's finding). Tests inject
/// [`DayRollConfig::disabled`] (the auto-advance policy: a disabled roll
/// never constructs a timer wake).
///
/// The day-start *time* is live settings state
/// (`SettingsSnapshot::day_start_time`); only the UTC offset is
/// boot-fixed (the `resolve_day_start` convention), so the calendar is
/// re-minted per use from the two.
#[derive(Debug, Clone)]
pub struct DayRollConfig {
    /// `false` ⇒ the day tick never runs.
    pub enabled: bool,
    /// The device's fixed UTC offset, resolved once at boot.
    pub offset: FixedOffset,
}

impl DayRollConfig {
    /// The off state used by the state crate's test suites and doc
    /// example; the production wiring never calls it. The offset value
    /// is irrelevant while disabled (UTC keeps the type total without
    /// smuggling in a hidden production policy).
    ///
    /// # Panics
    ///
    /// Never (the zero-offset constructor is infallible by value range).
    #[must_use]
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            offset: FixedOffset::east_opt(0).expect("zero is a valid offset"),
        }
    }
}
