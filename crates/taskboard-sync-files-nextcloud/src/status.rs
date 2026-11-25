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

use crate::error::TransportError;

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
    /// The cycle unwound at a shutdown checkpoint (#1085): it ended
    /// mid-flight — no failure counted, no backoff armed, no report
    /// sent. The next boot's cycle re-derives everything from the
    /// server listing (the protocol is crash-safe by construction).
    Aborted,
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

/// Why a moved board could not be adopted; the Deck binding is intact
/// and the lane retries each cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoardMoveRefusal {
    Collision,
    NotBoundToMovedDeck,
    RecordMalformed,
    RecordNewerFormat { format: u32 },
}

/// A device's view of a board another device moved here from Deck.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoardMoveFinding {
    Adopting {
        moved_by: DeviceId,
        committed_at: DateTime<Utc>,
    },
    Adopted {
        moved_by: DeviceId,
        committed_at: DateTime<Utc>,
    },
    Refused(BoardMoveRefusal),
}

/// A board lane's typed failure class (the transport outcome's
/// status-table projection, `#941`): the lane retries next tick, and
/// this fact is what keeps the status surfaces honest — a failing
/// lane must never read neutral Ok. Deliberately narrower than
/// [`TransportError`] (whose network variant cannot be `Eq`); no
/// message text survives the projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaneFailure {
    Transport,
    Unauthorized,
    Forbidden,
    NotFound,
    BadRequest,
    Conflict,
    PreconditionFailed,
    RateLimited,
    /// A document the lane produced or read did not decode.
    Payload,
    Server(u16),
    Unavailable,
}

impl From<&TransportError> for LaneFailure {
    fn from(err: &TransportError) -> Self {
        match err {
            TransportError::Transport(_) => Self::Transport,
            TransportError::Json(_) | TransportError::PropfindListing(_) => Self::Payload,
            TransportError::Unauthorized => Self::Unauthorized,
            TransportError::Forbidden => Self::Forbidden,
            TransportError::NotFound => Self::NotFound,
            // The invalid base URL and the 400 are the same class for
            // a lane: this build's request was wrong before the wire.
            TransportError::InvalidBaseUrl | TransportError::BadRequest => Self::BadRequest,
            TransportError::Conflict => Self::Conflict,
            TransportError::PreconditionFailed => Self::PreconditionFailed,
            TransportError::RateLimited { .. } => Self::RateLimited,
            TransportError::Server(status) => Self::Server(*status),
            TransportError::Unavailable => Self::Unavailable,
        }
    }
}

/// One board lane's last-tick outcome (typed, per bound board).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaneOutcome {
    /// The lane ran to completion this tick.
    Ok,
    /// The lane failed; the typed transport projection; retried next
    /// tick.
    Failed(LaneFailure),
}

/// The latest files-sync state a consumer can observe.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FilesSyncStatus {
    pub last_cycle: Option<CycleOutcome>,
    /// The board lanes' last-tick outcomes, keyed by the board's files
    /// identity — only boards the lanes currently serve appear; a lane
    /// that succeeded this tick carries [`LaneOutcome::Ok`] (the map
    /// is the whole truth of the last tick, not an accumulating
    /// alarm). Empty means no files-bound boards ran yet.
    pub lanes: BTreeMap<uuid::Uuid, LaneOutcome>,
    /// The live stop-write set (the fork alarm's paths).
    pub forked_paths: BTreeSet<String>,
    /// Peer files parked (not folded) and why.
    pub parked: BTreeMap<String, ParkKind>,
    /// When this device last wrote its own heartbeat (`info.json`).
    pub last_heartbeat_written: Option<DateTime<Utc>>,
    /// Latest decoded heartbeat per peer (freshness is an age display,
    /// never an alarm — no thresholds live here).
    pub peers: BTreeMap<DeviceId, PeerFreshness>,
    /// Peers that already absorbed this device's current view yet
    /// whose heartbeat state hash disagrees with it past the
    /// quiescence window (two consecutive quiet cycles — the pure
    /// derivation in `cycle.rs`). A lagging peer never appears here.
    /// Empty means converged — or not yet judgeable.
    pub diverged_from: BTreeSet<DeviceId>,
    /// Boards moved from Deck by another device, by files identity.
    pub moved_boards: BTreeMap<uuid::Uuid, BoardMoveFinding>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::TransportError;

    /// Kills: any swapped, renamed, or dropped projection arm in
    /// `From<&TransportError> for LaneFailure` — the exhaustive
    /// one-row-per-variant table, including the deliberate collapses
    /// (`InvalidBaseUrl` joins `BadRequest`; the `Json`/`PropfindListing`
    /// pair joins `Payload`; `Server` carries the status through).
    #[tokio::test]
    async fn lane_failure_projection_table_covers_every_transport_error() {
        // The one variant whose payload is a foreign error type: the
        // real reqwest construction (the connect-refused dial, the
        // mapping.rs precedent).
        let transport_err = reqwest::get("http://127.0.0.1:1/")
            .await
            .expect_err("dialing port 1 refuses");
        let rows: Vec<(TransportError, LaneFailure)> = vec![
            (
                TransportError::Transport(transport_err),
                LaneFailure::Transport,
            ),
            (
                TransportError::Json(serde_json::from_str::<()>("nope").unwrap_err()),
                LaneFailure::Payload,
            ),
            (
                TransportError::PropfindListing(
                    crate::extract::extract_207(b"not a 207").unwrap_err(),
                ),
                LaneFailure::Payload,
            ),
            (TransportError::Unauthorized, LaneFailure::Unauthorized),
            (TransportError::Forbidden, LaneFailure::Forbidden),
            (TransportError::NotFound, LaneFailure::NotFound),
            (TransportError::InvalidBaseUrl, LaneFailure::BadRequest),
            (TransportError::BadRequest, LaneFailure::BadRequest),
            (TransportError::Conflict, LaneFailure::Conflict),
            (
                TransportError::PreconditionFailed,
                LaneFailure::PreconditionFailed,
            ),
            (
                TransportError::RateLimited {
                    retry_after: Some(std::time::Duration::from_secs(7)),
                },
                LaneFailure::RateLimited,
            ),
            (TransportError::Server(521), LaneFailure::Server(521)),
            (TransportError::Unavailable, LaneFailure::Unavailable),
        ];
        for (err, expected) in rows {
            assert_eq!(
                LaneFailure::from(&err),
                expected,
                "the {err:?} projection drifted"
            );
        }
    }
}
