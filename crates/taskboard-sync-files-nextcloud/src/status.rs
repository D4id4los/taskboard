// SPDX-License-Identifier: MIT OR Apache-2.0
//! The files-sync status types and the typed findings (decision
//! D-FT6). Chunk 2 *emits* findings — latest-state semantics on a
//! `tokio::sync::watch<FilesSyncStatus>`; the surface that consumes
//! them (sync status view, CLI `status` line, runbooks) is chunk 4's.
//! "Surface loudly" in this chunk means exactly: the typed state
//! transition plus a `tracing::error!` record on the fork alarm.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};

use taskboard_sync_files::ids::DeviceId;

/// A peer device's latest decoded heartbeat — the freshness inputs
/// (design Part F's "synced as of N min ago" consumer). Presence
/// facts only; the divergence comparison consumes `info.json`'s
/// `state_hash` actor-side and never surfaces it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerFreshness {
    /// When the peer last wrote its heartbeat (presence freshness).
    pub written_at: DateTime<Utc>,
    /// The newest entry clock the peer has folded (data freshness).
    pub last_clock: DateTime<Utc>,
}

/// How the last cycle ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CycleOutcome {
    /// The cycle ran to its end; `published` says whether a PUT went
    /// out, `read_devices` how many peers contributed, and
    /// `import_rejected` how many peer rows could not become engine
    /// inputs (still relayed — the counter is chunk 4's surface).
    Ok {
        published: bool,
        read_devices: usize,
        import_rejected: usize,
    },
    /// The cycle failed (typed transport outcome or a loud finding);
    /// the actor's backoff reschedules.
    Failed,
}

/// Why a peer file is parked (D-SF15): folded from nothing, stored
/// verbatim on the server, surfaced as a finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParkKind {
    /// The peer speaks a newer document format — fold nothing from it
    /// (our older reader cannot represent its rows honestly).
    NewerFormat { format: u32 },
    /// The file did not decode — left untouched on the server (the
    /// trashbin is the user's recovery path; readers never delete).
    Quarantined,
}

/// The latest files-sync state a consumer can observe.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FilesSyncStatus {
    pub last_cycle: Option<CycleOutcome>,
    /// The live stop-write set (the fork alarm's paths).
    pub forked_paths: BTreeSet<String>,
    /// Peer files parked (not folded) and why.
    pub parked: BTreeMap<String, ParkKind>,
    /// When this device last wrote its own heartbeat (`info.json`).
    pub last_heartbeat_written: Option<DateTime<Utc>>,
    /// Latest decoded heartbeat per peer (freshness is an age display,
    /// never an alarm — no thresholds live here).
    pub peers: BTreeMap<DeviceId, PeerFreshness>,
    /// Peers whose heartbeat state hash disagrees with this device's
    /// current view past the quiescence window (two consecutive quiet
    /// cycles — the pure derivation in `cycle.rs`). Empty means
    /// converged — or not yet judgeable.
    pub diverged_from: BTreeSet<DeviceId>,
}
