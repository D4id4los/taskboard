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
/// names one; absent means "defaults only", never an error.
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

/// `[sync]` table: plain seconds fields converted at the
/// [`SyncActorConfig`] mapping — no string duration parsing.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SyncSection {
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

/// Configuration failure classes. Nothing here matches provider error
/// *text*; semantic checks are structural.
#[derive(Debug, thiserror::Error)]
// Box the figment source: the error would otherwise dominate the Result
// layout (clippy::result_large_err).
#[allow(clippy::result_large_err)]
pub enum ConfigError {
    /// A required field is absent (`field` names the dotted key).
    #[error("configuration incomplete")]
    Missing {
        /// The dotted config key that is absent.
        field: &'static str,
    },
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
    #[allow(clippy::result_large_err)] // the boxed figment source, once at boot
    pub fn load(explicit_path: Option<PathBuf>) -> Result<Self, ConfigError> {
        let path = match explicit_path {
            Some(p) => Some(p),
            None => std::env::var_os("TASKBOARD_CONFIG").map(PathBuf::from),
        };
        if let Some(p) = &path
            && !p.is_file()
        {
            return Err(ConfigError::FileNotFound(p.clone()));
        }

        let mut config: Self = figment(path).extract().map_err(ConfigError::Decode)?;
        if config.storage.db_path.is_none() {
            config.storage.db_path = default_db_path();
        }
        config.validate()?;
        Ok(config)
    }

    /// Structural validation of the decoded values. `load` runs this
    /// last; hand-built configs (tests, embedders) can call it directly.
    ///
    /// # Errors
    ///
    /// The first failing check, as a typed variant.
    #[allow(clippy::result_large_err)] // the boxed figment source, once at boot
    pub fn validate(&self) -> Result<(), ConfigError> {
        // Same normalization rules as `DeckClient::new`: the URL must
        // survive client construction.
        let url_ok = self
            .nextcloud
            .server_url
            .as_deref()
            .is_some_and(reqwest_host_ok);
        if !url_ok {
            return Err(ConfigError::InvalidServerUrl);
        }
        if self
            .nextcloud
            .username
            .as_deref()
            .is_none_or(|u| u.trim().is_empty())
        {
            return Err(ConfigError::EmptyUsername);
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
        if self
            .storage
            .db_path
            .as_ref()
            .is_none_or(|p| p.as_os_str().is_empty())
        {
            return Err(ConfigError::EmptyDbPath);
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
