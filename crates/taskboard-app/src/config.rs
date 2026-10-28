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
/// "clamped 0.5–3.0" — enforced, never silently clamped. Delegates to
/// the `UiScale` quantity's consts (same numbers, one source).
pub const UI_SCALE_MIN: f64 = taskboard_ui_slint::UiScale::MIN;
/// See [`UI_SCALE_MIN`].
pub const UI_SCALE_MAX: f64 = taskboard_ui_slint::UiScale::MAX;

/// `[reminders] tick_secs`: engine tick cadence.
pub const DEFAULT_REMINDER_TICK_SECS: u64 = 30;
/// `[reminders] default_before`: the implicit reminder's offset.
pub const DEFAULT_REMINDER_BEFORE: &str = "5m";

/// `[preferences] day_start_time`: midnight rollover, the D-F6 default
/// (a night-owl install sets e.g. `"05:00"`).
pub const DEFAULT_DAY_START_TIME: &str = "00:00";

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
    /// Web kiosk dashboard (opt-in). An absent table is today's
    /// behaviour: no HTTP surface, no port, no task. Skipped when at
    /// its off default so pin-less configs serialize unchanged.
    #[serde(default, skip_serializing_if = "WebSection::is_default")]
    pub web: WebSection,
    /// IPC client deadlines. Skipped when at its default so pin-less
    /// configs serialize unchanged.
    #[serde(default, skip_serializing_if = "IpcSection::is_default")]
    pub ipc: IpcSection,
    /// Device-local user preferences. Never synced (no document key,
    /// no lane) until the D5 `settings` section exists; each device
    /// keeps its own. Overrides via
    /// `TASKBOARD_PREFERENCES__DAY_START_TIME`.
    #[serde(default)]
    pub preferences: PreferencesSection,
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

/// `[reminders] sink`: which notifier delivers fired reminders.
/// `auto` — the default — resolves by `[app] mode` at boot (D-R13):
/// desktop gets toasts day one with zero configuration; daemon/kiosk
/// keep the journald trail. An unknown value has no meaningful drop
/// (the `KeymapPreset` rule): it fails the decode, never defaults.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SinkChoice {
    /// Resolve by `[app] mode`: `desktop` → toasts, `daemon`/`kiosk`
    /// → the log trail.
    #[default]
    Auto,
    /// The journald trail (`notifications::LogSink`), on every mode.
    Log,
    /// Desktop toasts (`notifications::DesktopSink`), on every mode.
    Desktop,
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
    /// Which notifier delivers fired reminders (D-R13).
    #[serde(default)]
    pub sink: SinkChoice,
}

impl Default for RemindersSection {
    fn default() -> Self {
        Self {
            enabled: default_reminders_enabled(),
            tick_secs: default_reminder_tick(),
            default_before: default_reminder_before(),
            sink: SinkChoice::default(),
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

/// `[preferences]` table: device-local user preferences. Like
/// `[reminders]`, the time-of-day is a raw string parsed in
/// [`AppConfig::validate`] — a bad value must surface as the typed
/// `invalid-day-start-time` kind, never a silent default, and the
/// raw form stays stored so `config print` echoes the user's own
/// spelling.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreferencesSection {
    /// Local wall-clock time-of-day at which "today" begins (the
    /// day-boundary rollover; a night-owl sets `"05:00"`). Validated
    /// typed — an unparseable value is
    /// [`ConfigError::InvalidDayStartTime`], never a silent default.
    #[serde(default = "default_day_start_time")]
    pub day_start_time: String,
}

impl Default for PreferencesSection {
    fn default() -> Self {
        Self {
            day_start_time: default_day_start_time(),
        }
    }
}

fn default_day_start_time() -> String {
    DEFAULT_DAY_START_TIME.to_owned()
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
    /// Device-local stack column widths (round-1 D20): board uuid →
    /// stack uuid → width in logical px. Never synced (no domain key);
    /// pruned by the UI request forwarder at write time. Skipped when
    /// empty so width-less configs serialize unchanged (the `web`
    /// precedent).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub stack_widths: BTreeMap<uuid::Uuid, BTreeMap<uuid::Uuid, u32>>,
    /// The due-soon horizon in whole days (round-3 D14/#230): handed
    /// to the UI as a typed delta. `0` is legal and means "nothing is
    /// due soon" (an empty soon set); anything above ten years is the
    /// typed [`ConfigError::InvalidDueSoonDays`] — never a silent
    /// clamp.
    #[serde(default = "default_due_soon_days")]
    pub due_soon_days: u32,
}

impl Default for UiSection {
    fn default() -> Self {
        Self {
            scale: default_ui_scale(),
            keymap: KeymapSection::default(),
            stack_widths: BTreeMap::new(),
            due_soon_days: default_due_soon_days(),
        }
    }
}

fn default_due_soon_days() -> u32 {
    1
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
/// decodes to the default (no pin, no rotation) — today's behaviour.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KioskSection {
    /// The board the wall display pins to, resolved typed against the
    /// live boards at kiosk boot — see
    /// [`AppConfig::resolve_fixed_board`] for the full pin matrix.
    /// Skipped when absent so pin-less configs serialize unchanged
    /// (the `sync.connectors` precedent).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fixed_board: Option<uuid::Uuid>,
    /// The display-mode rotation (K13, issue #363). Its presence *is*
    /// the enablement — an absent table is no rotation, the pre-K13
    /// behaviour byte-for-byte.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation: Option<RotationSection>,
}

/// `[kiosk.rotation]`: the kiosk ambient display's rotation profile
/// (issue #363, KDF-10's reserved key space). Both keys are required
/// inside a present table — an implicit `modes`/`dwell_secs` default
/// would be a silent rotation nobody configured.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RotationSection {
    /// The display modes to cycle, in declared rotation order. Exactly
    /// two distinct entries (validated typed in [`AppConfig::validate`]
    /// and mirrored by the UI-side minting constructor).
    pub modes: Vec<KioskRotationMode>,
    /// Per-mode display time in seconds; the [`KIOSK_ROTATION_MIN_DWELL_SECS`]
    /// floor is enforced typed in [`AppConfig::validate`].
    pub dwell_secs: u64,
}

/// One rotation display mode. Lowercase TOML spelling; the order of
/// the enum's declaration is the stable config vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum KioskRotationMode {
    /// The pinned board's kanban reading.
    Board,
    /// The today view's upcoming-oriented reading.
    Today,
}

/// `[kiosk.rotation] dwell_secs` typed floor: a wall display flipping
/// faster than this is a strobe, never an ambient cadence — refused,
/// never clamped (the [`WEB_REFRESH_MIN_SECS`] precedent).
pub const KIOSK_ROTATION_MIN_DWELL_SECS: u64 = 10;

/// `[web]`: the web kiosk dashboard's deployment profile (K11). The
/// table's presence *plus* `enabled` spawns the read-only HTTP surface;
/// `enabled = false` or an absent table never spawn one. Field-level
/// defaults apply whenever the table is present; the struct's `Default`
/// is the absent-table placeholder (`enabled = false`) that keeps
/// section-less configs byte-identical on serialization.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WebSection {
    /// Spawns the dashboard server iff the section is present and this
    /// is `true`. Defaults to `true` inside a present table — writing
    /// `[web]` is the opt-in. Skipped when off so the absent-table
    /// placeholder serializes to nothing.
    #[serde(
        default = "default_web_enabled",
        skip_serializing_if = "web_enabled_is_off"
    )]
    pub enabled: bool,
    /// Listen address. The loopback default is the trusted arm; a
    /// non-loopback bind additionally requires a resolvable capability
    /// token (checked at boot, where the token file lives).
    #[serde(
        default = "default_web_bind",
        skip_serializing_if = "web_bind_is_default"
    )]
    pub bind: String,
    /// Meta-refresh cadence in seconds; the self-DoS floor
    /// [`WEB_REFRESH_MIN_SECS`] is enforced typed in
    /// [`AppConfig::validate`].
    #[serde(
        default = "default_web_refresh",
        skip_serializing_if = "web_refresh_is_default"
    )]
    pub refresh_secs: u64,
    /// Explicit capability-token override. Unset = generated (or
    /// reloaded from the persisted token file beside the database).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
}

impl Default for WebSection {
    fn default() -> Self {
        Self {
            enabled: false,
            bind: default_web_bind(),
            refresh_secs: DEFAULT_WEB_REFRESH_SECS,
            token: None,
        }
    }
}

impl WebSection {
    /// The absent-table test behind `skip_serializing_if`: the off
    /// placeholder serializes to nothing, so a config without `[web]`
    /// renders unchanged.
    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

fn default_web_enabled() -> bool {
    true
}

fn default_web_bind() -> String {
    DEFAULT_WEB_BIND.to_owned()
}

fn default_web_refresh() -> u64 {
    DEFAULT_WEB_REFRESH_SECS
}

// serde's `skip_serializing_if` demands the `fn(&T) -> bool` shape.
#[allow(clippy::trivially_copy_pass_by_ref)]
fn web_enabled_is_off(enabled: &bool) -> bool {
    !*enabled
}

fn web_bind_is_default(bind: &str) -> bool {
    bind == DEFAULT_WEB_BIND
}

#[allow(clippy::trivially_copy_pass_by_ref)] // serde's skip fn shape
fn web_refresh_is_default(secs: &u64) -> bool {
    *secs == DEFAULT_WEB_REFRESH_SECS
}

/// `[web] bind`: loopback by default — the trusted, no-token arm.
pub const DEFAULT_WEB_BIND: &str = "127.0.0.1:8787";
/// `[web] refresh_secs` default: the meta-refresh cadence.
pub const DEFAULT_WEB_REFRESH_SECS: u64 = 30;
/// `[web] refresh_secs` typed floor: a faster poll is a self-DoS, not a
/// feature — refused, never clamped.
pub const WEB_REFRESH_MIN_SECS: u64 = 5;

/// `[ipc]`: the one-shot CLI client's daemon-facing deadlines (ADR 0012,
/// amended 2026-10-27). The table's absence is today's derived budget —
/// the struct's `Default` is the absent-table placeholder that keeps
/// section-less configs byte-identical on serialization.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IpcSection {
    /// Overrides the derived client read budget **wholesale**: every
    /// covered request waits exactly this many seconds for the daemon's
    /// answer, instead of the verb's `--timeout-secs` + margin (or the
    /// 30 s flag-less ceiling). `0` disables the deadline entirely —
    /// the recorded supervision stance spelled as a value.
    ///
    /// Deliberate leniency, documented at the type (A1 posture): a
    /// configured value *can* preempt an honest long verb (a 5 s pin
    /// turns a legitimate 60 s `sync` into `ipc-timeout` while the
    /// daemon finishes the work). That is the operator's explicit
    /// choice; the flag+margin derivation is the default precisely so
    /// the honest world needs no override.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_read_timeout_secs: Option<u64>,
}

impl IpcSection {
    /// The absent-table test behind `skip_serializing_if`: the
    /// placeholder serializes to nothing, so a config without `[ipc]`
    /// renders unchanged.
    #[must_use]
    pub fn is_default(&self) -> bool {
        self.client_read_timeout_secs.is_none()
    }
}

/// The `[reminders] default_before` grammar (one definition site, shared
/// with the CLI `--before` flag): a non-empty sequence of
/// `<unsigned-int><unit>` components, unit ∈ `s | m | h | d`, no sign, no
/// spaces; the total must fit a `TimeDelta` (`"0s"`, `"30s"`, `"5m"`,
/// `"1h30m"`, `"2d"`). The grammar itself lives in the domain
/// (`ReminderOffset::parse_text_form` — the UI parses the same form);
/// this wrapper negates the parsed offset back into the before-magnitude.
#[must_use]
pub fn parse_before(raw: &str) -> Option<chrono::TimeDelta> {
    taskboard_domain::ReminderOffset::parse_text_form(raw).map(|offset| -offset.as_duration())
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
    /// The config file exists but cannot be opened for reading
    /// (`PermissionDenied` is the common case; EIO/ELOOP et al. fold
    /// into the same honest bucket — one variant, not an errno
    /// taxonomy); the payload names the path. Distinct from
    /// [`ConfigError::Decode`]: the content was never seen, so a
    /// decode verdict would be a lie (issue #386 — the kiosk's
    /// root-tightened config restart-looped on `kind="decode"`).
    #[error("config file unreadable")]
    UnreadableFile {
        /// The config path that could not be opened.
        path: PathBuf,
        /// The underlying open failure.
        #[source]
        source: std::io::Error,
    },
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
    /// A `[kiosk.rotation]` value is unusable: fewer than two distinct
    /// modes (a one-mode "rotation" is a no-op wearing a config), or a
    /// `dwell_secs` below the typed floor. The payload names the field.
    #[error("kiosk setting invalid")]
    InvalidKioskConfig {
        /// The dotted config key that is invalid.
        field: &'static str,
    },
    /// `[kiosk]` has no `fixed_board` but two or more live boards at
    /// kiosk boot: with no ring (the pin is the scope), the only
    /// correct pin-less multi-board boot is a typed refusal naming the
    /// remedy — set `fixed_board`. The payload carries the live-board
    /// count.
    #[error("kiosk pin required")]
    KioskPinRequired {
        /// How many live boards the boot saw.
        count: usize,
    },
    /// A `[web]` field carries an unusable value: `refresh_secs` below
    /// the typed floor, or a `bind` that does not parse as a socket
    /// address. The payload names the field.
    #[error("web setting invalid")]
    InvalidWebConfig {
        /// The dotted config key that is invalid.
        field: &'static str,
    },
    /// `[preferences] day_start_time` does not parse as a time-of-day.
    #[error("day start time invalid")]
    InvalidDayStartTime {
        /// The dotted config key that is invalid.
        field: &'static str,
    },
    /// `[ui] due_soon_days` exceeds the ten-year horizon ceiling.
    #[error("due soon days invalid")]
    InvalidDueSoonDays {
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
            Self::UnreadableFile { .. } => "unreadable-file",
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
            Self::InvalidKioskConfig { .. } => "kiosk-setting-invalid",
            Self::KioskPinRequired { .. } => "kiosk-pin-required",
            Self::InvalidWebConfig { .. } => "invalid-web-config",
            Self::InvalidDayStartTime { .. } => "invalid-day-start-time",
            Self::InvalidDueSoonDays { .. } => "invalid-due-soon-days",
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
    /// [`ConfigError::UnreadableFile`] when the file at `path` exists
    /// but cannot be opened; [`ConfigError::Decode`] when the providers
    /// produce an undecodable tree.
    #[allow(clippy::result_large_err)] // the figment source, once at boot
    pub fn extract_raw(path: Option<&Path>) -> Result<Self, ConfigError> {
        let probe_path = path.map_or_else(|| PathBuf::from(DEFAULT_CONFIG_PATH), Path::to_path_buf);
        probe_readable(&probe_path)?;
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
                if !taskboard_domain::ConnectorId::is_kebab_name(&entry.name) {
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
            .preferences
            .day_start_time
            .parse::<chrono::NaiveTime>()
            .is_err()
        {
            return Err(ConfigError::InvalidDayStartTime {
                field: "preferences.day_start_time",
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
        if taskboard_ui_slint::UiScale::new(self.ui.scale).is_err() {
            return Err(ConfigError::InvalidScale { field: "ui.scale" });
        }
        // `[ui] due_soon_days`: 0 is legal (an empty soon set); the
        // ten-year ceiling is the fail-fast bound (the InvalidScale
        // precedent — never a silent clamp).
        if self.ui.due_soon_days > 3652 {
            return Err(ConfigError::InvalidDueSoonDays {
                field: "ui.due_soon_days",
            });
        }
        // `[web]`: the decoded (or default) table validates always —
        // the absent table's placeholder carries valid defaults, so the
        // floor and the parse refusal only ever fire on real values.
        if self.web.refresh_secs < WEB_REFRESH_MIN_SECS {
            return Err(ConfigError::InvalidWebConfig {
                field: "web.refresh_secs",
            });
        }
        if self.web.bind.parse::<std::net::SocketAddr>().is_err() {
            return Err(ConfigError::InvalidWebConfig { field: "web.bind" });
        }
        // `[kiosk.rotation]`: a present table validates its shape —
        // two distinct modes (a one-mode rotation or a duplicate are
        // config mistakes, not runtime fallbacks) and the dwell floor.
        if let Some(rotation) = &self.kiosk.rotation {
            let distinct = rotation
                .modes
                .iter()
                .collect::<std::collections::HashSet<_>>()
                .len();
            if rotation.modes.len() < 2 || distinct < rotation.modes.len() {
                return Err(ConfigError::InvalidKioskConfig {
                    field: "kiosk.rotation.modes",
                });
            }
            if rotation.dwell_secs < KIOSK_ROTATION_MIN_DWELL_SECS {
                return Err(ConfigError::InvalidKioskConfig {
                    field: "kiosk.rotation.dwell_secs",
                });
            }
        }
        Ok(())
    }

    /// Resolves the `[kiosk] fixed_board` pin against the live board
    /// ids captured at boot. Pure — the caller owns capturing the
    /// boards. The full pin matrix (K13):
    ///
    /// | pin | live boards | outcome |
    /// |---|---|---|
    /// | some | pin live | the pin |
    /// | some | pin absent | [`ConfigError::UnknownFixedBoard`] |
    /// | none | 0 or 1 | `Ok(None)` — the seed rule picks the single live board deterministically, and an empty boot seeds nothing until boards arrive |
    /// | none | ≥ 2 | [`ConfigError::KioskPinRequired`] — the deployment contract is "the wall shows the pinned board or nothing boots"; a multi-board ring is explicitly out of scope |
    ///
    /// Never a silent default, never a runtime warning. Called only
    /// under `mode = "kiosk"` — the pin is the kiosk deployment's
    /// contract, not a global invariant, so other modes (the
    /// operator's maintenance session on the same config file) ignore
    /// it. Boards arriving *after* a 0/1-board boot are out of this
    /// function's reach by construction: the seed rule pins the first
    /// live board then, the recorded fallback (no runtime refusal).
    ///
    /// # Errors
    ///
    /// [`ConfigError::UnknownFixedBoard`] when the pin names no live
    /// board; [`ConfigError::KioskPinRequired`] when no pin faces a
    /// multi-board boot.
    #[allow(clippy::result_large_err)] // mirrors the enum's one grant above
    pub fn resolve_fixed_board(
        &self,
        live_board_ids: &[uuid::Uuid],
    ) -> Result<Option<uuid::Uuid>, ConfigError> {
        match self.kiosk.fixed_board {
            None if live_board_ids.len() >= 2 => Err(ConfigError::KioskPinRequired {
                count: live_board_ids.len(),
            }),
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

    /// The day-boundary roll's injected config (DR-6/2 D-E8): always
    /// enabled (no user-facing knob — the roll is the recurring-series
    /// feature's core materialization), with the calendar built from the
    /// validated `[preferences] day_start_time` and the device's fixed
    /// UTC offset resolved once at boot (the `resolve_day_start`
    /// convention; the UI keeps its own derivation).
    ///
    /// # Panics
    ///
    /// Never on a validated config (an unparseable `day_start_time` is
    /// rejected by [`AppConfig::validate`]).
    #[must_use]
    pub fn day_calendar(&self) -> taskboard_state::DayRollConfig {
        let day_start_time = self
            .preferences
            .day_start_time
            .parse::<chrono::NaiveTime>()
            .expect("day_start_time validated");
        let offset = *chrono::Local::now().offset();
        taskboard_state::DayRollConfig {
            enabled: true,
            calendar: taskboard_domain::DayCalendar {
                day_start_time,
                offset,
            },
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

/// The read(NotFound → empty)→parse skeleton shared by every config
/// overlay writer (round-1 chunk-2 extraction): the file is operator
/// territory, so writers mutate a table subtree of the parsed
/// `DocumentMut` and write it back atomically — comments, formatting,
/// and unrelated tables are `toml_edit`-preserved by construction.
/// An absent file parses as an empty document (the writer creates it).
pub(crate) fn overlay_document(path: &Path) -> std::io::Result<toml_edit::DocumentMut> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(err) => return Err(err),
    };
    text.parse::<toml_edit::DocumentMut>()
        .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err))
}

/// Distinguishes "exists but cannot be read" from a content failure
/// before the figment read: figment folds every provider failure into
/// `Decode`, so a permission denial on the config used to surface as
/// `kind="decode"` ("configuration rejected") — issue #386's second
/// half. An open failure other than a raced delete (the absent-file
/// path, as before the probe) is [`ConfigError::UnreadableFile`].
/// TOCTOU window: permissions may change between this probe and
/// figment's own read — a boot-once race, accepted.
#[allow(clippy::result_large_err)] // same enum, same once-at-boot shape
fn probe_readable(path: &Path) -> Result<(), ConfigError> {
    if !path.is_file() {
        return Ok(());
    }
    match std::fs::File::open(path) {
        Ok(_) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(ConfigError::UnreadableFile {
            path: path.to_path_buf(),
            source,
        }),
    }
}

/// Atomically rewrites the config file at `path` with `contents`
/// (overlay on connect, key strip on logout — the only two writers of
/// this artifact): tmp sidecar in the same directory, then rename.
/// The sidecar is created owner-only (F12-01; ADR 0012 — the
/// `repl_history` idiom, never a process-global umask mutation) and
/// before the rename adopts the destination's existing permission
/// bits: the file's mode is operator territory (issue #386 — a
/// root-run `connect` on the kiosk appliance must not tighten the
/// runbook-installed 0644 config to 0600 `root:root`), so a rewrite
/// preserves it; only a file this writer *creates* (no prior mode)
/// lands 0600. The sidecar itself only ever carries the mode it
/// finalizes to — owner-only until the destination's stat (any stat
/// failure means "no mode to preserve" and fails safe toward the
/// stricter created-mode) may re-mode it to the preserved bits. That
/// preserved mode is masked to `0o777` — a config never carries
/// setuid/setgid/sticky.
pub(crate) fn write_config_atomic(path: &Path, contents: &str) -> std::io::Result<()> {
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
        #[cfg(unix)]
        if let Ok(meta) = std::fs::metadata(path) {
            use std::os::unix::fs::PermissionsExt as _;
            let mut perms = std::fs::metadata(&tmp)?.permissions();
            perms.set_mode(meta.permissions().mode() & 0o777);
            std::fs::set_permissions(&tmp, perms)?;
        }
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
    fn ui_scale_default_agrees_with_the_quantity_default() {
        assert_eq!(
            taskboard_ui_slint::UiScale::default().get(),
            DEFAULT_UI_SCALE
        );
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
    fn due_soon_days_defaults_to_one_and_round_trips() {
        let round: AppConfig = Figment::from(Serialized::defaults(AppConfig::default()))
            .extract()
            .expect("defaults extract");
        assert_eq!(round.ui.due_soon_days, 1, "absent decodes to the default");
        let mut c = valid();
        c.ui.due_soon_days = 14;
        c.validate().expect("14 validates");
        let round: AppConfig = Figment::from(Serialized::defaults(AppConfig::default()))
            .merge(Toml::string("[ui]\ndue_soon_days = 14"))
            .extract()
            .expect("14 decodes");
        assert_eq!(round.ui.due_soon_days, 14, "14 round-trips");
    }

    #[test]
    fn due_soon_days_zero_is_legal_and_huge_is_typed_rejection() {
        let mut c = valid();
        c.ui.due_soon_days = 0;
        c.validate().expect("0 is legal: an empty soon set");
        let mut c = valid();
        c.ui.due_soon_days = 4000;
        assert!(matches!(
            c.validate(),
            Err(ConfigError::InvalidDueSoonDays {
                field: "ui.due_soon_days"
            })
        ));
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

    /// T-14 — the `stack_widths` section round-trips (nested tables,
    /// uuid keys, u32 values).
    #[test]
    fn ui_stack_widths_round_trips() {
        let board = uuid::Uuid::from_u128(0x1001);
        let stack = uuid::Uuid::from_u128(0x2001);
        let toml = format!("[ui.stack_widths]\n'{board}' = {{ '{stack}' = 240 }}\n");
        let round: AppConfig = Figment::from(Serialized::defaults(AppConfig::default()))
            .merge(Toml::string(&toml))
            .extract()
            .expect("the nested table decodes");
        assert_eq!(
            round
                .ui
                .stack_widths
                .get(&board)
                .and_then(|stacks| stacks.get(&stack)),
            Some(&240u32)
        );
    }

    /// T-14 — old configs parse unchanged: an absent key is the empty
    /// map, and the default section serializes without the key (no
    /// empty husk).
    #[test]
    fn ui_stack_widths_absent_key_is_empty_and_serializes_to_nothing() {
        let round: AppConfig = Figment::from(Serialized::defaults(AppConfig::default()))
            .merge(Toml::string("[ui]\nscale = 1.5\n"))
            .extract()
            .expect("a width-less config decodes");
        assert!(round.ui.stack_widths.is_empty(), "old configs parse");

        let serialized = serde_json::to_value(AppConfig::default().ui).expect("serialize");
        assert!(
            serialized.get("stack_widths").is_none(),
            "the default section serializes without the key"
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
        assert_eq!(round.reminders.sink, SinkChoice::Auto);
        assert_eq!(round.preferences.day_start_time, DEFAULT_DAY_START_TIME);
    }

    #[test]
    fn sink_decodes_the_three_kebab_words() {
        for (raw, expected) in [
            ("auto", SinkChoice::Auto),
            ("log", SinkChoice::Log),
            ("desktop", SinkChoice::Desktop),
        ] {
            let config: AppConfig = Figment::from(Serialized::defaults(AppConfig::default()))
                .merge(Toml::string(
                    format!("[reminders]\nsink = '{raw}'").as_str(),
                ))
                .extract()
                .unwrap_or_else(|_| panic!("sink = '{raw}' decodes"));
            assert_eq!(config.reminders.sink, expected);
        }
    }

    #[test]
    fn unknown_sink_value_is_a_decode_error() {
        // The KeymapPreset rule: an unknown enum value has no
        // meaningful drop — it fails the decode, never defaults.
        let result: Result<AppConfig, _> =
            Figment::from(Serialized::defaults(AppConfig::default()))
                .merge(Toml::string("[reminders]\nsink = 'sms'"))
                .extract();
        assert!(result.is_err());
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
    fn validate_rejects_unparseable_day_start_times() {
        for raw in ["25:00", "5:60", "", "5m", "tomorrow", "024:00", "5 m"] {
            let mut c = valid();
            c.preferences.day_start_time = raw.into();
            assert!(
                matches!(
                    c.validate(),
                    Err(ConfigError::InvalidDayStartTime {
                        field: "preferences.day_start_time"
                    })
                ),
                "{raw} must be rejected as InvalidDayStartTime"
            );
        }
    }

    #[test]
    fn valid_day_start_times_decode_and_validate() {
        for raw in ["05:30", "23:59:59", "05:00"] {
            let config: Result<AppConfig, _> =
                Figment::from(Serialized::from(&valid(), "valid-day-start"))
                    .merge(Toml::string(&format!(
                        "[preferences]\nday_start_time = '{raw}'"
                    )))
                    .extract();
            let mut decoded = config.expect("a well-formed [preferences] table decodes");
            decoded.storage.db_path = Some(PathBuf::from("/tmp/taskboard.db"));
            decoded
                .validate()
                .expect("a parseable time-of-day validates");
            assert_eq!(decoded.preferences.day_start_time, raw);
        }
    }

    #[test]
    fn unknown_key_in_the_preferences_table_is_a_decode_error() {
        let result: Result<AppConfig, _> =
            Figment::from(Serialized::defaults(AppConfig::default()))
                .merge(Toml::string("[preferences]\ntoken = 'x'"))
                .extract();
        assert!(result.is_err());
    }

    #[test]
    fn day_start_time_error_kind_is_the_stable_discriminant() {
        assert_eq!(
            ConfigError::InvalidDayStartTime {
                field: "preferences.day_start_time"
            }
            .kind(),
            "invalid-day-start-time"
        );
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

    // ---- the config write contract (issue #386) ----------------------
    // The mode rule lives here, on the shared atomic writer, not on any
    // command: preserve the existing file's mode (operator territory —
    // the kiosk runbook installs 0644 and a root-run `connect` must not
    // tighten it to 0600 root:root); only a *created* file lands 0600
    // (F12-01/ADR 0012), as does the sidecar at every point before the
    // rename finalizes it.

    /// Asserts the mode bits recorded in the metadata view (never the
    /// access-as-another-user view, which root would defeat).
    #[cfg(unix)]
    fn mode_is(path: &Path) -> u32 {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::metadata(path)
            .expect("file exists")
            .permissions()
            .mode()
            & 0o777
    }

    /// Seeds `path` with `contents` at exactly `mode` (the runbook's
    /// `install -m` equivalent).
    #[cfg(unix)]
    fn seed_with_mode(path: &Path, contents: &str, mode: u32) {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::write(path, contents).expect("seed written");
        let mut perms = std::fs::metadata(path).expect("meta").permissions();
        perms.set_mode(mode);
        std::fs::set_permissions(path, perms).expect("mode set");
    }

    #[cfg(unix)]
    #[test]
    fn a_rewrite_preserves_an_existing_configs_mode() {
        let dir = tempfile::tempdir().expect("tempdir");
        let config = dir.path().join("taskboard.toml");
        seed_with_mode(&config, "[app]\nmode = \"desktop\"\n", 0o644);

        write_config_atomic(&config, "[app]\nmode = \"kiosk\"\n").expect("rewrite");

        assert_eq!(mode_is(&config), 0o644, "the operator's mode survives");
    }

    #[cfg(unix)]
    #[test]
    fn a_rewrite_preserves_a_group_readable_mode() {
        let dir = tempfile::tempdir().expect("tempdir");
        let config = dir.path().join("taskboard.toml");
        seed_with_mode(&config, "[sync]\n", 0o640);

        write_config_atomic(&config, "[sync]\n").expect("rewrite");

        assert_eq!(mode_is(&config), 0o640, "no floor, no widening");
    }

    #[cfg(unix)]
    #[test]
    fn a_rewrite_preserves_owner_only() {
        let dir = tempfile::tempdir().expect("tempdir");
        let config = dir.path().join("taskboard.toml");
        seed_with_mode(&config, "[sync]\n", 0o600);

        write_config_atomic(&config, "[sync]\n").expect("rewrite");

        assert_eq!(mode_is(&config), 0o600, "a tightened install stays tight");
    }

    #[cfg(unix)]
    #[test]
    fn a_created_config_is_owner_only() {
        let dir = tempfile::tempdir().expect("tempdir");
        let config = dir.path().join("taskboard.toml");

        write_config_atomic(&config, "[sync]\n").expect("create");

        assert_eq!(mode_is(&config), 0o600, "no prior mode: fail-safe 0600");
    }

    #[cfg(unix)]
    #[test]
    fn the_preserved_mode_is_masked_to_permission_bits() {
        let dir = tempfile::tempdir().expect("tempdir");
        let config = dir.path().join("taskboard.toml");
        seed_with_mode(&config, "[sync]\n", 0o2644);

        write_config_atomic(&config, "[sync]\n").expect("rewrite");

        assert_eq!(
            mode_is(&config),
            0o644,
            "setgid/setuid/sticky never carry onto a config"
        );
    }

    /// Issue #386's second half: a config that exists but cannot be
    /// opened is `UnreadableFile`, never `Decode` — the content was
    /// never seen, so "configuration rejected" would be a lie. Skips
    /// honestly when even 0000 is readable (euid 0, ACL override).
    #[cfg(unix)]
    #[test]
    fn an_unreadable_config_file_is_typed_not_decode() {
        use std::io::Read as _;
        let dir = tempfile::tempdir().expect("tempdir");
        let config = dir.path().join("taskboard.toml");
        seed_with_mode(&config, "[sync]\n", 0o000);
        if std::fs::File::open(&config).is_ok_and(|mut f| f.read(&mut [0u8]).is_ok()) {
            return; // root (or equivalent): the pin is not exercisable here
        }

        let err = AppConfig::extract_raw(Some(&config)).expect_err("unreadable config is an error");

        assert!(
            matches!(err, ConfigError::UnreadableFile { .. }),
            "an unreadable file is its own kind, got {:?}",
            err.kind()
        );
        assert_eq!(err.kind(), "unreadable-file");
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
            assert!(
                taskboard_domain::ConnectorId::is_kebab_name(name),
                "{name} must pass"
            );
        }
        for name in ["Work", "a_b", "-a", "a-", "a--b", "", "a b", "Ärger"] {
            assert!(
                !taskboard_domain::ConnectorId::is_kebab_name(name),
                "{name} must fail"
            );
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
    fn rotation_section_absent_decodes_to_no_rotation() {
        let config = decode("[kiosk]\n").expect("decodes");
        assert_eq!(
            config.kiosk.rotation, None,
            "an absent [kiosk.rotation] table is no rotation"
        );
        let mut config = config;
        config.storage.db_path = Some(PathBuf::from("/tmp/t.db"));
        config.validate().expect("the absent table validates");
        // The absent-table shape serializes unchanged (the deny/
        // skip contract over the whole `[kiosk]` section).
        let value = serde_json::to_value(config.kiosk).expect("serializes");
        assert_eq!(value, serde_json::json!({}));
    }

    #[test]
    fn rotation_section_decodes_modes_and_dwell() {
        let toml = "[kiosk.rotation]\nmodes = ['today', 'board']\ndwell_secs = 30\n";
        let config = decode(toml).expect("decodes");
        let rotation = config.kiosk.rotation.as_ref().expect("table present");
        assert_eq!(
            rotation.modes,
            vec![KioskRotationMode::Today, KioskRotationMode::Board],
            "declared order is preserved, lowercase spellings decode"
        );
        assert_eq!(rotation.dwell_secs, 30);
    }

    #[test]
    fn rotation_missing_modes_is_a_decode_error() {
        let err =
            decode("[kiosk.rotation]\ndwell_secs = 30\n").expect_err("no silent default mode list");
        assert!(matches!(err, ConfigError::Decode(_)));
    }

    #[test]
    fn rotation_missing_dwell_is_a_decode_error() {
        let err = decode("[kiosk.rotation]\nmodes = ['board', 'today']\n")
            .expect_err("no silent default dwell");
        assert!(matches!(err, ConfigError::Decode(_)));
    }

    #[test]
    fn rotation_unknown_key_is_a_decode_error() {
        let err = decode("[kiosk.rotation]\nmodes = ['board', 'today']\ndwell_sec = 30\n")
            .expect_err("the rotation table's typo guard");
        assert!(matches!(err, ConfigError::Decode(_)));
    }

    #[test]
    fn rotation_needs_two_distinct_modes_is_typed() {
        let mut config = valid();
        config.kiosk.rotation = Some(RotationSection {
            modes: vec![KioskRotationMode::Board],
            dwell_secs: 30,
        });
        match config.validate() {
            Err(ConfigError::InvalidKioskConfig { field }) => {
                assert_eq!(field, "kiosk.rotation.modes");
            }
            other => panic!("expected InvalidKioskConfig, got {other:?}"),
        }
        config.kiosk.rotation = Some(RotationSection {
            modes: vec![KioskRotationMode::Board, KioskRotationMode::Board],
            dwell_secs: 30,
        });
        match config.validate() {
            Err(ConfigError::InvalidKioskConfig { field }) => {
                assert_eq!(field, "kiosk.rotation.modes");
            }
            other => panic!("expected InvalidKioskConfig for duplicates, got {other:?}"),
        }
    }

    #[test]
    fn rotation_dwell_below_the_floor_is_typed() {
        let mut config = valid();
        config.kiosk.rotation = Some(RotationSection {
            modes: vec![KioskRotationMode::Board, KioskRotationMode::Today],
            dwell_secs: KIOSK_ROTATION_MIN_DWELL_SECS - 1,
        });
        match config.validate() {
            Err(ConfigError::InvalidKioskConfig { field }) => {
                assert_eq!(field, "kiosk.rotation.dwell_secs");
            }
            other => panic!("expected InvalidKioskConfig, got {other:?}"),
        }
        // The floor itself passes — a floor, never a ceiling.
        config.kiosk.rotation.as_mut().expect("set").dwell_secs = KIOSK_ROTATION_MIN_DWELL_SECS;
        config.validate().expect("the floor value validates");
    }

    #[test]
    fn resolve_fixed_board_refuses_an_unpinned_multi_board_boot() {
        let config = valid();
        let live = [uuid::Uuid::new_v4(), uuid::Uuid::new_v4()];
        match config.resolve_fixed_board(&live) {
            Err(ConfigError::KioskPinRequired { count }) => assert_eq!(count, 2),
            other => panic!("expected KioskPinRequired, got {other:?}"),
        }
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
            (
                ConfigError::InvalidKioskConfig {
                    field: "kiosk.rotation.dwell_secs",
                },
                "kiosk-setting-invalid",
            ),
            (
                ConfigError::KioskPinRequired { count: 2 },
                "kiosk-pin-required",
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
        assert_eq!(
            ConfigError::UnreadableFile {
                path: PathBuf::from("/x"),
                source: std::io::Error::from(std::io::ErrorKind::PermissionDenied),
            }
            .kind(),
            "unreadable-file"
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

    // ---- the [web] dashboard profile (K11) ---------------------------

    #[test]
    fn web_section_absent_decodes_disabled_and_serializes_nothing() {
        let config = decode("[app]\nmode = 'daemon'\n").expect("decodes");
        assert!(!config.web.enabled, "an absent table never spawns");
        assert_eq!(config.web.bind, DEFAULT_WEB_BIND);
        assert_eq!(config.web.refresh_secs, DEFAULT_WEB_REFRESH_SECS);
        let mut config = config;
        config.storage.db_path = Some(PathBuf::from("/tmp/t.db"));
        config.validate().expect("the absent section validates");
        // The serialization fence: a section-less config renders without
        // any `web` key (the `kiosk.fixed_board` precedent).
        let value = serde_json::to_value(WebSection::default()).expect("serializes");
        assert_eq!(value, serde_json::json!({}));
    }

    #[test]
    fn web_section_present_defaults_to_enabled_loopback_and_30s() {
        let config = decode("[web]\n").expect("a bare table decodes");
        assert!(config.web.enabled, "the table's presence is the opt-in");
        assert_eq!(config.web.bind, DEFAULT_WEB_BIND);
        assert_eq!(config.web.refresh_secs, DEFAULT_WEB_REFRESH_SECS);
        assert_eq!(config.web.token, None);
        let mut config = config;
        config.storage.db_path = Some(PathBuf::from("/tmp/t.db"));
        config.validate().expect("the default table validates");
    }

    #[test]
    fn web_section_decodes_every_field_and_round_trips_the_token() {
        let config = decode(
            "[web]\nenabled = false\nbind = '192.168.1.10:9000'\n\
             refresh_secs = 60\ntoken = 'sekrit'\n",
        )
        .expect("decodes");
        assert!(!config.web.enabled);
        assert_eq!(config.web.bind, "192.168.1.10:9000");
        assert_eq!(config.web.refresh_secs, 60);
        assert_eq!(config.web.token.as_deref(), Some("sekrit"));
    }

    // ---- the [ipc] client-deadline section (ADR 0012, amended) -------

    #[test]
    fn ipc_section_absent_serializes_nothing_and_decodes_default() {
        let config = decode("[app]\nmode = 'daemon'\n").expect("decodes");
        assert_eq!(config.ipc.client_read_timeout_secs, None);
        // The serialization fence: a section-less config renders without
        // any `ipc` key (the `web` precedent).
        let value = serde_json::to_value(IpcSection::default()).expect("serializes");
        assert_eq!(value, serde_json::json!({}));
    }

    #[test]
    fn ipc_section_decodes_snake_case_and_zero() {
        let config = decode("[ipc]\nclient_read_timeout_secs = 45\n").expect("decodes");
        assert_eq!(config.ipc.client_read_timeout_secs, Some(45));
        // `0` is meaningful: the disable value.
        let config = decode("[ipc]\nclient_read_timeout_secs = 0\n").expect("decodes");
        assert_eq!(config.ipc.client_read_timeout_secs, Some(0));
        let value = serde_json::to_value(IpcSection {
            client_read_timeout_secs: Some(45),
        })
        .expect("ser");
        assert_eq!(value, serde_json::json!({"client_read_timeout_secs": 45}));
    }

    #[test]
    fn ipc_section_refuses_unknown_keys() {
        assert!(decode("[ipc]\nread-timeout = 5\n").is_err());
    }

    #[test]
    fn web_refresh_secs_below_the_floor_is_typed() {
        for secs in [0, 1, 4] {
            let mut config = valid();
            config.web.enabled = true;
            config.web.refresh_secs = secs;
            assert!(
                matches!(
                    config.validate(),
                    Err(ConfigError::InvalidWebConfig {
                        field: "web.refresh_secs"
                    })
                ),
                "{secs} must be refused, never clamped"
            );
        }
        let mut config = valid();
        config.web.refresh_secs = WEB_REFRESH_MIN_SECS;
        config.validate().expect("the floor itself passes");
    }

    #[test]
    fn web_bind_that_does_not_parse_is_typed() {
        let mut config = valid();
        config.web.bind = "localhost:8787".into();
        assert!(matches!(
            config.validate(),
            Err(ConfigError::InvalidWebConfig { field: "web.bind" })
        ));
    }

    #[test]
    fn web_refusals_validate_even_while_disabled() {
        // Consistent with the sync/reminders seconds fields: a bad
        // value is a bad value regardless of the enable bit.
        let mut config = valid();
        config.web.enabled = false;
        config.web.refresh_secs = 1;
        assert!(matches!(
            config.validate(),
            Err(ConfigError::InvalidWebConfig { .. })
        ));
    }

    #[test]
    fn unknown_key_in_the_web_table_is_a_decode_error() {
        let err = decode("[web]\ntoekn = 'x'\n").expect_err("typo guard");
        assert!(matches!(err, ConfigError::Decode(_)));
    }

    #[test]
    fn the_web_error_kind_extends_the_stable_discriminants() {
        assert_eq!(
            ConfigError::InvalidWebConfig { field: "web.bind" }.kind(),
            "invalid-web-config"
        );
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
