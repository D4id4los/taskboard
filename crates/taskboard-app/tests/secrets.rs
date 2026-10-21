// SPDX-License-Identifier: AGPL-3.0-only
//! S-series: secrets — redaction, key derivation, env store, keyring
//! error mapping, and the fake-store round trip through the port.

use common::{FakeCredentialStore, env_lock, remove_env, set_env};
use taskboard_app::secrets::{
    AppPassword, CredentialError, CredentialStore, EnvCredentialStore, FileCredentialStore,
    KeyringStore, credential_key,
};

mod common;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

/// S1: `AppPassword` redaction is a leak floor — the formatted output of
/// a secret-carrying value must not contain the secret (in the same
/// sanctioned class as garbage-input ceilings; strictly-better code
/// keeps this passing).
#[test]
fn s1_app_password_debug_redacts() {
    let pw = AppPassword::new("s3cret-app-pw");
    for formatted in [format!("{pw:?}"), format!("{pw:#?}")] {
        assert!(!formatted.contains("s3cret-app-pw"), "{formatted}");
        assert!(formatted.contains("[redacted]"), "{formatted}");
    }
    // And the only read access works.
    assert_eq!(pw.expose(), "s3cret-app-pw");
}

/// S2: `credential_key` is `username@host`, stable under garbage inputs,
/// never panicking (pure function).
#[test]
fn s2_credential_key_shape() {
    assert_eq!(
        credential_key("https://cloud.example.com/nextcloud", "alice"),
        "alice@cloud.example.com"
    );
    for server in ["", "   ", "not a url", "://", "https://", "https://:8080"] {
        let key = credential_key(server, "u");
        assert_eq!(key, credential_key(server, "u"), "stable for {server:?}");
        assert!(key.starts_with("u@"), "shape kept for {server:?}");
    }
}

/// S3: the env store loads `TASKBOARD_APP_PASSWORD` when present (and
/// non-empty), reports `NotFound` when absent.
#[test]
fn s3_env_store_load_hit_and_miss() {
    let _env = env_lock();

    set_env("TASKBOARD_APP_PASSWORD", "env-secret");
    let hit = EnvCredentialStore.load("https://x", "u");
    remove_env("TASKBOARD_APP_PASSWORD");
    assert!(matches!(&hit, Ok(pw) if pw.expose() == "env-secret"));

    let miss = EnvCredentialStore.load("https://x", "u");
    assert!(matches!(miss, Err(CredentialError::NotFound)));
}

/// S4: the env store is read-only by construction.
#[test]
fn s4_env_store_write_operations_are_unsupported() {
    assert!(matches!(
        EnvCredentialStore.store("https://x", "u", AppPassword::new("v")),
        Err(CredentialError::Unsupported)
    ));
    assert!(matches!(
        EnvCredentialStore.delete("https://x", "u"),
        Err(CredentialError::Unsupported)
    ));
}

/// S5: the keyring error mapping is matched on variant *shape* —
/// constructible backend errors map to `Backend`, absence maps to
/// `NotFound`. The real Secret Service round trip is the `#[ignore]`d
/// S7.
#[test]
fn s5_keyring_error_mapping_table() {
    // The production mapping (used by every `KeyringStore` operation):
    // absence is `NotFound`, every other backend failure is `Backend`
    // carrying the source.
    assert!(matches!(
        taskboard_app::secrets::map_keyring_error(keyring::Error::NoEntry),
        CredentialError::NotFound
    ));
    assert!(matches!(
        taskboard_app::secrets::map_keyring_error(keyring::Error::PlatformFailure(
            "no secret service".into()
        )),
        CredentialError::Backend(_)
    ));
    // And the store itself is a zero-sized port impl (linkage smoke).
    let _ = KeyringStore;
}

/// S6: the fake store round-trips through the port, including per-server
/// scoping (a key stored for one server is invisible to another).
#[test]
fn s6_fake_store_round_trip() {
    let store = FakeCredentialStore::with("https://a.example.com", "alice", "pw-a");
    store
        .store("https://b.example.com", "alice", AppPassword::new("pw-b"))
        .expect("store");

    assert!(matches!(
        store.load("https://a.example.com", "alice"),
        Ok(ref pw) if pw.expose() == "pw-a"
    ));
    assert!(matches!(
        store.load("https://b.example.com", "alice"),
        Ok(ref pw) if pw.expose() == "pw-b"
    ));
    assert!(matches!(
        store.load("https://c.example.com", "alice"),
        Err(CredentialError::NotFound)
    ));

    store
        .delete("https://a.example.com", "alice")
        .expect("delete");
    assert!(matches!(
        store.load("https://a.example.com", "alice"),
        Err(CredentialError::NotFound)
    ));
}

/// S7: the real keyring round trip against the developer's own Secret
/// Service — run manually in the tier-3 runbook (`TASKBOARD_IT_KEYRING=1`).
#[test]
#[ignore = "requires a desktop Secret Service; run with TASKBOARD_IT_KEYRING=1"]
fn it_keyring_local_round_trip() {
    if std::env::var("TASKBOARD_IT_KEYRING").as_deref() != Ok("1") {
        return; // skip never fails
    }
    let server = "https://keyring-test.example.com";
    let store = KeyringStore;
    store
        .store(server, "roundtrip", AppPassword::new("local-secret"))
        .expect("store into keyring");
    let loaded = store.load(server, "roundtrip");
    store.delete(server, "roundtrip").expect("cleanup");
    assert!(matches!(&loaded, Ok(pw) if pw.expose() == "local-secret"));
}

/// The file store under test, rooted at its own tempdir.
fn file_store() -> (tempfile::TempDir, FileCredentialStore) {
    let dir = common::tempdir();
    let store = FileCredentialStore::new(dir.path());
    (dir, store)
}

/// S8: the file store round-trips through the port with per-account
/// scoping; delete makes the credential invisible and deleting an
/// absent credential is `NotFound` (the keyring `NoEntry` mapping).
#[test]
fn s8_file_store_round_trip_and_account_scoping() {
    let (_dir, store) = file_store();
    store
        .store("https://a.example.com", "alice", AppPassword::new("pw-a"))
        .expect("store a");
    store
        .store("https://b.example.com", "alice", AppPassword::new("pw-b"))
        .expect("store b");

    assert!(matches!(
        store.load("https://a.example.com", "alice"),
        Ok(ref pw) if pw.expose() == "pw-a"
    ));
    assert!(matches!(
        store.load("https://b.example.com", "alice"),
        Ok(ref pw) if pw.expose() == "pw-b"
    ));

    store
        .delete("https://a.example.com", "alice")
        .expect("delete");
    assert!(matches!(
        store.load("https://a.example.com", "alice"),
        Err(CredentialError::NotFound)
    ));
    assert!(matches!(
        store.delete("https://a.example.com", "alice"),
        Err(CredentialError::NotFound)
    ));
}

/// S9: the key-file ceremony — generated by the first `store` (37
/// bytes, 0600), never regenerated, and every missing/corrupt/mismatch
/// shape fails typed on `load`.
#[test]
fn s9_key_file_ceremony() {
    let (dir, store) = file_store();
    let key_path = dir.path().join(taskboard_app::secrets::KEY_FILE_NAME);

    // load before anything stored: nothing stored, not a key error.
    assert!(matches!(
        store.load("https://x", "u"),
        Err(CredentialError::NotFound)
    ));
    assert!(!key_path.exists(), "load never generates the key file");

    store
        .store("https://x", "u", AppPassword::new("pw"))
        .expect("store");
    let original = std::fs::read(&key_path).expect("key file exists");
    assert_eq!(original.len(), 37);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&key_path)
            .expect("meta")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600, "key file created 0600");
    }

    // A second store does not regenerate the key.
    store
        .store("https://x", "u", AppPassword::new("pw2"))
        .expect("re-store");
    assert_eq!(std::fs::read(&key_path).expect("key file"), original);

    // Envelope present + key removed: loud, the remedy is re-login.
    std::fs::remove_file(&key_path).expect("remove key");
    assert!(matches!(
        store.load("https://x", "u"),
        Err(CredentialError::KeyFileMissing)
    ));

    // Corrupt key file: format-discovered, typed. (The fixture is
    // chmodded to 0600 first — the every-use mode fence fires before
    // the format check, per the ceremony.)
    std::fs::write(&key_path, b"not a key file at all").expect("write junk");
    #[cfg(unix)]
    std::fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o600))
        .expect("chmod fixture");
    assert!(matches!(
        store.load("https://x", "u"),
        Err(CredentialError::KeyFileCorrupt)
    ));

    // A fresh key cannot open the old envelope: auth failure, not
    // silent re-encryption or fallback.
    let (dir2, store2) = file_store();
    store2
        .store("https://x", "u", AppPassword::new("other"))
        .expect("store 2");
    let fresh_key =
        std::fs::read(dir2.path().join(taskboard_app::secrets::KEY_FILE_NAME)).expect("key 2");
    std::fs::write(&key_path, fresh_key).expect("transplant");
    #[cfg(unix)]
    std::fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o600))
        .expect("chmod fixture");
    assert!(matches!(
        store.load("https://x", "u"),
        Err(CredentialError::EnvelopeAuth)
    ));
}

/// S10 (unix): the 0600 fence — a key file weakened to 0644 fails
/// `KeyFileInsecureMode` on both `load` and `store`, and stays failed
/// until the operator restores 0600 (never an auto-chmod).
#[test]
#[cfg(unix)]
fn s10_key_file_mode_fence_never_auto_fixes() {
    let (dir, store) = file_store();
    let key_path = dir.path().join(taskboard_app::secrets::KEY_FILE_NAME);
    store
        .store("https://x", "u", AppPassword::new("pw"))
        .expect("store");

    std::fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o644)).expect("chmod 644");
    assert!(matches!(
        store.load("https://x", "u"),
        Err(CredentialError::KeyFileInsecureMode)
    ));
    assert!(matches!(
        store.store("https://x", "u", AppPassword::new("pw2")),
        Err(CredentialError::KeyFileInsecureMode)
    ));

    // The operator restores the mode; the fence lifts.
    std::fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o600)).expect("chmod 600");
    assert!(matches!(
        store.load("https://x", "u"),
        Ok(ref pw) if pw.expose() == "pw"
    ));
}

/// S11: no silent fallback — with the env escape hatch set *and* the
/// file store selected with a corrupt envelope, `load` returns the
/// typed file-store error, never the env value. Plus the wiring probe:
/// `production_store(File, dir)` is file-backed.
#[test]
fn s11_no_silent_fallback_to_env_or_keyring() {
    let _guard = env_lock();
    set_env(taskboard_app::secrets::ENV_PASSWORD_VAR, "env-password");
    let (dir, store) = file_store();
    store
        .store("https://x", "u", AppPassword::new("pw"))
        .expect("store");
    let envelope = dir.path().join("credentials").join(format!(
        "{}.bin",
        taskboard_app::secrets::hex_account_name(credential_key("https://x", "u").as_bytes())
    ));
    std::fs::write(&envelope, b"TBCE\x01garbage-not-an-envelope").expect("corrupt");

    assert!(matches!(
        store.load("https://x", "u"),
        Err(CredentialError::EnvelopeCorrupt)
    ));

    // The wiring probe: the config-selected production store for File
    // is the file store rooted where the config says.
    let dir2 = common::tempdir();
    let wired = taskboard_app::commands::production_store(
        taskboard_app::config::CredentialStoreKind::File,
        dir2.path(),
    );
    wired
        .store("https://w", "u", AppPassword::new("wired"))
        .expect("wired store");
    assert!(
        dir2.path()
            .join(taskboard_app::secrets::KEY_FILE_NAME)
            .exists()
    );
    remove_env(taskboard_app::secrets::ENV_PASSWORD_VAR);
}

/// S12: the leak floor — raw envelope bytes carry neither the
/// plaintext nor the raw key bytes, and the store's `Debug` carries no
/// secret (S1-class).
#[test]
fn s12_leak_floor_envelope_and_debug() {
    let (dir, store) = file_store();
    store
        .store("https://x", "u", AppPassword::new("super-secret-pw"))
        .expect("store");
    let envelope_path = dir.path().join("credentials").join(format!(
        "{}.bin",
        taskboard_app::secrets::hex_account_name(credential_key("https://x", "u").as_bytes())
    ));
    let raw = std::fs::read(envelope_path).expect("envelope");
    let key = std::fs::read(dir.path().join(taskboard_app::secrets::KEY_FILE_NAME)).expect("key");
    assert!(!raw.windows(15).any(|w| w == b"super-secret-pw"));
    assert!(!raw.windows(8).any(|w| *w == key[..8]));
    let formatted = format!("{store:?}");
    assert!(!formatted.contains("super-secret-pw"));
}

/// S13: nonce freshness — storing the same secret twice produces two
/// envelopes with different nonce regions (structural read of offsets
/// 5..17) and different bytes overall.
#[test]
fn s13_fresh_nonce_per_write() {
    let (dir, store) = file_store();
    let envelope_path = |dir: &tempfile::TempDir| {
        dir.path().join("credentials").join(format!(
            "{}.bin",
            taskboard_app::secrets::hex_account_name(credential_key("https://x", "u").as_bytes())
        ))
    };
    store
        .store("https://x", "u", AppPassword::new("same"))
        .expect("store 1");
    let first = std::fs::read(envelope_path(&dir)).expect("envelope 1");
    store
        .store("https://x", "u", AppPassword::new("same"))
        .expect("store 2");
    let second = std::fs::read(envelope_path(&dir)).expect("envelope 2");

    assert_ne!(first, second, "fresh nonce changes the whole blob");
    assert_ne!(&first[5..17], &second[5..17], "nonce region differs");
}
