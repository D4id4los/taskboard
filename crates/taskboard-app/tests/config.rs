// SPDX-License-Identifier: AGPL-3.0-only
//! C-series: config loading through the real figment chain — defaults,
//! file, and environment precedence, typed failure classes.
//!
//! Env-mutating cases serialize behind the process env mutex; unit-level
//! provider tests (in `config.rs`) avoid process env entirely.

use std::path::PathBuf;

use common::{EnvOverride, env_lock, remove_env, set_env};
use taskboard_app::commands::config;
use taskboard_app::config::{
    AppConfig, ConfigError, CredentialStoreKind, DEFAULT_BACKOFF_INITIAL_SECS,
    DEFAULT_BACKOFF_MAX_SECS, DEFAULT_POLL_INTERVAL_SECS,
};

mod common;

fn tempdir() -> tempfile::TempDir {
    tempfile::tempdir().expect("tempdir")
}

/// C1: with no file at the default path (only the two required fields
/// supplied via env), every other default parses — and the db path
/// default resolves under the platform data dir.
#[test]
fn c1_defaults_parse_with_no_file() {
    let _env = env_lock(); // cwd is process-global, like the env
    let dir = tempdir();
    let original = std::env::current_dir().expect("cwd");
    std::env::set_current_dir(dir.path()).expect("chdir");
    let _server = EnvOverride::set(
        "TASKBOARD_NEXTCLOUD__SERVER_URL",
        "https://cloud.example.com",
    );
    let _user = EnvOverride::set("TASKBOARD_NEXTCLOUD__USERNAME", "alice");
    let config = AppConfig::load(None).expect("defaults load");
    std::env::set_current_dir(original).expect("restore cwd");
    let db = config.db_path();
    assert!(db.ends_with("taskboard/taskboard.db"), "db default: {db:?}");
    assert_eq!(config.sync.poll_interval_secs, DEFAULT_POLL_INTERVAL_SECS);
    assert_eq!(config.app.mode, taskboard_app::config::AppMode::Daemon);
    assert_eq!(
        config.nextcloud.credential_store,
        CredentialStoreKind::Keyring
    );
}

/// C2: the toml file sets a value, the environment overrides it — and
/// `--config` picks the file.
#[test]
fn c2_toml_then_env_precedence() {
    let _env = env_lock();
    let dir = tempdir();
    let file = dir.path().join("custom.toml");
    std::fs::write(
        &file,
        "[nextcloud]\nserver_url = 'https://file.example.com'\nusername = 'alice'\n\n[sync]\npoll_interval_secs = 99\n",
    )
    .expect("write toml");

    set_env("TASKBOARD_SYNC__POLL_INTERVAL_SECS", "42");
    let config = AppConfig::load(Some(file.clone())).expect("loads");
    remove_env("TASKBOARD_SYNC__POLL_INTERVAL_SECS");

    assert_eq!(config.sync.poll_interval_secs, 42, "env beats the file");
    assert_eq!(
        config.nextcloud.server_url.as_deref(),
        Some("https://file.example.com"),
        "the file content survives the env override"
    );

    // Without the env override the file's value stands.
    let config = AppConfig::load(Some(file)).expect("loads");
    assert_eq!(config.sync.poll_interval_secs, 99);
}

/// C3: under the resolved `nextcloud` backend the required fields
/// surface one typed failure each; `validate` never panics on a partial
/// config. (Without a backend key the default config resolves
/// `standalone` and validates — C10.)
#[test]
fn c3_required_fields_missing() {
    let mut config = AppConfig::default();
    config.sync.backend = Some(taskboard_app::config::SyncBackend::Nextcloud);
    config.storage.db_path = Some(PathBuf::from("/tmp/x.db"));
    // `nextcloud` untouched: both required fields absent.
    assert!(matches!(
        config.validate(),
        Err(ConfigError::InvalidServerUrl)
    ));
    config.nextcloud.server_url = Some("https://cloud.example.com".into());
    assert!(matches!(config.validate(), Err(ConfigError::EmptyUsername)));
}

/// C4: a secret key inside `[nextcloud]` cannot even decode — the
/// mechanical no-secrets-in-TOML guarantee.
#[test]
fn c4_secret_in_toml_is_a_decode_error() {
    let dir = tempdir();
    let file = dir.path().join("taskboard.toml");
    std::fs::write(
        &file,
        "[nextcloud]\nserver_url = 'https://cloud.example.com'\nusername = 'alice'\ntoken = 'leak-me'\n",
    )
    .expect("write toml");
    let err = AppConfig::load(Some(file)).expect_err("token must not decode");
    assert!(matches!(err, ConfigError::Decode(_)));
}

/// C5: a foreign tier-3 env profile (`TASKBOARD_IT_NEXTCLOUD_*`) does
/// not break extraction — the root table tolerates unknown keys.
#[test]
fn c5_foreign_env_profile_is_ignored() {
    let _env = env_lock();
    let dir = tempdir();
    let file = dir.path().join("taskboard.toml");
    std::fs::write(
        &file,
        "[nextcloud]\nserver_url = 'https://cloud.example.com'\nusername = 'alice'\n",
    )
    .expect("write toml");
    set_env("TASKBOARD_IT_NEXTCLOUD_URL", "https://it.example.com");
    let config = AppConfig::load(Some(file)).expect("foreign profile ignored");
    assert_eq!(
        config.nextcloud.server_url.as_deref(),
        Some("https://cloud.example.com")
    );
}

/// C6: invalid values map to their typed classes (bad URL, zero
/// interval).
#[test]
fn c6_invalid_values_are_typed() {
    let _env = env_lock(); // figment reads process env during load
    let dir = tempdir();
    for (body, expected) in [
        (
            "[nextcloud]\nusername = 'alice'\nserver_url = 'nope'",
            "url",
        ),
        (
            "[nextcloud]\nusername = 'alice'\nserver_url = 'https://cloud.example.com'\n\n[sync]\npoll_interval_secs = 0",
            "duration",
        ),
        (
            "[nextcloud]\nusername = 'alice'\nserver_url = 'https://cloud.example.com'\n\n[sync]\nbackoff_initial_secs = 301",
            "duration",
        ),
    ] {
        let file = dir.path().join("taskboard.toml");
        std::fs::write(&file, format!("{body}\n")).expect("write toml");
        let result = AppConfig::load(Some(file.clone()));
        let typed = match result {
            Err(ConfigError::InvalidServerUrl) => "url",
            Err(ConfigError::InvalidDuration { .. }) => "duration",
            other => panic!("unexpected outcome for {body}: {other:?}"),
        };
        assert_eq!(typed, expected);
    }
}

/// C7: an explicitly requested missing file is `FileNotFound`, while an
/// absent *default-path* file is not an error (C1).
#[test]
fn c7_explicit_missing_file_is_file_not_found() {
    let err = AppConfig::load(Some(PathBuf::from("/nonexistent/taskboard.toml")))
        .expect_err("explicit missing file");
    assert!(matches!(err, ConfigError::FileNotFound(_)));
}

/// C8: `sync_actor_config()` maps the seconds fields onto the actor's
/// knobs (the `spawn_sync_actor` doc example's values are the defaults).
#[test]
fn c8_sync_actor_config_mapping() {
    let mut config = AppConfig::default();
    config.nextcloud.server_url = Some("https://cloud.example.com".into());
    config.nextcloud.username = Some("alice".into());
    config.storage.db_path = Some(PathBuf::from("/tmp/x.db"));
    config.validate().expect("valid");
    let cfg = config.sync_actor_config();
    assert_eq!(cfg.poll_interval.as_secs(), DEFAULT_POLL_INTERVAL_SECS);
    assert_eq!(cfg.backoff_initial.as_secs(), DEFAULT_BACKOFF_INITIAL_SECS);
    assert_eq!(cfg.backoff_max.as_secs(), DEFAULT_BACKOFF_MAX_SECS);
}

/// C9 (serde round trip lives in `config.rs` units): the
/// backoff invariant `backoff_initial <= backoff_max` is enforced.
#[test]
fn c9_backoff_ordering_enforced() {
    let mut config = AppConfig::default();
    config.nextcloud.server_url = Some("https://cloud.example.com".into());
    config.nextcloud.username = Some("alice".into());
    config.storage.db_path = Some(PathBuf::from("/tmp/x.db"));
    config.sync.backoff_initial_secs = 301;
    assert!(matches!(
        config.validate(),
        Err(ConfigError::InvalidDuration { .. })
    ));
}

/// C10: backend auto-resolution (ADR 0010) — no `[nextcloud] server_url`
/// resolves `standalone`, a present one resolves `nextcloud`, through
/// both the file and the env provider.
#[test]
fn c10_backend_auto_resolves_on_server_url() {
    let _env = env_lock();
    let mut config = AppConfig::default();
    config.storage.db_path = Some(PathBuf::from("/tmp/x.db"));
    config.validate().expect("stand-alone default validates");
    assert_eq!(
        config.sync_backend(),
        taskboard_app::config::SyncBackend::Standalone
    );

    config.nextcloud.server_url = Some("https://cloud.example.com".into());
    config.nextcloud.username = Some("alice".into());
    assert_eq!(
        config.sync_backend(),
        taskboard_app::config::SyncBackend::Nextcloud
    );

    // The env provider keys the same rule.
    let dir = tempdir();
    let file = dir.path().join("taskboard.toml");
    std::fs::write(&file, "").expect("write empty toml");
    let _server = EnvOverride::set("TASKBOARD_NEXTCLOUD__SERVER_URL", "https://env.example.com");
    let _user = EnvOverride::set("TASKBOARD_NEXTCLOUD__USERNAME", "alice");
    let config = AppConfig::load(Some(file)).expect("loads");
    assert_eq!(
        config.sync_backend(),
        taskboard_app::config::SyncBackend::Nextcloud
    );
}

/// C11: explicit `backend = "standalone"` keeps a `[nextcloud]` table
/// inert — missing credentials validate — but a *present* malformed URL
/// stays a loud typo guard.
#[test]
fn c11_explicit_standalone_inerts_nextcloud_but_keeps_typo_guard() {
    let mut config = AppConfig::default();
    config.sync.backend = Some(taskboard_app::config::SyncBackend::Standalone);
    config.storage.db_path = Some(PathBuf::from("/tmp/x.db"));
    config.validate().expect("standalone needs no credentials");

    config.nextcloud.server_url = Some("not a url".into());
    assert!(matches!(
        config.validate(),
        Err(ConfigError::InvalidServerUrl)
    ));
}

/// C12: the explicit `nextcloud` backend requires both fields — a
/// `[sync] backend` alone without a server stays typed-rejected.
#[test]
fn c12_explicit_nextcloud_requires_credentials() {
    let mut config = AppConfig::default();
    config.sync.backend = Some(taskboard_app::config::SyncBackend::Nextcloud);
    config.storage.db_path = Some(PathBuf::from("/tmp/x.db"));
    assert!(matches!(
        config.validate(),
        Err(ConfigError::InvalidServerUrl)
    ));
    config.nextcloud.server_url = Some("https://cloud.example.com".into());
    assert!(matches!(config.validate(), Err(ConfigError::EmptyUsername)));
}

/// C13: a typo'd backend value cannot decode (`deny_unknown_fields`
/// plus the kebab-case enum).
#[test]
fn c13_typoed_backend_value_is_a_decode_error() {
    let dir = tempdir();
    let file = dir.path().join("taskboard.toml");
    std::fs::write(
        &file,
        "[nextcloud]\nserver_url = 'https://cloud.example.com'\nusername = 'alice'\n\n[sync]\nbackend = 'none'\n",
    )
    .expect("write toml");
    let err = AppConfig::load(Some(file)).expect_err("typo must not decode");
    assert!(matches!(err, ConfigError::Decode(_)));
}

/// C10: `config print` — valid config prints (exit 0) with the resolved
/// path and backend; a decodes-but-invalid config reports `valid: false`
/// plus the typed `invalid_kind`, still exit 0; only an undecodable file
/// exits 1.
#[test]
fn c10_config_print_reports_validity_in_the_payload() {
    // The env is process-global and other C-series tests override
    // `TASKBOARD_*`: hold the lock so the payload reflects the file.
    let _env = env_lock();
    let dir = tempdir();
    std::fs::write(
        dir.path().join("valid.toml"),
        "[nextcloud]\nserver_url = 'https://cloud.example.com'\nusername = 'alice'\n\
         credential_store = 'env'\n[storage]\ndb_path = '/tmp/c10.db'\n",
    )
    .expect("write valid");
    std::fs::write(
        dir.path().join("invalid.toml"),
        "[nextcloud]\nserver_url = 'not a url'\nusername = 'alice'\ncredential_store = 'env'\n\
         [storage]\ndb_path = '/tmp/c10.db'\n",
    )
    .expect("write invalid");
    std::fs::write(dir.path().join("garbage.toml"), "= not toml =").expect("write garbage");

    let view = config::print(Some(dir.path().join("valid.toml"))).expect("valid config");
    assert!(view.valid);
    assert_eq!(view.invalid_kind, None);
    assert_eq!(
        view.backend, "nextcloud",
        "server_url present: auto resolves"
    );
    assert_eq!(
        view.config_path.as_deref(),
        Some(dir.path().join("valid.toml").to_str().expect("utf8"))
    );

    let view = config::print(Some(dir.path().join("invalid.toml"))).expect("invalid still prints");
    assert!(!view.valid);
    assert_eq!(view.invalid_kind.as_deref(), Some("invalid-server-url"));

    let err = config::print(Some(dir.path().join("garbage.toml"))).expect_err("garbage exits 1");
    assert_eq!(err.error_kind(), "decode");
}

/// C11: `config print` reflects the env overlay (the effective
/// configuration, not just the file).
#[test]
fn c11_config_print_includes_the_env_overlay() {
    let dir = tempdir();
    std::fs::write(dir.path().join("empty.toml"), "").expect("write empty");
    let _guard = env_lock();
    let _server = EnvOverride::set("TASKBOARD_NEXTCLOUD__SERVER_URL", "https://env.example.com");
    let _user = EnvOverride::set("TASKBOARD_NEXTCLOUD__USERNAME", "alice");
    let view = config::print(Some(dir.path().join("empty.toml"))).expect("print");
    assert_eq!(
        view.config.nextcloud.server_url.as_deref(),
        Some("https://env.example.com")
    );
    assert_eq!(view.backend, "nextcloud");
}

/// C12: the `[reminders]` table — an absent table keeps the figment
/// defaults, a partial TOML override merges per-key, and the env
/// provider overrides all three keys (bool/int/string coercion).
#[test]
fn c12_reminders_table_defaults_merge_and_env_override() {
    let _env = env_lock();
    let dir = tempdir();
    let file = dir.path().join("reminders.toml");

    // Absent table: defaults.
    std::fs::write(&file, "").expect("write empty toml");
    let config = AppConfig::load(Some(file.clone())).expect("loads");
    assert!(config.reminders.enabled);
    assert_eq!(
        config.reminders.tick_secs,
        taskboard_app::config::DEFAULT_REMINDER_TICK_SECS
    );
    assert_eq!(
        config.reminders.default_before,
        taskboard_app::config::DEFAULT_REMINDER_BEFORE
    );

    // Partial TOML override: an absent key keeps its default.
    std::fs::write(&file, "[reminders]\ntick_secs = 90\n").expect("write toml");
    let config = AppConfig::load(Some(file.clone())).expect("loads");
    assert!(config.reminders.enabled, "absent key keeps the default");
    assert_eq!(config.reminders.tick_secs, 90);

    // Env overrides (figment coerces bool/int; the duration stays a
    // string so file and env decode identically).
    let _enabled = EnvOverride::set("TASKBOARD_REMINDERS__ENABLED", "false");
    let _tick = EnvOverride::set("TASKBOARD_REMINDERS__TICK_SECS", "5");
    let _before = EnvOverride::set("TASKBOARD_REMINDERS__DEFAULT_BEFORE", "1h30m");
    let config = AppConfig::load(Some(file)).expect("loads");
    assert!(!config.reminders.enabled);
    assert_eq!(config.reminders.tick_secs, 5);
    assert_eq!(config.reminders.default_before, "1h30m");
}

/// C13: the strict-table rule holds for `[reminders]`, and semantic
/// violations surface as the typed `invalid-duration` kind through
/// `load`.
#[test]
fn c13_reminders_strict_table_and_typed_durations() {
    let _env = env_lock();
    let dir = tempdir();
    let file = dir.path().join("reminders.toml");

    std::fs::write(&file, "[reminders]\ntoken = 'x'\n").expect("write toml");
    let err = AppConfig::load(Some(file.clone())).expect_err("unknown key is a decode error");
    assert_eq!(err.kind(), "decode");

    std::fs::write(&file, "[reminders]\ndefault_before = '5min'\n").expect("write toml");
    let err = AppConfig::load(Some(file.clone())).expect_err("bad duration");
    assert_eq!(err.kind(), "invalid-duration");

    std::fs::write(&file, "[reminders]\ntick_secs = 0\n").expect("write toml");
    let err = AppConfig::load(Some(file)).expect_err("zero tick");
    assert_eq!(err.kind(), "invalid-duration");
}

/// C14: the `[reminders]` → `ReminderEngineConfig` mapping through the
/// real chain — enabled passthrough, tick seconds, and the sign flip
/// (`"0s"` maps to the at-due offset).
#[test]
fn c14_reminders_engine_config_mapping() {
    let mut config = AppConfig::default();
    config.reminders.tick_secs = 12;
    config.reminders.default_before = "2m".into();
    config.storage.db_path = Some(PathBuf::from("/tmp/x.db"));
    config.validate().expect("validates");
    let engine = config.reminder_engine_config();
    assert!(engine.enabled);
    assert_eq!(engine.tick, std::time::Duration::from_secs(12));
    assert_eq!(
        engine.defaults.default_before.as_duration().num_seconds(),
        -120
    );

    config.reminders.default_before = "0s".into();
    assert_eq!(
        config.reminder_engine_config().defaults.default_before,
        taskboard_domain::ReminderOffset::at_due()
    );
}

/// C15: the `[ui]` table through the real chain — env override works
/// (`TASKBOARD_UI__SCALE`), defaults round-trip at 1.0.
#[test]
fn c15_ui_scale_env_override() {
    let _env = env_lock();
    let _scale = EnvOverride::set("TASKBOARD_UI__SCALE", "1.5");
    let config = AppConfig::load(None).expect("loads with the override");
    assert!(
        (config.ui.scale - 1.5).abs() < f64::EPSILON,
        "env override reaches ui.scale"
    );
    remove_env("TASKBOARD_UI__SCALE");
    let defaults = AppConfig::default();
    assert!((defaults.ui.scale - taskboard_app::config::DEFAULT_UI_SCALE).abs() < f64::EPSILON);
}

/// C16: the `config path` diagnosis is pure resolution — an explicit
/// `--config` is reported for reads and writes alike, existence never
/// checked (that is `config print`/`load`'s typed failure, not this
/// verb's).
#[test]
fn c16_explicit_path_is_reported_for_reads_and_writes() {
    let _env = env_lock();
    remove_env("TASKBOARD_CONFIG");
    let dir = tempdir();
    let file = dir.path().join("does-not-exist.toml");
    let view = config::path(Some(file.clone()));
    assert_eq!(view.config_path.map(PathBuf::from), Some(file.clone()));
    assert_eq!(PathBuf::from(&view.write_path), file);
}

/// C16: `TASKBOARD_CONFIG` names both answers — the read resolution
/// and the write target follow the same env binding.
#[test]
fn c16_env_override_names_both_answers() {
    let _env = env_lock();
    let dir = tempdir();
    let file = dir.path().join("env-named.toml");
    set_env("TASKBOARD_CONFIG", file.to_str().expect("utf-8 path"));
    let view = config::path(None);
    remove_env("TASKBOARD_CONFIG");
    assert_eq!(view.config_path.map(PathBuf::from), Some(file.clone()));
    assert_eq!(PathBuf::from(&view.write_path), file);
}

/// C16: the fresh-machine divergence the verb exists to show — reads
/// resolve to nothing, but `connect`/`login` would write the XDG
/// candidate (whose directory the writer creates).
#[test]
fn c16_fresh_machine_diverges_read_none_write_xdg() {
    let _env = env_lock();
    remove_env("TASKBOARD_CONFIG");
    let xdg = tempdir();
    let cwd = tempdir();
    let original = std::env::current_dir().expect("cwd");
    std::env::set_current_dir(cwd.path()).expect("chdir");
    let _xdg = EnvOverride::set("XDG_CONFIG_HOME", xdg.path().to_str().expect("utf-8"));
    let view = config::path(None);
    std::env::set_current_dir(original).expect("restore cwd");
    assert_eq!(view.config_path, None);
    assert_eq!(
        PathBuf::from(&view.write_path),
        xdg.path().join("taskboard").join("taskboard.toml")
    );
}

/// C16: an existing CWD install keeps its file as the write target —
/// the write fallback's row (`config_path` stays `None`: the CWD file
/// is not a read resolution).
#[test]
fn c16_existing_cwd_install_is_the_write_target() {
    let _env = env_lock();
    remove_env("TASKBOARD_CONFIG");
    let xdg = tempdir();
    let cwd = tempdir();
    std::fs::write(cwd.path().join("taskboard.toml"), "").expect("cwd install");
    let original = std::env::current_dir().expect("cwd");
    std::env::set_current_dir(cwd.path()).expect("chdir");
    let _xdg = EnvOverride::set("XDG_CONFIG_HOME", xdg.path().to_str().expect("utf-8"));
    let view = config::path(None);
    std::env::set_current_dir(original).expect("restore cwd");
    assert_eq!(view.config_path, None);
    // The CWD default's own spelling: an existing install keeps its
    // file (`DEFAULT_CONFIG_PATH`, resolved against the process CWD).
    assert_eq!(
        PathBuf::from(&view.write_path),
        PathBuf::from(taskboard_app::config::DEFAULT_CONFIG_PATH)
    );
}

/// The `[ipc]` section decodes `snake_case` and the env lane overrides it
/// (`TASKBOARD_IPC__CLIENT_READ_TIMEOUT_SECS`, figment for free).
#[test]
fn the_ipc_section_decodes_and_the_env_lane_overrides() {
    let _env = env_lock();
    let dir = tempdir();
    let file = dir.path().join("ipc.toml");
    std::fs::write(&file, "[ipc]\nclient_read_timeout_secs = 7\n").expect("write toml");
    let config = AppConfig::load(Some(file.clone())).expect("loads");
    assert_eq!(config.ipc.client_read_timeout_secs, Some(7));

    remove_env("TASKBOARD_IPC__CLIENT_READ_TIMEOUT_SECS");
    set_env("TASKBOARD_IPC__CLIENT_READ_TIMEOUT_SECS", "3");
    let config = AppConfig::load(Some(file.clone())).expect("loads");
    remove_env("TASKBOARD_IPC__CLIENT_READ_TIMEOUT_SECS");
    assert_eq!(
        config.ipc.client_read_timeout_secs,
        Some(3),
        "env beats file"
    );

    // `0` decodes too — the disable value.
    std::fs::write(&file, "[ipc]\nclient_read_timeout_secs = 0\n").expect("write toml");
    let config = AppConfig::load(Some(file)).expect("loads");
    assert_eq!(config.ipc.client_read_timeout_secs, Some(0));
}
