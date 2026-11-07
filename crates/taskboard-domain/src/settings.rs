// SPDX-License-Identifier: MIT OR Apache-2.0
//! The live settings snapshot ([`SettingsSnapshot`], GUI review round
//! 5 chunk 5): the typed, validated projection of every editable
//! General / Reminders / Sync-Timing / Display setting that the state
//! engine owns and every consuming actor re-reads per iteration.
//!
//! The snapshot is a **value, not a patch**: the app lane assembles the
//! full snapshot from the freshly proven config file after every
//! per-field commit and sends it whole through
//! `StateCommand::UpdateSettings`; the engine replaces its copy
//! wholesale. No partial-update variant exists.
//!
//! Minting discipline (the house A1 pattern): the constrained scalars
//! are private-field newtypes with a fallible constructor at the trust
//! boundary — the app lane mints from a `validate()`-proven config, so
//! an invalid value is unrepresentable by the time it reaches the
//! engine, which therefore never re-validates. The serde lane is the
//! documented exception: `Deserialize` re-mints *leniently* (an
//! out-of-range integer decodes to the type's default), because the
//! serialized-state readers (snapshot tests, future embedders) are not
//! a trust boundary — the command path is, and there minting is
//! fallible and refusing is the contract.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::days::DayCalendar;
use crate::focus::FocusTiming;
use crate::reminders::{ReminderDefaults, ReminderOffset};

/// The `[sync] poll_interval_secs` default: the sync lane keeps its
/// 30 s cadence — the deadline-driven reminder chunk (round-5 chunk 6)
/// split the reminder lane off this shared default. Also
/// [`TickSecs`]'s serde-leniency target (zero decodes to it) — the
/// leniency is type-level, so both lanes share the fallback; the
/// reminder lane's *snapshot* default is minted explicitly below.
const DEFAULT_SECS: u64 = 30;
/// The `[reminders] tick_secs` default (round-5 chunk 6, D2): the tick
/// is the fallback sweep, never the firing mechanism — 1 s keeps the
/// worst-case firing lag at one second on a quiet deadline query.
const DEFAULT_TICK_SECS: u64 = 1;
/// The `[sync] backoff_initial_secs` default.
const DEFAULT_BACKOFF_INITIAL_SECS: u64 = 5;
/// The `[sync] backoff_max_secs` default.
const DEFAULT_BACKOFF_MAX_SECS: u64 = 300;
/// The `[ui] due_soon_days` default (one day).
const DEFAULT_DUE_SOON_DAYS: u32 = 1;
/// The `[ui] due_soon_days` garbage-input ceiling (ten years of days)
/// — the config layer's `InvalidDueSoonDays` rule, mirrored.
const DUE_SOON_DAYS_MAX: u32 = 3652;

/// A whole-second cadence value (≥ 1): the `[reminders] tick_secs` /
/// `[sync] poll_interval_secs` / `[sync] backoff_*_secs` validation
/// kind (the config's `InvalidDuration` arm), minted once here.
///
/// Hand-rolled A1 newtype: private field, fallible [`TickSecs::new`]
/// rejecting zero. The serde decode is **lenient by documented
/// choice** (zero decodes to [`TickSecs::default`]) — see the module
/// docs for why the serialized lane is not the trust boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "u64", into = "u64")]
pub struct TickSecs(u64);

/// The cadence mint rejected zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("cadence seconds must be at least 1")]
pub struct InvalidTickSecs;

impl TickSecs {
    /// The mint: `secs` must be at least 1.
    ///
    /// # Errors
    ///
    /// [`InvalidTickSecs`] when `secs` is 0.
    pub const fn new(secs: u64) -> Result<Self, InvalidTickSecs> {
        if secs == 0 {
            Err(InvalidTickSecs)
        } else {
            Ok(Self(secs))
        }
    }

    /// The seconds (≥ 1 always).
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    /// The cadence as a `std` duration (the timer arms' input shape).
    #[must_use]
    pub const fn duration(self) -> Duration {
        Duration::from_secs(self.0)
    }
}

impl Default for TickSecs {
    /// The sync lane's cadence default (30 s) and the serde leniency
    /// target (the shared type cannot split the lanes' leniency; the
    /// reminder lane's snapshot default is minted explicitly).
    fn default() -> Self {
        Self(DEFAULT_SECS)
    }
}

/// The lenient serde re-mint: zero decodes to the default (the
/// serialized lane is not the trust boundary — module docs). The
/// `TryFrom` shape is serde's `try_from` contract, hence the lint
/// grant over `From`.
#[allow(clippy::infallible_try_from)]
impl TryFrom<u64> for TickSecs {
    type Error = std::convert::Infallible;

    fn try_from(secs: u64) -> Result<Self, Self::Error> {
        Ok(Self::new(secs).unwrap_or_default())
    }
}

impl From<TickSecs> for u64 {
    fn from(secs: TickSecs) -> Self {
        secs.0
    }
}

/// The `[ui] due_soon_days` horizon (0 ≤ days ≤ 3652): zero is legal
/// (no "soon" set beyond today), ten years of days is the typo
/// ceiling — the config's `InvalidDueSoonDays` rule, minted once here.
///
/// Hand-rolled A1 newtype; the serde decode is lenient (an over-ceiling
/// value decodes to [`DueSoonDays::default`]) — module docs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "u32", into = "u32")]
pub struct DueSoonDays(u32);

/// The horizon mint exceeded the ten-year ceiling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("due-soon horizon must be at most 3652 days")]
pub struct InvalidDueSoonDays;

impl DueSoonDays {
    /// The mint: `days` must be at most 3652 (zero is legal).
    ///
    /// # Errors
    ///
    /// [`InvalidDueSoonDays`] when `days` exceeds the ceiling.
    pub const fn new(days: u32) -> Result<Self, InvalidDueSoonDays> {
        if days > DUE_SOON_DAYS_MAX {
            Err(InvalidDueSoonDays)
        } else {
            Ok(Self(days))
        }
    }

    /// The day count (≤ 3652 always).
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }

    /// The horizon as a chrono delta (the UI's due-soon derivation
    /// input shape).
    #[must_use]
    pub fn horizon(self) -> chrono::TimeDelta {
        chrono::TimeDelta::days(i64::from(self.0))
    }
}

impl Default for DueSoonDays {
    /// One day.
    fn default() -> Self {
        Self(DEFAULT_DUE_SOON_DAYS)
    }
}

/// The lenient serde re-mint: an over-ceiling value decodes to the
/// default (module docs; the `TryFrom` shape is serde's contract).
#[allow(clippy::infallible_try_from)]
impl TryFrom<u32> for DueSoonDays {
    type Error = std::convert::Infallible;

    fn try_from(days: u32) -> Result<Self, Self::Error> {
        Ok(Self::new(days).unwrap_or_default())
    }
}

impl From<DueSoonDays> for u32 {
    fn from(days: DueSoonDays) -> Self {
        days.0
    }
}

/// The `[reminders] sink` choice, mirrored into the domain so the
/// snapshot carries it typed (the app's `SinkChoice` and the UI lane's
/// `ReminderSink` map onto it at their boundaries).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ReminderSinkChoice {
    /// Resolve by run mode at boot.
    #[default]
    Auto,
    /// The log trail.
    Log,
    /// Desktop toasts.
    Desktop,
}

/// The live settings projection the engine owns (module docs).
///
/// The type carries **no cross-field invariant**: `backoff_initial_secs
/// ≤ backoff_max_secs` is enforced where it lives today — the config's
/// `validate()` ladder and the overlay's prove gate refuse an ordering
/// violation before any mint. `focus` is `None` while the `[focus]`
/// section is disabled (the `focus_config()` precedent); the
/// `focus_enabled` flag is carried separately because the apply
/// registry treats enabling as restart-scoped while a running runtime
/// honors a live disable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
// One bool per independent config knob (the UI lane's
// `SettingsSnapshot` precedent): grouping them would invent couplings
// the config does not have.
#[allow(clippy::struct_excessive_bools)]
pub struct SettingsSnapshot {
    /// `[preferences] day_start_time`, already typed.
    pub day_start_time: chrono::NaiveTime,
    /// `[reminders] enabled`.
    pub reminders_enabled: bool,
    /// `[reminders] tick_secs`.
    pub tick_secs: TickSecs,
    /// `[reminders] default_before` in the domain's sign convention
    /// (≤ 0, "added to duedate").
    pub reminder_before: ReminderOffset,
    /// `[reminders] sink`.
    pub reminder_sink: ReminderSinkChoice,
    /// `[sync] poll_interval_secs`.
    pub poll_interval_secs: TickSecs,
    /// `[sync] backoff_initial_secs`.
    pub backoff_initial_secs: TickSecs,
    /// `[sync] backoff_max_secs`.
    pub backoff_max_secs: TickSecs,
    /// `[ui] due_soon_days`.
    pub due_soon_days: DueSoonDays,
    /// `[focus] enabled`.
    pub focus_enabled: bool,
    /// The minted `[focus]` timings; `None` while the section is off.
    #[serde(with = "focus_timing_serde")]
    pub focus: Option<FocusTiming>,
    /// `[focus] chain`.
    pub focus_chain: bool,
    /// `[focus] notify`.
    pub focus_notify: bool,
}

impl Default for SettingsSnapshot {
    /// The test-policy snapshot (the retired `ReminderEngineConfig::
    /// disabled()` precedent): reminders **off** so a fixture-built
    /// engine performs zero timer work, every cadence at its production
    /// default, midnight day start, focus off. The production boot
    /// never uses it — the app mints from the validated config.
    fn default() -> Self {
        Self {
            day_start_time: chrono::NaiveTime::MIN,
            reminders_enabled: false,
            // The reminder lane's own default (1 s), minted explicitly —
            // `TickSecs::default` is the sync lane's 30 s.
            tick_secs: TickSecs(DEFAULT_TICK_SECS),
            reminder_before: ReminderOffset::at_due(),
            reminder_sink: ReminderSinkChoice::Auto,
            poll_interval_secs: TickSecs::default(),
            backoff_initial_secs: TickSecs(DEFAULT_BACKOFF_INITIAL_SECS),
            backoff_max_secs: TickSecs(DEFAULT_BACKOFF_MAX_SECS),
            due_soon_days: DueSoonDays::default(),
            focus_enabled: false,
            focus: None,
            focus_chain: false,
            focus_notify: false,
        }
    }
}

impl SettingsSnapshot {
    /// The implicit-reminder defaults the planner and the firing
    /// evaluator take.
    #[must_use]
    pub const fn reminder_defaults(&self) -> ReminderDefaults {
        ReminderDefaults {
            default_before: self.reminder_before,
        }
    }

    /// The day calendar for one device: the live day-start time paired
    /// with the boot-fixed UTC offset (the `resolve_day_start`
    /// convention — the offset is device-boot state, never a setting).
    #[must_use]
    pub const fn day_calendar(&self, offset: chrono::FixedOffset) -> DayCalendar {
        DayCalendar {
            day_start_time: self.day_start_time,
            offset,
        }
    }

    /// The focus timing a running runtime honors: `Some` only while
    /// the mode is enabled *and* the timings are minted — a live
    /// disable parks the runtime even though the timings still exist.
    #[must_use]
    pub fn live_focus(&self) -> Option<FocusTiming> {
        if self.focus_enabled { self.focus } else { None }
    }
}

/// The `settings` wire section's roaming subset (DR-9 D-BF3): the
/// user-preference settings that sync across devices, classified
/// against the D5 notes — a roamer is **user intent** (identical on a
/// kiosk and a desktop); machine facts (cadence tuning, notification
/// capability, display hardware, paths, secrets) stay device-local.
/// The classification is pinned as data by
/// `roaming_setting_names_is_exactly_the_classified_list` — adding a
/// roamer means extending this list (the pin fails first); a machine-
/// or secret-adjacent key can never enter silently.
pub const ROAMING_SETTING_NAMES: &[&str] = &[
    "day_start_time",
    "reminders_enabled",
    "reminder_before",
    "due_soon_days",
    "focus_enabled",
    "focus_timing",
    "focus_chain",
    "focus_notify",
];

/// The typed value of one roaming setting (the variants above, each
/// carrying its validated domain type). The serde lane rides each
/// value's own serde; a decode that fails a mint fails the row (the
/// wire codec's fail-closed `BadBody`, never a silent default) —
/// except `DueSoonDays` and `FocusTiming`, whose *documented* lenient
/// serde this lane deliberately reuses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RoamingSettingValue {
    /// `[preferences] day_start_time`.
    DayStartTime(chrono::NaiveTime),
    /// `[reminders] enabled`.
    RemindersEnabled(bool),
    /// `[reminders] default_before` (the domain sign convention).
    ReminderBefore(ReminderOffset),
    /// `[ui] due_soon_days`.
    DueSoonDays(DueSoonDays),
    /// `[focus] enabled`.
    FocusEnabled(bool),
    /// `[focus]` timings (`None` while disabled/unminted — the
    /// snapshot's own serde lane).
    FocusTiming(Option<FocusTiming>),
    /// `[focus] chain`.
    FocusChain(bool),
    /// `[focus] notify`.
    FocusNotify(bool),
}

impl RoamingSettingValue {
    /// The stable wire/storage key of this setting — one arm per
    /// variant, and the strings are exactly
    /// [`ROAMING_SETTING_NAMES`] (the data pin holds them together).
    #[must_use]
    pub const fn key_name(&self) -> &'static str {
        match self {
            Self::DayStartTime(_) => "day_start_time",
            Self::RemindersEnabled(_) => "reminders_enabled",
            Self::ReminderBefore(_) => "reminder_before",
            Self::DueSoonDays(_) => "due_soon_days",
            Self::FocusEnabled(_) => "focus_enabled",
            Self::FocusTiming(_) => "focus_timing",
            Self::FocusChain(_) => "focus_chain",
            Self::FocusNotify(_) => "focus_notify",
        }
    }

    /// Mints a value from its key name and its serde body. The fallible
    /// constructor at the settings-section trust boundary: an unknown
    /// name (a future build's roamer) or a shape that fails the value's
    /// own decode is `None` — the wire codec rejects the row, never
    /// invents a default.
    #[must_use]
    pub fn decode(name: &str, body: &serde_json::Value) -> Option<Self> {
        if !ROAMING_SETTING_NAMES.contains(&name) {
            return None;
        }
        // The enum is externally tagged: the variant (and so the key
        // name it mints) rides the body — the name argument only gates
        // the roam-set membership, never a second mapping to keep in
        // sync.
        serde_json::from_value::<Self>(body.clone()).ok()
    }
}

/// One roaming row: the typed value and its per-key write clock (the
/// section recipe — every row is clocked, LWW per key).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoamingSettingRow {
    /// The setting's value.
    pub value: RoamingSettingValue,
    /// The row's write clock (the wire row's `clock`).
    pub clock: chrono::DateTime<chrono::Utc>,
}

/// The roaming settings home: rows keyed by
/// [`RoamingSettingValue::key_name`]. Persisted like any synced home
/// (one row per key); the *snapshot* (`SettingsSnapshot`) stays the
/// app lane's wholesale value — consumption wiring is future work.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoamingSettings(pub std::collections::BTreeMap<String, RoamingSettingRow>);

/// What one [`import_settings`] pass did (the [`crate::reflection::
/// ReflectionImport`] shape): the persistence batch (every action is
/// `UpsertRoamingSetting`) and the per-row verdict counts.
#[derive(Debug, Default, PartialEq)]
pub struct SettingsImport {
    /// `UpsertRoamingSetting` per applied row, in input order.
    pub actions: Vec<crate::persistence::PersistenceAction>,
    /// Rows that changed the home.
    pub applied: usize,
    /// Rows the clock guard declined (older than local) or that were
    /// identical to the local row already (the idempotence skip).
    pub skipped_stale: usize,
}

/// Folds imported settings rows into the roaming home — the
/// [`crate::reflection::import_reflections`] shape verbatim: per key
/// the clock guard is `>=` (ties go to the imported row — the fold
/// winner adoption argument, unchanged), and an identical row is the
/// idempotence skip. Pure over `state` otherwise; reads no clock.
pub fn import_settings(
    state: &mut RoamingSettings,
    rows: &[crate::messages::ImportedSetting],
) -> SettingsImport {
    let mut report = SettingsImport::default();
    for row in rows {
        let name = row.value.key_name().to_owned();
        let adopt = match state.0.get(&name) {
            None => true,
            Some(local) => row.clock >= local.clock && row.value != local.value,
        };
        if adopt {
            let setting = RoamingSettingRow {
                value: row.value,
                clock: row.clock,
            };
            state.0.insert(name.clone(), setting);
            report
                .actions
                .push(crate::persistence::PersistenceAction::UpsertRoamingSetting(
                    name, setting,
                ));
            report.applied += 1;
        } else {
            report.skipped_stale += 1;
        }
    }
    report
}

/// The `Option<FocusTiming>` serde lane: the four validated numbers as
/// a plain struct on the wire (the same [`crate::focus::
/// FocusTimingWire`] grammar the strict lane rides); a decode that
/// fails the [`FocusTiming::try_new`] mint is lenient (→ `None`, the
/// disabled shape) — module docs.
mod focus_timing_serde {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    use crate::focus::{FocusTiming, FocusTimingWire};

    // serde's `with` passes `&Option<T>`; the ref_option lint's
    // `Option<&T>` shape is not available at this boundary.
    #[allow(clippy::ref_option)]
    pub(super) fn serialize<S: Serializer>(
        timing: &Option<FocusTiming>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        timing
            .as_ref()
            .map(FocusTimingWire::of)
            .serialize(serializer)
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<FocusTiming>, D::Error> {
        let wire = Option::<FocusTimingWire>::deserialize(deserializer)?;
        Ok(wire.and_then(|w| {
            FocusTiming::try_new(
                chrono::TimeDelta::seconds(w.work_secs),
                chrono::TimeDelta::seconds(w.respite_secs),
                chrono::TimeDelta::seconds(w.long_break_secs),
                w.long_break_every,
            )
            .ok()
        }))
    }
}

#[test]
fn probe_decode() {
    let v = RoamingSettingValue::DayStartTime(chrono::NaiveTime::from_hms_opt(5, 30, 0).unwrap());
    let raw = serde_json::to_value(v).unwrap();
    eprintln!("raw = {raw}");
    eprintln!(
        "decode = {:?}",
        crate::settings::RoamingSettingValue::decode("day_start_time", &raw)
    );
    let direct: Result<RoamingSettingValue, _> = serde_json::from_value(raw.clone());
    eprintln!("direct = {direct:?}");
    let time: Result<chrono::NaiveTime, _> = serde_json::from_value(raw["DayStartTime"].clone());
    eprintln!("time = {time:?}");
}

#[cfg(test)]
mod roaming_tests {
    use super::*;
    use crate::messages::ImportedSetting;
    use crate::persistence::{PersistenceAction, apply_actions};

    fn ts(secs: i64) -> chrono::DateTime<chrono::Utc> {
        use chrono::TimeZone;
        chrono::Utc.timestamp_opt(secs, 0).unwrap()
    }

    fn imported(value: RoamingSettingValue, clock_secs: i64) -> ImportedSetting {
        ImportedSetting {
            value,
            clock: ts(clock_secs),
        }
    }

    /// THE PIN (dispatch hard rule): the roam-set is data — the exact
    /// classified list, name by name. The negative legs name the
    /// notorious machine/secret families so their absence is asserted,
    /// never implied by the count.
    #[test]
    fn roaming_setting_names_is_exactly_the_classified_list() {
        assert_eq!(
            ROAMING_SETTING_NAMES,
            &[
                "day_start_time",
                "reminders_enabled",
                "reminder_before",
                "due_soon_days",
                "focus_enabled",
                "focus_timing",
                "focus_chain",
                "focus_notify",
            ]
        );
        // Machine cadence / capability / hardware / secret families:
        // none of these may ever enter the roam-set (each roams only
        // by consciously extending the list — this pin fails first).
        for banned in [
            "tick_secs",
            "poll_interval_secs",
            "backoff_initial_secs",
            "backoff_max_secs",
            "reminder_sink",
            "scale",
            "window_geometry",
            "stack_widths",
            "data_dir",
            "db_path",
            "server_url",
            "account",
            "password",
            "token",
            "secret",
            "keyring",
            "locale",
        ] {
            assert!(
                !ROAMING_SETTING_NAMES.contains(&banned),
                "{banned} must never roam"
            );
        }
    }

    /// Every variant names a key in the pinned list, and every pinned
    /// name is some variant's key — the list and the enum cannot drift.
    #[test]
    fn key_names_and_the_pinned_list_are_a_bijection() {
        let samples = [
            RoamingSettingValue::DayStartTime(chrono::NaiveTime::from_hms_opt(5, 0, 0).unwrap()),
            RoamingSettingValue::RemindersEnabled(true),
            RoamingSettingValue::ReminderBefore(ReminderOffset::at_due()),
            RoamingSettingValue::DueSoonDays(DueSoonDays::new(2).unwrap()),
            RoamingSettingValue::FocusEnabled(true),
            RoamingSettingValue::FocusTiming(None),
            RoamingSettingValue::FocusChain(true),
            RoamingSettingValue::FocusNotify(true),
        ];
        let mut names: Vec<&str> = samples.iter().map(RoamingSettingValue::key_name).collect();
        names.sort_unstable();
        let mut pinned = ROAMING_SETTING_NAMES.to_vec();
        pinned.sort_unstable();
        assert_eq!(names, pinned);
    }

    /// A future build's unknown key fails the mint: `None`, never a
    /// default (the codec rejects the row; the wire relays it).
    #[test]
    fn an_unknown_key_fails_the_mint() {
        assert_eq!(
            RoamingSettingValue::decode("focus_ambient", &serde_json::json!(true)),
            None
        );
    }

    /// A value that fails its own mint fails the decode (fail-closed).
    #[test]
    fn an_unmintable_value_fails_the_decode() {
        assert_eq!(
            RoamingSettingValue::decode("day_start_time", &serde_json::json!("not-a-time")),
            None
        );
    }

    #[test]
    fn a_fresh_row_inserts_and_emits_its_action() {
        let mut home = RoamingSettings::default();
        let row = imported(RoamingSettingValue::RemindersEnabled(true), 1_800_000_000);
        let report = import_settings(&mut home, std::slice::from_ref(&row));
        assert_eq!(report.applied, 1);
        assert_eq!(report.skipped_stale, 0);
        assert_eq!(
            home.0["reminders_enabled"].value,
            RoamingSettingValue::RemindersEnabled(true)
        );
        assert_eq!(report.actions.len(), 1);
        let PersistenceAction::UpsertRoamingSetting(name, setting) = &report.actions[0] else {
            panic!("the import emits the upsert action");
        };
        assert_eq!(name, "reminders_enabled");
        assert_eq!(setting.value, RoamingSettingValue::RemindersEnabled(true));
        assert_eq!(setting.clock, row.clock);
    }

    /// The clock guard is `>=` — ties go to the imported row (the
    /// fold-winner adoption rule); an older row is skipped.
    #[test]
    fn newer_and_tied_rows_adopt_older_rows_skip() {
        let mut home = RoamingSettings::default();
        let first = imported(RoamingSettingValue::FocusEnabled(false), 1_000);
        let newer = imported(RoamingSettingValue::FocusEnabled(true), 2_000);
        let older = imported(RoamingSettingValue::FocusEnabled(false), 500);
        let tied_other = imported(RoamingSettingValue::FocusNotify(true), 2_000);
        let report = import_settings(&mut home, &[first, newer, older, tied_other]);
        assert_eq!(report.applied, 3);
        assert_eq!(report.skipped_stale, 1);
        assert_eq!(
            home.0["focus_enabled"].value,
            RoamingSettingValue::FocusEnabled(true)
        );
        assert_eq!(home.0["focus_enabled"].clock, ts(2_000));
        assert!(home.0.contains_key("focus_notify"));
    }

    /// An identical row (same value, same clock) is the idempotence
    /// skip: a replayed report is a full no-op.
    #[test]
    fn an_identical_row_is_the_idempotence_skip() {
        let mut home = RoamingSettings::default();
        let row = imported(
            RoamingSettingValue::DueSoonDays(DueSoonDays::new(3).unwrap()),
            42,
        );
        import_settings(&mut home, std::slice::from_ref(&row));
        let report = import_settings(&mut home, &[row]);
        assert_eq!(report.applied, 0);
        assert_eq!(report.skipped_stale, 1);
        assert_eq!(report.actions, Vec::new());
    }

    /// The actions replay to exactly the folded home (the
    /// actions-mirror-state law, through `apply_actions` itself).
    #[test]
    fn the_emitted_batch_replays_to_the_folded_home() {
        let mut home = RoamingSettings::default();
        let rows = [
            imported(RoamingSettingValue::RemindersEnabled(true), 10),
            imported(RoamingSettingValue::FocusChain(true), 20),
        ];
        let folded = import_settings(&mut home, &rows);
        let mut state = crate::persistence::PersistedState::default();
        apply_actions(&mut state, &folded.actions);
        assert_eq!(state.roaming, home);
    }

    /// The value serde round-trips through the enum's own lane.
    #[test]
    fn values_roundtrip_through_serde() {
        let timing = crate::focus::FocusTiming::try_new(
            chrono::TimeDelta::minutes(25),
            chrono::TimeDelta::minutes(5),
            chrono::TimeDelta::minutes(15),
            4,
        )
        .unwrap();
        let samples = [
            RoamingSettingValue::DayStartTime(chrono::NaiveTime::from_hms_opt(5, 30, 0).unwrap()),
            RoamingSettingValue::ReminderBefore(
                ReminderOffset::before(chrono::TimeDelta::minutes(-10)).unwrap(),
            ),
            RoamingSettingValue::DueSoonDays(DueSoonDays::new(2).unwrap()),
            RoamingSettingValue::FocusTiming(Some(timing)),
            RoamingSettingValue::FocusTiming(None),
        ];
        for sample in samples {
            let raw = serde_json::to_value(sample).unwrap();
            let back: RoamingSettingValue = serde_json::from_value(raw).unwrap();
            assert_eq!(back, sample);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn timing() -> FocusTiming {
        FocusTiming::try_new(
            chrono::TimeDelta::minutes(25),
            chrono::TimeDelta::minutes(5),
            chrono::TimeDelta::minutes(15),
            4,
        )
        .expect("valid timing")
    }

    #[test]
    fn tick_secs_rejects_zero_and_accepts_one() {
        assert_eq!(TickSecs::new(0), Err(InvalidTickSecs));
        assert_eq!(TickSecs::new(1).map(TickSecs::get), Ok(1));
        assert_eq!(
            TickSecs::new(7).map(TickSecs::duration),
            Ok(Duration::from_secs(7))
        );
    }

    #[test]
    fn due_soon_days_accepts_zero_and_the_ceiling_rejects_above() {
        assert_eq!(DueSoonDays::new(0).map(DueSoonDays::get), Ok(0));
        assert_eq!(DueSoonDays::new(3652).map(DueSoonDays::get), Ok(3652));
        assert_eq!(DueSoonDays::new(3653), Err(InvalidDueSoonDays));
        assert_eq!(
            DueSoonDays::new(2).map(DueSoonDays::horizon),
            Ok(chrono::TimeDelta::days(2))
        );
    }

    #[test]
    fn the_serde_lane_remints_leniently_to_the_defaults() {
        let tick: TickSecs = serde_json::from_str("0").expect("lenient");
        assert_eq!(tick, TickSecs::default());
        let days: DueSoonDays = serde_json::from_str("99999").expect("lenient");
        assert_eq!(days, DueSoonDays::default());
        // A missing field decodes to the field default (the
        // `#[serde(default)]` reader contract).
        let snapshot: SettingsSnapshot = serde_json::from_str("{}").expect("lenient");
        assert_eq!(snapshot, SettingsSnapshot::default());
    }

    #[test]
    fn a_full_snapshot_round_trips_through_serde() {
        let snapshot = SettingsSnapshot {
            day_start_time: chrono::NaiveTime::from_hms_opt(5, 0, 0).unwrap(),
            reminders_enabled: true,
            tick_secs: TickSecs::new(2).unwrap(),
            reminder_before: ReminderOffset::before(chrono::TimeDelta::minutes(-10)).unwrap(),
            reminder_sink: ReminderSinkChoice::Desktop,
            poll_interval_secs: TickSecs::new(11).unwrap(),
            backoff_initial_secs: TickSecs::new(3).unwrap(),
            backoff_max_secs: TickSecs::new(9).unwrap(),
            due_soon_days: DueSoonDays::new(3).unwrap(),
            focus_enabled: true,
            focus: Some(timing()),
            focus_chain: true,
            focus_notify: true,
        };
        let json = serde_json::to_string(&snapshot).expect("serializes");
        let back: SettingsSnapshot = serde_json::from_str(&json).expect("deserializes");
        assert_eq!(back, snapshot);
    }

    #[test]
    fn an_unmintable_focus_timing_decodes_to_the_disabled_shape() {
        let json = r#"{"focus":{"work_secs":0,"respite_secs":300,"long_break_secs":900,"long_break_every":4}}"#;
        let snapshot: SettingsSnapshot = serde_json::from_str(json).expect("lenient");
        assert_eq!(snapshot.focus, None);
    }

    #[test]
    fn live_focus_is_none_while_disabled_even_with_timings() {
        let mut snapshot = SettingsSnapshot {
            focus: Some(timing()),
            ..SettingsSnapshot::default()
        };
        assert_eq!(snapshot.live_focus(), None);
        snapshot.focus_enabled = true;
        assert_eq!(snapshot.live_focus(), Some(timing()));
    }

    #[test]
    fn the_derived_views_carry_the_snapshot_fields() {
        let before = ReminderOffset::before(chrono::TimeDelta::minutes(-5)).unwrap();
        let snapshot = SettingsSnapshot {
            day_start_time: chrono::NaiveTime::from_hms_opt(4, 30, 0).unwrap(),
            reminder_before: before,
            ..SettingsSnapshot::default()
        };
        assert_eq!(snapshot.reminder_defaults().default_before, before);
        let offset = chrono::FixedOffset::east_opt(3600).unwrap();
        let day = snapshot.day_calendar(offset);
        assert_eq!(day.day_start_time, snapshot.day_start_time);
        assert_eq!(day.offset, offset);
    }

    proptest! {
        /// Random `u64` → mint: exactly zero is refused, everything
        /// else round-trips through `get`.
        #[test]
        fn tick_secs_mint_is_exactly_the_nonzero_predicate(n in any::<u64>()) {
            if let Ok(secs) = TickSecs::new(n) {
                prop_assert_eq!(secs.get(), n);
            } else {
                prop_assert_eq!(n, 0);
            }
        }

        /// Random `u32` → mint: the ceiling is the only refusal.
        #[test]
        fn due_soon_days_mint_is_exactly_the_ceiling_predicate(n in any::<u32>()) {
            if let Ok(days) = DueSoonDays::new(n) {
                prop_assert_eq!(days.get(), n);
            } else {
                prop_assert!(n > DUE_SOON_DAYS_MAX);
            }
        }

        /// The serde lane never fails and lands on a minted value.
        #[test]
        fn tick_secs_serde_always_lands_on_a_minted_value(n in any::<u64>()) {
            let decoded: TickSecs = serde_json::from_str(&n.to_string()).expect("lenient");
            prop_assert!(decoded.get() >= 1);
            if n >= 1 {
                prop_assert_eq!(decoded.get(), n);
            }
        }
    }
}
