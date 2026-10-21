// SPDX-License-Identifier: AGPL-3.0-only
//! Credential resolution (ADR 0008): a [`CredentialStore`] port with
//! three config-selected implementations — [`KeyringStore`]
//! (production default), [`EnvCredentialStore`] (the headless/CI
//! escape hatch), and [`FileCredentialStore`] (the kiosk store: a
//! generated 0600 key file beside the db plus ChaCha20-Poly1305
//! envelopes under `<db-dir>/credentials/`).
//!
//! There is **no silent fallback**: the store is chosen explicitly by
//! `[nextcloud] credential_store`, and a headless machine without a
//! Secret Service surfaces a typed [`CredentialError::Backend`] (whose
//! `Display` names the env alternative) rather than degrading quietly.
//!
//! Secrets travel in the redacting [`AppPassword`] newtype: `Debug`
//! prints `[redacted]`, there is no `Display`, and the only read access
//! is [`AppPassword::expose`] — consumed exactly once by
//! `DeckClient::new`.

use keyring::Entry;
use std::io::Write as _;
use std::path::Path;
use std::sync::Arc;

/// The keyring service name every taskboard entry lives under.
pub const KEYRING_SERVICE: &str = "taskboard";

/// The environment variable [`EnvCredentialStore`] reads.
pub const ENV_PASSWORD_VAR: &str = "TASKBOARD_APP_PASSWORD";

/// The Nextcloud app password. Redacting `Debug`, no `Display`; the only
/// read access is [`expose`](AppPassword::expose). Never logged, never
/// serialized.
#[derive(Clone)]
pub struct AppPassword(String);

impl AppPassword {
    /// Wraps a secret. Callers own keeping the source out of logs.
    #[must_use]
    pub fn new(secret: impl Into<String>) -> Self {
        Self(secret.into())
    }

    /// The only read access; consumed by `DeckClient::new`.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for AppPassword {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("AppPassword").field(&"[redacted]").finish()
    }
}

/// Credential failure classes, matched on variant shape only.
#[derive(Debug, thiserror::Error)]
pub enum CredentialError {
    /// No credential is stored for this (server, username).
    #[error("no credential stored")]
    NotFound,
    /// The OS credential backend is unavailable or failed. On a headless
    /// Linux box this is the signal to opt into `credential_store =
    /// "env"` explicitly.
    #[error(
        "credential backend unavailable (headless? set `credential_store = \"env\"` and export TASKBOARD_APP_PASSWORD)"
    )]
    Backend(#[source] keyring::Error),
    /// This store cannot `store`/`delete` (the env store is read-only).
    #[error("operation not supported by this store")]
    Unsupported,
    /// The file store's envelope exists but the key file does not. The
    /// remedy is a re-login (`store` regenerates); silently regenerating
    /// on `load` would orphan the envelope behind an unverifiable key.
    #[error("credential key file is missing (re-login to regenerate)")]
    KeyFileMissing,
    /// The key file's permission mode is not 0600 (unix). Never
    /// auto-corrected: the operator fixes the mode, the error is the
    /// fence.
    #[error(
        "credential key file is not mode 0600 (fix the mode; it is not corrected automatically)"
    )]
    KeyFileInsecureMode,
    /// The key file is not a taskboard key file (wrong magic, version,
    /// or length).
    #[error("credential key file is corrupt")]
    KeyFileCorrupt,
    /// The envelope file is too short or carries the wrong magic.
    #[error("credential envelope is corrupt")]
    EnvelopeCorrupt,
    /// The envelope's magic is valid but its format version is unknown
    /// (future formats fail loudly, never misdecode).
    #[error("credential envelope version {0} is not supported")]
    EnvelopeVersion(u8),
    /// The Poly1305 tag did not verify — a wrong key or a tampered
    /// envelope, deliberately indistinguishable.
    #[error("credential envelope failed authentication")]
    EnvelopeAuth,
    /// An unexpected filesystem failure (permission denied, disk full,
    /// a key-file creation race).
    #[error("credential store filesystem failure")]
    StoreIo(#[source] std::io::Error),
    /// The OS randomness source failed during key/nonce generation.
    #[error("secure randomness unavailable")]
    Entropy(#[source] getrandom::Error),
}

/// OS-side credential storage seam (fakes over mocks: tests use a
/// `HashMap`-backed store; production selects via
/// `[nextcloud] credential_store`).
pub trait CredentialStore: Send + Sync + std::fmt::Debug {
    /// Loads the app password for `(server, username)`.
    ///
    /// # Errors
    ///
    /// [`CredentialError`] — typed, see the variant docs.
    fn load(&self, server: &str, username: &str) -> Result<AppPassword, CredentialError>;

    /// Stores the app password for `(server, username)` (Phase 6
    /// `login`; shipped now so the seam is final).
    ///
    /// # Errors
    ///
    /// [`CredentialError`] — typed, see the variant docs.
    fn store(
        &self,
        server: &str,
        username: &str,
        secret: AppPassword,
    ) -> Result<(), CredentialError>;

    /// Removes the app password for `(server, username)` (Phase 6
    /// `logout`).
    ///
    /// # Errors
    ///
    /// [`CredentialError`] — typed, see the variant docs.
    fn delete(&self, server: &str, username: &str) -> Result<(), CredentialError>;
}

/// The `keyring` adapter: `Entry::new("taskboard", credential_key(..))`.
///
/// Error mapping: `keyring::Error::NoEntry` →
/// [`CredentialError::NotFound`]; everything else →
/// [`CredentialError::Backend`] — matched on variant shape, never
/// message text.
#[derive(Debug, Default)]
pub struct KeyringStore;

impl CredentialStore for KeyringStore {
    fn load(&self, server: &str, username: &str) -> Result<AppPassword, CredentialError> {
        let entry = entry(server, username)?;
        entry
            .get_password()
            .map(AppPassword)
            .map_err(map_keyring_error)
    }

    fn store(
        &self,
        server: &str,
        username: &str,
        secret: AppPassword,
    ) -> Result<(), CredentialError> {
        let entry = entry(server, username)?;
        entry
            .set_password(secret.expose())
            .map_err(CredentialError::Backend)
    }

    fn delete(&self, server: &str, username: &str) -> Result<(), CredentialError> {
        let entry = entry(server, username)?;
        entry.delete_credential().map_err(map_keyring_error)
    }
}

fn entry(server: &str, username: &str) -> Result<Entry, CredentialError> {
    Entry::new(KEYRING_SERVICE, &credential_key(server, username)).map_err(CredentialError::Backend)
}

/// The keyring → typed error mapping: `NoEntry` means "nothing stored"
/// ([`CredentialError::NotFound`]); every other backend failure is a
/// [`CredentialError::Backend`] carrying the source. Matched on variant
/// shape, never message text.
/// The public mapping is part of the error-mapping contract (S5 pins it);
/// exposed for tests and future callers that translate keyring errors.
#[must_use]
pub fn map_keyring_error(err: keyring::Error) -> CredentialError {
    match err {
        keyring::Error::NoEntry => CredentialError::NotFound,
        other => CredentialError::Backend(other),
    }
}

/// Read-only escape hatch for headless machines and CI: loads
/// [`ENV_PASSWORD_VAR`]; `store`/`delete` are
/// [`CredentialError::Unsupported`]. The variable is deliberately *not*
/// `TASKBOARD_`-nested-config shaped: it is a secret read by this
/// module, not config consumed by figment.
#[derive(Debug, Default)]
pub struct EnvCredentialStore;

impl CredentialStore for EnvCredentialStore {
    fn load(&self, _server: &str, _username: &str) -> Result<AppPassword, CredentialError> {
        // Non-secret env names may use std::env; the value itself is
        // never logged or formatted.
        match std::env::var(ENV_PASSWORD_VAR) {
            Ok(secret) if !secret.is_empty() => Ok(AppPassword(secret)),
            _ => Err(CredentialError::NotFound),
        }
    }

    fn store(
        &self,
        _server: &str,
        _username: &str,
        _secret: AppPassword,
    ) -> Result<(), CredentialError> {
        Err(CredentialError::Unsupported)
    }

    fn delete(&self, _server: &str, _username: &str) -> Result<(), CredentialError> {
        Err(CredentialError::Unsupported)
    }
}

/// Entry identity: `{username}@{host-of-server}` — the server host is
/// part of the key, so a config edit pointing at another server cannot
/// silently reuse a stale password. Hostless/garbage inputs still
/// produce a stable key (pure function, never panics): the trimmed input
/// stands in for the host.
#[must_use]
pub fn credential_key(server: &str, username: &str) -> String {
    let rest = server.split_once("://").map_or(server, |(_, r)| r);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    let with_port = authority.rsplit_once('@').map_or(authority, |(_, a)| a);
    let host = with_port.rsplit_once(':').map_or(with_port, |(h, _)| h);
    let host = if host.trim().is_empty() {
        server.trim()
    } else {
        host
    };
    format!("{username}@{host}")
}

/// The file store's on-disk layout, rooted at the db's parent
/// (`AppConfig::credential_base_dir`): `credential.key` (0600,
/// machine-generated, account-neutral) and one AEAD envelope per
/// account under `credentials/`.
pub const KEY_FILE_NAME: &str = "credential.key";
/// The directory under the base dir holding the per-account envelopes.
pub const ENVELOPE_DIR_NAME: &str = "credentials";

const KEY_MAGIC: [u8; 4] = *b"TBCK";
const ENVELOPE_MAGIC: [u8; 4] = *b"TBCE";
/// The only envelope/key-file format version; anything else fails
/// typed instead of being misdecoded.
const FORMAT_VERSION: u8 = 1;
const KEY_BYTES: usize = 32;
const NONCE_BYTES: usize = 12;
/// `key-file = magic(4) || version(1) || key(32)`.
const KEY_FILE_LEN: usize = 4 + 1 + KEY_BYTES;
/// `envelope = magic(4) || version(1) || nonce(12) || ciphertext || tag(16)`.
const ENVELOPE_HEADER_LEN: usize = 4 + 1 + NONCE_BYTES;
const TAG_BYTES: usize = 16;

/// `ChaCha20Poly1305` with the `zeroize` feature: the cipher's key
/// schedule zeroizes on drop. Key bytes transiently held in this
/// module's own buffers are best-effort only — no zeroizing `Vec`
/// exists in `forbid(unsafe_code)` Rust; stated, not over-promised.
type Aead = chacha20poly1305::ChaCha20Poly1305;

/// The kiosk `CredentialStore`: one machine-generated key file
/// (0600, `TBCK` format) guarding per-account ChaCha20-Poly1305
/// envelopes. Threat-model honesty (ADR 0008): protects against other
/// local users, accidental world-readable copies, and env-var
/// visibility — not against root or full-disk theft; strictly better
/// than plaintext or env-in-unit-file, strictly available where the
/// keyring is not, weaker than the keyring where it is.
///
/// Never a silent fallback: every failure is a typed
/// [`CredentialError`], and an absent key file never regenerates on
/// `load`.
#[derive(Debug)]
pub struct FileCredentialStore {
    base_dir: std::path::PathBuf,
}

impl FileCredentialStore {
    /// Roots the store at `base_dir` (the db's parent). The key file
    /// and the envelope directory are only created by [`store`]
    /// ([`FileCredentialStore::store`]); `load` and `delete` never create
    /// anything.
    #[must_use]
    pub fn new(base_dir: &Path) -> Self {
        Self {
            base_dir: base_dir.to_path_buf(),
        }
    }

    fn key_path(&self) -> std::path::PathBuf {
        self.base_dir.join(KEY_FILE_NAME)
    }

    fn envelope_path(&self, server: &str, username: &str) -> std::path::PathBuf {
        self.base_dir.join(ENVELOPE_DIR_NAME).join(format!(
            "{}.bin",
            hex_lower(credential_key(server, username).as_bytes())
        ))
    }

    /// Reads + validates the key file: existence → mode (unix) →
    /// format. Every step fails typed; nothing is ever auto-created or
    /// auto-chmodded here.
    fn read_key(&self) -> Result<[u8; KEY_BYTES], CredentialError> {
        let bytes = std::fs::read(self.key_path()).map_err(|err| {
            if err.kind() == std::io::ErrorKind::NotFound {
                CredentialError::KeyFileMissing
            } else {
                CredentialError::StoreIo(err)
            }
        })?;
        enforce_key_file_mode(&self.key_path())?;
        parse_key_file(&bytes)
    }

    /// The key ceremony: returns the existing key file's key, or (first
    /// `store` use only) generates one and writes it 0600 + fsync. A
    /// concurrent-creation race surfaces typed (`StoreIo`) rather than
    /// silently reusing a key this process never saw.
    fn load_or_create_key(&self) -> Result<[u8; KEY_BYTES], CredentialError> {
        match self.read_key() {
            Ok(key) => Ok(key),
            Err(CredentialError::KeyFileMissing) => {
                std::fs::create_dir_all(&self.base_dir).map_err(CredentialError::StoreIo)?;
                let mut key = [0u8; KEY_BYTES];
                getrandom::fill(&mut key).map_err(CredentialError::Entropy)?;
                let mut file = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(self.key_path())
                    .map_err(CredentialError::StoreIo)?;
                set_create_mode_0600(&file)?;
                file.write_all(&encode_key_file(&key))
                    .and_then(|()| file.sync_all())
                    .map_err(CredentialError::StoreIo)?;
                Ok(key)
            }
            Err(err) => Err(err),
        }
    }

    /// Atomic 0600 envelope write: temp sibling + fsync + rename, so a
    /// crash mid-`store` leaves either the old or no envelope, never a
    /// torn one.
    fn write_envelope_atomic(path: &std::path::Path, bytes: &[u8]) -> Result<(), CredentialError> {
        let dir = path.parent().ok_or_else(|| {
            CredentialError::StoreIo(std::io::Error::other("envelope path has no parent"))
        })?;
        std::fs::create_dir_all(dir).map_err(CredentialError::StoreIo)?;
        let mut suffix = [0u8; 8];
        getrandom::fill(&mut suffix).map_err(CredentialError::Entropy)?;
        let tmp = dir.join(format!(
            ".{}.tmp-{}",
            path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("envelope"),
            hex_lower(&suffix)
        ));
        let written = (|| -> Result<(), CredentialError> {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&tmp)
                .map_err(CredentialError::StoreIo)?;
            set_create_mode_0600(&file)?;
            file.write_all(bytes).map_err(CredentialError::StoreIo)?;
            file.sync_all().map_err(CredentialError::StoreIo)
        })();
        if written.is_err() {
            let _ = std::fs::remove_file(&tmp);
            return written;
        }
        std::fs::rename(&tmp, path).map_err(|err| {
            let _ = std::fs::remove_file(&tmp);
            CredentialError::StoreIo(err)
        })
    }
}

impl CredentialStore for FileCredentialStore {
    fn load(&self, server: &str, username: &str) -> Result<AppPassword, CredentialError> {
        let path = self.envelope_path(server, username);
        let blob = std::fs::read(&path).map_err(|err| {
            if err.kind() == std::io::ErrorKind::NotFound {
                // Nothing stored — identical to the keyring pre-login
                // semantics, regardless of key state.
                CredentialError::NotFound
            } else {
                CredentialError::StoreIo(err)
            }
        })?;
        let key = self.read_key()?;
        let plaintext = open_envelope(&key, server, username, &blob)?;
        String::from_utf8(plaintext)
            .map(AppPassword)
            .map_err(|_err| CredentialError::EnvelopeCorrupt)
    }

    fn store(
        &self,
        server: &str,
        username: &str,
        secret: AppPassword,
    ) -> Result<(), CredentialError> {
        let key = self.load_or_create_key()?;
        let blob = seal_envelope(&key, server, username, secret.expose().as_bytes())?;
        Self::write_envelope_atomic(&self.envelope_path(server, username), &blob)
    }

    fn delete(&self, server: &str, username: &str) -> Result<(), CredentialError> {
        let path = self.envelope_path(server, username);
        std::fs::remove_file(&path).map_err(|err| match err.kind() {
            std::io::ErrorKind::NotFound => CredentialError::NotFound,
            _ => CredentialError::StoreIo(err),
        })
        // The key file is never deleted here: it is account-neutral
        // and may guard other accounts' envelopes.
    }
}

/// The unix half of the 0600 fence: every key-file use verifies the
/// mode and refuses anything weaker (`KeyFileInsecureMode`) — never an
/// auto-chmod, never a warning-and-continue. On Windows there is no
/// mode bit; the documented stance is the user-profile ACL
/// inheritance.
#[cfg(unix)]
fn enforce_key_file_mode(path: &Path) -> Result<(), CredentialError> {
    use std::os::unix::fs::MetadataExt;
    let mode = std::fs::metadata(path)
        .map_err(CredentialError::StoreIo)?
        .mode()
        & 0o777;
    if mode == 0o600 {
        Ok(())
    } else {
        Err(CredentialError::KeyFileInsecureMode)
    }
}

#[cfg(not(unix))]
fn enforce_key_file_mode(_path: &Path) -> Result<(), CredentialError> {
    Ok(())
}

/// Applies 0600 to a freshly created file (unix `OpenOptionsExt` —
/// safe API; subject to umask, which the every-use mode fence turns
/// into a typed failure rather than a silent weaken).
#[cfg(unix)]
fn set_create_mode_0600(file: &std::fs::File) -> Result<(), CredentialError> {
    use std::os::unix::fs::PermissionsExt;
    file.set_permissions(std::fs::Permissions::from_mode(0o600))
        .map_err(CredentialError::StoreIo)
}

#[cfg(not(unix))]
fn set_create_mode_0600(_file: &std::fs::File) -> Result<(), CredentialError> {
    Ok(())
}

/// Serializes a generated key into the `TBCK` key-file format.
#[must_use]
fn encode_key_file(key: &[u8; KEY_BYTES]) -> Vec<u8> {
    let mut out = Vec::with_capacity(KEY_FILE_LEN);
    out.extend_from_slice(&KEY_MAGIC);
    out.push(FORMAT_VERSION);
    out.extend_from_slice(key);
    out
}

/// Parses key-file bytes; wrong length, magic, or version →
/// [`CredentialError::KeyFileCorrupt`] (format-discovered: a stray
/// non-key file at the key path fails typed, never used as key bytes).
fn parse_key_file(bytes: &[u8]) -> Result<[u8; KEY_BYTES], CredentialError> {
    if bytes.len() != KEY_FILE_LEN || bytes[..4] != KEY_MAGIC || bytes[4] != FORMAT_VERSION {
        return Err(CredentialError::KeyFileCorrupt);
    }
    let mut key = [0u8; KEY_BYTES];
    key.copy_from_slice(&bytes[5..]);
    Ok(key)
}

/// Encrypts `plaintext` into the `TBCE` envelope: fresh 12-byte random
/// nonce per write (random-nonce usage per RFC 8439; this store writes
/// on the order of once per login, unimaginably below the 2^32-message
/// discipline), AAD binding magic || version || account key so the
/// header cannot be stripped or an envelope spliced onto another
/// account without failing the tag.
fn seal_envelope(
    key: &[u8; KEY_BYTES],
    server: &str,
    username: &str,
    plaintext: &[u8],
) -> Result<Vec<u8>, CredentialError> {
    use chacha20poly1305::aead::{AeadInOut, KeyInit};
    let mut nonce = [0u8; NONCE_BYTES];
    getrandom::fill(&mut nonce).map_err(CredentialError::Entropy)?;
    let cipher = Aead::new(&(*key).into());
    let mut buffer = plaintext.to_vec();
    cipher
        .encrypt_in_place(&nonce.into(), &aad(server, username), &mut buffer)
        .map_err(|_err| {
            // Unreachable for an in-memory buffer; a failure here is an
            // unexpected internal fault, surfaced typed.
            CredentialError::StoreIo(std::io::Error::other("envelope seal failed"))
        })
        .map(|()| {
            let mut out = Vec::with_capacity(ENVELOPE_HEADER_LEN + buffer.len());
            out.extend_from_slice(&ENVELOPE_MAGIC);
            out.push(FORMAT_VERSION);
            out.extend_from_slice(&nonce);
            out.extend_from_slice(&buffer);
            out
        })
}

/// Decrypts an envelope; short/wrong-magic → `EnvelopeCorrupt`, unknown
/// version → `EnvelopeVersion`, tag failure → `EnvelopeAuth` (wrong key
/// and tampering are deliberately indistinguishable).
fn open_envelope(
    key: &[u8; KEY_BYTES],
    server: &str,
    username: &str,
    blob: &[u8],
) -> Result<Vec<u8>, CredentialError> {
    use chacha20poly1305::aead::{AeadInOut, KeyInit};
    if blob.len() < ENVELOPE_HEADER_LEN + TAG_BYTES || blob[..4] != ENVELOPE_MAGIC {
        return Err(CredentialError::EnvelopeCorrupt);
    }
    if blob[4] != FORMAT_VERSION {
        return Err(CredentialError::EnvelopeVersion(blob[4]));
    }
    let mut nonce = [0u8; NONCE_BYTES];
    nonce.copy_from_slice(&blob[5..ENVELOPE_HEADER_LEN]);
    let mut buffer = blob[ENVELOPE_HEADER_LEN..].to_vec();
    let cipher = Aead::new(&(*key).into());
    cipher
        .decrypt_in_place(&nonce.into(), &aad(server, username), &mut buffer)
        .map_err(|_err| CredentialError::EnvelopeAuth)?;
    Ok(buffer)
}

/// The envelope AAD: magic || version || `credential_key` bytes.
fn aad(server: &str, username: &str) -> Vec<u8> {
    let mut aad = Vec::with_capacity(5 + credential_key(server, username).len());
    aad.extend_from_slice(&ENVELOPE_MAGIC);
    aad.push(FORMAT_VERSION);
    aad.extend_from_slice(credential_key(server, username).as_bytes());
    aad
}

/// The public name of the filename encoder: lowercase hex of the
/// account key bytes (lossless, collision-free, filesystem-safe).
/// Exposed for tests and operators locating an envelope on disk.
#[must_use]
pub fn hex_account_name(bytes: &[u8]) -> String {
    hex_lower(bytes)
}

/// Lowercase hex, no deps: account keys are arbitrary UTF-8 (the
/// garbage-input guarantees of `credential_key`), so filenames
/// hex-encode them — lossless, collision-free, filesystem-safe.
#[must_use]
fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[usize::from(byte >> 4)] as char);
        out.push(HEX[usize::from(byte & 0x0f)] as char);
    }
    out
}

/// Resolves the config-selected credential store — the one wiring point
/// every booting lifecycle (daemon, desktop UI) shares, so the
/// backend-selection policy has exactly one definition site.
#[must_use]
pub fn resolve_credential_store(
    kind: crate::config::CredentialStoreKind,
    base_dir: &Path,
) -> Arc<dyn CredentialStore> {
    match kind {
        crate::config::CredentialStoreKind::Keyring => Arc::new(KeyringStore),
        crate::config::CredentialStoreKind::Env => Arc::new(EnvCredentialStore),
        crate::config::CredentialStoreKind::File => Arc::new(FileCredentialStore::new(base_dir)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_password_debug_redacts_and_has_no_display() {
        let pw = AppPassword::new("s3cret-app-pw");
        let formatted = format!("{pw:?}");
        assert!(!formatted.contains("s3cret-app-pw"));
        assert!(formatted.contains("[redacted]"));
        assert_eq!(pw.expose(), "s3cret-app-pw");
    }

    #[test]
    fn credential_key_is_username_at_host() {
        assert_eq!(
            credential_key("https://cloud.example.com", "alice"),
            "alice@cloud.example.com"
        );
        // Port is part of the address but not of the identity key.
        assert_eq!(
            credential_key("http://localhost:8080/nextcloud", "bob"),
            "bob@localhost"
        );
    }

    #[test]
    fn credential_key_is_stable_for_garbage_inputs() {
        for server in ["", "   ", "not a url", "://", "https://"] {
            let key = credential_key(server, "u");
            assert_eq!(key, credential_key(server, "u"), "stable for {server:?}");
            assert!(key.starts_with("u@"), "shape kept for {server:?}");
        }
    }

    #[test]
    fn envelope_round_trips_under_a_fixed_key() {
        let key = [7u8; KEY_BYTES];
        let blob = seal_envelope(&key, "https://cloud.example.com", "alice", b"pw").expect("seal");
        assert_eq!(&blob[..4], b"TBCE");
        assert_eq!(blob[4], FORMAT_VERSION);
        assert_eq!(
            open_envelope(&key, "https://cloud.example.com", "alice", &blob).expect("open"),
            b"pw".to_vec()
        );
    }

    #[test]
    fn envelope_corruption_fences_are_typed() {
        let key = [7u8; KEY_BYTES];
        let blob = seal_envelope(&key, "https://x", "u", b"pw").expect("seal");

        // Short file and wrong magic are corrupt, not auth failures.
        assert!(matches!(
            open_envelope(&key, "https://x", "u", &blob[..20]),
            Err(CredentialError::EnvelopeCorrupt)
        ));
        let mut wrong_magic = blob.clone();
        wrong_magic[0] = b'X';
        assert!(matches!(
            open_envelope(&key, "https://x", "u", &wrong_magic),
            Err(CredentialError::EnvelopeCorrupt)
        ));

        // An unknown future version fails loudly, never misdecodes.
        let mut future = blob.clone();
        future[4] = 2;
        assert!(matches!(
            open_envelope(&key, "https://x", "u", &future),
            Err(CredentialError::EnvelopeVersion(2))
        ));

        // A flipped ciphertext bit is an auth failure.
        let mut flipped = blob.clone();
        let last = flipped.len() - 1;
        flipped[last] ^= 0x01;
        assert!(matches!(
            open_envelope(&key, "https://x", "u", &flipped),
            Err(CredentialError::EnvelopeAuth)
        ));

        // The AAD binds the account: the same bytes decoded under
        // another account fail closed.
        assert!(matches!(
            open_envelope(&key, "https://x", "other", &blob),
            Err(CredentialError::EnvelopeAuth)
        ));
    }

    #[test]
    fn key_file_parses_only_the_exact_format() {
        let key = [9u8; KEY_BYTES];
        let bytes = encode_key_file(&key);
        assert_eq!(bytes.len(), KEY_FILE_LEN);
        assert_eq!(&bytes[..4], b"TBCK");
        assert_eq!(parse_key_file(&bytes).expect("parses"), key);

        let mut short = bytes.clone();
        short.pop();
        let mut long = bytes.clone();
        long.push(0); // trailing junk
        for corrupt in [short, long] {
            assert!(matches!(
                parse_key_file(&corrupt),
                Err(CredentialError::KeyFileCorrupt)
            ));
        }
        let mut wrong_magic = bytes.clone();
        wrong_magic[3] = b'X';
        assert!(matches!(
            parse_key_file(&wrong_magic),
            Err(CredentialError::KeyFileCorrupt)
        ));
        let mut wrong_version = bytes;
        wrong_version[4] = 2;
        assert!(matches!(
            parse_key_file(&wrong_version),
            Err(CredentialError::KeyFileCorrupt)
        ));
    }

    #[test]
    fn account_filenames_hex_encode_losslessly() {
        for account in [
            "alice@cloud.example.com",
            "u@/:*?\"<>| spaces\n",
            "é中 ",
            "",
        ] {
            let name = hex_lower(account.as_bytes());
            assert!(name.bytes().all(|b| b.is_ascii_hexdigit()));
            let mut back = Vec::with_capacity(name.len() / 2);
            for [hi, lo] in name.as_bytes().as_chunks::<2>().0 {
                let hi = char::from(*hi).to_digit(16).expect("hex");
                let lo = char::from(*lo).to_digit(16).expect("hex");
                let byte = u8::try_from((hi << 4) | lo).expect("two hex digits");
                back.push(byte);
            }
            assert_eq!(back, account.as_bytes());
        }
        assert_ne!(
            hex_lower(credential_key("https://a", "u").as_bytes()),
            hex_lower(credential_key("https://b", "u").as_bytes())
        );
    }

    #[test]
    fn new_error_variants_carry_no_secret_material() {
        let key = [7u8; KEY_BYTES];
        let blob = seal_envelope(&key, "https://x", "u", b"super-secret-pw").expect("seal");
        let formats = vec![
            format!("{:?}", CredentialError::KeyFileMissing),
            format!("{:?}", CredentialError::KeyFileInsecureMode),
            format!("{:?}", CredentialError::KeyFileCorrupt),
            format!("{:?}", CredentialError::EnvelopeCorrupt),
            format!("{:?}", CredentialError::EnvelopeVersion(9)),
            format!(
                "{:?}",
                open_envelope(&[8u8; KEY_BYTES], "https://x", "u", &blob).expect_err("auth fails")
            ),
            format!(
                "{:?}",
                open_envelope(&key, "https://x", "u", b"TBCE").expect_err("corrupt")
            ),
        ];
        for formatted in formats {
            assert!(!formatted.contains("super-secret-pw"));
            assert!(!formatted.contains("AAAA")); // no raw key bytes leak as base64
        }
    }

    #[test]
    fn env_store_is_read_only() {
        assert!(matches!(
            EnvCredentialStore.store("https://x", "u", AppPassword::new("v")),
            Err(CredentialError::Unsupported)
        ));
        assert!(matches!(
            EnvCredentialStore.delete("https://x", "u"),
            Err(CredentialError::Unsupported)
        ));
    }
}
