// SPDX-License-Identifier: AGPL-3.0-only

//! The connection-management ceremony core (round-1 chunk 3, plan D1):
//! the non-interactive halves of the connect ceremony, extracted by
//! *moving bodies verbatim* out of [`crate::commands::connect`] so the
//! REPL one-shot and the UI request forwarder share them (ADR 0014,
//! D25b). Extract, don't rewrite: the unchanged connect/logout E2E
//! suites are the equivalence gate (plan A11).

use std::path::Path;
use std::sync::Arc;

use taskboard_sync_nextcloud::{DeckClient, DeckError};

use crate::commands::{CommandError, map_deck_error};
use crate::config::{ConnectorCfg, SyncBackend};
use crate::secrets::{AppPassword, CredentialError, CredentialStore};

/// Verifies the credentials against the server (`boards()` is the
/// verification call). Moved verbatim from `connect.rs::verify`
/// (plan F-B1), parameterized by fields instead of the one-shot's
/// private `Identity`.
///
/// # Errors
///
/// [`CommandError`] — the mapped [`DeckError`] classes; nothing is
/// persisted by the caller unless this returned `Ok`.
pub(crate) async fn verify_credentials(
    server: &str,
    user: &str,
    password: &str,
) -> Result<(), CommandError> {
    let client = DeckClient::new(server, user, password).map_err(map_deck_error)?;
    match client.boards().await {
        Ok(_) => Ok(()),
        Err(err) => {
            tracing::info!(variant = deck_variant(&err), "connect verification failed");
            Err(map_deck_error(err))
        }
    }
}

/// Stores the new secret; on a re-point, deletes the replaced
/// identity's secret **after** the new store (D7 — a crash between
/// leaves an orphan, never a broken install). The deletion tolerates
/// `NotFound` (idempotent) and `Unsupported` (the env store has no
/// secret to orphan); any other failure is typed. Moved verbatim from
/// `connect.rs::store_secret` (plan F-B1), parameterized by fields.
///
/// # Errors
///
/// [`CommandError::Credential`] on a store failure other than the
/// tolerated rotation arms.
pub(crate) fn store_and_rotate_secret(
    store: &Arc<dyn CredentialStore>,
    server: &str,
    user: &str,
    password: &str,
    old: Option<(&str, &str)>,
) -> Result<String, CommandError> {
    // A read-only store (env) is a *successful* connect with
    // `stored: "env"`; no secret is ever written to any file.
    let stored = match store.store(server, user, AppPassword::new(password.to_owned())) {
        Ok(()) => "keyring",
        Err(CredentialError::Unsupported) => "env",
        Err(err) => return Err(CommandError::Credential(err)),
    };
    if let Some((old_server, old_user)) = old {
        match store.delete(old_server, old_user) {
            Ok(()) | Err(CredentialError::NotFound | CredentialError::Unsupported) => {}
            Err(err) => return Err(CommandError::Credential(err)),
        }
    }
    Ok(stored.to_owned())
}

/// The bare variant name of a `DeckError` (log-only; never the secret,
/// never a wire payload). Moved with [`verify_credentials`].
fn deck_variant(err: &DeckError) -> &'static str {
    match err {
        DeckError::InvalidBaseUrl => "InvalidBaseUrl",
        DeckError::InvalidUpload => "InvalidUpload",
        DeckError::Transport(_) => "Transport",
        DeckError::Json(_) => "Json",
        DeckError::Unauthorized => "Unauthorized",
        DeckError::Forbidden => "Forbidden",
        DeckError::NotFound => "NotFound",
        DeckError::BadRequest => "BadRequest",
        DeckError::Conflict => "Conflict",
        DeckError::PreconditionFailed => "PreconditionFailed",
        DeckError::RateLimited { .. } => "RateLimited",
        DeckError::Server(_) => "Server",
        DeckError::Unavailable => "Unavailable",
    }
}

/// The `sync.connectors` item plus a scalar-world signal: `None`
/// means the file has no array at all (the scalar world — refused).
fn connectors_item(
    doc: &mut toml_edit::DocumentMut,
) -> Result<&mut toml_edit::Item, std::io::Error> {
    // The legacy-singleton exclusion is the writer-level backstop: a
    // `[nextcloud]` *identity* (server_url/username present — the same
    // predicate validate's `NextcloudTableWithConnectors` keys on)
    // beside the array would decode-reject, so refuse instead of
    // writing a mixed file. A `[nextcloud]` husk holding only
    // `credential_store` (process-global; the migration leaves it)
    // stays a valid settings-write target (plan §6, M11).
    let identity_husk = doc
        .get("nextcloud")
        .and_then(toml_edit::Item::as_table)
        .is_some_and(|table| table.contains_key("server_url") || table.contains_key("username"));
    if identity_husk {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "[nextcloud] identity present: the scalar world is not a settings-write target",
        ));
    }
    doc.get_mut("sync")
        .and_then(|sync| sync.get_mut("connectors"))
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "no sync.connectors array: the scalar world is not a settings-write target",
            )
        })
}

/// The entry's name as stored in the document (both renderings key
/// the upsert on it).
fn entry_name(item: &toml_edit::Item) -> Option<&str> {
    item.get("name").and_then(toml_edit::Item::as_str)
}

/// Builds the `toml_edit` table for one connector entry (field order
/// matching the config's declaration order; deterministic rendering
/// via `toml_edit::value`). Shared with the legacy-config migration's
/// entry rendering (plan §1) — one entry-rendering rule, no duplicate.
pub(crate) fn connector_entry_table(entry: &ConnectorCfg) -> toml_edit::Table {
    let mut table = toml_edit::Table::new();
    table.insert("name", toml_edit::value(entry.name.as_str()));
    table.insert("backend", toml_edit::value(backend_word(entry.backend)));
    table.insert("server_url", toml_edit::value(entry.server_url.as_str()));
    table.insert("username", toml_edit::value(entry.username.as_str()));
    table
}

/// The same entry as an inline-table value (the plain-array
/// rendering's shape).
fn connector_entry_inline(entry: &ConnectorCfg) -> toml_edit::Value {
    let mut table = toml_edit::InlineTable::new();
    table.insert("name", toml_edit::Value::from(entry.name.as_str()));
    table.insert(
        "backend",
        toml_edit::Value::from(backend_word(entry.backend)),
    );
    table.insert(
        "server_url",
        toml_edit::Value::from(entry.server_url.as_str()),
    );
    table.insert("username", toml_edit::Value::from(entry.username.as_str()));
    toml_edit::Value::InlineTable(table)
}

/// The config word for a backend (the serde spelling). Shared with the
/// legacy-config migration's entry rendering (plan §1).
pub(crate) fn backend_word(backend: SyncBackend) -> &'static str {
    match backend {
        SyncBackend::Nextcloud => "nextcloud",
        SyncBackend::Standalone => "standalone",
    }
}

/// Overlays one named `[[sync.connectors]]` entry into the config file
/// via `toml_edit` read-modify-write (comments, formatting, and every
/// other table preserved by construction; the whole-document write is
/// atomic temp + rename; the file's mode is preserved, a created file
/// is owner-only). An entry with the same `name` is
/// replaced in place (document order preserved); otherwise the entry
/// is appended. Both array renderings are navigated (array-of-tables
/// and plain array of inline tables — the REPL writer's rule). A
/// hand-made scalar world (a `[nextcloud]` *identity* beside the
/// array, or no array at all) is refused typed at the writer level —
/// the backstop; the handler migrates the reachable legacy world
/// ahead of the write (round-3 D21, plan §6/A3).
///
/// # Errors
///
/// [`std::io::Error`] of kind [`std::io::ErrorKind::InvalidData`]
/// against a scalar-world or unparseable file; IO errors from the
/// atomic write otherwise.
pub fn overlay_upsert_connector(path: &Path, entry: &ConnectorCfg) -> std::io::Result<()> {
    enum Upsert {
        Landed,
        NotAnArray,
    }
    let mut doc = crate::config::overlay_document(path)?;
    // The mutation borrows the document; the write happens after the
    // borrow scope ends.
    let outcome;
    {
        let connectors = connectors_item(&mut doc)?;
        if let Some(aot) = connectors.as_array_of_tables_mut() {
            let mut replaced = false;
            for existing in aot.iter_mut() {
                if existing.get("name").and_then(toml_edit::Item::as_str)
                    == Some(entry.name.as_str())
                {
                    *existing = connector_entry_table(entry);
                    replaced = true;
                    break;
                }
            }
            if !replaced {
                aot.push(connector_entry_table(entry));
            }
            outcome = Upsert::Landed;
        } else if let Some(array) = connectors.as_array_mut() {
            let mut replaced = false;
            for item in array.iter_mut() {
                let candidate = toml_edit::Item::Value(item.clone());
                if entry_name(&candidate) == Some(entry.name.as_str()) {
                    *item = connector_entry_inline(entry);
                    replaced = true;
                    break;
                }
            }
            if !replaced {
                array.push(connector_entry_inline(entry));
            }
            outcome = Upsert::Landed;
        } else {
            outcome = Upsert::NotAnArray;
        }
    }
    match outcome {
        Upsert::Landed => crate::config::write_config_atomic(path, doc.to_string().as_str()),
        Upsert::NotAnArray => Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "sync.connectors is not an array",
        )),
    }
}

/// Removes the named `[[sync.connectors]]` entry from the config file
/// (the same read-modify-write discipline as
/// [`overlay_upsert_connector`]; both array renderings navigated).
/// Removing the last entry removes the `connectors` key entirely — no
/// empty husk (the `[ui.stack_widths]` removal precedent). Unknown
/// name → `Ok(false)`, file untouched.
///
/// # Errors
///
/// [`std::io::Error`] — parse and write failures, typed.
pub fn overlay_remove_connector(path: &Path, name: &str) -> std::io::Result<bool> {
    let mut doc = crate::config::overlay_document(path)?;
    let Some(connectors) = doc
        .get_mut("sync")
        .and_then(|sync| sync.get_mut("connectors"))
    else {
        return Ok(false);
    };
    let removed;
    if let Some(aot) = connectors.as_array_of_tables_mut() {
        let position = aot
            .iter()
            .position(|entry| entry.get("name").and_then(toml_edit::Item::as_str) == Some(name));
        removed = position.map(|p| {
            aot.remove(p);
        });
    } else if let Some(array) = connectors.as_array_mut() {
        let position = array.iter().position(|item| {
            let candidate = toml_edit::Item::Value(item.clone());
            entry_name(&candidate) == Some(name)
        });
        removed = position.map(|p| {
            array.remove(p);
        });
    } else {
        removed = None;
    }
    let Some(()) = removed else {
        return Ok(false);
    };
    if let Some(sync) = doc.get_mut("sync").and_then(|sync| sync.as_table_mut()) {
        let empty = sync.get("connectors").is_some_and(|connectors| {
            connectors
                .as_array_of_tables()
                .is_some_and(toml_edit::ArrayOfTables::is_empty)
                || connectors
                    .as_array()
                    .is_some_and(toml_edit::Array::is_empty)
        });
        if empty {
            sync.remove("connectors");
        }
    }
    crate::config::write_config_atomic(path, doc.to_string().as_str())?;
    Ok(true)
}

/// Overlays `ui.scale = scale` into the config file — a bare `[ui]`
/// subtree write preserving every other key including the whole
/// `[ui.keymap]` table and `[ui.stack_widths]` (plan A5).
///
/// # Errors
///
/// [`std::io::Error`] — parse and write failures, typed.
pub fn overlay_set_ui_scale(
    path: &Path,
    scale: taskboard_ui_slint::UiScale,
) -> std::io::Result<()> {
    let mut doc = crate::config::overlay_document(path)?;
    ui_table(&mut doc)?["scale"] = toml_edit::value(scale.get());
    crate::config::write_config_atomic(path, doc.to_string().as_str())
}

/// Overlays `ui.keymap.preset = preset` (the config word:
/// `standard`/`emacs`/`vi`) into the config file; the `overrides`
/// subtree and every comment are preserved by construction (plan D9).
///
/// # Errors
///
/// [`std::io::Error`] — parse and write failures, typed.
pub fn overlay_set_keymap_preset(path: &Path, preset: &str) -> std::io::Result<()> {
    let mut doc = crate::config::overlay_document(path)?;
    ui_table(&mut doc)?["keymap"]["preset"] = toml_edit::value(preset);
    crate::config::write_config_atomic(path, doc.to_string().as_str())
}

/// The `[ui]` table access shared by the two `[ui]` overlay writers:
/// creates it when absent, refuses a non-table `[ui]` typed.
fn ui_table(doc: &mut toml_edit::DocumentMut) -> std::io::Result<&mut toml_edit::Table> {
    doc.entry("ui")
        .or_insert(toml_edit::Item::Table(toml_edit::Table::new()))
        .as_table_mut()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, "[ui] is not a table"))
}

#[cfg(test)]
mod overlay_tests {
    use super::*;

    fn seed_config(dir: &tempfile::TempDir, contents: &str) -> std::path::PathBuf {
        let path = dir.path().join("taskboard.toml");
        std::fs::write(&path, contents).expect("seed config");
        path
    }

    fn entry(name: &str, server: &str, user: &str) -> ConnectorCfg {
        ConnectorCfg {
            name: name.into(),
            backend: SyncBackend::Nextcloud,
            server_url: server.into(),
            username: user.into(),
        }
    }

    const AOT_FILE: &str = "# hand-tuned\n\
        [storage]\ndb_path = '/tmp/t.db'\n\
        [ui]\nscale = 1.5\n[ui.keymap]\npreset = \"vi\"\n[ui.keymap.overrides]\n\"task.complete\" = [\"x\"]\n\
        [[sync.connectors]]\nname = \"work\"\nbackend = \"nextcloud\"\n\
        server_url = \"https://old.example\"\nusername = \"old-user\"\n\
        [[sync.connectors]]\nname = \"personal\"\nbackend = \"nextcloud\"\n\
        server_url = \"https://home.example\"\nusername = \"home-user\"\n";

    /// A1 — upsert appends to an array file, preserving the leading
    /// comment and the unrelated tables; the file round-trips through
    /// `AppConfig::load`.
    #[test]
    fn upsert_appends_an_entry_preserving_comments_and_unrelated_tables() {
        let dir = tempfile::tempdir().expect("tempdir");
        let config = seed_config(&dir, AOT_FILE);
        let family = entry("family", "https://fam.example", "fam-user");

        overlay_upsert_connector(&config, &family).expect("upsert lands");

        let written = std::fs::read_to_string(&config).expect("read back");
        assert!(written.contains("# hand-tuned"), "comment survives");
        assert!(written.contains("scale = 1.5"), "[ui] survives");
        assert!(written.contains("preset = \"vi\""), "preset survives");
        assert!(
            written.contains("\"task.complete\" = [\"x\"]"),
            "overrides survive"
        );
        let loaded = crate::config::AppConfig::load(Some(config)).expect("round-trips");
        let entries = loaded.sync.connectors.as_deref().expect("array");
        assert_eq!(entries.len(), 3, "appended");
        assert_eq!(entries[2].name, "family");
        assert_eq!(entries[0].server_url, "https://old.example", "order kept");
    }

    /// A2 — upsert replaces the named entry in place; the other
    /// entries' order is preserved and no `[nextcloud]`/backend keys
    /// materialize.
    #[test]
    fn upsert_replaces_the_named_entry_in_place() {
        let dir = tempfile::tempdir().expect("tempdir");
        let config = seed_config(&dir, AOT_FILE);

        overlay_upsert_connector(&config, &entry("work", "https://new.example", "new-user"))
            .expect("upsert lands");

        let written = std::fs::read_to_string(&config).expect("read back");
        assert!(!written.contains("[nextcloud]"), "no scalar table created");
        let loaded = crate::config::AppConfig::load(Some(config)).expect("round-trips");
        let entries = loaded.sync.connectors.as_deref().expect("array");
        assert_eq!(entries.len(), 2, "replaced, not appended");
        assert_eq!(entries[0].name, "work");
        assert_eq!(entries[0].server_url, "https://new.example");
        assert_eq!(entries[1].name, "personal", "order preserved");
        assert!(loaded.sync.backend.is_none(), "no backend key written");
    }

    /// A2's inline-table twin: the same contract over the plain-array
    /// rendering (the REPL writer navigates it generically too).
    #[test]
    fn upsert_replaces_in_an_inline_table_array_and_round_trips() {
        let dir = tempfile::tempdir().expect("tempdir");
        let config = seed_config(
            &dir,
            "[sync]\nconnectors = [\
             { name = \"work\", backend = \"nextcloud\", \
             server_url = \"https://old.example\", username = \"old-user\" }, \
             { name = \"personal\", backend = \"nextcloud\", \
             server_url = \"https://home.example\", username = \"home-user\" }]\n",
        );

        overlay_upsert_connector(&config, &entry("work", "https://new.example", "new-user"))
            .expect("upsert lands");

        let loaded = crate::config::AppConfig::load(Some(config)).expect("round-trips");
        let entries = loaded.sync.connectors.as_deref().expect("array");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].server_url, "https://new.example");
        assert_eq!(entries[1].name, "personal");
    }

    /// A3 — the writer-level backstop: upsert against a scalar-world
    /// file is refused typed, file byte-untouched.
    #[test]
    fn upsert_refuses_the_scalar_world_typed() {
        let dir = tempfile::tempdir().expect("tempdir");
        let config = seed_config(
            &dir,
            "[nextcloud]\nserver_url = \"https://x\"\nusername = \"u\"\n",
        );
        let before = std::fs::read_to_string(&config).expect("read");

        let err = overlay_upsert_connector(&config, &entry("work", "https://n", "u"))
            .expect_err("scalar world refused");

        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
        assert_eq!(
            std::fs::read_to_string(&config).expect("read"),
            before,
            "the file is byte-untouched"
        );
    }

    #[test]
    fn upsert_refuses_a_file_without_any_sync_table() {
        let dir = tempfile::tempdir().expect("tempdir");
        let config = seed_config(&dir, "[app]\nmode = \"desktop\"\n");
        let err = overlay_upsert_connector(&config, &entry("work", "https://n", "u"))
            .expect_err("no array to upsert into");
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    }

    /// A4 — remove deletes the named entry; removing the last entry
    /// removes the `connectors` key (no empty husk); unknown name →
    /// `Ok(false)`, file untouched.
    #[test]
    fn remove_deletes_the_named_entry_and_the_last_one_removes_the_key() {
        let dir = tempfile::tempdir().expect("tempdir");
        let config = seed_config(&dir, AOT_FILE);

        assert!(overlay_remove_connector(&config, "work").expect("removes"));
        let loaded = crate::config::AppConfig::load(Some(config.clone())).expect("round-trips");
        let entries = loaded.sync.connectors.as_deref().expect("array survives");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "personal");

        assert!(overlay_remove_connector(&config, "personal").expect("removes"));
        let written = std::fs::read_to_string(&config).expect("read");
        assert!(!written.contains("connectors"), "no empty husk");
        assert!(written.contains("scale = 1.5"), "[ui] survives");
        let loaded = crate::config::AppConfig::load(Some(config)).expect("round-trips");
        assert!(loaded.sync.connectors.is_none());
    }

    #[test]
    fn remove_of_an_unknown_name_is_false_and_touches_nothing() {
        let dir = tempfile::tempdir().expect("tempdir");
        let config = seed_config(&dir, AOT_FILE);
        let before = std::fs::read_to_string(&config).expect("read");

        assert!(!overlay_remove_connector(&config, "ghost").expect("typed"));

        assert_eq!(std::fs::read_to_string(&config).expect("read"), before);
    }

    /// A5 — the `[ui]` writers touch only their key: the overrides
    /// subtree, the scale, and the comments survive; both round-trip
    /// through the figment layer.
    #[test]
    fn ui_overlay_writers_write_only_their_key_and_round_trip() {
        let dir = tempfile::tempdir().expect("tempdir");
        let config = seed_config(&dir, AOT_FILE);

        overlay_set_ui_scale(
            &config,
            taskboard_ui_slint::UiScale::new(2.0).expect("valid"),
        )
        .expect("scale lands");
        overlay_set_keymap_preset(&config, "emacs").expect("preset lands");

        let written = std::fs::read_to_string(&config).expect("read");
        assert!(written.contains("# hand-tuned"));
        assert!(written.contains("\"task.complete\" = [\"x\"]"));
        let loaded = crate::config::AppConfig::load(Some(config)).expect("round-trips");
        assert!((loaded.ui.scale - 2.0).abs() < f64::EPSILON);
        assert_eq!(loaded.ui.keymap.preset, crate::config::KeymapPreset::Emacs);
        assert_eq!(loaded.ui.keymap.overrides.len(), 1, "overrides survive");
    }

    /// M11 — the relaxed backstop (round-3 D21, plan §6): a
    /// `[nextcloud]` husk holding only the process-global
    /// `credential_store` beside a valid array is a valid upsert
    /// target (the migration leaves that husk; refusing it would
    /// block every later Save).
    #[test]
    fn upsert_accepts_a_credential_store_husk_beside_the_array() {
        let dir = tempfile::tempdir().expect("tempdir");
        let config = seed_config(
            &dir,
            "[nextcloud]\ncredential_store = \"file\"\n\
             [[sync.connectors]]\nname = \"work\"\nbackend = \"nextcloud\"\n\
             server_url = \"https://old.example\"\nusername = \"old-user\"\n",
        );

        overlay_upsert_connector(&config, &entry("work", "https://new.example", "new-user"))
            .expect("the husk is not the scalar world");

        let loaded = crate::config::AppConfig::load(Some(config)).expect("round-trips");
        assert_eq!(loaded.sync.connectors.as_deref().expect("array").len(), 1);
        assert_eq!(
            loaded.nextcloud.credential_store,
            crate::config::CredentialStoreKind::File,
            "the process-global kind survives"
        );
    }
}
