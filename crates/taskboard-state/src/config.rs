// SPDX-License-Identifier: MIT OR Apache-2.0
//! Reminder scheduler and day-roll knobs for the engine's tick arm.

use std::time::Duration;

use taskboard_domain::DayCalendar;

/// Reminder scheduler knobs; `taskboard-app` maps figment's
/// `[reminders]` table onto them (`AppConfig::reminder_engine_config`).
/// Tests inject tiny/paused values.
#[derive(Debug, Clone)]
pub struct ReminderEngineConfig {
    /// `false` ⇒ the tick arm is never armed (the default-off
    /// test-helper policy; the auto-advance guard pins it).
    pub enabled: bool,
    /// Fixed tick cadence (production default 30 s, design §5.4).
    pub tick: Duration,
    /// Injected per-device defaults for the implicit reminder.
    pub defaults: taskboard_domain::ReminderDefaults,
}

impl ReminderEngineConfig {
    /// The off state used by the state crate's test suites and doc
    /// example; the production wiring never calls it — it builds the
    /// config from figment's `[reminders]` table
    /// (`AppConfig::reminder_engine_config`). The `defaults` value is
    /// irrelevant while disabled; the production defaults (5 m before
    /// due, 30 s tick) live in the app's figment defaults —
    /// deliberately not encoded here so this type carries no hidden
    /// production policy.
    #[must_use]
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            tick: Duration::from_secs(30),
            defaults: taskboard_domain::ReminderDefaults::default(),
        }
    }
}

/// Day-boundary roll knobs (DR-6/2 D-E8). Production boots it `enabled`
/// from the validated `[preferences] day_start_time` plus the device's
/// fixed offset — there is deliberately **no user-facing switch**: the
/// roll is the recurring-series feature's core materialization, and a
/// kiosk with reminders off still needs its series to appear
/// (anti-configurability, D-C8's finding). Tests inject
/// [`DayRollConfig::disabled`] (the auto-advance policy: a disabled
/// roll never constructs a timer wake).
#[derive(Debug, Clone)]
pub struct DayRollConfig {
    /// `false` ⇒ the day tick never runs.
    pub enabled: bool,
    /// The injected day calendar (the D-F6 vocabulary): the window-date
    /// "today" and the plain-calendar/instant conversions.
    pub calendar: DayCalendar,
}

impl DayRollConfig {
    /// The off state used by the state crate's test suites and doc
    /// example; the production wiring never calls it. The calendar value
    /// is irrelevant while disabled (UTC midnight keeps the type
    /// total without smuggling in a hidden production policy).
    ///
    /// # Panics
    ///
    /// Never (the zero-offset constructor is infallible by value range).
    #[must_use]
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            calendar: DayCalendar {
                day_start_time: chrono::NaiveTime::MIN,
                offset: chrono::FixedOffset::east_opt(0).expect("zero is a valid offset"),
            },
        }
    }
}
