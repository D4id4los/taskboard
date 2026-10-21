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

use std::collections::BTreeMap;
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
    /// Kiosk appliance deployment profile.
    #[serde(default)]
    pub kiosk: KioskSection,
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
    /// The encrypted-file store for machines without a Secret Service
    /// (kiosks): a generated 0600 key file beside the db plus AEAD
    /// envelopes under `<db-dir>/credentials/` (ADR 0008).
    File,
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
    /// The scalar is the legacy single-connector world: an array
    /// present alongside it is the typed [`ConfigError::
    /// ConnectorShapeMixed`] rejection (the two representations never
    /// mix; validation is loud because figment decodes the mix
    /// silently).
    #[serde(default)]
    pub backend: Option<SyncBackend>,
    /// The `[[sync.connectors]]` plural world. The `Option` is
    /// load-bearing: `None` (array absent) vs `Some(_)` (array present,
    /// including `Some(vec![])`) is the presence fact the resolution
    /// rule and every shape rejection key on. Env cannot set the array
    /// (figment limitation), so the decoded shape is the file's shape.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connectors: Option<Vec<ConnectorCfg>>,
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
            connectors: None,
            poll_interval_secs: default_poll_interval(),
            backoff_initial_secs: default_backoff_initial(),
            backoff_max_secs: default_backoff_max(),
        }
    }
}

/// One `[[sync.connectors]]` entry: a named connector with its own
/// identity. Every field is required — a missing field is a decode
/// failure (structure), not a semantic check. The entry's semantic
/// loudness (kebab + unique name, usable URL, non-empty username, a
/// real backend) lives in [`AppConfig::validate`]'s typed rejections.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectorCfg {
    /// Kebab-case, unique — the `ConnectorId` mint (DF-3's chain head).
    pub name: String,
    /// The connector kind (the module selector). A `standalone` *entry*
    /// is a config error: the array lists connectors; `standalone` is
    /// the absence of one.
    pub backend: SyncBackend,
    /// The connector's Nextcloud root URL.
    pub server_url: String,
    /// The connector's login name.
    pub username: String,
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
    /// Keyboard map (Q22): preset + user override deltas.
    #[serde(default)]
    pub keymap: KeymapSection,
}

impl Default for UiSection {
    fn default() -> Self {
        Self {
            scale: default_ui_scale(),
            keymap: KeymapSection::default(),
        }
    }
}

fn default_ui_scale() -> f64 {
    DEFAULT_UI_SCALE
}

/// `[ui.keymap]`'s preset choice (Q22 keymap half). Unknown values
/// are the config layer's fail-fast contract (D13's structure-vs-
/// semantics line): an unknown preset has no meaningful "drop", so
/// it decodes as the typed [`ConfigError::Decode`], unlike override
/// *values*, which are drop-and-warn findings.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum KeymapPreset {
    #[default]
    Standard,
    Emacs,
    Vi,
}

/// One override's sequences: a bare string is the single-stroke
/// degenerate case (D7's sugar); the array form carries chords.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OverrideSpec {
    Single(String),
    Sequence(Vec<String>),
}

impl OverrideSpec {
    /// Normalizes to the keymap layer's sequence form.
    #[must_use]
    pub fn to_sequences(&self) -> Vec<Vec<String>> {
        match self {
            Self::Single(stroke) => vec![vec![stroke.clone()]],
            Self::Sequence(strokes) => vec![strokes.clone()],
        }
    }
}

/// `[ui.keymap]`: preset name + flat per-action override deltas
/// (D14). Raw strings only — value-level validation (unknown action,
/// malformed stroke, conflicts) happens in the keymap layer at UI
/// start, drop-and-warn (Q15); the config layer owns only structure.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KeymapSection {
    #[serde(default)]
    pub preset: KeymapPreset,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub overrides: BTreeMap<String, OverrideSpec>,
}

/// `[kiosk]`: the kiosk appliance's deployment profile. An absent table
/// decodes to the default (no pin) — today's behaviour; the table grows
/// rotation keys only in the ambient-view chunk (K13).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KioskSection {
    /// The board the wall display pins to, resolved typed against the
    /// live boards at kiosk boot (see [`AppConfig::resolve_fixed_board`]):
    /// absent pin + N live boards is the ambient view's rotation scope,
    /// absent pin + 1 board is that board (KDF-10). Skipped when absent
    /// so pin-less configs serialize unchanged (the `sync.connectors`
    /// precedent).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fixed_board: Option<uuid::Uuid>,
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
    /// The scalar sync representation (`sync.backend`) and the array
    /// representation (`sync.connectors`) are both present — any mix of
    /// the two shapes is rejected (figment decodes the mix silently,
    /// including via `TASKBOARD_SYNC__BACKEND`; loudness lives here).
    #[error("sync.backend and sync.connectors are mutually exclusive shapes")]
    ConnectorShapeMixed,
    /// The `[nextcloud]` table carries identity keys
    /// (`server_url`/`username`) while `sync.connectors` is present —
    /// the array owns those keys. `credential_store` alone stays valid
    /// (process-global, not per-connector identity).
    #[error("nextcloud identity keys and sync.connectors are mutually exclusive shapes")]
    NextcloudTableWithConnectors,
    /// Two `[[sync.connectors]]` entries share a name; the payload
    /// names the offender.
    #[error("duplicate connector name")]
    DuplicateConnectorName {
        /// The duplicated connector name.
        name: String,
    },
    /// A `[[sync.connectors]]` entry's name is not kebab-case
    /// (`[a-z0-9-]`, no leading/trailing `-`, no `--`); the payload
    /// names the offender.
    #[error("connector name invalid")]
    InvalidConnectorName {
        /// The offending connector name.
        name: String,
    },
    /// A `[[sync.connectors]]` entry carries an unusable field value;
    /// the payload names which entry and which field (the per-entry
    /// analogue of `InvalidServerUrl`/`EmptyUsername` — without it a
    /// garbage entry URL surfaces only at boot, losing which entry
    /// offended).
    #[error("connector entry invalid")]
    InvalidConnectorEntry {
        /// The offending entry's name.
        name: String,
        /// The offending field within the entry (`server_url`,
        /// `username`, or `backend`).
        field: &'static str,
    },
    /// `[kiosk] fixed_board` names no live board (unknown UUID, or a
    /// board deleted before this boot); the payload names the pin.
    /// Resolution is a boot-time typed refusal — never a silent
    /// default, never a runtime warning.
    #[error("kiosk fixed board unknown")]
    UnknownFixedBoard {
        /// The pin that does not resolve.
        id: uuid::Uuid,
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
            Self::ConnectorShapeMixed => "connector-shape-mixed",
            Self::NextcloudTableWithConnectors => "nextcloud-table-with-connectors",
            Self::DuplicateConnectorName { .. } => "duplicate-connector-name",
            Self::InvalidConnectorName { .. } => "invalid-connector-name",
            Self::InvalidConnectorEntry { .. } => "invalid-connector-entry",
            Self::UnknownFixedBoard { .. } => "unknown-fixed-board",
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

    /// The backend word a surface displays for the *default* connector
    /// (D5-9): under the array world it is the first entry's kind; the
    /// legacy scalar world resolves through [`Self::sync_backend`].
    /// Legacy files produce byte-identical output.
    #[must_use]
    pub fn resolved_default_backend(&self) -> SyncBackend {
        match &self.sync.connectors {
            Some(entries) => entries
                .first()
                .map_or(SyncBackend::Standalone, |entry| entry.backend),
            None => self.sync_backend(),
        }
    }

    /// Structural validation of the decoded values. `load` runs this
    /// last; hand-built configs (tests, embedders) can call it directly.
    ///
    /// # Errors
    ///
    /// The first failing check, as a typed variant.
    #[allow(clippy::result_large_err)] // the figment source, once at boot
    #[allow(clippy::too_many_lines)] // one flat rejection ladder, in order
    pub fn validate(&self) -> Result<(), ConfigError> {
        // The shape rejections come first: with an array present *and*
        // garbage legacy `[nextcloud]` keys, the shape error must fire,
        // not `InvalidServerUrl` — the mix is the primary offense.
        if self.sync.backend.is_some() && self.sync.connectors.is_some() {
            return Err(ConfigError::ConnectorShapeMixed);
        }
        let nextcloud_identity_present =
            self.nextcloud.server_url.is_some() || self.nextcloud.username.is_some();
        if nextcloud_identity_present && self.sync.connectors.is_some() {
            return Err(ConfigError::NextcloudTableWithConnectors);
        }
        if let Some(entries) = &self.sync.connectors {
            let mut seen: Vec<&str> = Vec::with_capacity(entries.len());
            for entry in entries {
                if !is_kebab_name(&entry.name) {
                    return Err(ConfigError::InvalidConnectorName {
                        name: entry.name.clone(),
                    });
                }
                if seen.contains(&entry.name.as_str()) {
                    return Err(ConfigError::DuplicateConnectorName {
                        name: entry.name.clone(),
                    });
                }
                seen.push(&entry.name);
                if entry.backend == SyncBackend::Standalone {
                    return Err(ConfigError::InvalidConnectorEntry {
                        name: entry.name.clone(),
                        field: "backend",
                    });
                }
                if !reqwest_host_ok(&entry.server_url) {
                    return Err(ConfigError::InvalidConnectorEntry {
                        name: entry.name.clone(),
                        field: "server_url",
                    });
                }
                if entry.username.trim().is_empty() {
                    return Err(ConfigError::InvalidConnectorEntry {
                        name: entry.name.clone(),
                        field: "username",
                    });
                }
            }
        }
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

    /// Resolves the `[kiosk] fixed_board` pin against the live board
    /// ids captured at boot. Pure — the caller owns capturing the
    /// boards. A pin naming no live board (unknown UUID, or a deleted
    /// board) is the typed [`ConfigError::UnknownFixedBoard`]: never a
    /// silent default, never a runtime warning (the deployment
    /// contract is that the wall shows the pinned board or nothing
    /// boots). Called only under `mode = "kiosk"` — the pin is the
    /// kiosk deployment's contract, not a global invariant, so other
    /// modes (the operator's maintenance session on the same config
    /// file) ignore it.
    ///
    /// # Errors
    ///
    /// [`ConfigError::UnknownFixedBoard`] when the pin names no live
    /// board.
    #[allow(clippy::result_large_err)] // mirrors the enum's one grant above
    pub fn resolve_fixed_board(
        &self,
        live_board_ids: &[uuid::Uuid],
    ) -> Result<Option<uuid::Uuid>, ConfigError> {
        match self.kiosk.fixed_board {
            None => Ok(None),
            Some(pin) if live_board_ids.contains(&pin) => Ok(Some(pin)),
            Some(pin) => Err(ConfigError::UnknownFixedBoard { id: pin }),
        }
    }

    /// The directory the file credential store roots at: the db's
    /// parent, so `credential_store = "file"` needs no config keys of
    /// its own — relocating `[storage] db_path` relocates the key file
    /// and the envelope directory with it. Call after
    /// [`AppConfig::load`] (which fills the db default); a db path
    /// with no parent maps to the process directory.
    #[must_use]
    pub fn credential_base_dir(&self) -> &Path {
        self.db_path()
            .parent()
            .filter(|dir| !dir.as_os_str().is_empty())
            .unwrap_or(Path::new("."))
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
            // Stateless: engine and actor read one wall source. A fresh
            // Arc of the zero-sized clock is that same source.
            clock: std::sync::Arc::new(taskboard_domain::SystemClock),
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

/// The connector-name predicate (DF-3's config-half filter): non-empty,
/// every char in `[a-z0-9-]`, no leading or trailing `-`, no `--`. This
/// is `validate`'s input filter for the `ConnectorId` mint; the domain
/// newtype itself stays unfiltered (config validates, domain trusts).
#[must_use]
pub(crate) fn is_kebab_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !name.starts_with('-')
        && !name.ends_with('-')
        && !name.contains("--")
}

/// F12-03 (#261): pure cleartext classifier — true iff the scheme is
/// exactly `http`. Consulted at exactly two emission sites (the connect
/// ceremony and bootstrap's nextcloud arm), so the warning is
/// once-per-process by construction; never per request or per poll
/// cycle. No loopback exemption (the flag is informational, not a gate).
#[must_use]
pub(crate) fn url_is_cleartext(raw: &str) -> bool {
    matches!(raw.split_once("://"), Some(("http", _)))
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
/// constructing a client: scheme http/https, a non-empty host, and no
/// userinfo in the authority (F12-02 — reqwest never applies URL
/// userinfo as auth, so a pasted credential could only leak). The
/// sync crate parses the same string again at client construction, so
/// a drift here fails closed (bootstrap's `Client` variant).
fn reqwest_host_ok(raw: &str) -> bool {
    // Minimal structural parse (no extra dependency): scheme://host/…
    let Some((scheme, rest)) = raw.split_once("://") else {
        return false;
    };
    if !matches!(scheme, "http" | "https") {
        return false;
    }
    let authority = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    // Any `@` in the authority is userinfo (rejected even when the
    // user part is empty — fail-closed direction of the documented
    // parity policy).
    if authority.contains('@') {
        return false;
    }
    !authority.is_empty()
}

/// Atomically rewrites the config file at `path` with `contents`
/// (overlay on connect, key strip on logout — the only two writers of
/// this artifact): tmp sidecar in the same directory, then rename.
/// The sidecar is created owner-only (F12-01; ADR 0012 — the
/// `repl_history` idiom, never a process-global umask mutation) and
/// the rename carries the mode onto the final file, so every rewrite
/// re-establishes 0600 and a pre-hardening permissive config
/// self-heals at the next ceremony.
pub(crate) fn write_owner_only_atomic(path: &Path, contents: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("taskboard-tmp");
    {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt as _;
            options.mode(0o600);
        }
        let mut file = options.open(&tmp)?;
        std::io::Write::write_all(&mut file, contents.as_bytes())?;
        file.sync_all().ok();
    }
    std::fs::rename(&tmp, path)
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

    /// F12-02 (#260): userinfo-bearing server URLs are rejected like
    /// any other invalid URL — the credential was never applied as auth
    /// and could only leak (this is the config half; the sync crate's
    /// client rejects the same string at construction).
    #[test]
    fn validate_rejects_userinfo_urls_like_the_deck_client() {
        for raw in ["https://u:pw@example.com", "http://u@example.com"] {
            let mut c = valid();
            c.nextcloud.server_url = Some(raw.into());
            assert!(
                matches!(c.validate(), Err(ConfigError::InvalidServerUrl)),
                "{raw} must be rejected"
            );
        }
    }

    /// F12-03 (#261): the cleartext classifier is pure and
    /// scheme-only — the warning trigger is literally the `http://`
    /// scheme, no host classification.
    #[test]
    fn url_is_cleartext_classifies_by_scheme_only() {
        assert!(super::url_is_cleartext("http://cloud.example.com"));
        assert!(!super::url_is_cleartext("https://cloud.example.com"));
        assert!(!super::url_is_cleartext("ftp://x"));
        assert!(!super::url_is_cleartext("not a url"));
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
    fn credential_base_dir_is_the_db_parent() {
        let mut config = AppConfig::default();
        config.storage.db_path = Some(PathBuf::from("/srv/taskboard/taskboard.db"));
        assert_eq!(config.credential_base_dir(), Path::new("/srv/taskboard"));
        // A bare filename roots the store at the process directory.
        config.storage.db_path = Some(PathBuf::from("taskboard.db"));
        assert_eq!(config.credential_base_dir(), Path::new("."));
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

        let store: NextcloudSection =
            Figment::from(Serialized::defaults(NextcloudSection::default()))
                .merge(Toml::string("credential_store = 'file'"))
                .extract()
                .expect("kebab-case file store decodes");
        assert_eq!(store.credential_store, CredentialStoreKind::File);
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

    // ---- the [[sync.connectors]] plural world (chunk 5) --------------

    fn two_entry_toml() -> &'static str {
        "[[sync.connectors]]\nname = \"work\"\nbackend = \"nextcloud\"\n\
         server_url = \"https://nc.work.example\"\nusername = \"viktor\"\n\
         [[sync.connectors]]\nname = \"personal\"\nbackend = \"nextcloud\"\n\
         server_url = \"https://nc.home.example\"\nusername = \"vp\"\n"
    }

    /// Decodes a TOML literal through the same provider chain shape as
    /// `extract_raw` (defaults → toml), without touching the filesystem.
    #[allow(clippy::result_large_err)] // mirrors the production decoder
    fn decode(toml: &str) -> Result<AppConfig, ConfigError> {
        Figment::from(Serialized::defaults(AppConfig::default()))
            .merge(Toml::string(toml))
            .extract::<AppConfig>()
            .map_err(ConfigError::Decode)
    }

    #[test]
    fn connector_array_decodes_in_file_order_first_is_default() {
        let config = decode(two_entry_toml()).expect("decodes");
        let entries = config.sync.connectors.as_deref().expect("array present");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "work");
        assert_eq!(entries[1].name, "personal");
        assert_eq!(config.resolved_default_backend(), SyncBackend::Nextcloud);
        // The whole array config validates (db path is defaulted by the
        // Serialized defaults' absence — fill it explicitly).
        let mut config = config;
        config.storage.db_path = Some(PathBuf::from("/tmp/taskboard.db"));
        config.validate().expect("the array world validates");
    }

    #[test]
    fn unknown_key_in_a_connector_entry_is_a_decode_error() {
        let err = decode(
            "[[sync.connectors]]\nname = \"work\"\nbackend = \"nextcloud\"\n\
             server_url = \"https://nc.work.example\"\nusername = \"v\"\n\
             pol_interval = 5\n",
        )
        .expect_err("typo guard");
        assert!(matches!(err, ConfigError::Decode(_)));
    }

    #[test]
    fn empty_connector_array_alone_is_valid() {
        let config = decode("[sync]\nconnectors = []\n").expect("empty array decodes");
        assert_eq!(
            config.sync.connectors.as_deref(),
            Some(&[][..]),
            "Some(vec![]) — the array door to standalone"
        );
        let mut config = config;
        config.storage.db_path = Some(PathBuf::from("/tmp/taskboard.db"));
        config.validate().expect("zero connectors is a valid world");
    }

    #[test]
    #[allow(clippy::result_large_err)] // the decode closure mirrors production
    fn scalar_backend_with_an_array_is_the_typed_mix_rejection() {
        for backend in ["nextcloud", "standalone"] {
            let toml = format!("[sync]\nbackend = \"{backend}\"\n{}", two_entry_toml());
            assert!(
                matches!(
                    decode(&toml).and_then(|c| {
                        let mut c = c;
                        c.storage.db_path = Some(PathBuf::from("/tmp/t.db"));
                        c.validate()
                    }),
                    Err(ConfigError::ConnectorShapeMixed)
                ),
                "backend {backend} + array must mix-reject"
            );
        }
    }

    #[test]
    fn backend_nextcloud_with_an_empty_array_is_the_mix_rejection() {
        // Design rejection #6 (subsumed by #1's presence scope): the
        // scalar names a connector the empty array does not provide.
        let mut config = valid();
        config.nextcloud = NextcloudSection::default();
        config.sync.backend = Some(SyncBackend::Nextcloud);
        config.sync.connectors = Some(Vec::new());
        assert!(matches!(
            config.validate(),
            Err(ConfigError::ConnectorShapeMixed)
        ));
    }

    #[test]
    fn nextcloud_identity_keys_with_an_array_are_the_table_rejection() {
        for toml in [
            format!(
                "[nextcloud]\nserver_url = \"https://x.example\"\n{}",
                two_entry_toml()
            ),
            format!("[nextcloud]\nusername = \"alice\"\n{}", two_entry_toml()),
        ] {
            let mut config = decode(&toml).expect("decodes");
            config.storage.db_path = Some(PathBuf::from("/tmp/t.db"));
            assert!(matches!(
                config.validate(),
                Err(ConfigError::NextcloudTableWithConnectors)
            ));
        }
    }

    #[test]
    fn nextcloud_credential_store_alone_with_an_array_is_valid() {
        let toml = format!(
            "[nextcloud]\ncredential_store = \"env\"\n{}",
            two_entry_toml()
        );
        let mut config = decode(&toml).expect("decodes");
        config.storage.db_path = Some(PathBuf::from("/tmp/t.db"));
        config
            .validate()
            .expect("store-kind selection is process-global");
    }

    #[test]
    fn duplicate_connector_names_are_rejected_with_the_offender() {
        let toml = "[[sync.connectors]]\nname = \"work\"\nbackend = \"nextcloud\"\n\
             server_url = \"https://nc.work.example\"\nusername = \"v\"\n\
             [[sync.connectors]]\nname = \"work\"\nbackend = \"nextcloud\"\n\
             server_url = \"https://nc.other.example\"\nusername = \"w\"\n";
        let mut config = decode(toml).expect("decodes");
        config.storage.db_path = Some(PathBuf::from("/tmp/t.db"));
        match config.validate() {
            Err(ConfigError::DuplicateConnectorName { name }) => {
                assert_eq!(name, "work", "the payload names the offender");
            }
            other => panic!("expected DuplicateConnectorName, got {other:?}"),
        }
    }

    #[test]
    fn non_kebab_connector_names_are_rejected_with_the_offender() {
        for name in ["Work", "a_b", "-a", "a-", "a--b", "", "a b"] {
            // Hand-built: the predicate is shape-agnostic, and a quoted
            // TOML key round-trips every shape here anyway.
            let mut config = valid();
            config.nextcloud = NextcloudSection::default();
            config.sync.connectors = Some(vec![ConnectorCfg {
                name: name.to_owned(),
                backend: SyncBackend::Nextcloud,
                server_url: "https://nc.work.example".into(),
                username: "v".into(),
            }]);
            match config.validate() {
                Err(ConfigError::InvalidConnectorName { name: offender }) => {
                    assert_eq!(offender, name, "the payload names the offender");
                }
                other => panic!("expected InvalidConnectorName for {name:?}, got {other:?}"),
            }
        }
    }

    #[test]
    fn kebab_predicate_pins_both_lists() {
        for name in ["work", "nc-2", "personal", "a0"] {
            assert!(is_kebab_name(name), "{name} must pass");
        }
        for name in ["Work", "a_b", "-a", "a-", "a--b", "", "a b", "Ärger"] {
            assert!(!is_kebab_name(name), "{name} must fail");
        }
    }

    #[test]
    fn invalid_entry_fields_are_typed_with_the_entry_and_field() {
        let entry = |server_url: &str, username: &str, backend: SyncBackend| {
            let mut config = valid();
            config.nextcloud = NextcloudSection::default();
            config.sync.connectors = Some(vec![ConnectorCfg {
                name: "work".into(),
                backend,
                server_url: server_url.into(),
                username: username.into(),
            }]);
            config
        };
        match entry("no-scheme.example", "v", SyncBackend::Nextcloud).validate() {
            Err(ConfigError::InvalidConnectorEntry { name, field }) => {
                assert_eq!((name.as_str(), field), ("work", "server_url"));
            }
            other => panic!("expected InvalidConnectorEntry(server_url), got {other:?}"),
        }
        match entry("https://nc.example", "  ", SyncBackend::Nextcloud).validate() {
            Err(ConfigError::InvalidConnectorEntry { name, field }) => {
                assert_eq!((name.as_str(), field), ("work", "username"));
            }
            other => panic!("expected InvalidConnectorEntry(username), got {other:?}"),
        }
        match entry("https://nc.example", "v", SyncBackend::Standalone).validate() {
            Err(ConfigError::InvalidConnectorEntry { name, field }) => {
                assert_eq!((name.as_str(), field), ("work", "backend"));
            }
            other => panic!("expected InvalidConnectorEntry(backend), got {other:?}"),
        }
    }

    #[test]
    fn shape_rejections_fire_before_the_legacy_url_check() {
        // Array + scalar + garbage legacy URL: the mix is the primary
        // offense, not InvalidServerUrl.
        let mut config = valid();
        config.nextcloud.server_url = Some("garbage".into());
        config.sync.backend = Some(SyncBackend::Nextcloud);
        config.sync.connectors = Some(vec![ConnectorCfg {
            name: "work".into(),
            backend: SyncBackend::Nextcloud,
            server_url: "https://nc.work.example".into(),
            username: "v".into(),
        }]);
        assert!(matches!(
            config.validate(),
            Err(ConfigError::ConnectorShapeMixed)
        ));

        // Array + `[nextcloud]` garbage URL: the table rejection, not
        // InvalidServerUrl.
        let mut config = valid();
        config.nextcloud.server_url = Some("garbage".into());
        config.sync.connectors = Some(vec![ConnectorCfg {
            name: "work".into(),
            backend: SyncBackend::Nextcloud,
            server_url: "https://nc.work.example".into(),
            username: "v".into(),
        }]);
        assert!(matches!(
            config.validate(),
            Err(ConfigError::NextcloudTableWithConnectors)
        ));
    }

    /// Probe case 6's composition: a scalar from a *later provider* over
    /// an array file is the same merge-order shape as the env override
    /// (`TASKBOARD_SYNC__BACKEND` composes identically — env cannot set
    /// the array, probe case 7), and it must decode-then-mix-reject:
    /// the env scripting seam stays loud, not silently ignored.
    #[test]
    fn later_provider_scalar_over_an_array_file_mix_rejects() {
        let toml = format!("[sync]\nbackend = \"standalone\"\n{}", two_entry_toml());
        let config = decode(&toml).expect("figment decodes the mix silently");
        assert!(config.sync.connectors.is_some() && config.sync.backend.is_some());
        let mut config = config;
        config.storage.db_path = Some(PathBuf::from("/tmp/t.db"));
        assert!(matches!(
            config.validate(),
            Err(ConfigError::ConnectorShapeMixed)
        ));
    }

    // ---- the [kiosk] deployment profile (chunk 9) --------------------

    #[test]
    fn kiosk_section_absent_decodes_to_none_and_validates() {
        let config = decode("[app]\nmode = 'kiosk'\n").expect("decodes");
        assert_eq!(config.kiosk.fixed_board, None);
        let mut config = config;
        config.storage.db_path = Some(PathBuf::from("/tmp/t.db"));
        config.validate().expect("the absent section validates");
    }

    #[test]
    fn fixed_board_decodes_from_a_uuid_string() {
        let id = uuid::Uuid::new_v4();
        let toml = format!("[kiosk]\nfixed_board = '{id}'\n");
        let config = decode(&toml).expect("decodes");
        assert_eq!(config.kiosk.fixed_board, Some(id));
        // A pin-less section serializes empty — the skip contract that
        // keeps existing configs' rendered shape unchanged.
        let value = serde_json::to_value(KioskSection::default()).expect("serializes");
        assert_eq!(value, serde_json::json!({}));
    }

    #[test]
    fn malformed_fixed_board_is_a_decode_error() {
        let err = decode("[kiosk]\nfixed_board = 'not-a-uuid'\n")
            .expect_err("a non-UUID pin is structure, not semantics");
        assert!(matches!(err, ConfigError::Decode(_)));
    }

    #[test]
    fn unknown_key_in_the_kiosk_table_is_a_decode_error() {
        let err = decode("[kiosk]\nfixed_baord = 'not-a-uuid'\n")
            .expect_err("the typo guard rejects unknown keys");
        assert!(matches!(err, ConfigError::Decode(_)));
    }

    #[test]
    fn resolve_fixed_board_returns_none_without_a_pin() {
        let config = valid();
        assert_eq!(config.resolve_fixed_board(&[]).expect("resolves"), None);
        let live = [uuid::Uuid::new_v4()];
        assert_eq!(config.resolve_fixed_board(&live).expect("resolves"), None);
    }

    #[test]
    fn resolve_fixed_board_resolves_a_live_board() {
        let pin = uuid::Uuid::new_v4();
        let mut config = valid();
        config.kiosk.fixed_board = Some(pin);
        assert_eq!(
            config.resolve_fixed_board(&[pin]).expect("resolves"),
            Some(pin)
        );
    }

    #[test]
    fn resolve_fixed_board_rejects_an_unknown_board_naming_the_pin() {
        let pin = uuid::Uuid::new_v4();
        let mut config = valid();
        config.kiosk.fixed_board = Some(pin);
        for live in [&[][..], &[uuid::Uuid::new_v4()]] {
            match config.resolve_fixed_board(live) {
                Err(ConfigError::UnknownFixedBoard { id }) => assert_eq!(id, pin),
                other => panic!("expected UnknownFixedBoard, got {other:?}"),
            }
        }
    }

    #[test]
    fn the_new_kinds_extend_the_stable_discriminants() {
        for (err, kind) in [
            (
                ConfigError::UnknownFixedBoard {
                    id: uuid::Uuid::nil(),
                },
                "unknown-fixed-board",
            ),
            (ConfigError::ConnectorShapeMixed, "connector-shape-mixed"),
            (
                ConfigError::NextcloudTableWithConnectors,
                "nextcloud-table-with-connectors",
            ),
            (
                ConfigError::DuplicateConnectorName {
                    name: "work".into(),
                },
                "duplicate-connector-name",
            ),
            (
                ConfigError::InvalidConnectorName {
                    name: "work".into(),
                },
                "invalid-connector-name",
            ),
            (
                ConfigError::InvalidConnectorEntry {
                    name: "work".into(),
                    field: "server_url",
                },
                "invalid-connector-entry",
            ),
        ] {
            assert_eq!(err.kind(), kind);
        }
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

    // ---- [ui.keymap] (D14, T28/T29) ---------------------------------

    /// T28 — the keymap table decodes (defaults, bare-string sugar,
    /// array chords) and old configs without the table stay valid.
    #[test]
    fn keymap_section_decodes_with_defaults_and_sugar() {
        // Old config: no keymap table at all.
        let old: AppConfig = Figment::from(Serialized::defaults(AppConfig::default()))
            .merge(Toml::string("[ui]\nscale = 1.5"))
            .extract()
            .expect("old configs stay valid");
        assert_eq!(old.ui.keymap.preset, KeymapPreset::Standard);
        assert!(old.ui.keymap.overrides.is_empty());

        // Full table: named preset, bare-string sugar, chord array.
        let full: AppConfig = Figment::from(Serialized::defaults(AppConfig::default()))
            .merge(Toml::string(
                "[ui.keymap]\npreset = \"vi\"\n\n[ui.keymap.overrides]\n\"task.complete\" = [\"Ctrl+Space\"]\n\"app.exit\" = \"Ctrl+Q\"",
            ))
            .extract()
            .expect("keymap table decodes");
        assert_eq!(full.ui.keymap.preset, KeymapPreset::Vi);
        assert_eq!(
            full.ui
                .keymap
                .overrides
                .get("task.complete")
                .expect("chord"),
            &OverrideSpec::Sequence(vec!["Ctrl+Space".into()])
        );
        assert_eq!(
            full.ui.keymap.overrides.get("app.exit").expect("sugar"),
            &OverrideSpec::Single("Ctrl+Q".into())
        );
        assert_eq!(
            full.ui
                .keymap
                .overrides
                .get("app.exit")
                .expect("s")
                .to_sequences(),
            vec![vec!["Ctrl+Q".to_owned()]],
            "sugar normalizes to one single-stroke sequence"
        );
    }

    /// T29 — an unknown preset is the config layer's fail-fast
    /// contract: a typed decode error (structure), not a finding
    /// (D13's structure-vs-semantics line).
    #[test]
    fn unknown_preset_is_a_typed_decode_error() {
        let err: figment::Error = Figment::from(Serialized::defaults(AppConfig::default()))
            .merge(Toml::string("[ui.keymap]\npreset = \"bogus\""))
            .extract::<AppConfig>()
            .map(|_| unreachable!("unknown preset rejected"))
            .unwrap_err();
        // Through the load path this collapses into ConfigError::Decode
        // (`extract_raw` maps it); the provider error itself is the
        // structural decode failure.
        assert_eq!(ConfigError::Decode(err).kind(), "decode");
    }

    /// Unknown keys inside the keymap table are decode errors too
    /// (the table keeps the workspace's reject-unknown-keys rule).
    #[test]
    fn unknown_keymap_keys_are_decode_errors() {
        let result: Result<AppConfig, figment::Error> =
            Figment::from(Serialized::defaults(AppConfig::default()))
                .merge(Toml::string("[ui.keymap]\ntokens = 1"))
                .extract();
        assert!(result.is_err(), "unknown keys are decode failures");
    }
}
