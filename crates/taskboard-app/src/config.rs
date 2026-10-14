// SPDX-License-Identifier: AGPL-3.0-only
//! Hierarchical application configuration (ADR 0008).
//!
//! Provider chain: built-in defaults → `taskboard.toml` → `TASKBOARD_*`
//! environment (`__` nests: `TASKBOARD_SYNC__POLL_INTERVAL_SECS` →
//! `sync.poll_interval_secs`). The config is **secret-free by
//! construction**: every table rejects unknown keys, so a `token` or
//! `password` key cannot even decode — credentials resolve through
//! [`crate::secrets`] instead.
//!
//! Semantic problems (missing required fields, invalid values) surface
//! as typed [`ConfigError`] variants through [`AppConfig::validate`];
//! provider failures collapse into [`ConfigError::Decode`] whose source
//! is logged, never pattern-matched.

use std::path::{Path, PathBuf};
use std::time::Duration;

use figment::Figment;
use figment::providers::{Env, Format, Serialized, Toml};
use serde::{Deserialize, Serialize};

use taskboard_sync_nextcloud::SyncActorConfig;

/// Default file consulted when neither `--config` nor `TASKBOARD_CONFIG`
/// names one; absent means "defaults only", never an error. Also the
/// write fallback when an existing CWD file is the active config or the
/// platform has no config dir (see [`AppConfig::write_path`]).
pub const DEFAULT_CONFIG_PATH: &str = "./taskboard.toml";

/// The prefix every environment override carries.
pub const ENV_PREFIX: &str = "TASKBOARD_";

/// `[sync] poll_interval_secs`: idle wait between cycles after a success.
pub const DEFAULT_POLL_INTERVAL_SECS: u64 = 30;
/// `[sync] backoff_initial_secs`: first failure backoff.
pub const DEFAULT_BACKOFF_INITIAL_SECS: u64 = 5;
/// `[sync] backoff_max_secs`: backoff saturation.
pub const DEFAULT_BACKOFF_MAX_SECS: u64 = 300;

/// `[app] shutdown_timeout_secs`: graceful-shutdown budget per actor join.
pub const DEFAULT_SHUTDOWN_TIMEOUT_SECS: u64 = 10;

/// `[ui] scale`: the default token multiplier (§6.1 of the GUI Phase 7
/// design; 1.0 = the designed baseline dimensions).
pub const DEFAULT_UI_SCALE: f64 = 1.0;

/// `[ui] scale`'s enforced range bounds (inclusive), per the design's
/// "clamped 0.5–3.0" — enforced, never silently clamped.
pub const UI_SCALE_MIN: f64 = 0.5;
/// See [`UI_SCALE_MIN`].
pub const UI_SCALE_MAX: f64 = 3.0;

/// `[reminders] tick_secs`: engine tick cadence.
pub const DEFAULT_REMINDER_TICK_SECS: u64 = 30;
/// `[reminders] default_before`: the implicit reminder's offset.
pub const DEFAULT_REMINDER_BEFORE: &str = "5m";

/// Fully-resolved, validated application configuration. Secret-free by
/// construction; `Debug` is safe to log.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppConfig {
    /// Frontend selection and daemon knobs.
    #[serde(default)]
    pub app: AppSection,
    /// Nextcloud server and credential-store selection.
    #[serde(default)]
    pub nextcloud: NextcloudSection,
    /// Local database location.
    #[serde(default)]
    pub storage: StorageSection,
    /// Sync actor knobs.
    #[serde(default)]
    pub sync: SyncSection,
    /// Reminder scheduler knobs.
    #[serde(default)]
    pub reminders: RemindersSection,
    /// Presentation-layer knobs (handed to the UI as typed values).
    #[serde(default)]
    pub ui: UiSection,
}

/// Which frontend the process runs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AppMode {
    /// Headless sync daemon (the M1 checkpoint).
    #[default]
    Daemon,
    /// Interactive desktop UI (Phase 7).
    Desktop,
    /// Low-power kiosk display (Phase 7).
    Kiosk,
}

/// `[app]` table.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppSection {
    /// Frontend selection; `daemon` until the UI phases land.
    #[serde(default)]
    pub mode: AppMode,
    /// Graceful-shutdown budget per actor join (ADR 0008 §5); on
    /// expiry the sync actor is aborted and shutdown continues.
    #[serde(default = "default_shutdown_timeout")]
    pub shutdown_timeout_secs: u64,
}

impl Default for AppSection {
    fn default() -> Self {
        Self {
            mode: AppMode::default(),
            shutdown_timeout_secs: default_shutdown_timeout(),
        }
    }
}

fn default_shutdown_timeout() -> u64 {
    DEFAULT_SHUTDOWN_TIMEOUT_SECS
}

/// How the Nextcloud app password is resolved.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CredentialStoreKind {
    /// The OS keyring (production default).
    #[default]
    Keyring,
    /// The `TASKBOARD_APP_PASSWORD` environment escape hatch
    /// (headless/CI; explicit opt-in, never a silent fallback).
    Env,
}

/// `[nextcloud]` table. `server_url` and `username` are required —
/// declared `Option` and checked by [`AppConfig::validate`] so the
/// failure is a typed variant, not a figment string.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NextcloudSection {
    /// Nextcloud root URL (http/https, non-empty host).
    pub server_url: Option<String>,
    /// Nextcloud login name.
    pub username: Option<String>,
    /// Where the app password comes from.
    #[serde(default)]
    pub credential_store: CredentialStoreKind,
}

/// `[storage]` table.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StorageSection {
    /// Database file; default `<data-dir>/taskboard/taskboard.db`
    /// (resolved by [`AppConfig::load`]; `~` is *not* expanded).
    pub db_path: Option<PathBuf>,
}

/// `[sync] backend`: which connector drives sync (ADR 0010).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SyncBackend {
    /// No connector: fully local operation (fresh-install default).
    Standalone,
    /// The Nextcloud Deck connector (first implementation).
    Nextcloud,
}

/// `[sync]` table: plain seconds fields converted at the
/// [`SyncActorConfig`] mapping — no string duration parsing.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SyncSection {
    /// Which connector drives sync; `None` = auto — `nextcloud` iff
    /// `nextcloud.server_url` is present, else `standalone` (ADR 0010).
    #[serde(default)]
    pub backend: Option<SyncBackend>,
    /// Idle wait between cycles after a success.
    #[serde(default = "default_poll_interval")]
    pub poll_interval_secs: u64,
    /// First failure backoff.
    #[serde(default = "default_backoff_initial")]
    pub backoff_initial_secs: u64,
    /// Backoff saturation.
    #[serde(default = "default_backoff_max")]
    pub backoff_max_secs: u64,
}

impl Default for SyncSection {
    fn default() -> Self {
        Self {
            backend: None,
            poll_interval_secs: default_poll_interval(),
            backoff_initial_secs: default_backoff_initial(),
            backoff_max_secs: default_backoff_max(),
        }
    }
}

fn default_poll_interval() -> u64 {
    DEFAULT_POLL_INTERVAL_SECS
}

fn default_backoff_initial() -> u64 {
    DEFAULT_BACKOFF_INITIAL_SECS
}

fn default_backoff_max() -> u64 {
    DEFAULT_BACKOFF_MAX_SECS
}

/// `[reminders]` table: seconds tick plus one string duration parsed in
/// [`AppConfig::validate`] — a bad value must surface as the typed
/// `invalid-duration` kind, not a generic decode failure, and the raw
/// string stays stored so `config print` echoes the user's own form.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemindersSection {
    /// `false` disables the reminder engine's tick entirely.
    #[serde(default = "default_reminders_enabled")]
    pub enabled: bool,
    /// Tick cadence in seconds.
    #[serde(default = "default_reminder_tick")]
    pub tick_secs: u64,
    /// The implicit reminder's offset before due (`30s|5m|1h30m|2d`
    /// grammar; see [`parse_before`]).
    #[serde(default = "default_reminder_before")]
    pub default_before: String,
}

impl Default for RemindersSection {
    fn default() -> Self {
        Self {
            enabled: default_reminders_enabled(),
            tick_secs: default_reminder_tick(),
            default_before: default_reminder_before(),
        }
    }
}

fn default_reminders_enabled() -> bool {
    true
}

fn default_reminder_tick() -> u64 {
    DEFAULT_REMINDER_TICK_SECS
}

fn default_reminder_before() -> String {
    DEFAULT_REMINDER_BEFORE.to_owned()
}

/// `[ui]` table: presentation-layer settings. The UI crate reads no
/// configuration itself (IO-free by design); `taskboard-app` validates
/// here and hands the values over as typed data.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UiSection {
    /// Token multiplier for the theme's dimensions (`0.5–3.0`,
    /// validated — out-of-range is the typed [`ConfigError::InvalidScale`],
    /// never a silent clamp).
    #[serde(default = "default_ui_scale")]
    pub scale: f64,
}

impl Default for UiSection {
    fn default() -> Self {
        Self {
            scale: default_ui_scale(),
        }
    }
}

fn default_ui_scale() -> f64 {
    DEFAULT_UI_SCALE
}

/// The `[reminders] default_before` grammar (one definition site, shared
/// with the CLI `--before` flag): a non-empty sequence of
/// `<unsigned-int><unit>` components, unit ∈ `s | m | h | d`, no sign, no
/// spaces; the total must fit a `TimeDelta` (`"0s"`, `"30s"`, `"5m"`,
/// `"1h30m"`, `"2d"`).
#[must_use]
pub fn parse_before(raw: &str) -> Option<chrono::TimeDelta> {
    if raw.is_empty() {
        return None;
    }
    let bytes = raw.as_bytes();
    let mut total: i64 = 0;
    let mut i = 0;
    while i < bytes.len() {
        let digits_start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i == digits_start {
            return None; // a component without its number
        }
        let value = i64::try_from(raw[digits_start..i].parse::<u64>().ok()?).ok()?;
        let unit_secs = match bytes.get(i)? {
            b's' => 1,
            b'm' => 60,
            b'h' => 3_600,
            b'd' => 86_400,
            _ => return None,
        };
        i += 1;
        total = total.checked_add(value.checked_mul(unit_secs)?)?;
    }
    chrono::TimeDelta::try_seconds(total)
}

/// The XDG read-path candidate, pure over an injected base dir:
/// `<config-dir>/taskboard/taskboard.toml` (`$XDG_CONFIG_HOME/taskboard/…`
/// on Linux, platform-appropriate elsewhere).
#[must_use]
pub fn xdg_candidate(config_dir: Option<&Path>) -> Option<PathBuf> {
    config_dir.map(|dir| dir.join("taskboard").join("taskboard.toml"))
}

/// The XDG candidate *when the file exists* — presence-checked, so an
/// absent XDG file falls through to the CWD default.
fn xdg_if_present(config_dir: Option<&Path>) -> Option<PathBuf> {
    xdg_candidate(config_dir).filter(|path| path.is_file())
}

/// Rows 4–6 of the write-target table (see [`AppConfig::write_path`]):
/// pure over injected dirs, so the decision is unit-testable without
/// env or process-CWD control.
fn first_run_write_target(config_dir: Option<&Path>, cwd_default: &Path) -> PathBuf {
    if cwd_default.is_file() {
        return cwd_default.to_path_buf();
    }
    xdg_candidate(config_dir).unwrap_or_else(|| cwd_default.to_path_buf())
}

/// Configuration failure classes. Nothing here matches provider error
/// *text*; semantic checks are structural.
#[derive(Debug, thiserror::Error)]
// The figment source dominates the `Result` layout; allowed once — the
// error is constructed only at boot.
#[allow(clippy::result_large_err)]
pub enum ConfigError {
    /// An explicitly requested config file (`--config` /
    /// `TASKBOARD_CONFIG`) does not exist.
    #[error("explicit config file not found")]
    FileNotFound(PathBuf),
    /// The providers produced an undecodable tree; the source is logged
    /// by the caller, never matched on.
    #[error("configuration failed to decode")]
    Decode(#[source] figment::Error),
    /// `nextcloud.server_url` is not a usable http(s) URL.
    #[error("server URL invalid")]
    InvalidServerUrl,
    /// `nextcloud.username` is empty.
    #[error("username empty")]
    EmptyUsername,
    /// A `[sync]` duration is zero, or `backoff_initial` exceeds
    /// `backoff_max`.
    #[error("duration invalid")]
    InvalidDuration {
        /// The dotted config key that is invalid.
        field: &'static str,
    },
    /// `[storage] db_path` is empty (or the platform data dir is
    /// unavailable and no path was configured).
    #[error("storage path empty")]
    EmptyDbPath,
    /// `[ui] scale` is NaN, infinite, or outside the enforced
    /// `0.5–3.0` range.
    #[error("ui scale invalid")]
    InvalidScale {
        /// The dotted config key that is invalid.
        field: &'static str,
    },
}

impl ConfigError {
    /// The stable per-variant discriminant — the single vocabulary for
    /// both surfaces that report config errors by kind: `config print`'s
    /// `invalid_kind` payload field and the `--json` error envelope's
    /// `CommandError::Config` kind (typed variant names, never message
    /// text).
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Self::FileNotFound(_) => "file-not-found",
            Self::Decode(_) => "decode",
            Self::InvalidServerUrl => "invalid-server-url",
            Self::EmptyUsername => "empty-username",
            Self::InvalidDuration { .. } => "invalid-duration",
            Self::EmptyDbPath => "empty-db-path",
            Self::InvalidScale { .. } => "invalid-scale",
        }
    }
}

impl AppConfig {
    /// Loads and validates the configuration:
    /// defaults → toml (`--config` path > `TASKBOARD_CONFIG` >
    /// [`DEFAULT_CONFIG_PATH`], an absent default-path file contributes
    /// nothing) → `TASKBOARD_*` env, then fills the default db path and
    /// runs [`AppConfig::validate`].
    ///
    /// # Errors
    ///
    /// [`ConfigError`] — typed, see the variant docs.
    #[allow(clippy::result_large_err)] // the figment source, once at boot
    pub fn load(explicit_path: Option<PathBuf>) -> Result<Self, ConfigError> {
        let path = Self::config_path(explicit_path);
        if let Some(p) = &path
            && !p.is_file()
        {
            return Err(ConfigError::FileNotFound(p.clone()));
        }
        let config = Self::extract_raw(path.as_deref())?;
        config.validate()?;
        Ok(config)
    }

    /// Resolves the config path: the explicit `--config` value, else
    /// `TASKBOARD_CONFIG`, else the XDG candidate *when it exists*, else
    /// `None` (→ [`DEFAULT_CONFIG_PATH`] in the provider chain).
    /// [`AppConfig::load`], the dispatcher, and `config print` share
    /// this single rule.
    #[must_use]
    pub fn config_path(explicit_path: Option<PathBuf>) -> Option<PathBuf> {
        if explicit_path.is_some() {
            return explicit_path;
        }
        if let Some(env) = std::env::var_os("TASKBOARD_CONFIG") {
            return Some(PathBuf::from(env));
        }
        xdg_if_present(dirs::config_dir().as_deref())
    }

    /// The `login` write target: [`AppConfig::config_path`]'s answer when
    /// resolution finds one (explicit `--config` / `TASKBOARD_CONFIG` —
    /// present or not — / an existing XDG file); else — no config
    /// anywhere — the XDG candidate, whose directory the writer creates,
    /// unless a CWD-default file already exists (an existing install
    /// keeps its file). No platform config dir → the CWD default.
    #[must_use]
    pub fn write_path(explicit_path: Option<PathBuf>) -> PathBuf {
        match Self::config_path(explicit_path) {
            Some(path) => path,
            None => first_run_write_target(
                dirs::config_dir().as_deref(),
                Path::new(DEFAULT_CONFIG_PATH),
            ),
        }
    }

    /// Decodes the config *without validation* and fills the default db
    /// path: defaults → toml file at `path` (an absent file contributes
    /// nothing) → `TASKBOARD_*` env. A not-yet-valid config must not
    /// block `login` (the first-run command reads, overlays, then
    /// validates); every other command validates via [`AppConfig::load`].
    ///
    /// # Errors
    ///
    /// [`ConfigError::Decode`] when the providers produce an
    /// undecodable tree.
    #[allow(clippy::result_large_err)] // the figment source, once at boot
    pub fn extract_raw(path: Option<&Path>) -> Result<Self, ConfigError> {
        let mut config: Self = figment(path.map(PathBuf::from))
            .extract()
            .map_err(ConfigError::Decode)?;
        if config.storage.db_path.is_none() {
            config.storage.db_path = default_db_path();
        }
        Ok(config)
    }

    /// The effective sync backend (ADR 0010): the explicit `[sync]
    /// backend` value, else auto — `nextcloud` iff
    /// `nextcloud.server_url` is present (a half-configured file stays
    /// loud), else `standalone`. Every consumer resolves through this
    /// one accessor.
    #[must_use]
    pub fn sync_backend(&self) -> SyncBackend {
        self.sync.backend.unwrap_or_else(|| {
            if self.nextcloud.server_url.is_some() {
                SyncBackend::Nextcloud
            } else {
                SyncBackend::Standalone
            }
        })
    }

    /// Structural validation of the decoded values. `load` runs this
    /// last; hand-built configs (tests, embedders) can call it directly.
    ///
    /// # Errors
    ///
    /// The first failing check, as a typed variant.
    #[allow(clippy::result_large_err)] // the figment source, once at boot
    pub fn validate(&self) -> Result<(), ConfigError> {
        // Same normalization rules as `DeckClient::new`: a *present* URL
        // must survive client construction (typo guard, both backends).
        if let Some(raw) = self.nextcloud.server_url.as_deref()
            && !reqwest_host_ok(raw)
        {
            return Err(ConfigError::InvalidServerUrl);
        }
        // A present username must be non-empty; under the resolved
        // `nextcloud` backend both values are additionally required.
        let username_empty = self
            .nextcloud
            .username
            .as_deref()
            .is_some_and(|u| u.trim().is_empty());
        if username_empty {
            return Err(ConfigError::EmptyUsername);
        }
        if self.sync_backend() == SyncBackend::Nextcloud {
            if self.nextcloud.server_url.is_none() {
                return Err(ConfigError::InvalidServerUrl);
            }
            if self.nextcloud.username.is_none() {
                return Err(ConfigError::EmptyUsername);
            }
        }
        if self.sync.poll_interval_secs == 0 {
            return Err(ConfigError::InvalidDuration {
                field: "sync.poll_interval_secs",
            });
        }
        if self.sync.backoff_initial_secs == 0 {
            return Err(ConfigError::InvalidDuration {
                field: "sync.backoff_initial_secs",
            });
        }
        if self.sync.backoff_max_secs == 0 {
            return Err(ConfigError::InvalidDuration {
                field: "sync.backoff_max_secs",
            });
        }
        if self.sync.backoff_initial_secs > self.sync.backoff_max_secs {
            return Err(ConfigError::InvalidDuration {
                field: "sync.backoff_initial_secs",
            });
        }
        if parse_before(&self.reminders.default_before).is_none() {
            return Err(ConfigError::InvalidDuration {
                field: "reminders.default_before",
            });
        }
        if self.reminders.tick_secs == 0 {
            return Err(ConfigError::InvalidDuration {
                field: "reminders.tick_secs",
            });
        }
        if self
            .storage
            .db_path
            .as_ref()
            .is_none_or(|p| p.as_os_str().is_empty())
        {
            return Err(ConfigError::EmptyDbPath);
        }
        // Finite first: NaN/±inf must fail closed, not slip through
        // range comparisons (NaN compares false both ways).
        if !self.ui.scale.is_finite() || !(UI_SCALE_MIN..=UI_SCALE_MAX).contains(&self.ui.scale) {
            return Err(ConfigError::InvalidScale { field: "ui.scale" });
        }
        Ok(())
    }

    /// The resolved database path. Call after [`AppConfig::load`]
    /// (which fills the default) or after setting it manually.
    ///
    /// # Panics
    ///
    /// Never on a config produced by [`AppConfig::load`]; a hand-built
    /// config without `storage.db_path` panics here.
    #[must_use]
    pub fn db_path(&self) -> &Path {
        self.storage
            .db_path
            .as_deref()
            .expect("db path resolved by load()")
    }

    /// The `[sync]` table mapped onto the sync actor's knobs.
    #[must_use]
    pub fn sync_actor_config(&self) -> SyncActorConfig {
        SyncActorConfig {
            poll_interval: Duration::from_secs(self.sync.poll_interval_secs),
            backoff_initial: Duration::from_secs(self.sync.backoff_initial_secs),
            backoff_max: Duration::from_secs(self.sync.backoff_max_secs),
        }
    }

    /// The `[reminders]` table mapped onto the engine's scheduler knobs.
    /// The config speaks "before"; the domain speaks "added to duedate" —
    /// the negation is the whole mapping. Call after
    /// [`AppConfig::load`]/[`AppConfig::validate`].
    ///
    /// # Panics
    ///
    /// Never on a validated config (an unparseable `default_before` is
    /// rejected by [`AppConfig::validate`]).
    #[must_use]
    pub fn reminder_engine_config(&self) -> taskboard_state::ReminderEngineConfig {
        let parsed =
            parse_before(&self.reminders.default_before).expect("default_before validated");
        let default_before = if parsed.is_zero() {
            taskboard_domain::ReminderOffset::at_due()
        } else {
            taskboard_domain::ReminderOffset::before(-parsed)
                .expect("a positive before-duration negates to the domain's sign domain")
        };
        taskboard_state::ReminderEngineConfig {
            enabled: self.reminders.enabled,
            tick: Duration::from_secs(self.reminders.tick_secs),
            defaults: taskboard_domain::ReminderDefaults { default_before },
        }
    }

    /// The validated server URL. Call after [`AppConfig::validate`].
    ///
    /// # Panics
    ///
    /// Never on a validated config.
    #[must_use]
    pub fn server_url(&self) -> &str {
        self.nextcloud
            .server_url
            .as_deref()
            .expect("server_url validated")
    }

    /// The validated username. Call after [`AppConfig::validate`].
    ///
    /// # Panics
    ///
    /// Never on a validated config.
    #[must_use]
    pub fn username(&self) -> &str {
        self.nextcloud
            .username
            .as_deref()
            .expect("username validated")
    }
}

/// The provider chain (binding, plan §4.1).
fn figment(path: Option<PathBuf>) -> Figment {
    let path = path.unwrap_or_else(|| PathBuf::from(DEFAULT_CONFIG_PATH));
    Figment::from(Serialized::defaults(AppConfig::default()))
        .merge(Toml::file(path)) // absent file contributes nothing
        .merge(Env::prefixed(ENV_PREFIX).split("__"))
}

/// `<data-dir>/taskboard/taskboard.db`, or `None` when the platform has
/// no data dir (documented: `~` is not expanded).
fn default_db_path() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("taskboard").join("taskboard.db"))
}

/// The host-presence half of `DeckClient::new`'s URL rules, without
/// constructing a client: scheme http/https and a non-empty host. The
/// sync crate parses the same string again at client construction, so a
/// drift here fails closed (bootstrap's `Client` variant).
fn reqwest_host_ok(raw: &str) -> bool {
    // Minimal structural parse (no extra dependency): scheme://host/…
    let Some((scheme, rest)) = raw.split_once("://") else {
        return false;
    };
    if !matches!(scheme, "http" | "https") {
        return false;
    }
    let authority = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    let host = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    !host.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid() -> AppConfig {
        AppConfig {
            nextcloud: NextcloudSection {
                server_url: Some("https://cloud.example.com".into()),
                username: Some("alice".into()),
                credential_store: CredentialStoreKind::Keyring,
            },
            storage: StorageSection {
                db_path: Some(PathBuf::from("/tmp/taskboard.db")),
            },
            ..AppConfig::default()
        }
    }

    #[test]
    fn validate_accepts_a_complete_config() {
        valid().validate().expect("valid config validates");
    }

    #[test]
    fn validate_rejects_empty_username() {
        let mut c = valid();
        c.nextcloud.username = Some("   ".into());
        assert!(matches!(c.validate(), Err(ConfigError::EmptyUsername)));
    }

    #[test]
    fn validate_rejects_bad_urls_like_the_deck_client() {
        for raw in ["cloud.example.com", "ftp://x", "https://"] {
            let mut c = valid();
            c.nextcloud.server_url = Some(raw.into());
            assert!(
                matches!(c.validate(), Err(ConfigError::InvalidServerUrl)),
                "{raw} must be rejected"
            );
        }
    }

    #[test]
    fn validate_rejects_invalid_durations() {
        let mut c = valid();
        c.sync.poll_interval_secs = 0;
        assert!(matches!(
            c.validate(),
            Err(ConfigError::InvalidDuration {
                field: "sync.poll_interval_secs"
            })
        ));
        let mut c = valid();
        c.sync.backoff_initial_secs = 400;
        assert!(matches!(
            c.validate(),
            Err(ConfigError::InvalidDuration { .. })
        ));
    }

    #[test]
    fn validate_rejects_empty_db_path() {
        let mut c = valid();
        c.storage.db_path = Some(PathBuf::new());
        assert!(matches!(c.validate(), Err(ConfigError::EmptyDbPath)));
    }

    #[test]
    fn ui_scale_defaults_to_one_and_round_trips() {
        let round: AppConfig = Figment::from(Serialized::defaults(AppConfig::default()))
            .extract()
            .expect("defaults extract");
        assert!((round.ui.scale - DEFAULT_UI_SCALE).abs() < f64::EPSILON);
    }

    #[test]
    fn ui_scale_accepts_the_range_boundaries() {
        let mut c = valid();
        c.ui.scale = UI_SCALE_MIN;
        c.validate().expect("0.5 validates");
        let mut c = valid();
        c.ui.scale = UI_SCALE_MAX;
        c.validate().expect("3.0 validates");
    }

    #[test]
    fn ui_scale_rejects_out_of_range_and_non_finite() {
        for scale in [0.49, 3.01, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let mut c = valid();
            c.ui.scale = scale;
            assert!(
                matches!(
                    c.validate(),
                    Err(ConfigError::InvalidScale { field: "ui.scale" })
                ),
                "{scale} must be rejected as InvalidScale"
            );
        }
    }

    #[test]
    fn ui_scale_error_kind_is_the_stable_discriminant() {
        assert_eq!(
            ConfigError::InvalidScale { field: "ui.scale" }.kind(),
            "invalid-scale"
        );
    }

    #[test]
    fn unknown_key_in_the_ui_table_is_a_decode_error() {
        let result: Result<AppConfig, _> =
            Figment::from(Serialized::defaults(AppConfig::default()))
                .merge(Toml::string("[ui]\ntokens = 'x'"))
                .extract();
        assert!(result.is_err());
    }

    #[test]
    fn sync_actor_config_maps_the_seconds_fields() {
        let cfg = valid().sync_actor_config();
        assert_eq!(cfg.poll_interval, Duration::from_secs(30));
        assert_eq!(cfg.backoff_initial, Duration::from_secs(5));
        assert_eq!(cfg.backoff_max, Duration::from_secs(300));
    }

    #[test]
    fn serde_round_trip_preserves_defaults() {
        let original = AppConfig::default();
        let round: AppConfig = Figment::from(Serialized::from(&original, "round-trip"))
            .extract()
            .expect("serialize defaults and extract them back");
        assert_eq!(round.app.mode, AppMode::Daemon);
        assert_eq!(round.sync.poll_interval_secs, DEFAULT_POLL_INTERVAL_SECS);
        assert_eq!(
            round.nextcloud.credential_store,
            CredentialStoreKind::Keyring
        );
        assert!(round.reminders.enabled);
        assert_eq!(round.reminders.tick_secs, DEFAULT_REMINDER_TICK_SECS);
        assert_eq!(round.reminders.default_before, DEFAULT_REMINDER_BEFORE);
    }

    #[test]
    fn validate_rejects_bad_reminder_values() {
        let mut c = valid();
        c.reminders.default_before = "5min".into();
        assert!(matches!(
            c.validate(),
            Err(ConfigError::InvalidDuration {
                field: "reminders.default_before"
            })
        ));
        let mut c = valid();
        c.reminders.tick_secs = 0;
        assert!(matches!(
            c.validate(),
            Err(ConfigError::InvalidDuration {
                field: "reminders.tick_secs"
            })
        ));
    }

    #[test]
    fn reminder_engine_config_maps_the_table_with_the_sign_flip() {
        let mut c = valid();
        let engine = c.reminder_engine_config();
        assert!(engine.enabled);
        assert_eq!(engine.tick, Duration::from_secs(DEFAULT_REMINDER_TICK_SECS));
        assert_eq!(
            engine.defaults.default_before.as_duration().num_seconds(),
            -300,
            "the '5m' default speaks before (negative in domain terms)"
        );

        c.reminders.default_before = "0s".into();
        assert_eq!(
            c.reminder_engine_config().defaults.default_before,
            taskboard_domain::ReminderOffset::at_due()
        );

        c.reminders.enabled = false;
        c.reminders.tick_secs = 7;
        let engine = c.reminder_engine_config();
        assert!(!engine.enabled);
        assert_eq!(engine.tick, Duration::from_secs(7));
    }

    #[test]
    fn unknown_key_in_the_reminders_table_is_a_decode_error() {
        let result: Result<AppConfig, _> =
            Figment::from(Serialized::defaults(AppConfig::default()))
                .merge(Toml::string("[reminders]\ntoken = 'x'"))
                .extract();
        assert!(result.is_err());
    }

    #[test]
    fn parse_before_accepts_the_documented_grammar() {
        for (raw, secs) in [
            ("0s", 0),
            ("30s", 30),
            ("5m", 300),
            ("1h30m", 5_400),
            ("2d", 172_800),
            ("1d2h3m4s", 93_784),
            // Fits i64 seconds and a `TimeDelta`, so the grammar accepts
            // it (the arithmetic saturates downstream, at chrono's edges).
            ("999999999d", 86_399_999_913_600),
        ] {
            assert_eq!(
                parse_before(raw).map(|d| d.num_seconds()),
                Some(secs),
                "{raw} must parse"
            );
        }
    }

    #[test]
    fn parse_before_rejects_garbage_and_overflow() {
        for raw in ["", "-5m", "5min", "5 m", "m5", "1.5h", "5mX", "5", "m"] {
            assert!(parse_before(raw).is_none(), "{raw} must be rejected");
        }
        // Genuine i64 overflow across components.
        assert!(parse_before("9223372036854775807d9223372036854775807s").is_none());
    }

    #[test]
    fn kebab_case_enums_decode_from_toml_values() {
        let mode: AppSection = Figment::from(Serialized::defaults(AppSection::default()))
            .merge(Toml::string("mode = 'kiosk'"))
            .extract()
            .expect("kebab-case mode decodes");
        assert_eq!(mode.mode, AppMode::Kiosk);

        let store: NextcloudSection =
            Figment::from(Serialized::defaults(NextcloudSection::default()))
                .merge(Toml::string("credential_store = 'env'"))
                .extract()
                .expect("kebab-case credential store decodes");
        assert_eq!(store.credential_store, CredentialStoreKind::Env);
    }

    #[test]
    fn xdg_candidate_builds_the_platform_shape() {
        let dir = Path::new("/home/u/.config");
        assert_eq!(
            xdg_candidate(Some(dir)),
            Some(PathBuf::from("/home/u/.config/taskboard/taskboard.toml"))
        );
        assert_eq!(xdg_candidate(None), None);
    }

    #[test]
    fn xdg_if_present_requires_the_file_to_exist() {
        let dir = tempfile::tempdir().expect("tempdir");
        let candidate = dir.path().join("taskboard").join("taskboard.toml");
        assert_eq!(
            xdg_if_present(Some(dir.path())),
            None,
            "absent XDG falls through"
        );
        std::fs::create_dir_all(candidate.parent().expect("parent")).expect("mkdir");
        std::fs::write(
            &candidate, "[sync]
",
        )
        .expect("write");
        assert_eq!(xdg_if_present(Some(dir.path())), Some(candidate));
    }

    #[test]
    fn first_run_write_target_keeps_an_existing_cwd_default() {
        let cwd = tempfile::tempdir().expect("tempdir");
        let xdg = tempfile::tempdir().expect("tempdir");
        let cwd_default = cwd.path().join("taskboard.toml");
        std::fs::write(&cwd_default, "").expect("the existing CWD install");
        assert_eq!(
            first_run_write_target(Some(xdg.path()), &cwd_default),
            cwd_default,
            "an existing CWD config stays the active file (row 4)"
        );
    }

    #[test]
    fn first_run_write_target_writes_the_xdg_candidate_when_nothing_exists() {
        let xdg = tempfile::tempdir().expect("tempdir");
        let cwd = tempfile::tempdir().expect("tempdir");
        let cwd_default = cwd.path().join("taskboard.toml");
        let target = first_run_write_target(Some(xdg.path()), &cwd_default);
        assert_eq!(
            target,
            xdg.path().join("taskboard").join("taskboard.toml"),
            "fresh machine: the XDG candidate (row 5)"
        );
        assert!(
            !target.parent().expect("parent").exists(),
            "the writer creates the directory, none pre-exists"
        );
    }

    #[test]
    fn first_run_write_target_falls_back_to_cwd_without_a_config_dir() {
        let cwd = tempfile::tempdir().expect("tempdir");
        let cwd_default = cwd.path().join("taskboard.toml");
        assert_eq!(
            first_run_write_target(None, &cwd_default),
            cwd_default,
            "no platform config dir: today's CWD fallback (row 6)"
        );
    }

    #[test]
    fn config_error_kinds_are_stable_discriminants() {
        for (err, kind) in [
            (ConfigError::InvalidServerUrl, "invalid-server-url"),
            (ConfigError::EmptyUsername, "empty-username"),
            (
                ConfigError::InvalidDuration {
                    field: "sync.poll_interval_secs",
                },
                "invalid-duration",
            ),
            (ConfigError::EmptyDbPath, "empty-db-path"),
        ] {
            assert_eq!(err.kind(), kind);
        }
        // The provider-backed variants need real errors; assert via a
        // decode failure and an explicit file-not-found.
        let decode = Figment::from(Serialized::defaults(AppConfig::default()))
            .merge(Toml::string("not toml"))
            .extract::<AppConfig>()
            .expect_err("garbage");
        assert_eq!(ConfigError::Decode(decode).kind(), "decode");
        assert_eq!(
            ConfigError::FileNotFound(PathBuf::from("/x")).kind(),
            "file-not-found"
        );
    }

    /// The normative secret-in-TOML case: a `token` key inside a table
    /// cannot even decode (mechanical enforcement, plan C4's unit half).
    #[test]
    fn unknown_key_in_a_table_is_a_decode_error() {
        let result: Result<AppConfig, _> =
            Figment::from(Serialized::defaults(AppConfig::default()))
                .merge(Toml::string("[nextcloud]\ntoken = 'leak'"))
                .extract();
        assert!(result.is_err());
    }
}
