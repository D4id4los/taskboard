// SPDX-License-Identifier: AGPL-3.0-only
//! S-series: secrets — redaction, key derivation, env store, keyring
//! error mapping, and the fake-store round trip through the port.

use common::{FakeCredentialStore, env_lock, remove_env, set_env};
use taskboard_app::secrets::{
    AppPassword, CredentialError, CredentialStore, EnvCredentialStore, KeyringStore, credential_key,
};

mod common;

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
