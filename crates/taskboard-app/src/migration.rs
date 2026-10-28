// SPDX-License-Identifier: AGPL-3.0-only

//! Legacy single-server config auto-migration (round-3 D21): a file
//! carrying the scalar `[nextcloud]` identity world gains one
//! `[[sync.connectors]]` entry named [`crate::connectors::
//! DEFAULT_CONNECTOR_NAME`] — on serving boot, at the connect
//! ceremony's entry, and ahead of every settings write (plan
//! `2026-10-26-ui-r3-legacy-config-migration-plan.md`).
//!
//! The trigger and the transform are one total function over the
//! `toml_edit` **document** (file bytes), never the figment-merged
//! config: `TASKBOARD_*` env can inject the identity keys, and a
//! merged-view trigger would write the array beside env-injected
//! identity keys — the next load is the typed
//! `NextcloudTableWithConnectors` rejection and the boot dies. The
//! doc-based trigger migrates only when the identity *lives in the
//! file* (plan §3, the recorded corner).
//!
//! The [`AppConfig`] mirror exists for the boot persist-failure
//! fallback only: when the file cannot be rewritten (read-only media),
//! the app still runs the migrated shape in memory.
//!
//! Secrets are never read and never written: the migration is
//! config-file-only, and the credential store stays keyed
//! server+username.

use std::path::{Path, PathBuf};

use crate::config::{AppConfig, ConnectorCfg, SyncBackend};

/// The backup suffix appended to the config file's name, in the same
/// directory (`taskboard.toml.bak-legacy`). Written only when the
/// document mutates.
const BACKUP_SUFFIX: &str = ".bak-legacy";

/// The log-safe host of a server URL (r2 D34): scheme- and
/// userinfo-stripped, up to the first path/query/fragment character.
/// Tracing fields carry host and name only — never the full URL,
/// never credentials. Relocated here from `ui_requests` so the
/// migration log honours the same rule (plan §1).
pub(crate) fn url_host(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let rest = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    rest.rsplit_once('@').map_or(rest, |(_, host)| host)
}

/// The pure trigger + transform over the parsed config document: all
/// trigger preconditions are checked before any mutation, so `false`
/// means byte-untouched-by-construction. Returns whether the document
/// was mutated (the legacy shape migrated).
///
/// Trigger: no `sync.connectors` of any rendering; a `[nextcloud]`
/// table with non-empty `server_url` **and** `username` strings (a
/// half-configured file stays loud — today's boot refusal is
/// preserved); `[sync] backend` absent or `"nextcloud"` (an explicit
/// `standalone` install is not the legacy single-server config —
/// migrating it would flip standalone semantics).
pub(crate) fn migrate_document(doc: &mut toml_edit::DocumentMut) -> bool {
    // Array world (or anything parked at that key): never migrated —
    // hand-made mixes are the validator's job.
    if doc
        .get("sync")
        .and_then(|sync| sync.get("connectors"))
        .is_some()
    {
        return false;
    }
    // A non-table `[sync]` is garbage the validator rejects; not ours.
    if doc
        .get("sync")
        .is_some_and(|sync| sync.as_table().is_none())
    {
        return false;
    }
    let backend_ok = doc
        .get("sync")
        .and_then(|sync| sync.get("backend"))
        .is_none_or(|backend| backend.as_str() == Some("nextcloud"));
    if !backend_ok {
        return false;
    }
    let Some(nextcloud) = doc.get("nextcloud").and_then(toml_edit::Item::as_table) else {
        return false;
    };
    let identity = match (
        non_empty(nextcloud.get("server_url")),
        non_empty(nextcloud.get("username")),
    ) {
        (Some(server), Some(user)) => (server.to_owned(), user.to_owned()),
        _ => return false,
    };
    migrate_document_inner(doc, identity)
}

fn non_empty(item: Option<&toml_edit::Item>) -> Option<&str> {
    item.and_then(toml_edit::Item::as_str)
        .filter(|value| !value.trim().is_empty())
}

/// The one-pass transform, preconditions already held: fold the loud
/// scalar `backend` word into the entry's own field (keeping it beside
/// the array is `ConnectorShapeMixed` — the upgrade would brick boot),
/// strip the identity keys (a `credential_store` value survives —
/// process-global, stripping the table would flip the store kind),
/// append the one entry.
fn migrate_document_inner(doc: &mut toml_edit::DocumentMut, identity: (String, String)) -> bool {
    let entry = ConnectorCfg {
        name: crate::connectors::DEFAULT_CONNECTOR_NAME.to_owned(),
        backend: SyncBackend::Nextcloud,
        server_url: identity.0,
        username: identity.1,
    };
    // Drop `sync.backend` first: the `[sync]` table either already
    // exists (its decor is preserved) or is created fresh below.
    if let Some(sync) = doc.get_mut("sync").and_then(toml_edit::Item::as_table_mut) {
        sync.remove("backend");
    }
    let sync = doc
        .entry("sync")
        .or_insert(toml_edit::Item::Table(toml_edit::Table::new()))
        .as_table_mut()
        .expect("[sync] table-ness was a trigger precondition");
    let mut connectors = toml_edit::ArrayOfTables::new();
    connectors.push(crate::connections::connector_entry_table(&entry));
    sync.insert("connectors", toml_edit::Item::ArrayOfTables(connectors));

    if let Some(nextcloud) = doc
        .get_mut("nextcloud")
        .and_then(toml_edit::Item::as_table_mut)
    {
        nextcloud.remove("server_url");
        nextcloud.remove("username");
        if nextcloud.is_empty() {
            doc.remove("nextcloud");
        }
    }
    true
}

/// The in-memory mirror of [`migrate_document`] over the merged
/// config — the persist-failure fallback (plan §2.1), never a file
/// write. Same trigger, same shape; `validate()` after a `true`
/// return is the caller's premise ([`migrate_config_file`]'s callers
/// ladder on it).
pub(crate) fn migrate_config_in_memory(config: &mut AppConfig) -> bool {
    if config.sync.connectors.is_some() {
        return false;
    }
    if config
        .sync
        .backend
        .is_some_and(|backend| backend == SyncBackend::Standalone)
    {
        return false;
    }
    let (Some(server), Some(user)) = (
        config.nextcloud.server_url.as_deref(),
        config.nextcloud.username.as_deref(),
    ) else {
        return false;
    };
    if server.trim().is_empty() || user.trim().is_empty() {
        return false;
    }
    config.sync.connectors = Some(vec![ConnectorCfg {
        name: crate::connectors::DEFAULT_CONNECTOR_NAME.to_owned(),
        backend: SyncBackend::Nextcloud,
        server_url: server.to_owned(),
        username: user.to_owned(),
    }]);
    config.sync.backend = None;
    config.nextcloud.server_url = None;
    config.nextcloud.username = None;
    true
}

/// Migrates the config file at `path` when it carries the legacy
/// scalar shape: parse (absent file → `Ok(false)`; unparseable → the
/// existing `InvalidData` io-error, unchanged from today's decode
/// failure) → [`migrate_document`] → on a mutation: backup (best
/// effort, [`BACKUP_SUFFIX`]) then the atomic write (comment- and
/// mode-preserving). Returns whether the file was rewritten.
///
/// # Errors
///
/// [`std::io::Error`] — read, parse (as `InvalidData`), and write
/// failures, typed; the backup copy failure is only *warned* (the
/// transform is comment-preserving and the payload is two keys — a
/// failed backup must not block a working upgrade).
pub(crate) fn migrate_config_file(path: &Path) -> std::io::Result<bool> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(err) => return Err(err),
    };
    let mut doc = text
        .parse::<toml_edit::DocumentMut>()
        .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err))?;
    if !migrate_document(&mut doc) {
        return Ok(false);
    }
    let backup = backup_path(path);
    if let Err(err) = std::fs::copy(path, &backup) {
        tracing::warn!(
            error = %err,
            "legacy config migration: the backup copy failed; continuing"
        );
    }
    crate::config::write_config_atomic(path, doc.to_string().as_str())?;
    Ok(true)
}

fn backup_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().map_or_else(
        || std::ffi::OsString::from("taskboard.toml"),
        std::ffi::OsString::from,
    );
    name.push(BACKUP_SUFFIX);
    path.with_file_name(name)
}

/// The serving-boot wrapper (plan §2.1): load, then migrate the file
/// when it is legacy-shaped. Read-only observations (`status`,
/// `daemon stop`, the IPC client probe) and `config print` never come
/// through here — a read command must not mutate the operator's file.
///
/// Ladder:
/// - no mutation → the loaded config as-is;
/// - mutation + successful re-load → the migrated config (info log:
///   names and hosts only);
/// - mutation + re-load failure (the both-file-and-env-identity
///   corner: env keeps injecting identity keys beside the array) →
///   the backup is restored best-effort and the original scalar
///   config is served, loud in the log; a restore-reload that also
///   fails propagates typed — the only honest terminal when neither
///   shape can be served;
/// - the write itself failing (read-only media, full disk) → the
///   in-memory mirror, served when it validates; else the original
///   scalar config (the app runs the legacy world; the settings
///   writes retry the migration).
///
/// # Errors
///
/// [`crate::config::ConfigError`] — the load's own typed failures,
/// and the terminal propagate of the corner ladder above.
pub fn load_migrated(explicit: Option<PathBuf>) -> Result<AppConfig, crate::config::ConfigError> {
    let config = AppConfig::load(explicit.clone())?;
    let Some(path) = AppConfig::config_path(explicit.clone()) else {
        return Ok(config);
    };
    match migrate_config_file(&path) {
        Ok(false) => Ok(config),
        Ok(true) => match AppConfig::load(Some(path.clone())) {
            Ok(migrated) => {
                log_migrated(&migrated);
                Ok(migrated)
            }
            Err(reload_err) => {
                restore_backup(&path);
                match AppConfig::load(explicit) {
                    Ok(scalar) => {
                        tracing::warn!(
                            error = %reload_err,
                            "the migrated config failed to reload (identity also \
                             injected via env?); the backup was restored and the \
                             legacy config is served"
                        );
                        Ok(scalar)
                    }
                    Err(err) => Err(err),
                }
            }
        },
        Err(persist_err) => {
            tracing::warn!(
                error = %persist_err,
                "legacy config migration could not persist; trying the in-memory mirror"
            );
            let mut mirrored = config.clone();
            if migrate_config_in_memory(&mut mirrored) && mirrored.validate().is_ok() {
                tracing::warn!(
                    "the legacy single-server config runs migrated in memory; \
                     the file keeps the legacy shape until a writable moment \
                     (the next settings write completes the migration)"
                );
                return Ok(mirrored);
            }
            tracing::warn!("the legacy config is served as-is; the migration was not possible");
            Ok(config)
        }
    }
}

fn log_migrated(config: &AppConfig) {
    for entry in config.sync.connectors.iter().flatten() {
        tracing::info!(
            connector = %entry.name,
            host = %url_host(&entry.server_url),
            "legacy single-server config migrated to sync.connectors (backup: \
             <config>.bak-legacy)"
        );
    }
}

fn restore_backup(path: &Path) {
    let backup = backup_path(path);
    match std::fs::copy(&backup, path) {
        Ok(_) => tracing::warn!("the legacy config backup was restored"),
        Err(err) => tracing::warn!(
            error = %err,
            "the legacy config backup could not be restored"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LEGACY_FILE: &str = "# hand-tuned\n\
        [storage]\ndb_path = '/tmp/t.db'\n\
        [ui]\nscale = 1.5\n[ui.keymap]\npreset = \"vi\"\n\
        [preferences]\nday_start_time = \"07:30\"\n\
        [nextcloud]\nserver_url = \"https://cloud.example\"\nusername = \"alice\"\n\
        credential_store = \"file\"\n";

    /// r2 D34 — the log-safe host extraction (relocated with the fn):
    /// scheme, userinfo, path, query, and fragment all stripped; a
    /// schemeless URL keeps its authority.
    #[test]
    fn url_host_strips_everything_but_the_authority() {
        assert_eq!(
            url_host("https://cloud.example.com/nc"),
            "cloud.example.com"
        );
        assert_eq!(
            url_host("http://user:pw@host.example:8080/?x=1"),
            "host.example:8080"
        );
        assert_eq!(url_host("https://a.example#frag"), "a.example");
        assert_eq!(url_host("host.example/path"), "host.example");
        assert_eq!(url_host("https://bare.example"), "bare.example");
    }

    /// M1 — the hand-tuned legacy file round-trips: exactly one entry
    /// (the default name, backend folded in), identity keys gone, the
    /// process-global `credential_store` kept, comments and unrelated
    /// tables preserved; idempotent (second application is a no-op,
    /// bytes equal).
    #[test]
    fn m1_the_document_round_trip_is_complete_and_idempotent() {
        let mut doc = LEGACY_FILE
            .parse::<toml_edit::DocumentMut>()
            .expect("parses");
        assert!(migrate_document(&mut doc), "the legacy shape triggers");
        let once = doc.to_string();

        assert!(once.contains("# hand-tuned"), "comment survives");
        assert!(once.contains("scale = 1.5"), "[ui] survives");
        assert!(once.contains("preset = \"vi\""), "keymap survives");
        assert!(
            once.contains("day_start_time = \"07:30\""),
            "preferences survive"
        );
        assert!(
            once.contains("credential_store = \"file\""),
            "store kind kept"
        );
        let reloaded = once.parse::<toml_edit::DocumentMut>().expect("re-parses");
        let entry = reloaded
            .get("sync")
            .and_then(|sync| sync.get("connectors"))
            .and_then(toml_edit::Item::as_array_of_tables)
            .expect("the array exists");
        assert_eq!(entry.len(), 1, "exactly one entry");
        let first = entry.iter().next().expect("the one entry");
        assert_eq!(
            first.get("name").and_then(toml_edit::Item::as_str),
            Some(crate::connectors::DEFAULT_CONNECTOR_NAME),
            "the load-bearing id (DB bindings decode through it)"
        );
        assert_eq!(
            first.get("backend").and_then(toml_edit::Item::as_str),
            Some("nextcloud")
        );
        assert_eq!(
            first.get("server_url").and_then(toml_edit::Item::as_str),
            Some("https://cloud.example")
        );
        assert_eq!(
            first.get("username").and_then(toml_edit::Item::as_str),
            Some("alice")
        );
        assert!(
            reloaded
                .get("nextcloud")
                .and_then(toml_edit::Item::as_table)
                .is_some_and(|t| t.len() == 1 && t.contains_key("credential_store")),
            "the [nextcloud] husk keeps only credential_store"
        );
        assert!(
            !reloaded
                .get("sync")
                .and_then(|sync| sync.get("backend"))
                .is_some(),
            "the scalar backend word is folded into the entry"
        );

        // Idempotence: the migrated shape no longer triggers.
        assert!(!migrate_document(&mut doc), "second run is a no-op");
        assert_eq!(doc.to_string(), once, "bytes untouched by the second run");
    }

    /// M2 — the property: migrating twice is identity, bytes and
    /// in-memory mirror alike, over generated legacy-shaped inputs.
    mod property {
        use super::super::*;
        use proptest::prelude::*;

        fn legacy_doc(server: &str, user: &str, backend: Option<&str>) -> String {
            let backend_line =
                backend.map_or(String::new(), |word| format!("backend = \"{word}\"\n"));
            format!("[nextcloud]\n{backend_line}server_url = {server:?}\nusername = {user:?}\n")
        }

        proptest! {
            #[test]
            fn double_document_migration_is_identity(
                server in "[a-z]+\\.[a-z]{2,}",
                user in "[a-z0-9-]{1,12}",
                backend in proptest::option::of(Just("nextcloud")),
            ) {
                let text = legacy_doc(&server, &user, backend);
                let mut once = text.parse::<toml_edit::DocumentMut>().expect("parses");
                prop_assert!(migrate_document(&mut once));
                let once_bytes = once.to_string();
                let mut twice = once_bytes.parse::<toml_edit::DocumentMut>().expect("parses");
                prop_assert!(!migrate_document(&mut twice));
                prop_assert_eq!(twice.to_string(), once_bytes);
            }

            #[test]
            fn double_in_memory_migration_is_identity(
                server in "https://[a-z]+\\.[a-z]{2,}",
                user in "[a-z0-9-]{1,12}",
            ) {
                let make = || {
                    let mut config = AppConfig::default();
                    config.nextcloud.server_url = Some(server.clone());
                    config.nextcloud.username = Some(user.clone());
                    config
                };
                let mut once = make();
                prop_assert!(migrate_config_in_memory(&mut once));
                let mut twice = once.clone();
                prop_assert!(!migrate_config_in_memory(&mut twice));
                prop_assert_eq!(format!("{once:?}"), format!("{twice:?}"));
            }
        }
    }

    /// M3 — the trigger matrix: no identity, an empty username, a
    /// missing username, an explicit `standalone` backend → `false`;
    /// an explicit `nextcloud` backend → `true`; an array already
    /// present → `false` (hand-made mixes are the validator's job).
    #[test]
    fn m3_the_trigger_matrix_refuses_every_non_legacy_shape() {
        let mutates = |text: &str| {
            let mut doc = text.parse::<toml_edit::DocumentMut>().expect("parses");
            migrate_document(&mut doc)
        };
        assert!(
            !mutates("[nextcloud]\nusername = \"alice\"\n"),
            "no server_url"
        );
        assert!(
            !mutates("[nextcloud]\nserver_url = \"https://c.example\"\n"),
            "no username"
        );
        assert!(
            !mutates("[nextcloud]\nserver_url = \"https://c.example\"\nusername = \"\"\n"),
            "empty username"
        );
        assert!(
            !mutates(
                "[sync]\nbackend = \"standalone\"\n\
             [nextcloud]\nserver_url = \"https://c.example\"\nusername = \"alice\"\n",
            ),
            "explicit standalone is not the legacy world"
        );
        assert!(
            mutates(
                "[sync]\nbackend = \"nextcloud\"\n\
             [nextcloud]\nserver_url = \"https://c.example\"\nusername = \"alice\"\n",
            ),
            "explicit nextcloud migrates"
        );
        assert!(
            !mutates(
                "[nextcloud]\nserver_url = \"https://c.example\"\nusername = \"alice\"\n\
             [[sync.connectors]]\nname = \"work\"\nbackend = \"nextcloud\"\n\
             server_url = \"https://w.example\"\nusername = \"bob\"\n",
            ),
            "the array world never migrates"
        );
        assert!(
            !mutates(
                "[nextcloud]\nserver_url = \"https://c.example\"\nusername = \"alice\"\n\
             [sync]\nconnectors = []\n",
            ),
            "the inline-array rendering counts as present"
        );
    }

    /// M4 — the in-memory mirror validates on its own, and the
    /// env-identity corner (mirror plus injected identity keys) fails
    /// `validate` — pinning the boot ladder's keep-original arm.
    #[test]
    fn m4_the_mirror_validates_and_the_env_corner_does_not() {
        let mut config = AppConfig {
            ..AppConfig::default()
        };
        config.storage.db_path = Some("/tmp/taskboard-m4.db".into());
        config.nextcloud.server_url = Some("https://cloud.example".into());
        config.nextcloud.username = Some("alice".into());
        assert!(migrate_config_in_memory(&mut config));
        config.validate().expect("the mirrored shape validates");

        let mut env_corner = config.clone();
        env_corner.nextcloud.server_url = Some("https://env.example".into());
        env_corner.nextcloud.username = Some("bob".into());
        assert!(
            matches!(
                env_corner.validate(),
                Err(crate::config::ConfigError::NextcloudTableWithConnectors)
            ),
            "env-injected identity beside the array is the mixed shape"
        );
    }

    /// M5 — the file shell: the backup carries the exact pre-image
    /// bytes; an absent file is `Ok(false)`; an unwritable location
    /// surfaces the io-error (the caller decides the fallback).
    #[test]
    fn m5_the_file_shell_backs_up_and_reports_io_failures() {
        let dir = tempfile::tempdir().expect("tempdir");
        let config = dir.path().join("taskboard.toml");
        std::fs::write(&config, LEGACY_FILE).expect("seed");

        assert!(migrate_config_file(&config).expect("migrates"));
        let backup = dir.path().join("taskboard.toml.bak-legacy");
        assert_eq!(
            std::fs::read_to_string(&backup).expect("backup exists"),
            LEGACY_FILE,
            "the backup is the exact pre-image"
        );
        assert!(
            !migrate_config_file(&config).expect("second run"),
            "idempotent"
        );

        // Absent file: no migration, no error.
        assert!(
            !migrate_config_file(&dir.path().join("absent.toml")).expect("absent is Ok(false)"),
        );

        // An unwritable directory surfaces the write error (unix only:
        // the 0o500 trick is a POSIX permission).
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let ro = tempfile::tempdir().expect("tempdir");
            let ro_config = ro.path().join("taskboard.toml");
            std::fs::write(&ro_config, LEGACY_FILE).expect("seed");
            let mut perms = std::fs::metadata(ro.path()).expect("stat").permissions();
            perms.set_mode(0o500);
            std::fs::set_permissions(ro.path(), perms).expect("chmod");
            let err = migrate_config_file(&ro_config).expect_err("the write is refused");
            assert_eq!(err.kind(), std::io::ErrorKind::PermissionDenied);
            let mut perms = std::fs::metadata(ro.path()).expect("stat").permissions();
            perms.set_mode(0o700);
            std::fs::set_permissions(ro.path(), perms).expect("chmod back");
        }
    }

    /// M6 — the `load_migrated` ladder: the happy path returns the
    /// migrated config; a persist failure serves the mirrored config
    /// and leaves the file untouched; the re-load-mix corner restores
    /// the backup and returns the scalar config.
    mod ladder {
        use super::*;

        #[test]
        fn the_happy_path_migrates_and_reloads() {
            let dir = tempfile::tempdir().expect("tempdir");
            let config = dir.path().join("taskboard.toml");
            std::fs::write(
                &config,
                "[nextcloud]\nserver_url = \"https://cloud.example\"\nusername = \"alice\"\n",
            )
            .expect("seed");

            let loaded = load_migrated(Some(config.clone())).expect("serves");
            let entries = loaded.sync.connectors.as_deref().expect("array world");
            assert_eq!(entries.len(), 1);
            assert_eq!(entries[0].name, crate::connectors::DEFAULT_CONNECTOR_NAME);
            assert_eq!(loaded.nextcloud.server_url, None, "identity keys moved");
            assert!(
                dir.path().join("taskboard.toml.bak-legacy").is_file(),
                "the backup exists"
            );
        }

        #[cfg(unix)]
        #[test]
        fn a_persist_failure_serves_the_mirror_and_leaves_the_file_untouched() {
            use std::os::unix::fs::PermissionsExt as _;
            let dir = tempfile::tempdir().expect("tempdir");
            let config = dir.path().join("taskboard.toml");
            let before =
                "[nextcloud]\nserver_url = \"https://cloud.example\"\nusername = \"alice\"\n";
            std::fs::write(&config, before).expect("seed");
            let mut perms = std::fs::metadata(dir.path()).expect("stat").permissions();
            perms.set_mode(0o500);
            std::fs::set_permissions(dir.path(), perms).expect("chmod");

            let loaded = load_migrated(Some(config.clone())).expect("the mirror serves");
            assert!(
                loaded.sync.connectors.is_some(),
                "the in-memory mirror is the array world"
            );
            let mut perms = std::fs::metadata(dir.path()).expect("stat").permissions();
            perms.set_mode(0o700);
            std::fs::set_permissions(dir.path(), perms).expect("chmod back");
            assert_eq!(
                std::fs::read_to_string(&config).expect("read"),
                before,
                "the file is byte-untouched"
            );
        }

        #[test]
        fn the_reload_mix_corner_restores_the_backup_and_serves_scalar() {
            // The corner needs the re-load to fail after the write —
            // reachable only with env-injected identity, which a test
            // process cannot set safely in parallel. The ladder arm is
            // exercised at its seam instead: a migrate that writes, a
            // poisoned re-load, and the restore behavior are each
            // pinned; this test pins the *restore* step by direct
            // construction.
            let dir = tempfile::tempdir().expect("tempdir");
            let config = dir.path().join("taskboard.toml");
            let legacy =
                "[nextcloud]\nserver_url = \"https://cloud.example\"\nusername = \"alice\"\n";
            std::fs::write(&config, legacy).expect("seed");
            assert!(migrate_config_file(&config).expect("migrates"));
            restore_backup(&config);
            assert_eq!(
                std::fs::read_to_string(&config).expect("read"),
                legacy,
                "the backup was restored verbatim"
            );
            let scalar = AppConfig::load(Some(config.clone())).expect("the scalar world loads");
            assert!(scalar.sync.connectors.is_none());
        }
    }
}
