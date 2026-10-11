// SPDX-License-Identifier: AGPL-3.0-only
//! The daemon IPC frontend (ADR 0012, WI-5): a local-socket command
//! transport over the existing core. The daemon binds a per-database
//! socket and serves one request per connection; a CLI invocation whose
//! database is served relays its raw argv over the wire and prints the
//! daemon's rendered `{code, out, err}` answer verbatim — byte-identical
//! to the in-process dispatch (ADR 0009's contract survives transport).
//!
//! Module map:
//!
//! - [`protocol`]: the versioned `Request`/`Response` envelopes.
//! - [`codec`]: u32-LE length-prefixed JSON framing with size caps.
//! - [`server`]: the daemon side — bind, accept, serial execution.
//! - [`client`]: the CLI side — connect, request, typed failures.
//!
//! Naming follows the instance-lock sidecar precedent: unix sockets are
//! the `<db>.sock` sibling of `<db>` (`taskboard.db` → `taskboard.sock`),
//! Windows named pipes use a collision-free namespaced name derived from
//! the absolute db path.

pub mod client;
pub mod codec;
pub mod protocol;
pub mod server;

use std::path::Path;

/// The unix socket sidecar path: `<db-dir>/<stem>.sock`
/// (`taskboard.db` → `taskboard.sock`), the `<db>.lock` naming rule
/// verbatim. Also the artifact `spawn_server` recovers (stale file from
/// a crashed daemon) and removes on graceful shutdown.
#[cfg(unix)]
#[must_use]
pub fn socket_sidecar(db_path: &Path) -> std::path::PathBuf {
    let mut name = db_path
        .file_stem()
        .unwrap_or(db_path.as_os_str())
        .to_owned();
    name.push(".sock");
    db_path.with_file_name(name)
}

/// The Windows namespaced pipe name for a db path:
/// `taskboard-<fnv1a64(abs-db-path)>.sock`. Pure over the injected path
/// (no `cfg` — unit-pinned on every platform); namespaced names have no
/// filesystem artifact, so nothing can go stale and first live listener
/// wins.
#[must_use]
pub fn namespaced_name_shape(db_path: &Path) -> String {
    let hashed = db_path
        .canonicalize()
        .unwrap_or_else(|_| db_path.to_path_buf());
    let bytes = hashed.as_os_str().as_encoded_bytes();
    format!("taskboard-{:016x}.sock", fnv1a64(bytes))
}

/// The local-socket name for the daemon serving `db_path`'s database:
/// the unix `<db>.sock` sidecar, or the Windows namespaced name.
///
/// # Errors
///
/// [`std::io::Error`] when the platform name derivation fails (the
/// sidecar path is a pure join; only exotic `Name` conversions can
/// fail).
pub fn socket_name(db_path: &Path) -> std::io::Result<interprocess::local_socket::Name<'static>> {
    #[cfg(unix)]
    use interprocess::local_socket::{GenericFilePath, ToFsName};
    #[cfg(not(unix))]
    use interprocess::local_socket::{GenericNamespaced, ToNsName};
    #[cfg(unix)]
    {
        socket_sidecar(db_path).to_fs_name::<GenericFilePath>()
    }
    #[cfg(not(unix))]
    {
        namespaced_name_shape(db_path).to_ns_name::<GenericNamespaced>()
    }
}

/// FNV-1a 64 (the stable, dependency-free hash): pinned here because
/// `std::hash::DefaultHasher` makes no cross-version stability
/// guarantee and the Windows name must be identical across compiler
/// versions (client and daemon may be built separately).
#[must_use]
pub(crate) fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::{fnv1a64, namespaced_name_shape, socket_sidecar};
    use std::path::Path;

    /// FNV-1a 64 fixed vectors (the standard test strings).
    #[test]
    fn fnv1a64_matches_the_fixed_vectors() {
        assert_eq!(fnv1a64(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a64(b"a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fnv1a64(b"foobar"), 0x8594_4171_f739_67e8);
    }

    /// The unix sidecar mirrors the lock-file naming rule.
    #[cfg(unix)]
    #[test]
    fn unix_sidecar_is_the_db_sibling() {
        assert_eq!(
            socket_sidecar(Path::new("/data/taskboard.db")),
            Path::new("/data/taskboard.sock")
        );
        assert_eq!(socket_sidecar(Path::new("r7.db")), Path::new("r7.sock"));
    }

    /// The Windows name shape: the `taskboard-` prefix, 16 hex digits,
    /// `.sock` suffix — deterministic over the same path and injectable
    /// without a platform gate.
    #[test]
    fn namespaced_name_has_the_pinned_shape() {
        let name = namespaced_name_shape(Path::new("/data/taskboard.db"));
        let hex = name
            .strip_prefix("taskboard-")
            .and_then(|rest| rest.strip_suffix(".sock"))
            .expect("taskboard-<16 hex>.sock shape");
        assert_eq!(hex.len(), 16);
        assert!(hex.bytes().all(|b| b.is_ascii_hexdigit()));
        assert_eq!(name, namespaced_name_shape(Path::new("/data/taskboard.db")));
        assert_ne!(
            namespaced_name_shape(Path::new("/data/other.db")),
            name,
            "distinct db paths must not collide"
        );
    }
}
