// SPDX-License-Identifier: MIT OR Apache-2.0
//! The device identity ceremony (decision D-FT7): the create-once
//! `device.json` in the app data dir. Path resolution stays app-side —
//! bootstrap injects the resolved `PathBuf`, so no `dirs` dependency
//! enters this crate.
//!
//! The ceremony's whole job is the fork guard: a corrupt file is a
//! **typed refusal, never a regeneration** — minting a new device id
//! over the old file's tree would silently fork the device
//! (D-SF4's alarm exists precisely for that condition). The write is
//! atomic (`<file>.tmp-<suffix>` then rename), which is why the
//! layout classifier's `Temporary` arm exists: a crash mid-write can
//! leak the temp file, never a torn identity.

use std::path::{Path, PathBuf};

use taskboard_domain::Clock;
use taskboard_sync_files::identity::DeviceIdentity;
use taskboard_sync_files::ids::DeviceId;
use thiserror::Error;

/// Typed failures of the identity ceremony.
#[derive(Debug, Error)]
pub enum DeviceIdentityError {
    /// The file exists but does not decode as a strict-envelope
    /// `DeviceIdentity`. **Never regenerated** — the caller parks the
    /// transport instead (silent-fork guard).
    #[error("device identity unusable")]
    Unusable {
        /// The offending file (diagnostics only).
        path: PathBuf,
    },
    /// The identity file could not be read or (on create) written.
    #[error("device identity io failure")]
    Io {
        /// The offending file.
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// Loads the identity at `path`, creating it exactly once when absent.
///
/// # Errors
///
/// [`DeviceIdentityError::Unusable`] on a decode failure (bytes left
/// untouched), [`DeviceIdentityError::Io`] on read/write failures.
pub async fn load_or_create(
    path: &Path,
    clock: &dyn Clock,
) -> Result<DeviceIdentity, DeviceIdentityError> {
    match tokio::fs::read(path).await {
        Ok(bytes) => {
            // Strict envelope, extras tolerated (the chunk-1 payload
            // decode posture). A corrupt file is never regenerated.
            serde_json::from_slice(&bytes).map_err(|_| DeviceIdentityError::Unusable {
                path: path.to_owned(),
            })
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            let identity = DeviceIdentity {
                device_id: DeviceId::mint(),
                created_at: clock.now(),
            };
            write_atomically(path, &identity).await?;
            Ok(identity)
        }
        Err(err) => Err(DeviceIdentityError::Io {
            path: path.to_owned(),
            source: err,
        }),
    }
}

/// Writes the payload through `<file>.tmp-<suffix>` then rename — a
/// crash mid-write leaks a temp file (which the tree classifier
/// tolerates as `Temporary`), never a torn identity.
async fn write_atomically(
    path: &Path,
    identity: &DeviceIdentity,
) -> Result<(), DeviceIdentityError> {
    let bytes = serde_json::to_vec_pretty(identity).expect("identity always serializes");
    let suffix = DeviceId::mint().to_string();
    let tmp = path.with_extension(format!("json.tmp-{suffix}"));
    tokio::fs::write(&tmp, &bytes)
        .await
        .map_err(|source| DeviceIdentityError::Io {
            path: tmp.clone(),
            source,
        })?;
    tokio::fs::rename(&tmp, path)
        .await
        .map_err(|source| DeviceIdentityError::Io {
            path: path.to_owned(),
            source,
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Utc};

    /// A fixed clock: the ceremony is deterministic under it.
    #[derive(Debug)]
    struct FixedClock(DateTime<Utc>);

    impl Clock for FixedClock {
        fn now(&self) -> DateTime<Utc> {
            self.0
        }
    }

    fn temp_dir() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("device.json");
        (dir, path)
    }

    #[tokio::test]
    async fn create_once_mints_writes_and_rereads_the_identity() {
        let (_dir, path) = temp_dir();
        let clock = FixedClock("2026-11-05T09:15:32Z".parse::<DateTime<Utc>>().unwrap());
        let identity = load_or_create(&path, &clock).await.expect("creates");
        // The re-read yields exactly the written identity (create-once:
        // a second ceremony call is a load, not a re-mint).
        let again = load_or_create(&path, &clock).await.expect("reloads");
        assert_eq!(identity, again);
        let raw = tokio::fs::read(&path).await.unwrap();
        let decoded: DeviceIdentity = serde_json::from_slice(&raw).unwrap();
        assert_eq!(decoded, identity);
        assert_eq!(identity.created_at, clock.0);
    }

    #[tokio::test]
    async fn the_atomic_write_leaves_no_temp_residue() {
        let (_dir, path) = temp_dir();
        let clock = FixedClock(Utc::now());
        let _ = load_or_create(&path, &clock).await.expect("creates");
        let mut siblings = tokio::fs::read_dir(path.parent().unwrap()).await.unwrap();
        while let Some(entry) = siblings.next_entry().await.unwrap() {
            assert_eq!(
                entry.file_name(),
                path.file_name().unwrap(),
                "a temp residue leaked beside the identity file"
            );
        }
    }

    #[tokio::test]
    async fn a_corrupt_file_is_a_typed_refusal_with_bytes_untouched() {
        let (_dir, path) = temp_dir();
        let corrupt = b"{{{ not json at all";
        tokio::fs::write(&path, corrupt).await.unwrap();
        let clock = FixedClock(Utc::now());
        match load_or_create(&path, &clock).await {
            Err(DeviceIdentityError::Unusable { path: p }) => assert_eq!(p, path),
            other => panic!("expected Unusable, got {other:?}"),
        }
        // The refusal never rewrites: the bytes are exactly what was there.
        let after = tokio::fs::read(&path).await.unwrap();
        assert_eq!(after, corrupt);
    }

    #[tokio::test]
    async fn extras_on_the_envelope_are_tolerated() {
        let (_dir, path) = temp_dir();
        tokio::fs::write(
            &path,
            br#"{"device_id":"0193a1b2-c3d4-7cc9-8a2e-5f1e2d3c4b5a","created_at":"2026-11-05T09:15:32Z","future_key":42}"#,
        )
        .await
        .unwrap();
        let identity = load_or_create(&path, &FixedClock(Utc::now()))
            .await
            .expect("decodes");
        assert_eq!(
            identity.device_id.to_string(),
            "0193a1b2-c3d4-7cc9-8a2e-5f1e2d3c4b5a"
        );
    }

    #[tokio::test]
    async fn a_missing_parent_is_an_io_failure() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("nope").join("device.json");
        // The atomic write's temp file sits beside the target, so the
        // Io failure's offending path names a sibling of `path` — the
        // structural fact (no rename ever happened) is what matters.
        match load_or_create(&path, &FixedClock(Utc::now())).await {
            Err(DeviceIdentityError::Io { path: p, .. }) => {
                assert_eq!(p.parent(), path.parent());
                assert!(p.to_string_lossy().contains(".tmp-"));
            }
            other => panic!("expected Io, got {other:?}"),
        }
    }
}
