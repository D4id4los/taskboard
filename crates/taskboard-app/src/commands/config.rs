// SPDX-License-Identifier: AGPL-3.0-only
//! `taskboard config print` (ADR 0009): a decode-only diagnostic that
//! *always* prints — validity is reported in the payload, not via the
//! exit code. Pre-core (no actor graph, no database, no lock).

use crate::cli::ConfigCmd;
use crate::commands::output::{ConfigPathView, ConfigView};
use crate::commands::{CommandCtx, CommandError, emit_to, report_to};
use crate::config::{AppConfig, ConfigError, SyncBackend};

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

    /// The key's value predicate: `Ok` yields the toml value verbatim
    /// (the toml layer stores the parsed form — the figment decode is
    /// the authoritative re-validation at next start).
    fn parse_value(self, value: &str) -> Result<toml_edit::Value, ConfigSetError> {
        let bad = || ConfigSetError::BadValue {
            key: self.dotted().0.to_owned() + "." + self.dotted().1,
        };
        let trimmed = value.trim();
        match self {
            Self::DayStart => trimmed
                .parse::<chrono::NaiveTime>()
                .map(|time| toml_edit::Value::from(time.to_string()))
                .map_err(|_| bad()),
            Self::TickSecs | Self::PollInterval | Self::BackoffInitial | Self::BackoffMax => {
                trimmed
                    .parse::<i64>()
                    .ok()
                    .filter(|secs| *secs > 0)
                    .map(toml_edit::Value::from)
                    .ok_or_else(bad)
            }
            Self::ReminderBefore => {
                if crate::config::parse_before(trimmed).is_some() {
                    Ok(toml_edit::Value::from(trimmed.to_owned()))
                } else {
                    Err(bad())
                }
            }
            Self::DueSoonDays => trimmed
                .parse::<i64>()
                .ok()
                .filter(|days| (0..=3652).contains(days))
                .map(toml_edit::Value::from)
                .ok_or_else(bad),
            Self::FocusWorkMins | Self::FocusBreakMins | Self::FocusLongBreakMins => trimmed
                .parse::<i64>()
                .ok()
                .filter(|mins| (1..=240).contains(mins))
                .map(toml_edit::Value::from)
                .ok_or_else(bad),
            Self::FocusLongBreakEvery => trimmed
                .parse::<i64>()
                .ok()
                .filter(|every| (1..=24).contains(every))
                .map(toml_edit::Value::from)
                .ok_or_else(bad),
            Self::RemindersEnabled
            | Self::FocusEnabled
            | Self::FocusChain
            | Self::FocusNotify
            | Self::FilesSyncEnabled => match trimmed {
                "true" => Ok(toml_edit::Value::from(true)),
                "false" => Ok(toml_edit::Value::from(false)),
                _ => Err(bad()),
            },
            Self::ReminderSink => match trimmed {
                "auto" | "log" | "desktop" => Ok(toml_edit::Value::from(trimmed.to_owned())),
                _ => Err(bad()),
            },
        }
    }
}

/// `config set <key> <value>` (T-18, #801): the file-layer write,
/// figment-respecting — env-locked keys refuse typed, unknown keys
/// and malformed values refuse typed, and a valid pair lands in the
/// toml layer through the atomic overlay writers.
///
/// # Errors
///
/// [`ConfigSetError`] — the typed refusal classes.
pub fn set(
    config_path: Option<std::path::PathBuf>,
    key: &str,
    value: &str,
) -> Result<(), ConfigSetError> {
    let write_path = AppConfig::write_path(config_path);
    let locked = AppConfig::env_overridden_keys(Some(write_path.as_path()))
        .map_err(|err| ConfigSetError::Io(std::io::Error::other(err.to_string())))?;
    set_with_locked(&write_path, key, value, &locked)
}

/// The lock-set seam (T-18): `set`'s body over a caller-supplied
/// locked set — the env truth-table's composition point (the
/// `env_overridden_keys` truth table owns the env-diff itself; tests
/// compose without touching process env).
fn set_with_locked(
    write_path: &std::path::Path,
    key: &str,
    value: &str,
    locked: &std::collections::BTreeSet<String>,
) -> Result<(), ConfigSetError> {
    let parsed = ConfigKey::from_dotted(key).ok_or_else(|| ConfigSetError::UnknownKey {
        key: key.to_owned(),
    })?;
    if locked.contains(key) {
        return Err(ConfigSetError::EnvLocked {
            key: key.to_owned(),
        });
    }
    let parsed_value = parsed.parse_value(value)?;
    write_dotted(write_path, parsed, Some(parsed_value))
}

/// `config unset <key>` (T-18, #801): removes the toml layer's
/// override so the default (or env layer) applies again.
///
/// # Errors
///
/// [`ConfigSetError`] — the typed refusal classes.
pub fn unset(config_path: Option<std::path::PathBuf>, key: &str) -> Result<(), ConfigSetError> {
    let parsed = ConfigKey::from_dotted(key).ok_or_else(|| ConfigSetError::UnknownKey {
        key: key.to_owned(),
    })?;
    let write_path = AppConfig::write_path(config_path);
    let locked = AppConfig::env_overridden_keys(Some(write_path.as_path()))
        .map_err(|err| ConfigSetError::Io(std::io::Error::other(err.to_string())))?;
    if locked.contains(key) {
        return Err(ConfigSetError::EnvLocked {
            key: key.to_owned(),
        });
    }
    write_dotted(&write_path, parsed, None)
}

/// The shared toml-layer writer: preserves every unrelated table by
/// construction (`toml_edit`), writes once, atomically.
fn write_dotted(
    path: &std::path::Path,
    key: ConfigKey,
    value: Option<toml_edit::Value>,
) -> Result<(), ConfigSetError> {
    let (section, field) = key.dotted();
    let mut doc = crate::config::overlay_document(path)?;
    let table = doc
        .entry(section)
        .or_insert(toml_edit::Item::Table(toml_edit::Table::new()))
        .as_table_mut()
        .ok_or_else(|| {
            ConfigSetError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("[{section}] is not a table"),
            ))
        })?;
    match value {
        Some(value) => {
            table.insert(field, toml_edit::Item::Value(value));
        }
        None => {
            table.remove(field);
        }
    }
    crate::config::write_config_atomic(path, doc.to_string().as_str())?;
    Ok(())
}

/// Dispatch glue so `dispatch` stays a flat table.
pub(crate) fn handle(cmd: &ConfigCmd, ctx: &mut CommandCtx<'_>) -> u8 {
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
        ConfigCmd::Set { ref key, ref value } => match set(ctx.config_path.clone(), key, value) {
            Ok(()) => {
                let _ = writeln!(
                    ctx.io.out,
                    "{key} set in the toml layer; it applies at the next start"
                );
                0
            }
            Err(err) => report_to(&mut ctx.io, &CommandError::ConfigSet(err)),
        },
        ConfigCmd::Unset { ref key } => match unset(ctx.config_path.clone(), key) {
            Ok(()) => {
                let _ = writeln!(
                    ctx.io.out,
                    "{key} removed from the toml layer; it applies at the next start"
                );
                0
            }
            Err(err) => report_to(&mut ctx.io, &CommandError::ConfigSet(err)),
        },
    }
}

#[cfg(test)]
mod set_tests {
    use super::*;
    use crate::config::DEFAULT_FOCUS_WORK_MINS;

    fn temp_config(tag: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("taskboard-config-set-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir.join("taskboard.toml")
    }

    /// D-5's happy arm: a known key writes the toml layer — the file
    /// carries the section, and a re-read through the figment stack
    /// (`extract_file_only`) sees the value.
    #[test]
    fn a_known_key_writes_the_toml_layer_and_rereads() {
        let path = temp_config("happy");
        let _ = std::fs::remove_file(&path);
        set(Some(path.clone()), "reminders.tick_secs", "45").expect("the write succeeds");
        let text = std::fs::read_to_string(&path).expect("the file exists");
        assert!(
            text.contains("tick_secs = 45"),
            "the toml layer carries the override: {text}"
        );
        let file_only =
            AppConfig::extract_file_only(Some(path.as_path())).expect("the file decodes");
        assert_eq!(file_only.reminders.tick_secs, 45, "figment re-read");
        assert!(
            AppConfig::load(Some(path.clone())).is_ok(),
            "the written config is a valid config"
        );
        std::fs::remove_dir_all(path.parent().expect("dir")).ok();
    }

    /// D-5's env arm: an env-overridden key refuses typed
    /// (`EnvLocked`) and writes nothing (composed over the lock-set
    /// seam — the env-diff truth table owns the env half).
    #[test]
    fn an_env_overridden_key_refuses_typed_without_writing() {
        let path = temp_config("envlocked");
        std::fs::write(&path, "[reminders]\ntick_secs = 30\n").expect("seed config");
        let mut locked = std::collections::BTreeSet::new();
        locked.insert("reminders.tick_secs".to_owned());
        let err = set_with_locked(&path, "reminders.tick_secs", "45", &locked)
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
    #[test]
    fn unknown_keys_and_malformed_values_refuse_typed_without_writing() {
        let path = temp_config("refusals");
        let _ = std::fs::remove_file(&path);
        let err = set(Some(path.clone()), "not.a.knob", "1").expect_err("the unknown key refuses");
        assert!(matches!(err, ConfigSetError::UnknownKey { .. }));
        let err =
            set(Some(path.clone()), "reminders.tick_secs", "0").expect_err("the zero tick refuses");
        assert!(
            matches!(err, ConfigSetError::BadValue { .. }),
            "the malformed value refuses: {err:?}"
        );
        let err = set(Some(path.clone()), "reminders.sink", "loud")
            .expect_err("the unknown sink refuses");
        assert!(matches!(err, ConfigSetError::BadValue { .. }));
        assert!(!path.exists(), "a refusal never creates the file");
        std::fs::remove_dir_all(path.parent().expect("dir")).ok();
    }

    /// D-5's unset arm: `config unset` removes the override so the
    /// default applies again (the figment re-read sees no override).
    #[test]
    fn unset_removes_the_key() {
        let path = temp_config("unset");
        set(Some(path.clone()), "focus.work_mins", "50").expect("set");
        unset(Some(path.clone()), "focus.work_mins").expect("unset");
        let file_only =
            AppConfig::extract_file_only(Some(path.as_path())).expect("the file decodes");
        assert_eq!(
            file_only.focus.work_mins, DEFAULT_FOCUS_WORK_MINS,
            "the override is gone; the default applies"
        );
        std::fs::remove_dir_all(path.parent().expect("dir")).ok();
    }
}
