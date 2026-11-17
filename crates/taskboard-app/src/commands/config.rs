// SPDX-License-Identifier: AGPL-3.0-only
//! `taskboard config print` (ADR 0009): a decode-only diagnostic that
//! *always* prints — validity is reported in the payload, not via the
//! exit code. Pre-core (no actor graph, no database, no lock).

use taskboard_domain::ReminderOffset;

use crate::cli::ConfigCmd;
use crate::commands::output::{ConfigPathView, ConfigSetView, ConfigView};
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
            Self::TickSecs | Self::PollInterval | Self::BackoffInitial | Self::BackoffMax => {
                trimmed
                    .parse::<i64>()
                    .ok()
                    .filter(|secs| *secs > 0)
                    .and_then(|secs| u64::try_from(secs).ok())
                    .map(TypedSetting::Secs)
                    .ok_or_else(bad)
            }
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
async fn set_with_locked(
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

/// `config unset <key>` (T-18, #801; propagation #845): removes the
/// toml layer's override through the prove-gate removal writer
/// (journey W-4) so the default (or env layer) applies again — the
/// same typed lanes as [`set`].
///
/// # Errors
///
/// [`ConfigSetError`] — the typed refusal classes.
pub async fn unset(
    config_path: Option<std::path::PathBuf>,
    key: &str,
) -> Result<ConfigSetView, ConfigSetError> {
    let write_path = AppConfig::write_path(config_path);
    let locked = AppConfig::env_overridden_keys(Some(write_path.as_path()))
        .map_err(|err| ConfigSetError::Io(std::io::Error::other(err.to_string())))?;
    unset_with_locked(&write_path, key, &locked, None).await
}

/// The lock-set seam for unset (the [`set_with_locked`] twin): lane
/// seam + env seam over the shared port and the removal writer.
async fn unset_with_locked(
    write_path: &std::path::Path,
    key: &str,
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
    let (section, field) = parsed.dotted();
    let outcome = run_settings_write(
        write_path,
        locked,
        &[key],
        |path| crate::settings_overlay::remove_key(path, section, field),
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
        value: None,
        applied_live: outcome.apply == LiveApply::Applied,
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
        ConfigCmd::Set { ref key, ref value } => {
            match set(ctx.config_path.clone(), key, value).await {
                Ok(view) => {
                    emit_to(&mut ctx.io, &view, &render_set_text(&view));
                    0
                }
                Err(err) => report_to(&mut ctx.io, &CommandError::ConfigSet(err)),
            }
        }
        ConfigCmd::Unset { ref key } => match unset(ctx.config_path.clone(), key).await {
            Ok(view) => {
                emit_to(&mut ctx.io, &view, &render_set_text(&view));
                0
            }
            Err(err) => report_to(&mut ctx.io, &CommandError::ConfigSet(err)),
        },
    }
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
    let locked = AppConfig::env_overridden_keys(Some(write_path.as_path()))
        .map_err(|err| ConfigSetError::Io(std::io::Error::other(err.to_string())));
    let locked = match locked {
        Ok(locked) => locked,
        Err(err) => return report_to(io, &CommandError::ConfigSet(err)),
    };
    let result = match *cmd {
        ConfigCmd::Set { ref key, ref value } => {
            set_with_locked(&write_path, key, value, &locked, Some(&app.engine)).await
        }
        ConfigCmd::Unset { ref key } => {
            unset_with_locked(&write_path, key, &locked, Some(&app.engine)).await
        }
        // Print/Path stay `IpcRoute::Local` (journey R-2); only
        // Set/Unset take the relay lane, so nothing else arrives here.
        ConfigCmd::Print | ConfigCmd::Path => {
            unreachable!("the relay lane carries only config set/unset (R-2)")
        }
    };
    match result {
        Ok(view) => {
            emit_to(io, &view, &render_set_text(&view));
            0
        }
        Err(err) => report_to(io, &CommandError::ConfigSet(err)),
    }
}

#[cfg(test)]
mod set_tests {
    use super::*;
    use std::sync::Arc;

    use crate::config::DEFAULT_FOCUS_WORK_MINS;
    use taskboard_domain::{StateCommand, SystemEvent};

    fn temp_config(tag: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("taskboard-config-set-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir.join("taskboard.toml")
    }

    /// A booted standalone core for the daemon-lane journeys (the
    /// `ui_requests` harness shape — fakes over mocks; a real engine).
    async fn daemon_fixture(tag: &str) -> (std::path::PathBuf, crate::App) {
        let dir = tempfile::tempdir().expect("tempdir");
        let config_path = dir.path().join(format!("taskboard-{tag}.toml"));
        std::fs::write(&config_path, "[reminders]\ntick_secs = 30\n").expect("seed config");
        let mut config = AppConfig::default();
        config.storage.db_path = Some(dir.path().join("taskboard.db"));
        config.sync.backend = Some(SyncBackend::Standalone);
        config.validate().expect("config valid");
        let app = crate::bootstrap::bootstrap(
            config,
            Arc::new(RefusingStore),
            Arc::new(crate::notifications::LogSink),
        )
        .await
        .expect("standalone boot");
        // The tempdir must outlive the assertion window: leak the
        // guard into the static lifetime of the test (the paths are
        // taken out first).
        let path = config_path.clone();
        std::mem::forget(dir);
        let _ = tag;
        (path, app)
    }

    /// A credential-refusing store (the bootstrap test's shape): the
    /// config arm never consults it.
    struct RefusingStore;
    impl std::fmt::Debug for RefusingStore {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("RefusingStore")
        }
    }
    impl crate::secrets::CredentialStore for RefusingStore {
        fn load(
            &self,
            _: &str,
            _: &str,
        ) -> Result<crate::secrets::AppPassword, crate::secrets::CredentialError> {
            Err(crate::secrets::CredentialError::NotFound)
        }
        fn store(
            &self,
            _: &str,
            _: &str,
            _: crate::secrets::AppPassword,
        ) -> Result<(), crate::secrets::CredentialError> {
            Err(crate::secrets::CredentialError::Unsupported)
        }
        fn delete(&self, _: &str, _: &str) -> Result<(), crate::secrets::CredentialError> {
            Err(crate::secrets::CredentialError::Unsupported)
        }
    }

    /// D-5's happy arm, now the typed next-start lane (US-2/S3,
    /// `applied_live: false` is a success): a known key writes the
    /// toml layer through the prove-gate overlay writer, and a
    /// re-read through the figment stack sees the value.
    #[tokio::test]
    async fn us2_s3_config_set_writes_the_toml_layer_and_reports_next_start() {
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
        let path = temp_config("unset");
        set(Some(path.clone()), "focus.work_mins", "50")
            .await
            .expect("set");
        let view = unset(Some(path.clone()), "focus.work_mins")
            .await
            .expect("unset");
        assert_eq!(view.key, "focus.work_mins");
        assert_eq!(view.value, None);
        assert!(!view.applied_live, "the local lane");
        let file_only =
            AppConfig::extract_file_only(Some(path.as_path())).expect("the file decodes");
        assert_eq!(
            file_only.focus.work_mins, DEFAULT_FOCUS_WORK_MINS,
            "the override is gone; the default applies"
        );
        std::fs::remove_dir_all(path.parent().expect("dir")).ok();
    }

    /// US-5 AC-2: `config print` stays truthful after a set — the
    /// printed effective value equals the written one (the file is
    /// written before any engine apply).
    #[tokio::test]
    async fn us5_s2_config_print_stays_truthful_after_a_set() {
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

    /// `us1_ac1`: a set against a running daemon's engine applies
    /// live — the engine publishes the new value AND the file layer
    /// carries it — and the typed view reports `applied_live: true`.
    #[tokio::test]
    async fn us1_ac1_the_daemon_lane_applies_live_and_writes_the_file() {
        let (path, app) = daemon_fixture("ac1").await;
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

    /// `us1_ac2`: the prove gate precedes the engine — a
    /// validity-crossing write refuses typed, the file is untouched,
    /// and the engine's published settings are unchanged (nothing was
    /// sent).
    #[tokio::test]
    async fn us1_ac2_the_daemon_lane_proves_before_the_engine() {
        let (path, app) = daemon_fixture("ac2").await;
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

    /// `us3_ac1`: unset propagates live on the daemon lane — the
    /// engine publishes the default-derived snapshot and the file no
    /// longer carries the key.
    #[tokio::test]
    async fn us3_ac1_the_daemon_lane_unset_propagates_live() {
        let (path, app) = daemon_fixture("ac3").await;
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
        let view = unset_with_locked(&path, "focus.work_mins", &locked, Some(&app.engine))
            .await
            .expect("unset lands");
        assert!(view.applied_live, "the live lane");
        assert_eq!(view.value, None);
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

    /// `us4_s1` on the daemon lane: an env-pinned key refuses typed
    /// before the write and before any engine command (the guard runs
    /// in the daemon's own env truth — D8's asymmetry).
    #[tokio::test]
    async fn us4_s1_the_daemon_lane_env_guard_refuses_typed() {
        let (path, app) = daemon_fixture("ac4").await;
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

    /// D7 on the daemon lane: an engine gone mid-flight is the typed
    /// next-start outcome, NOT an error — the file is written (the
    /// boot authority) and `applied_live: false`.
    #[tokio::test]
    async fn us1_s5_an_engine_gone_write_reports_the_next_start_lane() {
        let (path, app) = daemon_fixture("gone").await;
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
}
