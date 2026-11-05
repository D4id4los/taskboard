// SPDX-License-Identifier: MIT OR Apache-2.0
//! The verbatim etag newtype and the in-memory etag store (decision
//! D-FT4).
//!
//! [`Etag`] is the raw header value **as served** — quoted form
//! included. The type's whole job is the verbatim rule: never trim,
//! never strip, never re-quote (the characterization F4 finding — an
//! early `412` was a client double-quoting bug, not server behaviour —
//! is pinned at the wire by the docker tier, not here in a comment).
//!
//! [`EtagStore`] is process-local bookkeeping: last-known version
//! tokens per file plus the per-device directory tokens that drive
//! change detection. In-memory only — a restart starts empty, which
//! costs one honest full-read cycle (the fold is idempotent; no
//! persistence exists by design, D-SF19 zero migrations).

use std::collections::BTreeMap;
use std::fmt;

use taskboard_sync_files::ids::DeviceId;

/// A version token exactly as the server served it (`ETag` response
/// header value; the `If-Match` request header value it feeds).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Etag(String);

impl Etag {
    /// The only constructor: the raw header value, byte-for-byte.
    /// Implementors of the transport seam build these from responses;
    /// consumers hand them back untouched.
    #[must_use]
    pub fn new(raw: String) -> Self {
        Self(raw)
    }
}

impl fmt::Display for Etag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // The verbatim rule *is* the Display: the served octets go back
        // out as-is, so `If-Match` echoes what the server handed us.
        f.write_str(&self.0)
    }
}

impl AsRef<str> for Etag {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Last-known version tokens, keyed per file — plus the per-device
/// directory tokens that drive change detection (F6's cheap
/// whole-subtree dirty check).
///
/// Store-write rules (the cycle's contract): PROPFIND members write
/// `device_dirs` (and prune devices that left the listing); GET
/// responses write `files`; PUT responses overwrite the own path's
/// `files` entry. A guarded rewrite is only ever issued when `files`
/// holds a token for the path — a cold store GETs first (the adopting
/// read), never PUTs unguarded.
#[derive(Debug, Default)]
pub struct EtagStore {
    device_dirs: BTreeMap<DeviceId, Etag>,
    files: BTreeMap<String, Etag>,
}

impl EtagStore {
    /// Upserts a device directory's tree token from the latest listing.
    pub fn record_device_dir(&mut self, device: DeviceId, etag: Etag) {
        self.device_dirs.insert(device, etag);
    }

    /// Drops every device-directory token absent from `present` — the
    /// prune-on-vanish half of the PROPFIND write rule (the tree is the
    /// truth; the trashbin is the user's recovery).
    pub fn retain_device_dirs(&mut self, present: impl Fn(&DeviceId) -> bool) {
        self.device_dirs.retain(|device, _| present(device));
    }

    /// The last-known directory token of a device, for the dirty-set
    /// compare.
    #[must_use]
    pub fn device_dir_etag(&self, device: &DeviceId) -> Option<&Etag> {
        self.device_dirs.get(device)
    }

    /// Upserts a file's last-known token (GET/PUT responses).
    pub fn record_file(&mut self, relative_path: impl Into<String>, etag: Etag) {
        self.files.insert(relative_path.into(), etag);
    }

    /// Drops a file's token (e.g. the own-file-vanished disambiguation;
    /// the next cycle recreates via create-once).
    pub fn forget_file(&mut self, relative_path: &str) {
        self.files.remove(relative_path);
    }

    /// The last-known token of a file, for the `If-Match` guard
    /// (present = a guarded rewrite is issuable).
    #[must_use]
    pub fn file_etag(&self, relative_path: &str) -> Option<&Etag> {
        self.files.get(relative_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(name: &str) -> DeviceId {
        DeviceId::parse(name).expect("fixture id parses")
    }

    fn etag(raw: &str) -> Etag {
        Etag::new(raw.to_owned())
    }

    #[test]
    fn etag_display_and_as_ref_are_verbatim() {
        // The quoted, internally-spaced served form survives untouched —
        // the type carries octets, it normalises nothing.
        let served = "\"a-b-3\"-with space";
        let tag = etag(served);
        assert_eq!(tag.to_string(), served);
        assert_eq!(tag.as_ref(), served);
    }

    #[test]
    fn device_dir_etags_upsert_and_prune_on_vanish() {
        let alpha = device("0193a1b2-c3d4-7cc9-8a2e-5f1e2d3c4b5a");
        let beta = device("0193a1b2-c3d4-7cc9-8a2e-5f1e2d3c4b5b");
        let mut store = EtagStore::default();
        store.record_device_dir(alpha, etag("\"one\""));
        store.record_device_dir(beta, etag("\"two\""));
        assert_eq!(store.device_dir_etag(&alpha), Some(&etag("\"one\"")));
        // The next listing answers only for `beta`: `alpha` vanished.
        store.retain_device_dirs(|d| *d == beta);
        assert_eq!(store.device_dir_etag(&alpha), None);
        assert_eq!(store.device_dir_etag(&beta), Some(&etag("\"two\"")));
    }

    #[test]
    fn file_etags_upsert_and_forget() {
        let mut store = EtagStore::default();
        let path = "devices/0193a1b2-c3d4-7cc9-8a2e-5f1e2d3c4b5a/info.json";
        assert_eq!(store.file_etag(path), None);
        store.record_file(path, etag("\"t1\""));
        assert_eq!(store.file_etag(path), Some(&etag("\"t1\"")));
        // A PUT response overwrites; a vanish forgets.
        store.record_file(path, etag("\"t2\""));
        assert_eq!(store.file_etag(path), Some(&etag("\"t2\"")));
        store.forget_file(path);
        assert_eq!(store.file_etag(path), None);
    }

    #[test]
    fn the_store_has_no_read_back_beyond_the_cycles_questions() {
        // Structural pin, not a re-test: the accessors are exactly
        // `device_dir_etag` / `file_etag` plus the three write rules —
        // no bulk dump, no map exposure. The fields stay private, so
        // the pin holds by construction.
        let store = EtagStore::default();
        let _ = store.device_dir_etag(&device("0193a1b2-c3d4-7cc9-8a2e-5f1e2d3c4b5a"));
        let _ = store.file_etag("anything");
    }
}
