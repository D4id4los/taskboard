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

use chrono::{DateTime, Utc};
use taskboard_sync_files::ids::DeviceId;

/// Tolerance for second-granular remote-vs-local stamp comparisons:
/// 1 s of representation loss (both sides truncate to whole seconds,
/// #695's hosted evidence shows second-granular validator behavior) +
/// 1 s of NTP-class cross-machine skew (#133's audit premise). Matches
/// DR-4's `skew_band_secs` default by construction. Module-scope named
/// constant — no call-site magic numbers.
pub(crate) const SKEW_BAND_SECS: i64 = 2;

/// The stale-token 412 retry bound (the bounded-convergence spec): one
/// re-resolution per cycle. The bound is *structural* —
/// `cycle::resolve_own_412` performs at most one guarded re-PUT in its
/// control flow, with the typed `ForkDetected` give-up beyond it — the
/// constant names the law the structure implements, never consulted at
/// a call site.
#[cfg(test)]
pub(crate) const MAX_412_RESOLUTIONS: u32 = 1;

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
///
/// Dirty detection is **revision-field precedence** (the #695 fix):
/// an `ETag` equality alone never concludes "peer unchanged" — the peer
/// heartbeat's `written_at` (recorded here at read time) is the
/// revision, and a stamp advanced past its read mark by more than
/// [`SKEW_BAND_SECS`] re-dirties the peer even when every validator
/// was retained. Documented leniency: a peer that never serves a
/// decodable heartbeat falls back to pure `ETag` dirty detection (a
/// retained-etag rewrite of such a peer is invisible — today's
/// behavior, not a heuristic).
#[derive(Debug, Default)]
pub struct EtagStore {
    device_dirs: BTreeMap<DeviceId, Etag>,
    /// The directory token as of the device's last full read — the
    /// dirty compare's "already seen" half (F6: an unchanged tree etag
    /// skips the device's GETs).
    read_dirs: BTreeMap<DeviceId, Etag>,
    /// The peer heartbeat's `written_at` as of the device's last full
    /// read — the revision mark the dirty compare measures against
    /// (the #695 fix; never refreshed by the cheap heartbeat poll).
    read_heartbeats: BTreeMap<DeviceId, DateTime<Utc>>,
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

    /// The devices listed in the latest PROPFIND whose directory etag
    /// differs from their last-read etag — or whose freshly decoded
    /// heartbeat stamp (`heartbeats`: device → the peer heartbeat's
    /// `written_at`) advanced past its read mark by more than
    /// [`SKEW_BAND_SECS`] (the #695 revision-field precedence: a
    /// retained etag is never proof of sameness). Never-read devices
    /// are trivially dirty. In listing order (deterministic).
    #[must_use]
    pub fn dirty_devices(&self, heartbeats: &BTreeMap<DeviceId, DateTime<Utc>>) -> Vec<DeviceId> {
        self.device_dirs
            .iter()
            .filter(|(device, etag)| {
                if self.read_dirs.get(device) != Some(*etag) {
                    return true;
                }
                match (self.read_heartbeats.get(device), heartbeats.get(device)) {
                    (Some(mark), Some(stamp)) => {
                        *stamp > *mark + taskboard_domain::time::delta_seconds(SKEW_BAND_SECS)
                    }
                    _ => false,
                }
            })
            .map(|(device, _)| *device)
            .collect()
    }

    /// Marks a device's files as read at its current directory etag,
    /// recording the heartbeat stamp the read observed (the dirty
    /// compare's write half; the revision mark above).
    pub fn mark_device_read(
        &mut self,
        device: DeviceId,
        heartbeat_written_at: Option<DateTime<Utc>>,
    ) {
        if let Some(etag) = self.device_dirs.get(&device) {
            self.read_dirs.insert(device, etag.clone());
        }
        if let Some(stamp) = heartbeat_written_at {
            self.read_heartbeats.insert(device, stamp);
        }
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
    fn a_retained_etag_with_an_advanced_heartbeat_revision_is_dirty() {
        // The #695 false-clean bug: both validators identical, yet the
        // peer's heartbeat revision advanced beyond the skew band —
        // the peer must be pulled (revision-field precedence).
        let peer = device("0193a1b2-c3d4-7cc9-8a2e-5f1e2d3c4b5a");
        let read_at = "2026-11-05T09:00:00Z".parse().expect("stamp");
        let mut store = EtagStore::default();
        store.record_device_dir(peer, etag("\"one\""));
        store.mark_device_read(peer, Some(read_at));
        let stamps = BTreeMap::from([(peer, read_at + chrono::Duration::hours(1))]);
        assert_eq!(store.dirty_devices(&stamps), vec![peer]);
        // Within the band the stamp is unordered (second-granular
        // representation loss + clock skew), not newer: clean.
        let stamps = BTreeMap::from([(
            peer,
            read_at + taskboard_domain::time::delta_seconds(SKEW_BAND_SECS),
        )]);
        assert!(store.dirty_devices(&stamps).is_empty(), "clean");
        // A stamp at or before the mark: clean (quiet peer).
        let stamps = BTreeMap::from([(peer, read_at)]);
        assert!(store.dirty_devices(&stamps).is_empty(), "clean");
    }

    #[test]
    fn a_peer_without_a_heartbeat_revision_falls_back_to_the_etag_rule() {
        // Documented leniency: no decoded heartbeat (the stamps map
        // lacks the device) or no read mark ⇒ pure `ETag` dirty
        // detection, today's behavior.
        let peer = device("0193a1b2-c3d4-7cc9-8a2e-5f1e2d3c4b5a");
        let read_at = "2026-11-05T09:00:00Z".parse().expect("stamp");
        let later = read_at + chrono::Duration::hours(1);
        let mut store = EtagStore::default();
        store.record_device_dir(peer, etag("\"one\""));
        store.mark_device_read(peer, Some(read_at));
        // No decoded heartbeat this listing: nothing to measure.
        let stamps = BTreeMap::new();
        assert!(store.dirty_devices(&stamps).is_empty(), "clean");
        // A mark never recorded (a heartbeat-less read): same.
        let mut store = EtagStore::default();
        store.record_device_dir(peer, etag("\"one\""));
        store.mark_device_read(peer, None);
        let stamps = BTreeMap::from([(peer, later)]);
        assert!(store.dirty_devices(&stamps).is_empty(), "clean");
        // The etag rule still fires underneath.
        store.record_device_dir(peer, etag("\"two\""));
        assert_eq!(store.dirty_devices(&stamps), vec![peer]);
    }

    #[test]
    fn the_412_retry_bound_is_one_reresolution_per_cycle() {
        // The named law `resolve_own_412` implements structurally: a
        // 412 resolves at most once per cycle — a second 412 is the
        // typed give-up, never a third PUT.
        assert_eq!(MAX_412_RESOLUTIONS, 1);
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
