// SPDX-License-Identifier: AGPL-3.0-only
//! The dashboard capability token's resolution + generation ceremony
//! (K11, the K12 key-file ceremony shape).
//!
//! Resolution order: the explicit `[web] token` override wins; else the
//! persisted token file beside the database is reloaded; else a fresh
//! `uuid::v4` is generated and persisted, created with owner-only
//! permissions. A pre-existing file with looser permissions is the
//! typed [`TokenError::LoosePermissions`] — never a silent chmod, and
//! never a tokenless serve on a LAN bind (that refusal is the server
//! spawn's, which needs the resolution *origin*).

use std::io::Write as _;
use std::path::{Path, PathBuf};

/// Where the resolved token came from — the fact the LAN-bind refusal
/// keys on: only a *generated* token proves nothing was resolvable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenOrigin {
    /// The explicit `[web] token` operator override.
    Explicit,
    /// Reloaded from the persisted `<db-path>.web-token` file.
    Persisted,
    /// Freshly generated and persisted this boot.
    Generated,
}

/// Token ceremony failure classes (typed; surfaced by the daemon boot).
#[derive(Debug, thiserror::Error)]
pub enum TokenError {
    /// A token file already exists but carries group/other permission
    /// bits — refused so a world-readable capability can never silently
    /// serve; the remedy is removing the file and rebooting.
    #[error("web token file has loose permissions")]
    LoosePermissions(PathBuf),
    /// The token file exists but could not be read.
    #[error("web token file unreadable")]
    Read(PathBuf, #[source] std::io::Error),
    /// The token file could not be created or written.
    #[error("web token file unwritable")]
    Write(PathBuf, #[source] std::io::Error),
}

/// `<db-path>.web-token` — the token artifact beside the database (the
/// same directory the db, lock, and config sidecars live in, whose
/// permissions are the OS part of the auth story).
#[must_use]
pub fn token_path(db_path: &Path) -> PathBuf {
    let mut name = db_path.as_os_str().to_owned();
    name.push(".web-token");
    PathBuf::from(name)
}

/// Resolves the capability token: explicit override → persisted file →
/// fresh generation. The [`TokenOrigin`] travels with the token so the
/// spawn site can refuse a non-loopback bind whose token was just
/// minted (nothing resolvable was found — the operator was never shown
/// the capability, and the log line alone is not consent).
///
/// # Errors
///
/// [`TokenError`] — typed, see the variant docs.
pub fn resolve_token(
    explicit: Option<&str>,
    db_path: &Path,
) -> Result<(String, TokenOrigin), TokenError> {
    if let Some(token) = explicit {
        let token = token.trim();
        if !token.is_empty() {
            return Ok((token.to_owned(), TokenOrigin::Explicit));
        }
    }
    let path = token_path(db_path);
    match std::fs::read_to_string(&path) {
        Ok(raw) => {
            let token = raw.trim();
            if !token.is_empty() {
                check_permissions(&path)?;
                return Ok((token.to_owned(), TokenOrigin::Persisted));
            }
            // An empty file is not a token: fall through to generation
            // (which `create_new`s — replace the empty artifact first).
            std::fs::remove_file(&path).map_err(|err| TokenError::Write(path.clone(), err))?;
            generate(&path)
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => generate(&path),
        Err(err) => Err(TokenError::Read(path, err)),
    }
}

/// Mints a fresh `uuid::v4` token and persists it, creating the file
/// owner-only (0600 on unix; the create itself carries the mode — never
/// a chmod after the fact, so no window exists where the capability
/// sits world-readable).
fn generate(path: &Path) -> Result<(String, TokenOrigin), TokenError> {
    let token = uuid::Uuid::new_v4().simple().to_string();
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .map_err(|err| TokenError::Write(path.to_path_buf(), err))?;
    file.write_all(token.as_bytes())
        .and_then(|()| file.write_all(b"\n"))
        .map_err(|err| TokenError::Write(path.to_path_buf(), err))?;
    file.sync_all().ok();
    Ok((token, TokenOrigin::Generated))
}

/// The persisted token must be owner-only. Group/other bits on the
/// capability file are the loose-permissions refusal — the silent-chmod
/// alternative would paper over whatever put the file there.
#[cfg(unix)]
fn check_permissions(path: &Path) -> Result<(), TokenError> {
    use std::os::unix::fs::PermissionsExt as _;
    let mode = std::fs::metadata(path)
        .map_err(|err| TokenError::Read(path.to_path_buf(), err))?
        .permissions()
        .mode();
    if mode & 0o077 != 0 {
        return Err(TokenError::LoosePermissions(path.to_path_buf()));
    }
    Ok(())
}

/// Nothing to check on Windows: the token file's protection is the
/// volume's ACL story, exactly like the db beside it.
#[cfg(not(unix))]
fn check_permissions(_path: &Path) -> Result<(), TokenError> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{TokenError, TokenOrigin, resolve_token, token_path};

    fn tempdir() -> tempfile::TempDir {
        tempfile::tempdir().expect("tempdir")
    }

    /// The full ceremony: a first resolve generates and persists, a
    /// second resolve reloads the same token, and the file is
    /// owner-only.
    #[test]
    fn generate_then_reload_returns_the_same_token() {
        let dir = tempdir();
        let db = dir.path().join("taskboard.db");

        let (first, origin) = resolve_token(None, &db).expect("generates");
        assert_eq!(origin, TokenOrigin::Generated);
        let (second, origin) = resolve_token(None, &db).expect("reloads");
        assert_eq!(origin, TokenOrigin::Persisted);
        assert_eq!(first, second);
        assert_eq!(first.len(), 32, "a uuid::v4 simple string");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let mode = std::fs::metadata(token_path(&db))
                .expect("token file")
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600, "created owner-only");
        }
    }

    /// The explicit override wins over both the file and generation,
    /// and is never rewritten to disk.
    #[test]
    fn the_explicit_override_wins_and_writes_nothing() {
        let dir = tempdir();
        let db = dir.path().join("taskboard.db");
        let (token, origin) = resolve_token(Some("operator-secret"), &db).expect("resolves");
        assert_eq!(origin, TokenOrigin::Explicit);
        assert_eq!(token, "operator-secret");
        assert!(!token_path(&db).exists(), "generation never ran");

        // The override also dominates a persisted token.
        std::fs::write(token_path(&db), "persisted").expect("seed file");
        let (token, origin) = resolve_token(Some("operator-secret"), &db).expect("resolves");
        assert_eq!(
            (token.as_str(), origin),
            ("operator-secret", TokenOrigin::Explicit)
        );
    }

    /// A persisted token with group/other bits is the typed
    /// loose-permissions refusal — no silent chmod, no regeneration
    /// over the operator's artifact.
    #[cfg(unix)]
    #[test]
    fn loose_permissions_on_a_persisted_file_are_refused() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = tempdir();
        let db = dir.path().join("taskboard.db");
        let path = token_path(&db);
        std::fs::write(&path, "persisted").expect("seed file");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).expect("chmod");

        match resolve_token(None, &db) {
            Err(TokenError::LoosePermissions(p)) => assert_eq!(p, path),
            other => panic!("expected LoosePermissions, got {other:?}"),
        }
        // 0600 and owner-only-readables pass.
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).expect("chmod");
        let (_, origin) = resolve_token(None, &db).expect("0600 reloads");
        assert_eq!(origin, TokenOrigin::Persisted);
    }

    /// An unwritable token location is the typed write refusal (the
    /// token file's parent is missing), never a panic and never a
    /// tokenless continue.
    #[test]
    fn an_unwritable_location_is_the_typed_write_refusal() {
        let dir = tempdir();
        let db = dir.path().join("missing-dir").join("taskboard.db");
        match resolve_token(None, &db) {
            Err(TokenError::Write(path, _)) => assert_eq!(path, token_path(&db)),
            other => panic!("expected Write, got {other:?}"),
        }
    }

    /// An unreadable persisted artifact (a directory in the file's
    /// place) is the typed read refusal.
    #[cfg(unix)]
    #[test]
    fn an_unreadable_persisted_artifact_is_the_typed_read_refusal() {
        let dir = tempdir();
        let db = dir.path().join("taskboard.db");
        std::fs::create_dir(token_path(&db)).expect("a directory blocks the read");
        match resolve_token(None, &db) {
            Err(TokenError::Read(path, _)) => assert_eq!(path, token_path(&db)),
            other => panic!("expected Read, got {other:?}"),
        }
    }
}
