// SPDX-License-Identifier: AGPL-3.0-only
//! `taskboard config print` (ADR 0009): a decode-only diagnostic that
//! *always* prints — validity is reported in the payload, not via the
//! exit code. Pre-core (no actor graph, no database, no lock).

use taskboard_domain::ReminderOffset;

use crate::cli::ConfigCmd;
use crate::commands::output::{
    ConfigBatchSetView, ConfigBatchUnsetView, ConfigPathView, ConfigReloadView, ConfigSetView,
    ConfigView,
};
use crate::commands::{CommandCtx, CommandError, emit_to, report_to};
use crate::config::{AppConfig, ConfigError, SinkChoice, SyncBackend};
use crate::settings_overlay::OverlayError;
use crate::ui_requests::{LiveApply, SettingsWriteError, run_settings_write};

/// Prints the effective config. Only an undecodable config fails (exit
/// 1 — nothing honest to print); a decodes-but-invalid config prints
/// `valid: false` plus the typed `invalid_kind` and exits 0.
///
/// # Errors
///
/// [`CommandError`] — the `decode` kind for provider failures; never
/// a validation failure (that travels in the payload).
pub fn print(config_path: Option<std::path::PathBuf>) -> Result<ConfigView, CommandError> {
    let config_path = AppConfig::config_path(config_path);
    // Decode + db-path fill, no validation gate (the `login` precedent).
    let config = AppConfig::extract_raw(config_path.as_deref())?;
    let invalid_kind = config
        .validate()
        .err()
        .map(|err: ConfigError| err.kind().to_owned());
    Ok(ConfigView {
        config_path: config_path.as_ref().map(|p| p.display().to_string()),
        // The default connector's kind word (D5-9): the array world
        // displays its first entry, the legacy world today's resolution.
        backend: match config.resolved_default_backend() {
            SyncBackend::Standalone => "standalone",
            SyncBackend::Nextcloud => "nextcloud",
        }
        .to_owned(),
        valid: invalid_kind.is_none(),
        invalid_kind,
        config,
    })
}

/// The human-readable config view: deterministic `key: value` lines
/// (path, backend, validity, then the four sections' fields). Plain
/// formatting — the config is secret-free by construction.
#[must_use]
pub fn render_text(view: &ConfigView) -> String {
    let c = &view.config;
    let mut lines = vec![
        format!(
            "config_path: {}",
            view.config_path.as_deref().unwrap_or("(none)")
        ),
        format!("backend: {}", view.backend),
        format!("valid: {}", view.valid),
    ];
    if let Some(kind) = &view.invalid_kind {
        lines.push(format!("invalid_kind: {kind}"));
    }
    lines.push(format!("app.mode: {:?}", c.app.mode).to_lowercase());
    lines.push(format!(
        "app.shutdown_timeout_secs: {}",
        c.app.shutdown_timeout_secs
    ));
    lines.push(format!(
        "nextcloud.server_url: {}",
        c.nextcloud.server_url.as_deref().unwrap_or("(unset)")
    ));
    lines.push(format!(
        "nextcloud.username: {}",
        c.nextcloud.username.as_deref().unwrap_or("(unset)")
    ));
    lines.push(format!(
        "nextcloud.credential_store: {}",
        match c.nextcloud.credential_store {
            crate::config::CredentialStoreKind::Keyring => "keyring",
            crate::config::CredentialStoreKind::Env => "env",
            crate::config::CredentialStoreKind::File => "file",
        }
    ));
    lines.push(format!(
        "storage.db_path: {}",
        c.storage
            .db_path
            .as_ref()
            .map_or_else(|| "(unset)".to_owned(), |p| p.display().to_string())
    ));
    lines.push(format!(
        "sync.backend: {}",
        c.sync
            .backend
            .map_or_else(|| "(auto)".to_owned(), |b| format!("{b:?}").to_lowercase())
    ));
    lines.push(format!(
        "sync.poll_interval_secs: {}",
        c.sync.poll_interval_secs
    ));
    lines.push(format!(
        "sync.backoff_initial_secs: {}",
        c.sync.backoff_initial_secs
    ));
    lines.push(format!(
        "sync.backoff_max_secs: {}",
        c.sync.backoff_max_secs
    ));
    lines.join("\n")
}

/// The resolved-path diagnosis: which file reads resolve and where
/// `connect`/`login` writes. Pure path resolution — no decode, no
/// validation, no lock, no boot; infallible. Both resolution calls
/// take the original `explicit` option: `write_path` re-resolves
/// internally, so passing the already-resolved read path would collapse
/// the very divergence this verb exists to show.
#[must_use]
pub fn path(explicit: Option<std::path::PathBuf>) -> ConfigPathView {
    ConfigPathView {
        config_path: AppConfig::config_path(explicit.clone())
            .as_ref()
            .map(|p| p.display().to_string()),
        write_path: AppConfig::write_path(explicit).display().to_string(),
    }
}

/// The human-readable path diagnosis: deterministic `key: value` lines
/// in `print`'s style.
#[must_use]
pub fn render_path_text(view: &ConfigPathView) -> String {
    [
        format!(
            "config_path: {}",
            view.config_path.as_deref().unwrap_or("(none)")
        ),
        format!("write_path: {}", view.write_path),
    ]
    .join("\n")
}

/// The `config set`/`unset` knob table (T-18, #801): the dotted key
/// vocabulary is exactly the GUI settings editor's `Field::key()`
/// set plus the choice bools — one vocabulary, two modality fronts.
/// Recorded CLI-only exceptions to the `Field::key()` vocabulary:
/// `files_sync.anchor` (round-6 D-6.6) and `files_sync.poll_secs`
/// (#1051) — the Files row's editor lattice does not carry them (the
/// `poll_secs` GUI field is #1083); the apply-registry's lattice test
/// lists both explicitly.
/// The value predicates are re-expressed here against the app's own
/// config bounds (the settings form's mirrors live in the UI crate,
/// which the CLI cannot depend on; the grammars that ARE shared ride
/// their domain/config owners — `parse_before`,
/// `ReminderOffset::parse_text_form` — so nothing drifts where a
/// shared owner exists).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConfigKey {
    DayStart,
    RemindersEnabled,
    TickSecs,
    ReminderBefore,
    ReminderSink,
    PollInterval,
    BackoffInitial,
    BackoffMax,
    DueSoonDays,
    FocusEnabled,
    FocusWorkMins,
    FocusBreakMins,
    FocusChain,
    FocusLongBreakMins,
    FocusLongBreakEvery,
    FocusNotify,
    FilesSyncEnabled,
    FilesSyncPollSecs,
}

/// The typed refusal of one `config set`/`unset` invocation (T-18):
/// `matches!`-able at the arm, rendered through the error envelope.
#[derive(Debug, thiserror::Error)]
pub enum ConfigSetError {
    /// The key is overridden by a `TASKBOARD_*` env value — the file
    /// write would be dead text under the env layer (the same honesty
    /// the GUI's env-locked refusal renders).
    #[error("the key is set by an environment variable")]
    EnvLocked {
        /// The refused dotted key.
        key: String,
    },
    /// The key is not in the knob table (the GUI editor's vocabulary
    /// is the contract; whole-section or connection writes stay out).
    #[error("unknown config key")]
    UnknownKey {
        /// The unknown dotted key.
        key: String,
    },
    /// The value failed the key's predicate; nothing wrote.
    #[error("the value does not fit the key")]
    BadValue {
        /// The refused dotted key.
        key: String,
    },
    /// The toml-layer read or atomic write failed.
    #[error("the config file could not be written")]
    Io(#[from] std::io::Error),
    /// The prove gate refused (journey US-4/S3): the resulting document
    /// would not boot (a previously-valid file is left untouched — the
    /// overlay writers' attribution rule; the value fit the key's own
    /// predicate but the file as a whole would not start).
    #[error("the write would make the config unbootable")]
    Rejected(#[source] Box<ConfigError>),
}

/// The overlay writers' failure classes fold onto the typed refusal
/// lattice: the prove gate's `Rejected` and the writer IO both carry.
impl From<OverlayError> for ConfigSetError {
    fn from(err: OverlayError) -> Self {
        match err {
            OverlayError::Io(io) => Self::Io(io),
            OverlayError::Rejected(config_err) => Self::Rejected(config_err),
        }
    }
}

impl ConfigKey {
    fn from_dotted(key: &str) -> Option<Self> {
        Some(match key {
            "preferences.day_start_time" => Self::DayStart,
            "reminders.enabled" => Self::RemindersEnabled,
            "reminders.tick_secs" => Self::TickSecs,
            "reminders.default_before" => Self::ReminderBefore,
            "reminders.sink" => Self::ReminderSink,
            "sync.poll_interval_secs" => Self::PollInterval,
            "sync.backoff_initial_secs" => Self::BackoffInitial,
            "sync.backoff_max_secs" => Self::BackoffMax,
            "ui.due_soon_days" => Self::DueSoonDays,
            "focus.enabled" => Self::FocusEnabled,
            "focus.work_mins" => Self::FocusWorkMins,
            "focus.break_mins" => Self::FocusBreakMins,
            "focus.chain" => Self::FocusChain,
            "focus.long_break_mins" => Self::FocusLongBreakMins,
            "focus.long_break_every" => Self::FocusLongBreakEvery,
            "focus.notify" => Self::FocusNotify,
            "files_sync.enabled" => Self::FilesSyncEnabled,
            "files_sync.poll_secs" => Self::FilesSyncPollSecs,
            _ => return None,
        })
    }

    /// The `(section, field)` split (the vocabulary is exactly
    /// two-part; a deeper dotted key is an unknown key).
    fn dotted(self) -> (&'static str, &'static str) {
        match self {
            Self::DayStart => ("preferences", "day_start_time"),
            Self::RemindersEnabled => ("reminders", "enabled"),
            Self::TickSecs => ("reminders", "tick_secs"),
            Self::ReminderBefore => ("reminders", "default_before"),
            Self::ReminderSink => ("reminders", "sink"),
            Self::PollInterval => ("sync", "poll_interval_secs"),
            Self::BackoffInitial => ("sync", "backoff_initial_secs"),
            Self::BackoffMax => ("sync", "backoff_max_secs"),
            Self::DueSoonDays => ("ui", "due_soon_days"),
            Self::FocusEnabled => ("focus", "enabled"),
            Self::FocusWorkMins => ("focus", "work_mins"),
            Self::FocusBreakMins => ("focus", "break_mins"),
            Self::FocusChain => ("focus", "chain"),
            Self::FocusLongBreakMins => ("focus", "long_break_mins"),
            Self::FocusLongBreakEvery => ("focus", "long_break_every"),
            Self::FocusNotify => ("focus", "notify"),
            Self::FilesSyncEnabled => ("files_sync", "enabled"),
            Self::FilesSyncPollSecs => ("files_sync", "poll_secs"),
        }
    }

    /// The key's value predicate: `Ok` yields the *typed* value the
    /// overlay writer lands (journey W-3/D4 — the CLI's raw-toml
    /// writer retired onto the prove-gate overlay writers, so both
    /// lanes run the gate).
    fn parse_value(self, value: &str) -> Result<TypedSetting, ConfigSetError> {
        let bad = || ConfigSetError::BadValue {
            key: self.dotted().0.to_owned() + "." + self.dotted().1,
        };
        let trimmed = value.trim();
        match self {
            Self::DayStart => trimmed
                .parse::<chrono::NaiveTime>()
                .map(TypedSetting::DayStart)
                .map_err(|_| bad()),
            Self::TickSecs
            | Self::PollInterval
            | Self::BackoffInitial
            | Self::BackoffMax
            | Self::FilesSyncPollSecs => trimmed
                .parse::<i64>()
                .ok()
                .filter(|secs| *secs > 0)
                .and_then(|secs| u64::try_from(secs).ok())
                .map(TypedSetting::Secs)
                .ok_or_else(bad),
            Self::ReminderBefore => ReminderOffset::parse_text_form(trimmed)
                .map(TypedSetting::Before)
                .ok_or_else(bad),
            Self::DueSoonDays => trimmed
                .parse::<i64>()
                .ok()
                .filter(|days| (0..=3652).contains(days))
                .and_then(|days| u32::try_from(days).ok())
                .map(TypedSetting::DueSoonDays)
                .ok_or_else(bad),
            Self::FocusWorkMins | Self::FocusBreakMins | Self::FocusLongBreakMins => trimmed
                .parse::<i64>()
                .ok()
                .filter(|mins| (1..=240).contains(mins))
                .and_then(|mins| u64::try_from(mins).ok())
                .map(TypedSetting::FocusMins)
                .ok_or_else(bad),
            Self::FocusLongBreakEvery => trimmed
                .parse::<i64>()
                .ok()
                .filter(|every| (1..=24).contains(every))
                .and_then(|every| u64::try_from(every).ok())
                .map(TypedSetting::FocusEvery)
                .ok_or_else(bad),
            Self::RemindersEnabled
            | Self::FocusEnabled
            | Self::FocusChain
            | Self::FocusNotify
            | Self::FilesSyncEnabled => match trimmed {
                "true" => Ok(TypedSetting::Bool(true)),
                "false" => Ok(TypedSetting::Bool(false)),
                _ => Err(bad()),
            },
            Self::ReminderSink => match trimmed {
                "auto" => Ok(TypedSetting::Sink(SinkChoice::Auto)),
                "log" => Ok(TypedSetting::Sink(SinkChoice::Log)),
                "desktop" => Ok(TypedSetting::Sink(SinkChoice::Desktop)),
                _ => Err(bad()),
            },
        }
    }
    /// The overlay writer this value lands through (the prove gate's
    /// typed front — both lanes write through these). Keyed by the
    /// `ConfigKey` (several keys share a value shape but not a writer).
    #[allow(clippy::too_many_lines)] // one arm per knob, kept flat
    fn write(self, path: &std::path::Path, typed: &TypedSetting) -> Result<(), OverlayError> {
        match (self, typed) {
            (Self::DayStart, TypedSetting::DayStart(time)) => {
                crate::settings_overlay::set_day_start(path, *time)
            }
            (Self::RemindersEnabled, TypedSetting::Bool(value)) => {
                crate::settings_overlay::set_reminders_enabled(path, *value)
            }
            (Self::TickSecs, TypedSetting::Secs(secs)) => {
                crate::settings_overlay::set_reminder_tick(path, *secs)
            }
            (Self::ReminderBefore, TypedSetting::Before(offset)) => {
                crate::settings_overlay::set_reminder_before(path, *offset)
            }
            (Self::ReminderSink, TypedSetting::Sink(choice)) => {
                crate::settings_overlay::set_reminder_sink(path, *choice)
            }
            (Self::PollInterval, TypedSetting::Secs(secs)) => {
                crate::settings_overlay::set_poll_interval(path, *secs)
            }
            (Self::BackoffInitial, TypedSetting::Secs(secs)) => {
                crate::settings_overlay::set_backoff_initial(path, *secs)
            }
            (Self::BackoffMax, TypedSetting::Secs(secs)) => {
                crate::settings_overlay::set_backoff_max(path, *secs)
            }
            (Self::DueSoonDays, TypedSetting::DueSoonDays(days)) => {
                crate::settings_overlay::set_due_soon_days(path, *days)
            }
            (Self::FocusEnabled, TypedSetting::Bool(value)) => {
                crate::settings_overlay::set_focus_enabled(path, *value)
            }
            (Self::FocusWorkMins, TypedSetting::FocusMins(mins)) => {
                crate::settings_overlay::set_focus_work_mins(path, *mins)
            }
            (Self::FocusBreakMins, TypedSetting::FocusMins(mins)) => {
                crate::settings_overlay::set_focus_break_mins(path, *mins)
            }
            (Self::FocusChain, TypedSetting::Bool(value)) => {
                crate::settings_overlay::set_focus_chain(path, *value)
            }
            (Self::FocusLongBreakMins, TypedSetting::FocusMins(mins)) => {
                crate::settings_overlay::set_focus_long_break_mins(path, *mins)
            }
            (Self::FocusLongBreakEvery, TypedSetting::FocusEvery(every)) => {
                crate::settings_overlay::set_focus_long_break_every(path, *every)
            }
            (Self::FocusNotify, TypedSetting::Bool(value)) => {
                crate::settings_overlay::set_focus_notify(path, *value)
            }
            (Self::FilesSyncEnabled, TypedSetting::Bool(value)) => {
                crate::settings_overlay::set_files_sync_enabled(path, *value, None)
            }
            (Self::FilesSyncPollSecs, TypedSetting::Secs(secs)) => {
                crate::settings_overlay::set_files_sync_poll_secs(path, *secs)
            }
            _ => unreachable!("parse_value and write agree on the key's value shape"),
        }
    }
}

/// One parsed knob value (the CLI-side string → typed-value mint, D8 —
/// parsing stays CLI-side and precedes any lane choice): the value the
/// overlay writer lands, plus its canonical echo text (the typed view's
/// `value` field, US-5/S1).
#[derive(Debug, Clone, PartialEq)]
enum TypedSetting {
    DayStart(chrono::NaiveTime),
    Secs(u64),
    Before(ReminderOffset),
    DueSoonDays(u32),
    FocusMins(u64),
    FocusEvery(u64),
    Bool(bool),
    Sink(SinkChoice),
}

impl TypedSetting {
    /// The canonical echo: the typed value's own text form (never the
    /// raw user spelling).
    fn echo(&self) -> String {
        match self {
            Self::DayStart(time) => time.to_string(),
            Self::Secs(secs) | Self::FocusMins(secs) | Self::FocusEvery(secs) => secs.to_string(),
            Self::DueSoonDays(days) => days.to_string(),
            Self::Before(offset) => offset.text_form().clone(),
            Self::Bool(value) => value.to_string(),
            Self::Sink(choice) => match choice {
                SinkChoice::Auto => "auto",
                SinkChoice::Log => "log",
                SinkChoice::Desktop => "desktop",
            }
            .to_owned(),
        }
    }
}

/// `config set <key> <value>` (T-18, #801; propagation #845): the
/// local lane — the file-layer write through the prove-gate overlay
/// writers (journey D4: `write_dotted` is retired onto the shared
/// port), reporting the typed next-start outcome (`applied_live:
/// false`, a success). Env-locked keys refuse typed, unknown keys and
/// malformed values refuse typed, and a validity-crossing write
/// refuses at the prove gate with the file untouched.
///
/// # Errors
///
/// [`ConfigSetError`] — the typed refusal classes.
pub async fn set(
    config_path: Option<std::path::PathBuf>,
    key: &str,
    value: &str,
) -> Result<ConfigSetView, ConfigSetError> {
    let write_path = AppConfig::write_path(config_path);
    let locked = AppConfig::env_overridden_keys(Some(write_path.as_path()))
        .map_err(|err| ConfigSetError::Io(std::io::Error::other(err.to_string())))?;
    set_with_locked(&write_path, key, value, &locked, None).await
}

/// The lock-set seam (T-18): `set`'s body over a caller-supplied
/// locked set and an optional engine — the env truth-table's
/// composition point AND the lane seam (journey W-2/D4): the daemon's
/// config arm calls this with `Some(&engine)` (the live lane), the
/// CLI's local lane with `None` (the typed next-start lane). Parse
/// precedes everything (US-4/S2 — nothing is touched on a refusal);
/// the env guard precedes the write (US-4/S1); the prove gate
/// precedes any engine command (US-4/S3, US-1 AC-2).
pub(crate) async fn set_with_locked(
    write_path: &std::path::Path,
    key: &str,
    value: &str,
    locked: &std::collections::BTreeSet<String>,
    engine: Option<&taskboard_state::EngineHandle>,
) -> Result<ConfigSetView, ConfigSetError> {
    let parsed = ConfigKey::from_dotted(key).ok_or_else(|| ConfigSetError::UnknownKey {
        key: key.to_owned(),
    })?;
    if locked.contains(key) {
        return Err(ConfigSetError::EnvLocked {
            key: key.to_owned(),
        });
    }
    let typed = parsed.parse_value(value)?;
    let outcome = run_settings_write(
        write_path,
        locked,
        &[key],
        |path| parsed.write(path, &typed),
        engine,
    )
    .await
    .map_err(|err| match err {
        SettingsWriteError::EnvLocked => ConfigSetError::EnvLocked {
            key: key.to_owned(),
        },
        SettingsWriteError::Overlay(overlay) => ConfigSetError::from(overlay),
        SettingsWriteError::Config(config_err) => ConfigSetError::Rejected(config_err),
    })?;
    Ok(ConfigSetView {
        key: key.to_owned(),
        value: Some(typed.echo()),
        applied_live: outcome.apply == LiveApply::Applied,
    })
}

/// `config unset <k1> [<k2> …]` (T-18, #801; propagation #845; batch
/// #1014): removes every named toml-layer override as **one atomic
/// document** — one prove verdict, one write, one lane bit — so the
/// defaults (or env layer) apply again. The same typed lanes as
/// [`set_batch`]: zero keys, an unknown key, an env-pinned key, a
/// duplicate key, or a prove-gate refusal leaves the file
/// byte-identical and names the offending key typed. A one-key
/// invocation is the single-unset form of the widened grammar.
///
/// # Errors
///
/// [`ConfigBatchEditError`] — the typed refusal classes (the batch
/// lattice, shared with `config set`; `BadValue` is set-only).
pub async fn unset(
    config_path: Option<std::path::PathBuf>,
    keys: &[String],
) -> Result<ConfigBatchUnsetView, ConfigBatchEditError> {
    let write_path = AppConfig::write_path(config_path);
    let locked = AppConfig::env_overridden_keys(Some(write_path.as_path()))
        .map_err(|err| ConfigBatchEditError::Io(std::io::Error::other(err.to_string())))?;
    unset_batch_with_locked(&write_path, keys, &locked, None).await
}

/// The lock-set seam for the unset batch (the [`set_batch_with_locked`]
/// twin, journey W-2): parse *all* keys → env-guard *all* keys →
/// duplicate-key check → **one** `run_settings_write`-shaped port call
/// carrying the doc-level removals writer. The chain runs before the
/// document load, so the prove skeleton is never invoked on a doomed
/// batch. No size-1 fallback: one key flows through this same chain
/// (D-GRAMMAR/D-SCOPE — one verb, one error family).
async fn unset_batch_with_locked(
    write_path: &std::path::Path,
    keys: &[String],
    locked: &std::collections::BTreeSet<String>,
    engine: Option<&taskboard_state::EngineHandle>,
) -> Result<ConfigBatchUnsetView, ConfigBatchEditError> {
    if keys.is_empty() {
        return Err(ConfigBatchEditError::BadArity);
    }
    // Parse ALL keys before anything is touched (US-1/S1): each resolves
    // in the knob table, naming the offender. Unset carries no values,
    // so there is no value-parse step (D-ERROR: `BadValue` is set-only).
    let mut parsed_keys: Vec<(String, ConfigKey)> = Vec::new();
    for key in keys {
        let parsed = ConfigKey::from_dotted(key)
            .ok_or_else(|| ConfigBatchEditError::UnknownKey { key: key.clone() })?;
        parsed_keys.push((key.clone(), parsed));
    }
    // Env-guard ALL keys before the write (US-1/S2), naming the pin.
    for (key, _) in &parsed_keys {
        if locked.contains(key) {
            return Err(ConfigBatchEditError::EnvLocked { key: key.clone() });
        }
    }
    // Duplicate check after the guards (US-1/S3, AC-5): removal is
    // idempotent, but ambiguity is never resolved silently — the same
    // posture as the set batch's last-wins rejection (D-ATOMIC).
    let mut seen = std::collections::BTreeSet::new();
    for (key, _) in &parsed_keys {
        if !seen.insert(key.as_str()) {
            return Err(ConfigBatchEditError::DuplicateKey { key: key.clone() });
        }
    }
    let removals: Vec<(String, String)> = parsed_keys
        .iter()
        .map(|(_, parsed)| {
            let (section, key) = parsed.dotted();
            (section.to_owned(), key.to_owned())
        })
        .collect();
    let guard_keys: Vec<&str> = parsed_keys.iter().map(|(key, _)| key.as_str()).collect();
    let outcome = run_settings_write(
        write_path,
        locked,
        &guard_keys,
        |path| crate::settings_overlay::prove_and_remove_doc(path, &removals),
        engine,
    )
    .await
    .map_err(|err| match err {
        SettingsWriteError::EnvLocked => {
            // The port's own guard tripped; our named guard above
            // runs first, so this arm is a port-race paranoia — the
            // first batch key is the honest name available here.
            let key = guard_keys
                .iter()
                .find(|key| locked.contains(**key))
                .map(|key| (*key).to_owned())
                .unwrap_or_default();
            ConfigBatchEditError::EnvLocked { key }
        }
        SettingsWriteError::Overlay(overlay) => match overlay {
            OverlayError::Io(io) => ConfigBatchEditError::Io(io),
            OverlayError::Rejected(config_err) => ConfigBatchEditError::Rejected(config_err),
        },
        SettingsWriteError::Config(config_err) => ConfigBatchEditError::Rejected(config_err),
    })?;
    let applied_live = outcome.apply == LiveApply::Applied;
    let entries = parsed_keys
        .into_iter()
        .map(|(key, _)| ConfigSetView {
            key,
            value: None,
            applied_live,
        })
        .collect();
    Ok(ConfigBatchUnsetView {
        entries,
        applied_live,
    })
}

/// The typed set/unset receipt's human line (W-5's text lane): one
/// honest sentence — which lane applied. The wording is presentation;
/// the tests assert the typed view, never this text.
#[must_use]
pub fn render_set_text(view: &ConfigSetView) -> String {
    let key = &view.key;
    match (view.applied_live, &view.value) {
        (true, Some(value)) => format!("{key} set to {value}; the running app applies it now"),
        (false, Some(value)) => {
            format!("{key} set to {value}; it applies at the next start")
        }
        (true, None) => format!("{key} removed; the running app applies it now"),
        (false, None) => format!("{key} removed; it applies at the next start"),
    }
}

/// The typed refusal of one `config set`/`unset` *batch* invocation
/// (#910/#1014, journeys US-2/US-1): the set lattice plus the
/// batch-only classes. Every
/// refusal names the offending key where one exists, and no variant
/// in the chain before the write ever touches the file (the
/// parse/guard/duplicate chain runs before the document load — the
/// prove skeleton is never invoked on a doomed batch).
#[derive(Debug, thiserror::Error)]
pub enum ConfigBatchEditError {
    /// Zero pairs or an odd argv count for `config set` (`<k1> <v1>
    /// [<k2> <v2> …]`), or zero keys for `config unset` (`<k1> [<k2>
    /// …]`) — the arity grammars refuse before anything is touched
    /// (set US-2/S1; unset US-1/S1, D-GRAMMAR).
    #[error("expected an even list of key/value pairs (set), or at least one key (unset)")]
    BadArity,
    /// The same dotted key appears twice in one batch — ambiguity is
    /// never resolved silently (last-wins rejected, US-2 AC-5).
    #[error("duplicate key in the batch")]
    DuplicateKey {
        /// The duplicated dotted key.
        key: String,
    },
    /// One of the batch's keys is pinned by a `TASKBOARD_*` env value
    /// — the whole batch refuses typed naming the key, nothing
    /// written (US-2 AC-3).
    #[error("the key is set by an environment variable")]
    EnvLocked {
        /// The pinned dotted key.
        key: String,
    },
    /// A key is not in the knob table (US-2/S1).
    #[error("unknown config key")]
    UnknownKey {
        /// The unknown dotted key.
        key: String,
    },
    /// A value failed its key's predicate (US-2/S1, AC-2). Set-only:
    /// the unset batch carries no values, so this variant is never
    /// constructed from the unset path (D-ERROR).
    #[error("the value does not fit the key")]
    BadValue {
        /// The refused dotted key.
        key: String,
    },
    /// The toml-layer read or atomic write failed.
    #[error("the config file could not be written")]
    Io(#[from] std::io::Error),
    /// The prove gate refused the *whole* batch: the resulting
    /// document would not boot — a failing mid-batch key rolls back
    /// by construction (the write never happened, US-2 AC-4).
    #[error("the write would make the config unbootable")]
    Rejected(#[source] Box<ConfigError>),
}

/// The typed value → `toml_edit` item mapping for the batch writer
/// (`parse_value`'s inverse half): the same typed fronts the
/// per-intent writers land, minted as items for the one document.
fn batch_edit(parsed: ConfigKey, typed: &TypedSetting) -> crate::settings_overlay::BatchEdit {
    let (section, key) = parsed.dotted();
    let item = match (parsed, typed) {
        (ConfigKey::DayStart, TypedSetting::DayStart(time)) => {
            toml_edit::value(crate::settings_overlay::render_day_start(*time))
        }
        (
            ConfigKey::RemindersEnabled
            | ConfigKey::FocusEnabled
            | ConfigKey::FocusChain
            | ConfigKey::FocusNotify
            | ConfigKey::FilesSyncEnabled,
            TypedSetting::Bool(value),
        ) => toml_edit::value(*value),
        (
            ConfigKey::TickSecs
            | ConfigKey::PollInterval
            | ConfigKey::BackoffInitial
            | ConfigKey::BackoffMax
            | ConfigKey::FilesSyncPollSecs,
            TypedSetting::Secs(secs),
        ) => toml_edit::value(i64::try_from(*secs).unwrap_or(i64::MAX)),
        (
            ConfigKey::FocusWorkMins | ConfigKey::FocusBreakMins | ConfigKey::FocusLongBreakMins,
            TypedSetting::FocusMins(mins),
        ) => toml_edit::value(i64::try_from(*mins).unwrap_or(i64::MAX)),
        (ConfigKey::FocusLongBreakEvery, TypedSetting::FocusEvery(every)) => {
            toml_edit::value(i64::try_from(*every).unwrap_or(i64::MAX))
        }
        (ConfigKey::DueSoonDays, TypedSetting::DueSoonDays(days)) => {
            toml_edit::value(i64::from(*days))
        }
        (ConfigKey::ReminderBefore, TypedSetting::Before(offset)) => {
            toml_edit::value(offset.text_form().clone())
        }
        (ConfigKey::ReminderSink, TypedSetting::Sink(choice)) => toml_edit::value(match choice {
            SinkChoice::Auto => "auto",
            SinkChoice::Log => "log",
            SinkChoice::Desktop => "desktop",
        }),
        _ => unreachable!("parse_value and batch_edit agree on the key's value shape"),
    };
    crate::settings_overlay::BatchEdit {
        section: section.to_owned(),
        key: key.to_owned(),
        item,
    }
}

/// `config set <k1> <v1> [<k2> <v2> …]` (#910, journey US-2): the
/// local lane of the atomic batch — one document, one prove verdict,
/// one write, one lane bit for the whole invocation. Any refusal
/// (arity, unknown key, bad value, env-pinned key, duplicate key,
/// prove gate) leaves the file byte-identical and names the offending
/// key typed.
///
/// # Errors
///
/// [`ConfigBatchEditError`] — the typed refusal classes.
pub async fn set_batch(
    config_path: Option<std::path::PathBuf>,
    pairs: &[String],
) -> Result<ConfigBatchSetView, ConfigBatchEditError> {
    let write_path = AppConfig::write_path(config_path);
    let locked = AppConfig::env_overridden_keys(Some(write_path.as_path()))
        .map_err(|err| ConfigBatchEditError::Io(std::io::Error::other(err.to_string())))?;
    set_batch_with_locked(&write_path, pairs, &locked, None).await
}

/// The lock-set seam for the batch (the [`set_with_locked`] twin,
/// journey W-5): parse *all* pairs → env-guard *all* keys →
/// duplicate-key check → **one** `run_settings_write`-shaped port
/// call carrying the doc-level batch writer. The chain runs before
/// the document load, so the prove skeleton is never invoked on a
/// doomed batch.
async fn set_batch_with_locked(
    write_path: &std::path::Path,
    pairs: &[String],
    locked: &std::collections::BTreeSet<String>,
    engine: Option<&taskboard_state::EngineHandle>,
) -> Result<ConfigBatchSetView, ConfigBatchEditError> {
    if pairs.is_empty() || !pairs.len().is_multiple_of(2) {
        return Err(ConfigBatchEditError::BadArity);
    }
    // Parse ALL pairs before anything is touched (US-2/S1): each key
    // resolves in the knob table, each value parses against its
    // predicate.
    let mut parsed_pairs: Vec<(String, ConfigKey, TypedSetting)> = Vec::new();
    for [key, value] in pairs.as_chunks::<2>().0 {
        let parsed = ConfigKey::from_dotted(key)
            .ok_or_else(|| ConfigBatchEditError::UnknownKey { key: key.clone() })?;
        let Ok(typed) = parsed.parse_value(value) else {
            return Err(ConfigBatchEditError::BadValue { key: key.clone() });
        };
        parsed_pairs.push((key.clone(), parsed, typed));
    }
    // Env-guard ALL keys before the write (US-2/S2), naming the pin.
    for (key, _, _) in &parsed_pairs {
        if locked.contains(key) {
            return Err(ConfigBatchEditError::EnvLocked { key: key.clone() });
        }
    }
    // Duplicate check after the guards (US-2/S3, AC-5): last-wins is
    // never silently applied.
    let mut seen = std::collections::BTreeSet::new();
    for (key, _, _) in &parsed_pairs {
        if !seen.insert(key.as_str()) {
            return Err(ConfigBatchEditError::DuplicateKey { key: key.clone() });
        }
    }
    let edits: Vec<crate::settings_overlay::BatchEdit> = parsed_pairs
        .iter()
        .map(|(_, parsed, typed)| batch_edit(*parsed, typed))
        .collect();
    let guard_keys: Vec<&str> = parsed_pairs
        .iter()
        .map(|(key, _, _)| key.as_str())
        .collect();
    let outcome = run_settings_write(
        write_path,
        locked,
        &guard_keys,
        |path| crate::settings_overlay::prove_and_write_doc(path, &edits),
        engine,
    )
    .await
    .map_err(|err| match err {
        SettingsWriteError::EnvLocked => {
            // The port's own guard tripped; our named guard above
            // runs first, so this arm is a port-race paranoia — the
            // first batch key is the honest name available here.
            let key = guard_keys
                .iter()
                .find(|key| locked.contains(**key))
                .map(|key| (*key).to_owned())
                .unwrap_or_default();
            ConfigBatchEditError::EnvLocked { key }
        }
        SettingsWriteError::Overlay(overlay) => match overlay {
            OverlayError::Io(io) => ConfigBatchEditError::Io(io),
            OverlayError::Rejected(config_err) => ConfigBatchEditError::Rejected(config_err),
        },
        SettingsWriteError::Config(config_err) => ConfigBatchEditError::Rejected(config_err),
    })?;
    let applied_live = outcome.apply == LiveApply::Applied;
    let entries = parsed_pairs
        .into_iter()
        .map(|(key, _, typed)| ConfigSetView {
            key,
            value: Some(typed.echo()),
            applied_live,
        })
        .collect();
    Ok(ConfigBatchSetView {
        entries,
        applied_live,
    })
}

/// The typed batch receipt's human lines (W-5's text lane): one
/// honest line per entry (the per-key echoes), presentation only.
#[must_use]
pub fn render_batch_set_text(view: &ConfigBatchSetView) -> String {
    view.entries
        .iter()
        .map(render_set_text)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Dispatch glue so `dispatch` stays a flat table.
pub(crate) async fn handle(cmd: &ConfigCmd, ctx: &mut CommandCtx<'_>) -> u8 {
    match *cmd {
        ConfigCmd::Print => match print(ctx.config_path.clone()) {
            Ok(view) => {
                emit_to(&mut ctx.io, &view, &render_text(&view));
                0
            }
            Err(err) => report_to(&mut ctx.io, &err),
        },
        ConfigCmd::Path => {
            let view = path(ctx.config_path.clone());
            emit_to(&mut ctx.io, &view, &render_path_text(&view));
            0
        }
        ConfigCmd::Set { ref pairs } => match set_batch(ctx.config_path.clone(), pairs).await {
            Ok(view) => {
                emit_to(&mut ctx.io, &view, &render_batch_set_text(&view));
                0
            }
            Err(err) => report_to(&mut ctx.io, &CommandError::ConfigBatch(err)),
        },
        ConfigCmd::Unset { ref keys } => match unset(ctx.config_path.clone(), keys).await {
            Ok(view) => {
                emit_to(&mut ctx.io, &view, &render_batch_set_text(&view));
                0
            }
            Err(err) => report_to(&mut ctx.io, &CommandError::ConfigBatch(err)),
        },
        // `config reload` is `IpcRoute::IpcOnly` (journey US-2/S1,
        // D-RELOAD-ROUTE): main intercepts the verb before dispatch
        // ever sees it. This arm is the defensive fallback — the verb
        // has no local semantic (D-RELOAD-VERB), so the standing
        // IpcOnly refusal is the honest answer.
        ConfigCmd::Reload => report_to(&mut ctx.io, &CommandError::NoDaemon),
    }
}

/// The mutation arms' env lock set over the daemon's own env truth
/// (D8's recorded asymmetry); an unreadable config file is the typed
/// `ConfigSet::Io` report.
fn env_lock_set(
    write_path: &std::path::Path,
) -> Result<std::collections::BTreeSet<String>, CommandError> {
    AppConfig::env_overridden_keys(Some(write_path))
        .map_err(|err| ConfigSetError::Io(std::io::Error::other(err.to_string())))
        .map_err(CommandError::from)
}

/// The daemon's `config set`/`unset` arm (journey W-2, US-1/S3–S5):
/// the relayed Exec reaches the booted core, so the same seam runs
/// with the daemon's engine handed to the port — the live lane. The
/// daemon's env truth is authoritative for its own writes (D8's
/// recorded asymmetry); the reply's typed view travels back as the
/// Exec payload bytes.
pub(crate) async fn handle_daemon(
    cmd: &ConfigCmd,
    app: &crate::App,
    config_path: Option<&std::path::Path>,
    io: &mut crate::commands::CommandIo<'_>,
) -> u8 {
    let write_path = AppConfig::write_path(config_path.map(std::path::Path::to_path_buf));
    // The relay verbs keep their own typed classes end-to-end: a
    // `set`/`unset` batch refusal is a `ConfigBatch` report — one verb,
    // one error family (D-ERROR/D-SCOPE: the widened grammar routes the
    // one-key form through the same batch classes, a superset naming
    // the same key) — the relay never relabels an invocation it only
    // carries.
    match *cmd {
        ConfigCmd::Set { ref pairs } => {
            // The env lock set is the mutation arms' own precondition —
            // the reload arm deliberately skips it: the manual verb must
            // reach `reload_once` (and report its typed refusal) even
            // while the file is unbootable.
            let locked = match env_lock_set(&write_path) {
                Ok(locked) => locked,
                Err(err) => return report_to(io, &err),
            };
            match set_batch_with_locked(&write_path, pairs, &locked, Some(&app.engine)).await {
                Ok(view) => {
                    emit_to(io, &view, &render_batch_set_text(&view));
                    0
                }
                Err(err) => report_to(io, &CommandError::ConfigBatch(err)),
            }
        }
        ConfigCmd::Unset { ref keys } => {
            let locked = match env_lock_set(&write_path) {
                Ok(locked) => locked,
                Err(err) => return report_to(io, &err),
            };
            match unset_batch_with_locked(&write_path, keys, &locked, Some(&app.engine)).await {
                Ok(view) => {
                    emit_to(io, &view, &render_batch_set_text(&view));
                    0
                }
                Err(err) => report_to(io, &CommandError::ConfigBatch(err)),
            }
        }
        // The manual reload runs the watcher lane's own choreography
        // (D-RELOAD-PATH): `reload_once` is the *same function* the
        // watcher's debouncer calls, so manual and watcher convergence
        // are equivalent by construction (US-2 AC-1's pin). The outcome
        // travels to the invoking terminal as the Exec payload; no
        // broadcast is emitted here (the watcher's broadcast lane stays
        // watcher-triggered-only — double-reporting is noise, R-5).
        ConfigCmd::Reload => {
            let outcome = crate::config_watcher::reload_once(&write_path, &app.engine).await;
            let view = ConfigReloadView {
                outcome,
                path: write_path.clone(),
            };
            emit_to(io, &view, &render_reload_text(&view));
            0
        }
        // Print/Path stay `IpcRoute::Local` (journey R-2); only
        // Set/Unset/Reload take the relay lane, so nothing else arrives
        // here.
        ConfigCmd::Print | ConfigCmd::Path => {
            unreachable!("the relay lane carries only config set/unset/reload (R-2)")
        }
    }
}

/// The typed reload receipt's human line (US-2/S3's text lane): one
/// honest sentence — what the attempt did. The wording is
/// presentation; the tests assert the typed view, never this text.
#[must_use]
pub fn render_reload_text(view: &ConfigReloadView) -> String {
    let path = view.path.display();
    match view.outcome {
        taskboard_domain::ConfigReloadOutcome::Applied => {
            format!("reloaded from {path}; the running app applies it now")
        }
        taskboard_domain::ConfigReloadOutcome::Unchanged => {
            format!("{path} matches the running app's settings; nothing to apply")
        }
        taskboard_domain::ConfigReloadOutcome::SkippedTransient => {
            format!("{path} was unreadable at the attempt (mid-save?); retry")
        }
        taskboard_domain::ConfigReloadOutcome::RefusedDecode
        | taskboard_domain::ConfigReloadOutcome::RefusedValidate => {
            format!(
                "{path} is not bootable right now; the running app keeps its last-known-good settings"
            )
        }
        taskboard_domain::ConfigReloadOutcome::EngineGone => {
            format!("{path} stays the boot authority; the running app did not take the reload")
        }
        taskboard_domain::ConfigReloadOutcome::WatcherDisabled => {
            format!("{path} stays the boot authority; this session has no live reload")
        }
    }
}

#[cfg(test)]
mod set_tests {
    /// The #984 dated-pin gate (the #865 re-homing debt, tracked in the
    /// round-3 backlog): true when no `TASKBOARD_*` ambient is present.
    /// The set verbs' guard/persist reads compose the process env in
    /// production, and this lib crate forbids the env mutation that
    /// `taskboard_env_lock` needs — `tests/services/ui_settings.rs` is
    /// the hostile-oracle home for the re-homed legs.
    fn clean_ambient() -> bool {
        !std::env::vars_os().any(|(key, _)| key.as_encoded_bytes().starts_with(b"TASKBOARD_"))
    }

    use super::*;

    use crate::config::DEFAULT_FOCUS_WORK_MINS;
    use taskboard_domain::{StateCommand, SystemEvent};

    fn temp_config(tag: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("taskboard-config-set-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir.join("taskboard.toml")
    }

    /// D-5's happy arm, now the typed next-start lane (US-2/S3,
    /// `applied_live: false` is a success): a known key writes the
    /// toml layer through the prove-gate overlay writer, and a
    /// re-read through the figment stack sees the value.
    #[tokio::test]
    async fn us2_s3_config_set_writes_the_toml_layer_and_reports_next_start() {
        // Dated pin: the ambient gate, `clean_ambient`'s doc (#1113).
        if !clean_ambient() {
            return;
        }
        let path = temp_config("happy");
        let _ = std::fs::remove_file(&path);
        let view = set(Some(path.clone()), "reminders.tick_secs", "45")
            .await
            .expect("the write succeeds");
        assert_eq!(view.key, "reminders.tick_secs");
        assert_eq!(view.value.as_deref(), Some("45"));
        assert!(!view.applied_live, "the local lane is the next-start lane");
        let text = std::fs::read_to_string(&path).expect("the file exists");
        assert!(
            text.contains("tick_secs = 45"),
            "the toml layer carries the override: {text}"
        );
        let file_only =
            AppConfig::extract_file_only(Some(path.as_path())).expect("the file decodes");
        assert_eq!(file_only.reminders.tick_secs, 45, "figment re-read");
        assert!(
            AppConfig::extract_file_only(Some(path.as_path())).is_ok(),
            "the written config is a valid config"
        );
        std::fs::remove_dir_all(path.parent().expect("dir")).ok();
    }

    /// D-5's env arm: an env-overridden key refuses typed
    /// (`EnvLocked`) and writes nothing (composed over the lock-set
    /// seam — the env-diff truth table owns the env half).
    #[tokio::test]
    async fn an_env_overridden_key_refuses_typed_without_writing() {
        // Dated pin: the ambient gate, `clean_ambient`'s doc (#1113).
        if !clean_ambient() {
            return;
        }
        let path = temp_config("envlocked");
        std::fs::write(&path, "[reminders]\ntick_secs = 30\n").expect("seed config");
        let mut locked = std::collections::BTreeSet::new();
        locked.insert("reminders.tick_secs".to_owned());
        let err = set_with_locked(&path, "reminders.tick_secs", "45", &locked, None)
            .await
            .expect_err("the locked key refuses");
        assert!(
            matches!(err, ConfigSetError::EnvLocked { ref key } if key == "reminders.tick_secs"),
            "the typed refusal: {err:?}"
        );
        let text = std::fs::read_to_string(&path).expect("the file survives");
        assert!(text.contains("tick_secs = 30"), "nothing wrote: {text}");
        std::fs::remove_dir_all(path.parent().expect("dir")).ok();
    }

    /// D-5's refusal arms: an unknown key and a malformed value
    /// refuse typed and write nothing.
    #[tokio::test]
    async fn unknown_keys_and_malformed_values_refuse_typed_without_writing() {
        // Dated pin: the ambient gate, `clean_ambient`'s doc (#1113).
        if !clean_ambient() {
            return;
        }
        let path = temp_config("refusals");
        let _ = std::fs::remove_file(&path);
        let err = set(Some(path.clone()), "not.a.knob", "1")
            .await
            .expect_err("the unknown key refuses");
        assert!(matches!(err, ConfigSetError::UnknownKey { .. }));
        let err = set(Some(path.clone()), "reminders.tick_secs", "0")
            .await
            .expect_err("the zero tick refuses");
        assert!(
            matches!(err, ConfigSetError::BadValue { .. }),
            "the malformed value refuses: {err:?}"
        );
        let err = set(Some(path.clone()), "reminders.sink", "loud")
            .await
            .expect_err("the unknown sink refuses");
        assert!(matches!(err, ConfigSetError::BadValue { .. }));
        assert!(!path.exists(), "a refusal never creates the file");
        std::fs::remove_dir_all(path.parent().expect("dir")).ok();
    }

    /// `us4_s3` on the local lane: a set whose resulting document
    /// fails `AppConfig::validate` on a previously-valid file refuses
    /// typed (`Rejected`) and the file is byte-identical (the prove
    /// gate both lanes share, journey D4).
    #[tokio::test]
    async fn us4_s3_a_validity_crossing_set_refuses_and_touches_nothing() {
        // Dated pin: the ambient gate, `clean_ambient`'s doc (#1113).
        if !clean_ambient() {
            return;
        }
        let path = temp_config("proverefusal");
        std::fs::write(
            &path,
            "[sync]\npoll_interval_secs = 30\nbackoff_initial_secs = 5\nbackoff_max_secs = 300\n",
        )
        .expect("seed config");
        let err = set(Some(path.clone()), "sync.backoff_initial_secs", "400")
            .await
            .expect_err("the crossing write refuses");
        assert!(
            matches!(err, ConfigSetError::Rejected(_)),
            "the typed prove refusal: {err:?}"
        );
        let text = std::fs::read_to_string(&path).expect("the file survives");
        assert!(
            text.contains("backoff_initial_secs = 5"),
            "byte-identical file: {text}"
        );
        std::fs::remove_dir_all(path.parent().expect("dir")).ok();
    }

    /// D-5's unset arm: `config unset` removes the override through
    /// the prove-gate removal writer (journey W-4) so the default
    /// applies again (the figment re-read sees no override), and the
    /// typed view reports the next-start lane (US-3/S2).
    #[tokio::test]
    async fn us10_s2_config_unset_removes_the_key() {
        // Dated pin: the ambient gate, `clean_ambient`'s doc (#1113).
        if !clean_ambient() {
            return;
        }
        let path = temp_config("unset");
        set(Some(path.clone()), "focus.work_mins", "50")
            .await
            .expect("set");
        let view = unset(Some(path.clone()), &["focus.work_mins".to_owned()])
            .await
            .expect("unset");
        assert_eq!(view.entries.len(), 1);
        assert_eq!(view.entries[0].key, "focus.work_mins");
        assert_eq!(view.entries[0].value, None);
        assert!(!view.applied_live, "the local lane");
        let file_only =
            AppConfig::extract_file_only(Some(path.as_path())).expect("the file decodes");
        assert_eq!(
            file_only.focus.work_mins, DEFAULT_FOCUS_WORK_MINS,
            "the override is gone; the default applies"
        );
        std::fs::remove_dir_all(path.parent().expect("dir")).ok();
    }

    /// `us5_s1` (US-5 AC-1): the typed receipt's shape — `key`, the
    /// echoed `value` (canonical text form, not the raw spelling),
    /// and the lane bit — structural, machine-checkable.
    #[tokio::test]
    async fn us5_s1_the_typed_view_carries_key_value_and_lane_bit() {
        // Dated pin: the ambient gate, `clean_ambient`'s doc (#1113).
        if !clean_ambient() {
            return;
        }
        let path = temp_config("view");
        let _ = std::fs::remove_file(&path);
        let view = set(Some(path.clone()), "reminders.tick_secs", "45")
            .await
            .expect("set");
        assert_eq!(view.key, "reminders.tick_secs");
        assert_eq!(view.value.as_deref(), Some("45"));
        assert!(!view.applied_live, "the local lane bit");
        let view = unset(Some(path.clone()), &["reminders.tick_secs".to_owned()])
            .await
            .expect("unset");
        assert_eq!(view.entries.len(), 1);
        assert_eq!(view.entries[0].key, "reminders.tick_secs");
        assert_eq!(view.entries[0].value, None, "an unset echoes no value");
        assert!(!view.applied_live);
        std::fs::remove_dir_all(path.parent().expect("dir")).ok();
    }

    /// US-5 AC-2: `config print` stays truthful after a set — the
    /// printed effective value equals the written one (the file is
    /// written before any engine apply).
    #[tokio::test]
    async fn us5_s2_config_print_stays_truthful_after_a_set() {
        // Dated pin: the ambient gate, `clean_ambient`'s doc (#1113).
        if !clean_ambient() {
            return;
        }
        let path = temp_config("print");
        let view = set(Some(path.clone()), "reminders.tick_secs", "45")
            .await
            .expect("set");
        assert_eq!(view.value.as_deref(), Some("45"));
        let printed = print(Some(path.clone())).expect("print decodes");
        assert_eq!(printed.config.reminders.tick_secs, 45, "print is truthful");
        assert!(printed.valid, "the written file is valid");
        std::fs::remove_dir_all(path.parent().expect("dir")).ok();
    }

    // -- the daemon lane (journey W-2, US-1) ------------------------------

    /// `us1_s4` + `us1_s5` (US-1 AC-1): a set against a running
    /// daemon's engine applies live — the overlay write lands the
    /// proven file (S4), the engine takes the `UpdateSettings`
    /// snapshot (S5) — and the typed view reports
    /// `applied_live: true`.
    #[tokio::test]
    async fn us1_s4_the_daemon_lane_write_lands_and_applies_live() {
        // Dated pin: the ambient gate, `clean_ambient`'s doc (#1113).
        if !clean_ambient() {
            return;
        }
        let fixture = crate::test_support::standalone_boot_fixture("ac1").await;
        let (path, app) = (fixture.config_path.clone(), fixture.app);
        let locked = std::collections::BTreeSet::new();
        let view = set_with_locked(&path, "focus.work_mins", "50", &locked, Some(&app.engine))
            .await
            .expect("the live write lands");
        assert!(view.applied_live, "the live lane: {view:?}");
        assert_eq!(view.value.as_deref(), Some("50"));
        let published = app.engine.shared_state().load();
        let focus = published.settings.focus.as_ref().expect("focus on");
        assert_eq!(
            focus.work(),
            chrono::TimeDelta::minutes(50),
            "the engine took the value"
        );
        let text = std::fs::read_to_string(&path).expect("the file");
        assert!(text.contains("work_mins = 50"), "the file wrote: {text}");
        app.shutdown().await;
        let fixture = crate::test_support::standalone_boot_fixture("ac1").await;
        let (path, app) = (fixture.config_path.clone(), fixture.app);
        let locked = std::collections::BTreeSet::new();
        let view = set_with_locked(&path, "focus.work_mins", "50", &locked, Some(&app.engine))
            .await
            .expect("the live write lands");
        assert!(view.applied_live, "the live lane: {view:?}");
        assert_eq!(view.value.as_deref(), Some("50"));
        let published = app.engine.shared_state().load();
        let focus = published.settings.focus.as_ref().expect("focus on");
        assert_eq!(
            focus.work(),
            chrono::TimeDelta::minutes(50),
            "the engine took the value"
        );
        let text = std::fs::read_to_string(&path).expect("the file");
        assert!(text.contains("work_mins = 50"), "the file wrote: {text}");
        app.shutdown().await;
    }

    /// `us1_s6` (US-5/S1 on the live lane): the daemon arm's reply
    /// view is the typed `ConfigSetView` — JSON carries `key`,
    /// `value`, and the lane bit `applied_live: true`; the text lane
    /// is one honest line.
    #[tokio::test]
    async fn us1_s6_the_daemon_reply_view_carries_the_lane_bit() {
        // Dated pin: the ambient gate, `clean_ambient`'s doc (#1113).
        if !clean_ambient() {
            return;
        }
        let fixture = crate::test_support::standalone_boot_fixture("s6").await;
        let (path, app) = (fixture.config_path.clone(), fixture.app);
        let mut out = Vec::new();
        let mut err = Vec::new();
        let mut io = crate::commands::CommandIo {
            out: &mut out,
            err: &mut err,
            json: true,
        };
        let cmd = ConfigCmd::Set {
            pairs: vec!["focus.work_mins".to_owned(), "50".to_owned()],
        };
        let code = handle_daemon(&cmd, &app, Some(&path), &mut io).await;
        assert_eq!(code, 0, "a live set is a success");
        let view: ConfigBatchSetView =
            serde_json::from_slice(&out).expect("the typed view travels as JSON");
        assert!(view.applied_live, "the live lane bit");
        let entries: Vec<_> = view
            .entries
            .iter()
            .map(|entry| (entry.key.as_str(), entry.value.as_deref()))
            .collect();
        assert_eq!(
            entries,
            vec![("focus.work_mins", Some("50"))],
            "one per-key echo for the single-pair form"
        );
        app.shutdown().await;
        let fixture = crate::test_support::standalone_boot_fixture("s6").await;
        let (path, app) = (fixture.config_path.clone(), fixture.app);
        let mut out = Vec::new();
        let mut err = Vec::new();
        let mut io = crate::commands::CommandIo {
            out: &mut out,
            err: &mut err,
            json: true,
        };
        let cmd = ConfigCmd::Set {
            pairs: vec!["focus.work_mins".to_owned(), "50".to_owned()],
        };
        let code = handle_daemon(&cmd, &app, Some(&path), &mut io).await;
        assert_eq!(code, 0, "a live set is a success");
        let view: ConfigBatchSetView =
            serde_json::from_slice(&out).expect("the typed view travels as JSON");
        assert!(view.applied_live, "the live lane bit");
        let entries: Vec<_> = view
            .entries
            .iter()
            .map(|entry| (entry.key.as_str(), entry.value.as_deref()))
            .collect();
        assert_eq!(
            entries,
            vec![("focus.work_mins", Some("50"))],
            "one per-key echo for the single-pair form"
        );
        app.shutdown().await;
    }

    /// `us1_ac2`: the prove gate precedes the engine — a
    /// validity-crossing write refuses typed, the file is untouched,
    /// and the engine's published settings are unchanged (nothing was
    /// sent).
    #[tokio::test]
    async fn us1_ac2_the_daemon_lane_proves_before_the_engine() {
        // Dated pin: the ambient gate, `clean_ambient`'s doc (#1113).
        if !clean_ambient() {
            return;
        }
        let fixture = crate::test_support::standalone_boot_fixture("ac2").await;
        let (path, app) = (fixture.config_path.clone(), fixture.app);
        let before = app.engine.shared_state().load().settings.clone();
        let locked = std::collections::BTreeSet::new();
        let err = set_with_locked(
            &path,
            "sync.backoff_initial_secs",
            "400",
            &locked,
            Some(&app.engine),
        )
        .await
        .expect_err("the crossing write refuses");
        assert!(matches!(err, ConfigSetError::Rejected(_)), "{err:?}");
        assert_eq!(
            app.engine.shared_state().load().settings,
            before,
            "the engine never saw the refused write"
        );
        let text = std::fs::read_to_string(&path).expect("the file");
        assert!(
            !text.contains("backoff_initial_secs"),
            "the file is untouched (no sync section landed): {text}"
        );
        app.shutdown().await;
        let fixture = crate::test_support::standalone_boot_fixture("ac2").await;
        let (path, app) = (fixture.config_path.clone(), fixture.app);
        let before = app.engine.shared_state().load().settings.clone();
        let locked = std::collections::BTreeSet::new();
        let err = set_with_locked(
            &path,
            "sync.backoff_initial_secs",
            "400",
            &locked,
            Some(&app.engine),
        )
        .await
        .expect_err("the crossing write refuses");
        assert!(matches!(err, ConfigSetError::Rejected(_)), "{err:?}");
        assert_eq!(
            app.engine.shared_state().load().settings,
            before,
            "the engine never saw the refused write"
        );
        let text = std::fs::read_to_string(&path).expect("the file");
        assert!(
            !text.contains("backoff_initial_secs"),
            "the file is untouched (no sync section landed): {text}"
        );
        app.shutdown().await;
    }

    /// `us3_s3`: unset propagates live on the daemon lane — the
    /// engine publishes the default-derived snapshot and the file no
    /// longer carries the key.
    #[tokio::test]
    async fn us3_s3_the_daemon_lane_unset_propagates_live() {
        // Dated pin: the ambient gate, `clean_ambient`'s doc (#1113).
        if !clean_ambient() {
            return;
        }
        let fixture = crate::test_support::standalone_boot_fixture("ac3").await;
        let (path, app) = (fixture.config_path.clone(), fixture.app);
        let locked = std::collections::BTreeSet::new();
        set_with_locked(&path, "focus.work_mins", "50", &locked, Some(&app.engine))
            .await
            .expect("set live");
        assert_eq!(
            app.engine
                .shared_state()
                .load()
                .settings
                .focus
                .as_ref()
                .expect("minted")
                .work(),
            chrono::TimeDelta::minutes(50)
        );
        let view = unset_batch_with_locked(
            &path,
            &["focus.work_mins".to_owned()],
            &locked,
            Some(&app.engine),
        )
        .await
        .expect("unset lands");
        assert!(view.applied_live, "the live lane");
        assert_eq!(view.entries.len(), 1);
        assert_eq!(view.entries[0].value, None);
        let published = app.engine.shared_state().load();
        assert_ne!(
            published.settings.focus.as_ref().map(|focus| focus.work()),
            Some(chrono::TimeDelta::minutes(50)),
            "the engine fell back off the override"
        );
        let text = std::fs::read_to_string(&path).expect("the file");
        assert!(!text.contains("work_mins"), "the key left the file: {text}");
        app.shutdown().await;
        let fixture = crate::test_support::standalone_boot_fixture("ac3").await;
        let (path, app) = (fixture.config_path.clone(), fixture.app);
        let locked = std::collections::BTreeSet::new();
        set_with_locked(&path, "focus.work_mins", "50", &locked, Some(&app.engine))
            .await
            .expect("set live");
        assert_eq!(
            app.engine
                .shared_state()
                .load()
                .settings
                .focus
                .as_ref()
                .expect("minted")
                .work(),
            chrono::TimeDelta::minutes(50)
        );
        let view = unset_batch_with_locked(
            &path,
            &["focus.work_mins".to_owned()],
            &locked,
            Some(&app.engine),
        )
        .await
        .expect("unset lands");
        assert!(view.applied_live, "the live lane");
        assert_eq!(view.entries.len(), 1);
        assert_eq!(view.entries[0].value, None);
        let published = app.engine.shared_state().load();
        assert_ne!(
            published.settings.focus.as_ref().map(|focus| focus.work()),
            Some(chrono::TimeDelta::minutes(50)),
            "the engine fell back off the override"
        );
        let text = std::fs::read_to_string(&path).expect("the file");
        assert!(!text.contains("work_mins"), "the key left the file: {text}");
        app.shutdown().await;
    }

    /// `us1_s3` (the daemon arm's env guard, US-1/S3; also US-4/S1's
    /// daemon-side pin): an env-pinned key refuses typed before the
    /// write and before any engine command (the guard runs in the
    /// daemon's own env truth — D8's asymmetry).
    #[tokio::test]
    async fn us1_s3_the_daemon_lane_env_guard_refuses_typed() {
        // Dated pin: the ambient gate, `clean_ambient`'s doc (#1113).
        if !clean_ambient() {
            return;
        }
        let fixture = crate::test_support::standalone_boot_fixture("ac4").await;
        let (path, app) = (fixture.config_path.clone(), fixture.app);
        let mut locked = std::collections::BTreeSet::new();
        locked.insert("reminders.tick_secs".to_owned());
        let err = set_with_locked(
            &path,
            "reminders.tick_secs",
            "45",
            &locked,
            Some(&app.engine),
        )
        .await
        .expect_err("the locked key refuses");
        assert!(matches!(err, ConfigSetError::EnvLocked { .. }), "{err:?}");
        let text = std::fs::read_to_string(&path).expect("the file");
        assert!(text.contains("tick_secs = 30"), "nothing wrote: {text}");
        app.shutdown().await;
        let fixture = crate::test_support::standalone_boot_fixture("ac4").await;
        let (path, app) = (fixture.config_path.clone(), fixture.app);
        let mut locked = std::collections::BTreeSet::new();
        locked.insert("reminders.tick_secs".to_owned());
        let err = set_with_locked(
            &path,
            "reminders.tick_secs",
            "45",
            &locked,
            Some(&app.engine),
        )
        .await
        .expect_err("the locked key refuses");
        assert!(matches!(err, ConfigSetError::EnvLocked { .. }), "{err:?}");
        let text = std::fs::read_to_string(&path).expect("the file");
        assert!(text.contains("tick_secs = 30"), "nothing wrote: {text}");
        app.shutdown().await;
    }

    // -- the files_sync.poll_secs lane (#1051, the #673 writer twin) ------

    /// #1051 (the #673 writer twin): `config set files_sync.poll_secs`
    /// applies live — the minted snapshot carries the override, and the
    /// shorten/lengthen axis mints a fresh snapshot each set (the same
    /// sequence shape the #673 doorbell tests run actor-side).
    #[tokio::test]
    async fn files_sync_poll_secs_set_applies_live_and_mints_the_override() {
        // Dated pin: the ambient gate, `clean_ambient`'s doc (#1113).
        if !clean_ambient() {
            return;
        }
        let fixture = crate::test_support::standalone_boot_fixture("poll-lane").await;
        let (path, app) = (fixture.config_path.clone(), fixture.app);
        let locked = std::collections::BTreeSet::new();
        for secs in [12, 90, 2] {
            let view = set_with_locked(
                &path,
                "files_sync.poll_secs",
                &secs.to_string(),
                &locked,
                Some(&app.engine),
            )
            .await
            .expect("the live write lands");
            assert!(view.applied_live, "the live lane: {view:?}");
            assert_eq!(view.value.as_deref(), Some(secs.to_string().as_str()));
            assert_eq!(
                app.engine
                    .shared_state()
                    .load()
                    .settings
                    .files_sync_poll_secs,
                Some(taskboard_domain::TickSecs::new(secs).expect("secs > 0")),
                "the engine minted the override"
            );
        }
        app.shutdown().await;
        let fixture = crate::test_support::standalone_boot_fixture("poll-lane").await;
        let (path, app) = (fixture.config_path.clone(), fixture.app);
        let locked = std::collections::BTreeSet::new();
        for secs in [12, 90, 2] {
            let view = set_with_locked(
                &path,
                "files_sync.poll_secs",
                &secs.to_string(),
                &locked,
                Some(&app.engine),
            )
            .await
            .expect("the live write lands");
            assert!(view.applied_live, "the live lane: {view:?}");
            assert_eq!(view.value.as_deref(), Some(secs.to_string().as_str()));
            assert_eq!(
                app.engine
                    .shared_state()
                    .load()
                    .settings
                    .files_sync_poll_secs,
                Some(taskboard_domain::TickSecs::new(secs).expect("secs > 0")),
                "the engine minted the override"
            );
        }
        app.shutdown().await;
    }

    /// #1051: `config unset files_sync.poll_secs` removes the override
    /// live — the minted snapshot's `files_sync_poll_secs` is `None`
    /// (the actor's `unwrap_or` fallback to `[sync] poll_interval_secs`)
    /// and the key leaves the file.
    #[tokio::test]
    async fn files_sync_poll_secs_unset_propagates_live_to_none() {
        // Dated pin: the ambient gate, `clean_ambient`'s doc (#1113).
        if !clean_ambient() {
            return;
        }
        let fixture = crate::test_support::standalone_boot_fixture("poll-unset").await;
        let (path, app) = (fixture.config_path.clone(), fixture.app);
        let locked = std::collections::BTreeSet::new();
        set_with_locked(
            &path,
            "files_sync.poll_secs",
            "12",
            &locked,
            Some(&app.engine),
        )
        .await
        .expect("set live");
        let view = unset_batch_with_locked(
            &path,
            &["files_sync.poll_secs".to_owned()],
            &locked,
            Some(&app.engine),
        )
        .await
        .expect("unset lands");
        assert!(view.applied_live, "the live lane");
        assert_eq!(view.entries.len(), 1);
        assert_eq!(view.entries[0].value, None);
        assert_eq!(
            app.engine
                .shared_state()
                .load()
                .settings
                .files_sync_poll_secs,
            None,
            "the fallback rule: the override is gone"
        );
        let text = std::fs::read_to_string(&path).expect("the file");
        assert!(!text.contains("poll_secs"), "the key left the file: {text}");
        app.shutdown().await;
        let fixture = crate::test_support::standalone_boot_fixture("poll-unset").await;
        let (path, app) = (fixture.config_path.clone(), fixture.app);
        let locked = std::collections::BTreeSet::new();
        set_with_locked(
            &path,
            "files_sync.poll_secs",
            "12",
            &locked,
            Some(&app.engine),
        )
        .await
        .expect("set live");
        let view = unset_batch_with_locked(
            &path,
            &["files_sync.poll_secs".to_owned()],
            &locked,
            Some(&app.engine),
        )
        .await
        .expect("unset lands");
        assert!(view.applied_live, "the live lane");
        assert_eq!(view.entries.len(), 1);
        assert_eq!(view.entries[0].value, None);
        assert_eq!(
            app.engine
                .shared_state()
                .load()
                .settings
                .files_sync_poll_secs,
            None,
            "the fallback rule: the override is gone"
        );
        let text = std::fs::read_to_string(&path).expect("the file");
        assert!(!text.contains("poll_secs"), "the key left the file: {text}");
        app.shutdown().await;
    }

    /// #1051 + D3's batch arm: the batch lane carries the key too —
    /// one atomic document write, the value lands in the file.
    #[tokio::test]
    async fn files_sync_poll_secs_batch_set_lands_the_value() {
        // Dated pin: the ambient gate, `clean_ambient`'s doc (#1113).
        if !clean_ambient() {
            return;
        }
        let fixture = crate::test_support::standalone_boot_fixture("poll-batch").await;
        let (path, app) = (fixture.config_path.clone(), fixture.app);
        let locked = std::collections::BTreeSet::new();
        let view = set_batch_with_locked(
            &path,
            &["files_sync.poll_secs".to_owned(), "12".to_owned()],
            &locked,
            Some(&app.engine),
        )
        .await
        .expect("the batch lands");
        assert!(view.applied_live, "the live lane: {view:?}");
        let text = std::fs::read_to_string(&path).expect("the file");
        assert!(text.contains("poll_secs = 12"), "the file wrote: {text}");
        app.shutdown().await;
        let fixture = crate::test_support::standalone_boot_fixture("poll-batch").await;
        let (path, app) = (fixture.config_path.clone(), fixture.app);
        let locked = std::collections::BTreeSet::new();
        let view = set_batch_with_locked(
            &path,
            &["files_sync.poll_secs".to_owned(), "12".to_owned()],
            &locked,
            Some(&app.engine),
        )
        .await
        .expect("the batch lands");
        assert!(view.applied_live, "the live lane: {view:?}");
        let text = std::fs::read_to_string(&path).expect("the file");
        assert!(text.contains("poll_secs = 12"), "the file wrote: {text}");
        app.shutdown().await;
    }

    /// #1051's invalid-value rule: `0` and negatives refuse typed at
    /// the parse gate (before any lane choice, nothing written) — the
    /// same `> 0` predicate the `[sync]` secs keys run.
    #[tokio::test]
    async fn files_sync_poll_secs_invalid_values_refuse_typed_before_any_write() {
        // Dated pin: the ambient gate, `clean_ambient`'s doc (#1113).
        if !clean_ambient() {
            return;
        }
        let path = temp_config("poll-bad");
        let _ = std::fs::remove_file(&path);
        for value in ["0", "-3", "abc"] {
            let err = set(Some(path.clone()), "files_sync.poll_secs", value)
                .await
                .expect_err("the value refuses");
            assert!(
                matches!(err, ConfigSetError::BadValue { .. }),
                "{value}: typed refusal"
            );
        }
        let text = std::fs::read_to_string(&path);
        assert!(text.is_err(), "no file was created on any refusal");
        std::fs::remove_dir_all(path.parent().expect("dir")).ok();
    }

    /// The env guard for the new key (mirrors `us1_s3`): an
    /// env-pinned `files_sync.poll_secs` refuses typed before the
    /// write — the lock-set seam, no real env mutation.
    #[tokio::test]
    async fn files_sync_poll_secs_env_pinned_refuses_typed() {
        // Dated pin: the ambient gate, `clean_ambient`'s doc (#1113).
        if !clean_ambient() {
            return;
        }
        let path = temp_config("poll-env");
        let _ = std::fs::remove_file(&path);
        let mut locked = std::collections::BTreeSet::new();
        locked.insert("files_sync.poll_secs".to_owned());
        let err = set_with_locked(&path, "files_sync.poll_secs", "12", &locked, None)
            .await
            .expect_err("the locked key refuses");
        assert!(matches!(err, ConfigSetError::EnvLocked { .. }), "{err:?}");
        let text = std::fs::read_to_string(&path);
        assert!(text.is_err(), "nothing wrote: {text:?}");
        std::fs::remove_dir_all(path.parent().expect("dir")).ok();
    }

    // -- the batch surface (#910, journey US-2) ---------------------------

    /// `us2_s1_batch_parse_refusals_name_the_offending_key` (US-2
    /// AC-2): a batch whose second pair is malformed refuses typed
    /// naming that key, and the file is byte-identical to before —
    /// parse precedes everything, nothing is touched.
    #[tokio::test]
    async fn us2_s1_batch_parse_refusals_name_the_offending_key() {
        // Dated pin: the ambient gate, `clean_ambient`'s doc (#1113).
        if !clean_ambient() {
            return;
        }
        let path = temp_config("batch-parse");
        std::fs::write(&path, "[reminders]\ntick_secs = 30\n").expect("seed config");
        let before = std::fs::read_to_string(&path).expect("read");

        // Zero pairs: the even-pair grammar refuses, nothing touched.
        let err = set_batch(Some(path.clone()), &[])
            .await
            .expect_err("zero pairs refuse");
        assert!(matches!(err, ConfigBatchEditError::BadArity), "{err:?}");

        // Odd count: same class.
        let err = set_batch(Some(path.clone()), &["focus.work_mins".to_owned()])
            .await
            .expect_err("an odd argv refuses");
        assert!(matches!(err, ConfigBatchEditError::BadArity), "{err:?}");

        // Unknown key named.
        let err = set_batch(
            Some(path.clone()),
            &[
                "focus.work_mins".to_owned(),
                "50".to_owned(),
                "not.a.knob".to_owned(),
                "1".to_owned(),
            ],
        )
        .await
        .expect_err("the unknown key refuses");
        assert!(
            matches!(err, ConfigBatchEditError::UnknownKey { ref key } if key == "not.a.knob"),
            "the refusal names the offending key: {err:?}"
        );

        // Malformed second-pair value named; the first pair is
        // individually valid but the batch refuses wholesale.
        let err = set_batch(
            Some(path.clone()),
            &[
                "focus.work_mins".to_owned(),
                "50".to_owned(),
                "reminders.tick_secs".to_owned(),
                "0".to_owned(),
            ],
        )
        .await
        .expect_err("the malformed value refuses");
        assert!(
            matches!(err, ConfigBatchEditError::BadValue { ref key } if key == "reminders.tick_secs"),
            "the refusal names the offending key: {err:?}"
        );
        assert_eq!(
            std::fs::read_to_string(&path).expect("read"),
            before,
            "the file is byte-identical — no pair landed"
        );
        std::fs::remove_dir_all(path.parent().expect("dir")).ok();
    }

    /// `us2_s2_batch_env_guard_names_the_pinned_key` (US-2 AC-3): a
    /// batch containing an env-pinned key refuses typed naming the
    /// key, and NOTHING is written — not even the unpinned pairs.
    #[tokio::test]
    async fn us2_s2_batch_env_guard_names_the_pinned_key() {
        // Dated pin: the ambient gate, `clean_ambient`'s doc (#1113).
        if !clean_ambient() {
            return;
        }
        let path = temp_config("batch-env");
        std::fs::write(&path, "[reminders]\ntick_secs = 30\n").expect("seed config");
        let mut locked = std::collections::BTreeSet::new();
        locked.insert("reminders.tick_secs".to_owned());
        let err = set_batch_with_locked(
            &path,
            &[
                "focus.work_mins".to_owned(),
                "50".to_owned(),
                "reminders.tick_secs".to_owned(),
                "45".to_owned(),
            ],
            &locked,
            None,
        )
        .await
        .expect_err("the pinned key refuses the whole batch");
        assert!(
            matches!(err, ConfigBatchEditError::EnvLocked { ref key } if key == "reminders.tick_secs"),
            "the typed refusal names the pinned key: {err:?}"
        );
        let text = std::fs::read_to_string(&path).expect("read");
        assert!(
            text.contains("tick_secs = 30") && !text.contains("work_mins"),
            "nothing wrote: {text}"
        );
        std::fs::remove_dir_all(path.parent().expect("dir")).ok();
    }

    /// `us2_s3_duplicate_batch_key_refuses_before_any_write` (US-2
    /// AC-5): a duplicated key refuses typed; last-wins is never
    /// applied silently.
    #[tokio::test]
    async fn us2_s3_duplicate_batch_key_refuses_before_any_write() {
        // Dated pin: the ambient gate, `clean_ambient`'s doc (#1113).
        if !clean_ambient() {
            return;
        }
        let path = temp_config("batch-dup");
        std::fs::write(&path, "[reminders]\ntick_secs = 30\n").expect("seed config");
        let locked = std::collections::BTreeSet::new();
        let err = set_batch_with_locked(
            &path,
            &[
                "reminders.tick_secs".to_owned(),
                "45".to_owned(),
                "reminders.tick_secs".to_owned(),
                "60".to_owned(),
            ],
            &locked,
            None,
        )
        .await
        .expect_err("the duplicate refuses");
        assert!(
            matches!(err, ConfigBatchEditError::DuplicateKey { ref key } if key == "reminders.tick_secs"),
            "the typed refusal names the duplicated key: {err:?}"
        );
        let text = std::fs::read_to_string(&path).expect("read");
        assert!(text.contains("tick_secs = 30"), "nothing wrote: {text}");
        std::fs::remove_dir_all(path.parent().expect("dir")).ok();
    }

    /// `us2_s4_batch_lands_one_atomic_document` through the command
    /// surface (US-2 AC-1, the multi-pair shape): a valid batch lands
    /// all pairs as one document and the figment re-read carries all
    /// of them; `config print` stays truthful after the batch.
    #[tokio::test]
    async fn us2_s4_a_valid_batch_lands_all_pairs_as_one_document() {
        // Dated pin: the ambient gate, `clean_ambient`'s doc (#1113).
        if !clean_ambient() {
            return;
        }
        let path = temp_config("batch-ok");
        std::fs::write(&path, "[reminders]\ntick_secs = 30\n").expect("seed config");
        let view = set_batch(
            Some(path.clone()),
            &[
                "reminders.tick_secs".to_owned(),
                "45".to_owned(),
                "focus.work_mins".to_owned(),
                "50".to_owned(),
                "sync.poll_interval_secs".to_owned(),
                "60".to_owned(),
            ],
        )
        .await
        .expect("the batch lands");
        assert!(!view.applied_live, "the local lane bit");
        assert_eq!(view.entries.len(), 3);
        assert_eq!(view.entries[0].key, "reminders.tick_secs");
        assert_eq!(view.entries[0].value.as_deref(), Some("45"));
        let file_only = AppConfig::extract_file_only(Some(path.as_path())).expect("decodes");
        assert_eq!(file_only.reminders.tick_secs, 45);
        assert_eq!(file_only.focus.work_mins, 50);
        assert_eq!(file_only.sync.poll_interval_secs, 60);
        let printed = print(Some(path.clone())).expect("print decodes");
        assert!(printed.valid);
        assert_eq!(printed.config.focus.work_mins, 50, "print is truthful");
        std::fs::remove_dir_all(path.parent().expect("dir")).ok();
    }

    /// `us2_s5_batch_apply_is_one_update_settings` (US-2 AC-6 + AC-1's
    /// engine half): against a running daemon, a valid batch applies
    /// with exactly ONE whole-snapshot `UpdateSettings` for the whole
    /// invocation — observed via one `SettingsChanged` doorbell per
    /// applied batch — and the typed view reports `applied_live: true`
    /// with the per-key echoes.
    #[tokio::test]
    #[allow(clippy::too_many_lines)] // the gate's two lines trip the count
    async fn us2_s5_batch_apply_is_one_update_settings() {
        // Dated pin: the ambient gate, `clean_ambient`'s doc (#1113).
        if !clean_ambient() {
            return;
        }
        let fixture = crate::test_support::standalone_boot_fixture("batch-live").await;
        let (path, app) = (fixture.config_path.clone(), fixture.app);
        let mut doorbells = app.system.subscribe();
        let locked = std::collections::BTreeSet::new();
        let view = set_batch_with_locked(
            &path,
            &[
                "focus.work_mins".to_owned(),
                "50".to_owned(),
                "reminders.tick_secs".to_owned(),
                "45".to_owned(),
            ],
            &locked,
            Some(&app.engine),
        )
        .await
        .expect("the live batch lands");
        assert!(view.applied_live, "the live lane bit: {view:?}");
        let entries: Vec<_> = view
            .entries
            .iter()
            .map(|entry| {
                (
                    entry.key.as_str(),
                    entry.value.as_deref(),
                    entry.applied_live,
                )
            })
            .collect();
        assert_eq!(
            entries,
            vec![
                ("focus.work_mins", Some("50"), true),
                ("reminders.tick_secs", Some("45"), true),
            ],
            "per-key echoes, one shared lane bit"
        );
        // Exactly one engine apply for the whole batch: exactly one
        // SettingsChanged doorbell for this batch's invocation.
        let mut applied = 0_u32;
        while let Ok(event) = doorbells.try_recv() {
            if matches!(event, taskboard_domain::SystemEvent::SettingsChanged) {
                applied += 1;
            }
        }
        assert_eq!(applied, 1, "one UpdateSettings for the whole batch");
        let published = app.engine.shared_state().load();
        assert_eq!(
            published.settings.focus.as_ref().expect("minted").work(),
            chrono::TimeDelta::minutes(50),
            "the engine took the whole batch's snapshot"
        );
        app.shutdown().await;
        let fixture = crate::test_support::standalone_boot_fixture("batch-live").await;
        let (path, app) = (fixture.config_path.clone(), fixture.app);
        let mut doorbells = app.system.subscribe();
        let locked = std::collections::BTreeSet::new();
        let view = set_batch_with_locked(
            &path,
            &[
                "focus.work_mins".to_owned(),
                "50".to_owned(),
                "reminders.tick_secs".to_owned(),
                "45".to_owned(),
            ],
            &locked,
            Some(&app.engine),
        )
        .await
        .expect("the live batch lands");
        assert!(view.applied_live, "the live lane bit: {view:?}");
        let entries: Vec<_> = view
            .entries
            .iter()
            .map(|entry| {
                (
                    entry.key.as_str(),
                    entry.value.as_deref(),
                    entry.applied_live,
                )
            })
            .collect();
        assert_eq!(
            entries,
            vec![
                ("focus.work_mins", Some("50"), true),
                ("reminders.tick_secs", Some("45"), true),
            ],
            "per-key echoes, one shared lane bit"
        );
        // Exactly one engine apply for the whole batch: exactly one
        // SettingsChanged doorbell for this batch's invocation.
        let mut applied = 0_u32;
        while let Ok(event) = doorbells.try_recv() {
            if matches!(event, taskboard_domain::SystemEvent::SettingsChanged) {
                applied += 1;
            }
        }
        assert_eq!(applied, 1, "one UpdateSettings for the whole batch");
        let published = app.engine.shared_state().load();
        assert_eq!(
            published.settings.focus.as_ref().expect("minted").work(),
            chrono::TimeDelta::minutes(50),
            "the engine took the whole batch's snapshot"
        );
        app.shutdown().await;
    }

    /// The batch prove gate on the daemon lane: a batch whose later
    /// pair crosses the whole-document validity refuses typed, the
    /// file is untouched, and the engine never saw anything.
    #[tokio::test]
    async fn the_batch_prove_gate_refuses_wholesale_on_the_daemon_lane() {
        // Dated pin: the ambient gate, `clean_ambient`'s doc (#1113).
        if !clean_ambient() {
            return;
        }
        let fixture = crate::test_support::standalone_boot_fixture("batch-gate").await;
        let (path, app) = (fixture.config_path.clone(), fixture.app);
        let before = app.engine.shared_state().load().settings.clone();
        let locked = std::collections::BTreeSet::new();
        let err = set_batch_with_locked(
            &path,
            &[
                "reminders.tick_secs".to_owned(),
                "45".to_owned(),
                "sync.backoff_initial_secs".to_owned(),
                "400".to_owned(),
            ],
            &locked,
            Some(&app.engine),
        )
        .await
        .expect_err("the crossing pair refuses the whole batch");
        assert!(
            matches!(err, ConfigBatchEditError::Rejected(_)),
            "typed Rejected: {err:?}"
        );
        assert_eq!(
            app.engine.shared_state().load().settings,
            before,
            "the engine never saw the refused batch"
        );
        let text = std::fs::read_to_string(&path).expect("read");
        assert!(
            !text.contains("tick_secs = 45") && !text.contains("backoff"),
            "the file is untouched — no half-provisioned batch: {text}"
        );
        app.shutdown().await;
        let fixture = crate::test_support::standalone_boot_fixture("batch-gate").await;
        let (path, app) = (fixture.config_path.clone(), fixture.app);
        let before = app.engine.shared_state().load().settings.clone();
        let locked = std::collections::BTreeSet::new();
        let err = set_batch_with_locked(
            &path,
            &[
                "reminders.tick_secs".to_owned(),
                "45".to_owned(),
                "sync.backoff_initial_secs".to_owned(),
                "400".to_owned(),
            ],
            &locked,
            Some(&app.engine),
        )
        .await
        .expect_err("the crossing pair refuses the whole batch");
        assert!(
            matches!(err, ConfigBatchEditError::Rejected(_)),
            "typed Rejected: {err:?}"
        );
        assert_eq!(
            app.engine.shared_state().load().settings,
            before,
            "the engine never saw the refused batch"
        );
        let text = std::fs::read_to_string(&path).expect("read");
        assert!(
            !text.contains("tick_secs = 45") && !text.contains("backoff"),
            "the file is untouched — no half-provisioned batch: {text}"
        );
        app.shutdown().await;
    }

    /// D7 on the daemon lane: an engine gone mid-flight is the typed
    /// next-start outcome, NOT an error — the file is written (the
    /// boot authority) and `applied_live: false`.
    #[tokio::test]
    async fn us1_s5_an_engine_gone_write_reports_the_next_start_lane() {
        // Dated pin: the ambient gate, `clean_ambient`'s doc (#1113).
        if !clean_ambient() {
            return;
        }
        let fixture = crate::test_support::standalone_boot_fixture("gone").await;
        let (path, app) = (fixture.config_path.clone(), fixture.app);
        app.system
            .send(SystemEvent::Shutdown)
            .expect("the engine subscribes");
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                let probe = app.engine.execute(StateCommand::RequestSync).await;
                if matches!(probe, Err(taskboard_state::ExecuteError::EngineGone)) {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("the engine drained");
        let locked = std::collections::BTreeSet::new();
        let view = set_with_locked(
            &path,
            "reminders.tick_secs",
            "45",
            &locked,
            Some(&app.engine),
        )
        .await
        .expect("the write still lands");
        assert!(!view.applied_live, "the typed next-start outcome: {view:?}");
        let text = std::fs::read_to_string(&path).expect("the file");
        assert!(
            text.contains("tick_secs = 45"),
            "the file took the write: {text}"
        );
        let _keep = app;
        let fixture = crate::test_support::standalone_boot_fixture("gone").await;
        let (path, app) = (fixture.config_path.clone(), fixture.app);
        app.system
            .send(SystemEvent::Shutdown)
            .expect("the engine subscribes");
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                let probe = app.engine.execute(StateCommand::RequestSync).await;
                if matches!(probe, Err(taskboard_state::ExecuteError::EngineGone)) {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("the engine drained");
        let locked = std::collections::BTreeSet::new();
        let view = set_with_locked(
            &path,
            "reminders.tick_secs",
            "45",
            &locked,
            Some(&app.engine),
        )
        .await
        .expect("the write still lands");
        assert!(!view.applied_live, "the typed next-start outcome: {view:?}");
        let text = std::fs::read_to_string(&path).expect("the file");
        assert!(
            text.contains("tick_secs = 45"),
            "the file took the write: {text}"
        );
        let _keep = app;
    }

    // -- the batch unset (B7, #1014, journey US-1) ------------------------

    fn daemon_io<'a>(out: &'a mut Vec<u8>, err: &'a mut Vec<u8>) -> crate::commands::CommandIo<'a> {
        crate::commands::CommandIo {
            out,
            err,
            json: true,
        }
    }

    /// `us1_s1_batch_unset_parse_refusals_name_the_offending_key`
    /// (US-1/S1, AC-2): zero keys is the typed `BadArity` refusal; an
    /// unknown key in the batch names that key typed; both fire before
    /// anything is touched — the file is byte-identical.
    #[tokio::test]
    async fn us1_s1_batch_unset_parse_refusals_name_the_offending_key() {
        let path = temp_config("batch-unset-parse");
        let _ = std::fs::remove_file(&path);
        let err = unset(Some(path.clone()), &[])
            .await
            .expect_err("zero keys refuses");
        assert!(matches!(err, ConfigBatchEditError::BadArity), "{err:?}");
        let err = unset(
            Some(path.clone()),
            &["reminders.tick_secs".to_owned(), "no.such_key".to_owned()],
        )
        .await
        .expect_err("the unknown key refuses the batch");
        assert!(
            matches!(err, ConfigBatchEditError::UnknownKey { ref key } if key == "no.such_key"),
            "the refusal names the offender: {err:?}"
        );
        let text = std::fs::read_to_string(&path);
        assert!(text.is_err(), "no file was created on any refusal");
        std::fs::remove_dir_all(path.parent().expect("dir")).ok();
    }

    /// `us1_s2_batch_unset_env_guard_names_the_pinned_key` (US-1/S2,
    /// AC-3): an env-pinned key anywhere in the batch refuses the whole
    /// batch typed naming the key, before any write.
    #[tokio::test]
    async fn us1_s2_batch_unset_env_guard_names_the_pinned_key() {
        let path = temp_config("batch-unset-env");
        let _ = std::fs::remove_file(&path);
        let mut locked = std::collections::BTreeSet::new();
        locked.insert("focus.work_mins".to_owned());
        let err = unset_batch_with_locked(
            &path,
            &[
                "reminders.tick_secs".to_owned(),
                "focus.work_mins".to_owned(),
            ],
            &locked,
            None,
        )
        .await
        .expect_err("the pinned key refuses the batch");
        assert!(
            matches!(err, ConfigBatchEditError::EnvLocked { ref key } if key == "focus.work_mins"),
            "the refusal names the pin: {err:?}"
        );
        let text = std::fs::read_to_string(&path);
        assert!(text.is_err(), "nothing wrote");
        std::fs::remove_dir_all(path.parent().expect("dir")).ok();
    }

    /// `us1_s3_duplicate_batch_unset_key_refuses_before_any_write`
    /// (US-1/S3, AC-5): removal is idempotent, but a repeated key is
    /// ambiguity — never resolved silently; the refusal fires before
    /// the document load.
    #[tokio::test]
    async fn us1_s3_duplicate_batch_unset_key_refuses_before_any_write() {
        let path = temp_config("batch-unset-dup");
        let _ = std::fs::remove_file(&path);
        let err = unset(
            Some(path.clone()),
            &[
                "reminders.tick_secs".to_owned(),
                "reminders.tick_secs".to_owned(),
            ],
        )
        .await
        .expect_err("the repeat refuses");
        assert!(
            matches!(err, ConfigBatchEditError::DuplicateKey { ref key } if key == "reminders.tick_secs"),
            "the refusal names the repeat: {err:?}"
        );
        let text = std::fs::read_to_string(&path);
        assert!(text.is_err(), "nothing wrote");
        std::fs::remove_dir_all(path.parent().expect("dir")).ok();
    }

    /// `us1_s5_batch_unset_apply_is_one_update_settings` (US-1/S4–S5,
    /// AC-1/AC-6/AC-7): a multi-key unset on the daemon lane lands one
    /// atomic document and ONE engine apply for the whole batch — the
    /// one lane bit is true, each entry echoes `value: none`, both
    /// overrides leave the file and the published snapshot — and a
    /// batch naming an absent key alongside present keys succeeds
    /// idempotently for the absent one.
    #[tokio::test]
    async fn us1_s5_batch_unset_apply_is_one_update_settings() {
        // Dated pin: the ambient gate, `clean_ambient`'s doc (#1113).
        if !clean_ambient() {
            return;
        }
        let fixture = crate::test_support::standalone_boot_fixture("batch-unset").await;
        let (path, app) = (fixture.config_path.clone(), fixture.app);
        let locked = std::collections::BTreeSet::new();
        set_batch_with_locked(
            &path,
            &[
                "focus.work_mins".to_owned(),
                "50".to_owned(),
                "reminders.tick_secs".to_owned(),
                "45".to_owned(),
            ],
            &locked,
            Some(&app.engine),
        )
        .await
        .expect("the seeding batch lands");
        let view = unset_batch_with_locked(
            &path,
            &[
                "focus.work_mins".to_owned(),
                // Absent alongside present: idempotent success (AC-7).
                "focus.break_mins".to_owned(),
                "reminders.tick_secs".to_owned(),
            ],
            &locked,
            Some(&app.engine),
        )
        .await
        .expect("the batch lands");
        assert!(view.applied_live, "one lane bit for the whole batch");
        let entries: Vec<_> = view
            .entries
            .iter()
            .map(|entry| (entry.key.as_str(), entry.value.as_deref()))
            .collect();
        assert_eq!(
            entries,
            vec![
                ("focus.work_mins", None),
                ("focus.break_mins", None),
                ("reminders.tick_secs", None),
            ],
            "one value:none echo per key, argv order"
        );
        let text = std::fs::read_to_string(&path).expect("the file");
        assert!(
            !text.contains("work_mins") && !text.contains("tick_secs"),
            "both overrides left the file as one document: {text}"
        );
        let published = app.engine.shared_state().load();
        assert_ne!(
            published.settings.focus.as_ref().map(|focus| focus.work()),
            Some(chrono::TimeDelta::minutes(50)),
            "the engine fell back off the removed override"
        );
        assert_ne!(
            published.settings.tick_secs,
            taskboard_domain::TickSecs::new(45).expect("secs > 0"),
            "the engine fell back off the removed tick override"
        );
        app.shutdown().await;
    }

    /// `us2_s2_manual_reload_uses_the_watcher_lane_choreography`
    /// (US-2/S2, AC-1/AC-2/AC-3): the daemon arm calls the watcher
    /// lane's own `reload_once`, so an external edit converges typed
    /// `Applied`, an unchanged file reports `Unchanged` (no engine
    /// round trip), and a mid-save-shaped file (decodes, fails
    /// validate) reports `RefusedValidate` with the engine keeping its
    /// last-known-good snapshot. The outcome travels in the typed
    /// reload view with the resolved path (D-RELOAD-PATH/
    /// D-RELOAD-OUTCOME).
    #[tokio::test]
    async fn us2_s2_manual_reload_uses_the_watcher_lane_choreography() {
        // Dated pin: the ambient gate, `clean_ambient`'s doc (#1113).
        if !clean_ambient() {
            return;
        }
        let fixture = crate::test_support::standalone_boot_fixture("manual-reload").await;
        let (path, app) = (fixture.config_path.clone(), fixture.app);
        let cmd = ConfigCmd::Reload;
        // An external edit converges typed `Applied` through the same
        // choreography — the engine takes the file's truth.
        std::fs::write(&path, "[reminders]\ntick_secs = 55\n").expect("external edit");
        let mut out = Vec::new();
        let mut err = Vec::new();
        let code = handle_daemon(&cmd, &app, Some(&path), &mut daemon_io(&mut out, &mut err)).await;
        assert_eq!(code, 0);
        let view: ConfigReloadView =
            serde_json::from_slice(&out).expect("the typed view travels as JSON");
        assert_eq!(view.path, path, "the view names the reloaded file");
        assert_eq!(
            view.outcome,
            taskboard_domain::ConfigReloadOutcome::Applied,
            "the watcher lane's own Applied outcome"
        );
        assert_eq!(
            app.engine.shared_state().load().settings.tick_secs,
            taskboard_domain::TickSecs::new(55).expect("secs > 0"),
            "the engine converged to the file"
        );

        // Now the published snapshot equals the file: an unchanged
        // reload costs a decode, never an engine round trip (AC-2).
        let mut out = Vec::new();
        let mut err = Vec::new();
        let code = handle_daemon(&cmd, &app, Some(&path), &mut daemon_io(&mut out, &mut err)).await;
        assert_eq!(code, 0);
        let view: ConfigReloadView =
            serde_json::from_slice(&out).expect("the typed view travels as JSON");
        assert_eq!(
            view.outcome,
            taskboard_domain::ConfigReloadOutcome::Unchanged,
            "no-op costs a decode, never an engine round trip"
        );

        // A mid-save-shaped file (decodes, fails validate) refuses
        // typed and the engine keeps its last-known-good snapshot (AC-3,
        // the RefusedValidate class).
        std::fs::write(
            &path,
            "[sync]\nbackoff_initial_secs = 400\nbackoff_max_secs = 100\n",
        )
        .expect("invalid-but-decodable write");
        let mut out = Vec::new();
        let mut err = Vec::new();
        let code = handle_daemon(&cmd, &app, Some(&path), &mut daemon_io(&mut out, &mut err)).await;
        assert_eq!(code, 0);
        let view: ConfigReloadView =
            serde_json::from_slice(&out).expect("the typed view travels as JSON");
        assert_eq!(
            view.outcome,
            taskboard_domain::ConfigReloadOutcome::RefusedValidate,
            "the validate stage refused"
        );
        assert_eq!(
            app.engine.shared_state().load().settings.tick_secs,
            taskboard_domain::TickSecs::new(55).expect("secs > 0"),
            "the last-known-good snapshot stands"
        );
        app.shutdown().await;
    }

    /// `us2_s4_the_published_snapshot_reflects_a_manual_reload`
    /// (US-2/S4, W-9): the display-only leg — after the daemon arm's
    /// `Applied` reload, the engine's published snapshot carries the
    /// file's truth, which is exactly what the Settings editor renders
    /// (no Slint delta, R-1).
    #[tokio::test]
    async fn us2_s4_the_published_snapshot_reflects_a_manual_reload() {
        // Dated pin: the ambient gate, `clean_ambient`'s doc (#1113).
        if !clean_ambient() {
            return;
        }
        let fixture = crate::test_support::standalone_boot_fixture("reload-snapshot").await;
        let (path, app) = (fixture.config_path.clone(), fixture.app);
        std::fs::write(&path, "[reminders]\ntick_secs = 55\n").expect("external edit");
        let cmd = ConfigCmd::Reload;
        let mut out = Vec::new();
        let mut err = Vec::new();
        let code = handle_daemon(&cmd, &app, Some(&path), &mut daemon_io(&mut out, &mut err)).await;
        assert_eq!(code, 0);
        let view: ConfigReloadView =
            serde_json::from_slice(&out).expect("the typed view travels as JSON");
        assert_eq!(
            view.outcome,
            taskboard_domain::ConfigReloadOutcome::Applied,
            "the reload applied"
        );
        assert_eq!(
            app.engine.shared_state().load().settings.tick_secs,
            taskboard_domain::TickSecs::new(55).expect("secs > 0"),
            "the published snapshot the editor renders carries the file's truth"
        );
        app.shutdown().await;
    }
}

#[cfg(test)]
mod config_key_mapping_tests {
    use super::batch_edit;
    use super::{ConfigKey, TypedSetting};
    use crate::config::SinkChoice;

    /// Every variant round-trips through its dotted key — the M2 sweep
    /// (S3 lane, #327) found each `from_dotted` arm deletable with the
    /// full suite green.
    #[test]
    fn every_config_key_round_trips_through_its_dotted_name() {
        let all: [(ConfigKey, &str); 18] = [
            (ConfigKey::DayStart, "preferences.day_start_time"),
            (ConfigKey::RemindersEnabled, "reminders.enabled"),
            (ConfigKey::TickSecs, "reminders.tick_secs"),
            (ConfigKey::ReminderBefore, "reminders.default_before"),
            (ConfigKey::ReminderSink, "reminders.sink"),
            (ConfigKey::PollInterval, "sync.poll_interval_secs"),
            (ConfigKey::BackoffInitial, "sync.backoff_initial_secs"),
            (ConfigKey::BackoffMax, "sync.backoff_max_secs"),
            (ConfigKey::DueSoonDays, "ui.due_soon_days"),
            (ConfigKey::FocusEnabled, "focus.enabled"),
            (ConfigKey::FocusWorkMins, "focus.work_mins"),
            (ConfigKey::FocusBreakMins, "focus.break_mins"),
            (ConfigKey::FocusChain, "focus.chain"),
            (ConfigKey::FocusLongBreakMins, "focus.long_break_mins"),
            (ConfigKey::FocusLongBreakEvery, "focus.long_break_every"),
            (ConfigKey::FocusNotify, "focus.notify"),
            (ConfigKey::FilesSyncEnabled, "files_sync.enabled"),
            (ConfigKey::FilesSyncPollSecs, "files_sync.poll_secs"),
        ];
        for (key, dotted) in all {
            assert_eq!(ConfigKey::from_dotted(dotted), Some(key), "{dotted}");
            assert_eq!(
                key.dotted(),
                dotted.split_once('.').expect("two-part"),
                "{dotted}"
            );
        }
        assert!(ConfigKey::from_dotted("not.a.knob").is_none());
    }

    /// The boolean and sink vocabularies are the parse predicate — the
    /// M2 sweep found the `true`/`false`/`auto`/`log`/`desktop` arms
    /// deletable with the full suite green.
    #[test]
    fn the_bool_and_sink_predicates_accept_their_vocabulary() {
        assert!(matches!(
            ConfigKey::RemindersEnabled.parse_value("true"),
            Ok(TypedSetting::Bool(true))
        ));
        assert!(matches!(
            ConfigKey::RemindersEnabled.parse_value("false"),
            Ok(TypedSetting::Bool(false))
        ));
        assert!(ConfigKey::RemindersEnabled.parse_value("yes").is_err());
        assert!(matches!(
            ConfigKey::ReminderSink.parse_value("auto"),
            Ok(TypedSetting::Sink(SinkChoice::Auto))
        ));
        assert!(matches!(
            ConfigKey::ReminderSink.parse_value("log"),
            Ok(TypedSetting::Sink(SinkChoice::Log))
        ));
        assert!(matches!(
            ConfigKey::ReminderSink.parse_value("desktop"),
            Ok(TypedSetting::Sink(SinkChoice::Desktop))
        ));
        assert!(ConfigKey::ReminderSink.parse_value("somewhere").is_err());
    }

    /// The typed value mints the toml item with its (section, key) —
    /// the M2 sweep found six `batch_edit` arms deletable with the
    /// full suite green.
    #[test]
    fn batch_edit_mints_the_typed_value_as_the_toml_item() {
        let be = batch_edit(ConfigKey::DueSoonDays, &TypedSetting::DueSoonDays(14));
        assert_eq!(be.section, "ui");
        assert_eq!(be.key, "due_soon_days");
        assert_eq!(be.item.to_string(), "14");
        let be = batch_edit(ConfigKey::RemindersEnabled, &TypedSetting::Bool(true));
        assert_eq!(be.item.to_string(), "true");
        let be = batch_edit(ConfigKey::FocusLongBreakEvery, &TypedSetting::FocusEvery(4));
        assert_eq!(be.item.to_string(), "4");
        let be = batch_edit(
            ConfigKey::ReminderBefore,
            &TypedSetting::Before(
                taskboard_domain::ReminderOffset::parse_text_form("1h30m").expect("parses"),
            ),
        );
        assert_eq!(be.item.to_string(), "\"1h30m\"");
        let be = batch_edit(
            ConfigKey::ReminderSink,
            &TypedSetting::Sink(SinkChoice::Log),
        );
        assert_eq!(be.item.to_string(), "\"log\"");
        let be = batch_edit(
            ConfigKey::DayStart,
            &TypedSetting::DayStart(chrono::NaiveTime::from_hms_opt(7, 30, 0).expect("valid time")),
        );
        assert_eq!(be.item.to_string(), "\"07:30\"");
    }
}

#[cfg(test)]
mod config_write_lane_tests {
    use super::{AppConfig, set};

    /// The #984 dated-pin gate (`set_tests::clean_ambient`'s doc).
    fn clean_ambient() -> bool {
        !std::env::vars_os().any(|(key, _)| key.as_encoded_bytes().starts_with(b"TASKBOARD_"))
    }

    fn temp_config(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "taskboard-config-write-{tag}-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir.join("taskboard.toml")
    }

    /// Every knob lands through its own overlay writer — the M2 sweep
    /// (S3 lane, #327) found 13 `write` arms deletable with the full
    /// suite green (only `reminders.tick_secs` was pinned before).
    #[tokio::test]
    async fn every_knob_writes_through_its_overlay_writer() {
        if !clean_ambient() {
            return;
        }
        let knobs: [(&str, &str); 18] = [
            ("preferences.day_start_time", "07:30"),
            ("reminders.enabled", "true"),
            ("reminders.tick_secs", "45"),
            ("reminders.default_before", "10m"),
            ("reminders.sink", "desktop"),
            ("sync.poll_interval_secs", "120"),
            ("sync.backoff_initial_secs", "30"),
            ("sync.backoff_max_secs", "600"),
            ("ui.due_soon_days", "14"),
            ("focus.enabled", "true"),
            ("focus.work_mins", "25"),
            ("focus.break_mins", "5"),
            ("focus.chain", "false"),
            ("focus.long_break_mins", "15"),
            ("focus.long_break_every", "4"),
            ("focus.notify", "false"),
            ("files_sync.enabled", "true"),
            ("files_sync.poll_secs", "60"),
        ];
        let path = temp_config("table");
        let _ = std::fs::remove_file(&path);
        // `files_sync.enabled`'s prove gate refuses when no Nextcloud
        // connector resolves (`FilesSyncWithoutNextcloud`) — seed the
        // identity the backend resolution requires (`sync_backend`
        // reads `nextcloud.server_url`).
        std::fs::write(
            &path,
            "[nextcloud]\nserver_url = \"https://example.test\"\nusername = \"u\"\ncredential_store = \"file\"\n",
        )
        .expect("seed config");
        for (key, value) in knobs {
            set(Some(path.clone()), key, value)
                .await
                .unwrap_or_else(|err| panic!("{key} writes: {err:?}"));
            assert!(
                AppConfig::extract_file_only(Some(path.as_path())).is_ok(),
                "{key} leaves a bootable config"
            );
        }
        std::fs::remove_dir_all(path.parent().expect("dir")).ok();
    }
}
