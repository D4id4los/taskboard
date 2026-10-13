// SPDX-License-Identifier: MIT OR Apache-2.0
//! Reminder scheduler knobs for the engine's tick arm.

use std::time::Duration;

/// Reminder scheduler knobs; chunk 3 wires them from figment's
/// `[reminders]` table. Tests inject tiny/paused values.
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
    /// The off state used by every pre-existing test helper and the
    /// interim bootstrap wiring. The `defaults` value is irrelevant
    /// while disabled; production defaults (5 m before due) are
    /// figment's to encode in chunk 3 — deliberately not encoded here
    /// so this type carries no hidden production policy.
    #[must_use]
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            tick: Duration::from_secs(30),
            defaults: taskboard_domain::ReminderDefaults::default(),
        }
    }
}
