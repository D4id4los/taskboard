// SPDX-License-Identifier: AGPL-3.0-only
//! The per-database instance lock (ADR 0009): an OS advisory exclusive
//! lock on a `<db_path>.lock` sidecar, held for the process lifetime
//! and released automatically on process death — no staleness
//! heuristics exist to get wrong (the classic `O_EXCL` pitfall). The
//! PID in the file body is diagnostic only; the OS lock is
//! authoritative.
//!
//! Scope: any core boot locks (`daemon`, `boards select`, `tasks *`,
//! `sync`); pre-core commands (`login`, `boards list`) do not. The lock
//! is scoped to the database file, so worktrees/CI processes with
//! distinct db paths never contend. Caveat (documented): advisory locks
//! on NFS/network filesystems are unreliable — sqlite on NFS is already
//! unsupported territory.
//!
//! # Usage
//!
//! ```
//! # fn demo(dir: std::path::PathBuf) -> Result<(), Box<dyn std::error::Error>> {
//! use taskboard_app::lockfile::InstanceLock;
//!
//! let db = dir.join("taskboard.db");
//! let _lock = InstanceLock::acquire(&db)?;
//!
//! // A second instance on the same database fails fast (never blocks):
//! let second = InstanceLock::acquire(&db);
//! assert!(matches!(
//!     second,
//!     Err(taskboard_app::lockfile::LockError::Held { .. })
//! ));
//!
//! // Releasing (drop) lets the next instance in:
//! drop(_lock);
//! let _again = InstanceLock::acquire(&db)?;
//! # Ok(())
//! # }
//! # let dir = std::env::temp_dir().join(format!("taskboard-doc-lock-{}", std::process::id()));
//! # std::fs::create_dir_all(&dir).unwrap();
//! # demo(dir).unwrap();
//! ```

use std::fs::File;
use std::io::{Read as _, Seek as _, Write as _};
use std::path::{Path, PathBuf};

use fd_lock::RwLock;

/// Exclusive advisory lock on `<db_path>.lock`, held for the process
/// lifetime (dropped with [`crate::App`]). OS-released on death; the
/// PID in the file body is diagnostic only.
///
/// Implementation note: `fd-lock`'s guard borrows its `RwLock<File>`,
/// so the lock is `Box::leak`ed to give the guard a `'static` lifetime.
/// The leaked allocation (one file handle) lives exactly as long as it
/// needs to — the lock is process-lifetime by design — and the OS
/// releases everything on process death regardless.
pub struct InstanceLock {
    /// Kept for diagnostics: the sidecar path.
    path: PathBuf,
    _guard: fd_lock::RwLockWriteGuard<'static, File>,
}

/// Lock failure classes, matched on variant shape only.
#[derive(Debug, thiserror::Error)]
pub enum LockError {
    /// Another taskboard instance holds this database (fail fast,
    /// never block).
    #[error("another taskboard instance holds this database")]
    Held {
        /// The holder PID recorded in the sidecar (best-effort,
        /// diagnostic only).
        holder_pid: Option<u32>,
    },
    /// The lock file could not be opened or written.
    #[error("lock file could not be opened")]
    Io(#[source] std::io::Error),
}

impl InstanceLock {
    /// Opens/creates `<db_path>.lock` and takes the exclusive write
    /// lock (`try_lock` semantics: fail fast, never block), writing the
    /// holder PID into the file body.
    ///
    /// # Errors
    ///
    /// [`LockError`] — typed, see the variant docs.
    pub fn acquire(db_path: &Path) -> Result<Self, LockError> {
        let path = lock_path(db_path);
        // Create without truncating: an existing sidecar's recorded
        // holder PID must survive until *this* acquire overwrites it
        // (it is read for the `Held` diagnostic after a failed
        // try_lock).
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .map_err(LockError::Io)?;
        let lock: &'static mut RwLock<File> = Box::leak(Box::new(RwLock::new(file)));
        let mut guard = match lock.try_write() {
            Ok(guard) => guard,
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                return Err(LockError::Held {
                    holder_pid: read_holder_pid(&path),
                });
            }
            Err(err) => return Err(LockError::Io(err)),
        };
        let body = format!("pid={}\n", std::process::id());
        guard
            .seek(std::io::SeekFrom::Start(0))
            .and_then(|_| guard.write_all(body.as_bytes()))
            // Truncate to the new body: a shorter pid following a longer
            // one must not leave residue the diagnostic read cannot parse.
            .and_then(|()| guard.set_len(body.len() as u64))
            .and_then(|()| guard.flush())
            .map_err(LockError::Io)?;
        tracing::info!(lock = %path.display(), "instance lock acquired");
        Ok(Self {
            path,
            _guard: guard,
        })
    }

    /// The sidecar path (diagnostics).
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// `<db>.lock` next to `<db>` (`taskboard.db` → `taskboard.lock`).
fn lock_path(db_path: &Path) -> PathBuf {
    let mut name = db_path
        .file_stem()
        .unwrap_or(db_path.as_os_str())
        .to_owned();
    name.push(".lock");
    db_path.with_file_name(name)
}

/// Best-effort read of the recorded holder PID (diagnostic only).
fn read_holder_pid(path: &Path) -> Option<u32> {
    let mut text = String::new();
    File::open(path).ok()?.read_to_string(&mut text).ok()?;
    text.trim_start_matches("pid=").trim().parse().ok()
}

/// Best-effort read of the holder PID recorded in `db_path`'s lock
/// sidecar — the `status` client-side answer's diagnostic field. No
/// lock is taken (or held) by this read; the recorded PID may belong to
/// a dead process (diagnostic only, exactly like the sidecar itself).
#[must_use]
pub fn holder_pid(db_path: &Path) -> Option<u32> {
    read_holder_pid(&lock_path(db_path))
}
