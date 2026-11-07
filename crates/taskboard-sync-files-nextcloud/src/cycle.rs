// SPDX-License-Identifier: MIT OR Apache-2.0
//! The attach ceremony and the steady-cycle state machine (decision
//! D-FT8), generic over the [`FilesTransport`] seam: PROPFIND → dirty
//! / vanished sets → tolerant re-read → the chunk-1 fold over
//! `local + seed + peers` → the local-hash skip-compare → guarded
//! PUTs → status/backoff bookkeeping.
//!
//! The discipline this module exists to enforce:
//! - **Create-once everywhere on first contact** (`If-None-Match: *`);
//!   a `412` there is an *adoption* arm, never a fork.
//! - **Guarded rewrites**: a `put_if_match` is issued only when the
//!   etag store holds a token for the path — a cold store GETs first
//!   (the adopting read) and never PUTs unguarded.
//! - **The fork alarm** (D-SF4/D-FT6): an unexpected `412` on an own
//!   single-writer path is disambiguated by one GET — 404 ⇒
//!   `OwnFileVanished` (drop the token; recreate next cycle), 200 ⇒
//!   `ForkDetected` (stop-write that path forever, keep reading,
//!   `error!` loudly). No retry loop re-PUTs after a fork-indicating
//!   412.
//! - **Skip-PUT by local content hash** (F5): server etags churn per
//!   upload, so `StateHash::of` against the last *published* hash is
//!   the only skip criterion.
//! - **Readers never delete**: a vanished peer device's rows simply
//!   leave the merged view; nothing is removed on the server.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use taskboard_domain::Clock;
use taskboard_domain::{AppState, UserDataReport};
use taskboard_sync_files::document::{DOCUMENT_FORMAT, UserDocument, UserRecord, UserSections};
use taskboard_sync_files::fold::fold;
use taskboard_sync_files::hash::StateHash;
use taskboard_sync_files::heartbeat::DeviceInfo;
use taskboard_sync_files::identity::{DeviceIdentity, FolderIdentity};
use taskboard_sync_files::ids::DeviceId;
use taskboard_sync_files::journal::{decode_journal, journal_section};
use taskboard_sync_files::layout::{
    self, APPDATA_FILE_NAME, DEVICES_DIR_NAME, IDENTITY_FILE_NAME, ROOT_FOLDER_NAME,
    TREE_FORMAT_VERSION,
};
use tokio::sync::{mpsc, watch};

use crate::error::TransportError;
use crate::etag::EtagStore;
use crate::status::{CycleOutcome, FilesSyncStatus, ParkKind};
use crate::transport::{FilesTransport, PropfindListing, PutOutcome};

/// The device's own contribution to the merged view (D-SF5's "+ the
/// local engine state as one more input"; D-SF12's materialise-from-
/// state). Chunk 2 ships the empty document; chunk 3 derives it from
/// the published `ArcSwap<AppState>` — this trait is the seam that
/// chunk adds an implementation to, no surgery on the loop.
pub trait LocalDocument: Send + Sync {
    fn document(&self) -> UserDocument;
}

/// The chunk-3 production local contribution (D-ES8): the device's
/// published `AppState`, materialised into the `journal` section —
/// D-SF12's derive-from-state, no outbox, no command, no `LocalOp`
/// lane. One coherent `load()` per call; **every** entry encodes,
/// tombstones included (silent absence would let a peer resurrect a
/// deleted entry). An empty home omits the section; `EmptyDocument`
/// stays as the test/legacy impl.
pub struct PublishedJournal {
    state: Arc<arc_swap::ArcSwap<AppState>>,
}

impl PublishedJournal {
    /// Over the engine's published-state swap (the read side only).
    #[must_use]
    pub fn new(state: Arc<arc_swap::ArcSwap<AppState>>) -> Self {
        Self { state }
    }
}

impl std::fmt::Debug for PublishedJournal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PublishedJournal").finish_non_exhaustive()
    }
}

impl LocalDocument for PublishedJournal {
    fn document(&self) -> UserDocument {
        let snapshot = self.state.load_full();
        let rows = journal_section(&snapshot.reflection);
        UserDocument {
            format: DOCUMENT_FORMAT,
            sections: UserSections {
                journal: (!rows.is_empty()).then_some(rows),
                unknown: BTreeMap::default(),
            },
            unknown: BTreeMap::default(),
        }
    }
}

/// The chunk-2 production local contribution: nothing. The cycle,
/// config gate, and wiring are complete and honest without the engine
/// (peers fold, the device publishes its merged view, the heartbeat
/// carries its hash).
#[derive(Debug, Clone, Copy, Default)]
pub struct EmptyDocument;

impl LocalDocument for EmptyDocument {
    fn document(&self) -> UserDocument {
        UserDocument {
            format: DOCUMENT_FORMAT,
            sections: UserSections::default(),
            unknown: BTreeMap::default(),
        }
    }
}

/// Everything the cycle needs besides the transport and its mutable
/// state — the injected identity facts and knobs.
pub struct SyncContext {
    /// This device's identity (the `device.json` ceremony's product).
    pub identity: DeviceIdentity,
    /// The heartbeat's `app_version`.
    pub app_version: String,
    /// The clock every stamp the cycle mints rides.
    pub clock: Arc<dyn Clock>,
    /// The local contribution to the fold.
    pub local: Arc<dyn LocalDocument>,
    /// The engine's user-data lane (D-ES5): the import step's delivery
    /// seam. A closed lane logs once per delta and the actor keeps
    /// syncing — publishing still relays everything.
    pub report_tx: mpsc::Sender<UserDataReport>,
}

/// The actor's mutable state, process-local (decision D-FT4). A
/// restart starts empty — one honest full-read cycle (the fold is
/// idempotent; D-SF19 zero migrations).
#[derive(Debug, Default)]
pub struct LoopState {
    /// The attach ceremony completed; the steady cycle runs.
    attached: bool,
    pub etags: EtagStore,
    /// Last decoded peer document per device (excl. self).
    docs: BTreeMap<DeviceId, UserDocument>,
    /// Last decoded peer heartbeat (chunk 4's freshness input).
    heartbeats: BTreeMap<DeviceId, DeviceInfo>,
    /// Adopted own-file content (a former install's `appdata.json`),
    /// folded into every merged view until the seeding subtree
    /// vanishes — *retained*, not consumed-once: a once-consumed seed
    /// would silently drop the adopted rows at the second publish
    /// (the re-publication semantics matrix test 11 pins). Cleared
    /// when the self subtree leaves the listing or the path forks.
    publish_seed: Option<UserDocument>,
    /// Hash of the last successfully published view.
    published_hash: Option<StateHash>,
    /// Set at attach and on every appdata publish.
    pending_heartbeat: bool,
    /// The stop-write set (the fork alarm).
    forked_paths: BTreeSet<String>,
    /// Parked peer paths (D-SF15) → why.
    parked: BTreeMap<String, ParkKind>,
    /// Consecutive transport-class failures (the rescheduling input).
    pub failure_streak: u32,
    /// The server's advertised `Retry-After` floor, when it sent one.
    pub retry_after_floor: Duration,
    /// When this device last wrote its own heartbeat.
    last_heartbeat_at: Option<DateTime<Utc>>,
    /// The fingerprint of the last import delta the engine was told
    /// about (D-ES5's dedupe): absent-tombstone and undecodable-row
    /// deltas differ from the local view *forever* by design; without
    /// the fingerprint every cycle would re-send a no-op report. The
    /// engine is independently no-op-safe (D-ES6).
    last_import_fingerprint: Option<StateHash>,
    /// The undecodable-row count of that delta: a standing finding
    /// while the delta persists, so deduped cycles still report it.
    last_import_rejected: usize,
    /// This cycle's merged view — the divergence derivation's "our
    /// current settled view" input (set every Ok cycle).
    view: Option<UserDocument>,
    /// Consecutive quiet-and-disagreeing cycles (the quiescence
    /// window; D-FH3). Reset by a busy or an agreeing quiet cycle;
    /// held across failures.
    quiet_streak: u32,
    /// The current divergence finding (fires at `quiet_streak >= 2`).
    diverged_from: BTreeSet<DeviceId>,
}

/// The quiescence window's length: the finding fires on the second
/// consecutive quiet-and-disagreeing cycle (design Part F: "two
/// cycles + no pending writes").
const QUIET_CYCLES_TO_FIRE: u32 = 2;

/// The divergence window's pure rule (D-FH3): clock-free by
/// construction — the caller folds the cycle's classification (`quiet`
/// = no publish ∧ no pending heartbeat post-cycle) into the running
/// streak, and the disagreeing set is recomputed from the latest
/// evidence every call, so membership always follows the newest
/// observation. Exclusions are structural: a peer absent from `docs`
/// (parked — its park finding already says why) or from `heartbeats`
/// (no decoded heartbeat — no evidence, no finding) cannot appear in
/// the comparison; self is excluded because its heartbeat never enters
/// `heartbeats` (the fork alarm is that condition's specific finding);
/// a peer that has not absorbed our view yet is lagging, not divergent
/// (its heartbeat age is that finding) — see [`has_absorbed`].
fn advance_divergence(
    quiet_streak: u32,
    quiet: bool,
    view: &UserDocument,
    docs: &BTreeMap<DeviceId, UserDocument>,
    heartbeats: &BTreeMap<DeviceId, DeviceInfo>,
) -> (u32, BTreeSet<DeviceId>) {
    if !quiet {
        // Local writes in flight mean the fleet is mid-convergence;
        // firing here is noise.
        return (0, BTreeSet::new());
    }
    let view_hash = StateHash::of(view);
    let disagreeing: BTreeSet<DeviceId> = docs
        .iter()
        .filter(|(device, doc)| {
            heartbeats
                .get(*device)
                .is_some_and(|info| info.state_hash != view_hash)
                && has_absorbed(doc, view)
        })
        .map(|(device, _)| *device)
        .collect();
    if disagreeing.is_empty() {
        // Agreement is evidence too.
        return (0, BTreeSet::new());
    }
    let streak = quiet_streak.saturating_add(1);
    if streak >= QUIET_CYCLES_TO_FIRE {
        (streak, disagreeing)
    } else {
        (streak, BTreeSet::new())
    }
}

/// Whether a peer's published document already contains `view`:
/// folding the view into it changes nothing. Until then the peer is
/// behind this device (offline, or not cycled since our publish), and
/// its heartbeat hash describes an older view — lag, not divergence.
/// Structural and clock-free: no age threshold decides it.
fn has_absorbed(peer: &UserDocument, view: &UserDocument) -> bool {
    StateHash::of(&fold([peer, view])) == StateHash::of(&fold([peer]))
}

/// How a cycle ended, for the actor's rescheduling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CycleVerdict {
    Completed {
        published: bool,
        read_devices: usize,
    },
    /// The cycle failed. `retryable` marks the transport class (the
    /// backoff escalates on it); `retry_after` carries a parsed
    /// `Retry-After` floor when the server advertised one.
    Failed {
        retryable: bool,
        retry_after: Duration,
    },
}

/// Root-inclusive substrate path of a below-root layout path.
fn rooted(rel: &str) -> String {
    format!("{ROOT_FOLDER_NAME}/{rel}")
}

/// A structured finding record (the loud half of "surface loudly";
/// the watch carries the state half). Chunk 4 maps these onto the
/// status vocabulary.
fn finding(event: &str, path: &str) {
    tracing::warn!(event, path, "files-sync finding");
}

fn fork_alarm(path: &str) {
    tracing::error!(
        event = "ForkDetected",
        path,
        "another writer holds this device's single-writer path; \
         writes to it stop, reads continue"
    );
}

/// Runs one cycle: attach first when needed, then the steady cycle.
/// State mutations and the status watch update happen on every path
/// where they are meaningful (the stop-write set, parks, streaks).
pub async fn run_cycle<T: FilesTransport>(
    transport: &T,
    ctx: &SyncContext,
    st: &mut LoopState,
    status: &watch::Sender<FilesSyncStatus>,
) -> CycleVerdict {
    let verdict = if st.attached {
        steady_cycle(transport, ctx, st).await
    } else {
        attach(transport, ctx, st).await
    };
    match verdict {
        Ok((published, read_devices, import_rejected)) => {
            st.failure_streak = 0;
            st.retry_after_floor = Duration::ZERO;
            if let Some(view) = &st.view {
                let quiet = !published && !st.pending_heartbeat;
                let (streak, diverged) =
                    advance_divergence(st.quiet_streak, quiet, view, &st.docs, &st.heartbeats);
                st.quiet_streak = streak;
                st.diverged_from = diverged;
            }
            publish_status(
                status,
                Some(&CycleOutcome::Ok {
                    published,
                    read_devices,
                    import_rejected,
                }),
                st,
            );
            CycleVerdict::Completed {
                published,
                read_devices,
            }
        }
        Err(err) => {
            let retryable = err.is_transport();
            if !retryable {
                tracing::warn!(event = "CycleFailedTyped", error = %err, "files-sync finding");
            }
            st.failure_streak += 1;
            let retry_after = match &err {
                TransportError::RateLimited { retry_after } => retry_after.unwrap_or_default(),
                _ => Duration::ZERO,
            };
            st.retry_after_floor = retry_after;
            publish_status(status, Some(&CycleOutcome::Failed), st);
            CycleVerdict::Failed {
                retryable,
                retry_after,
            }
        }
    }
}

fn publish_status(
    status: &watch::Sender<FilesSyncStatus>,
    last_cycle: Option<&CycleOutcome>,
    st: &LoopState,
) {
    status.send_modify(|current| {
        current.last_cycle = last_cycle.cloned();
        current.forked_paths.clone_from(&st.forked_paths);
        current.parked.clone_from(&st.parked);
        current.last_heartbeat_written = st.last_heartbeat_at;
        current.peers = st
            .heartbeats
            .iter()
            .map(|(device, info)| {
                (
                    *device,
                    crate::status::PeerFreshness {
                        written_at: info.written_at,
                        last_clock: info.last_clock,
                    },
                )
            })
            .collect();
        current.diverged_from.clone_from(&st.diverged_from);
    });
}

// ------------------------------------------------------------------
// The attach ceremony (D-FT8, arms 2–9; arm 1 — device.json — ran at
// spawn; `Unusable` there parks the actor before any server request).
// Every arm's network failure is a cycle failure; the backoff retries
// the whole ceremony (it is idempotent).
// ------------------------------------------------------------------

/// The MKCOL ladder arm: created (201) or the exists-ok posture
/// (405); any other status is a typed error.
async fn mkcol_exists_ok<T: FilesTransport>(
    transport: &T,
    path: &str,
) -> Result<(), TransportError> {
    match transport.mkcol(path).await? {
        crate::transport::MkcolOutcome::Created | crate::transport::MkcolOutcome::AlreadyExists => {
            Ok(())
        }
    }
}

async fn attach<T: FilesTransport>(
    transport: &T,
    ctx: &SyncContext,
    st: &mut LoopState,
) -> Result<(bool, usize, usize), TransportError> {
    let self_dir = rooted(&format!("{DEVICES_DIR_NAME}/{}", ctx.identity.device_id));
    mkcol_exists_ok(transport, ROOT_FOLDER_NAME).await?;
    mkcol_exists_ok(transport, &rooted(DEVICES_DIR_NAME)).await?;
    mkcol_exists_ok(transport, &self_dir).await?;

    // identity.json: create-once; a 412 means another device won the
    // create race — adopt the winner (F3).
    let identity_path = rooted(IDENTITY_FILE_NAME);
    let folder = FolderIdentity {
        // A fresh v7 UUID minted at this one site; the payload is only
        // ever echoed back (never a path, never an order key — the
        // chunk-1 payload's contract), so the substrate's mint is its
        // source without a second uuid creation surface.
        folder_id: ctx.identity.device_id.as_uuid(),
        format_version: TREE_FORMAT_VERSION,
        created_at: ctx.identity.created_at,
    };
    let body = serde_json::to_vec(&folder)?;
    match transport.put_create(&identity_path, &body).await {
        Ok(_) => {}
        Err(TransportError::PreconditionFailed) => {
            let served = transport.get(&identity_path).await?;
            let adopted: FolderIdentity =
                serde_json::from_slice(&served.bytes).map_err(|_| TransportError::BadRequest)?;
            if adopted.format_version > TREE_FORMAT_VERSION {
                // A folder our older reader cannot represent: unusable,
                // fail closed (writing here would corrupt a newer tree).
                finding("FolderNewerFormat", &identity_path);
                return Err(TransportError::BadRequest);
            }
        }
        Err(other) => return Err(other),
    }

    // First read: the full dirty set (the store is cold, so every
    // listed peer re-reads — the honest full-read cycle).
    let listing = transport.propfind_devices().await?;
    ingest_listing(&listing, ctx, st);
    read_dirty_peers(transport, ctx, st).await?;

    // Own appdata: first publish (create-once, content = the merged
    // view). A 412 ⇒ adopt: the served content seeds the next
    // publish; a corrupt seed stops writes to the path (fail closed —
    // publishing an empty merged view over an unrecoverable own
    // history is data loss). Fold-first (D-ES5): the peers' history
    // enters the engine *before* this device's first document claims
    // the view — a device that published before importing could
    // resurrect its own tombstones over peers' deletes.
    let appdata_path = rooted(&layout::device_appdata_path(ctx.identity.device_id));
    let merged = merged_view(ctx, st);
    let import_rejected = import_step(ctx, st, &merged).await;
    st.view = Some(merged.clone());
    let appdata_bytes = serde_json::to_vec(&merged)?;
    match transport.put_create(&appdata_path, &appdata_bytes).await {
        Ok(outcome) => {
            record_put(st, &appdata_path, &outcome);
            st.published_hash = Some(StateHash::of(&merged));
            st.pending_heartbeat = true;
        }
        Err(TransportError::PreconditionFailed) => {
            adopt_own_file(transport, st, &appdata_path).await?;
        }
        Err(other) => return Err(other),
    }

    // Own heartbeat: create-once; a 412 ⇒ adopt (get + capture the
    // etag only — the content is ours to rewrite next cycle).
    let info_path = rooted(&layout::device_info_path(ctx.identity.device_id));
    let heartbeat = heartbeat_for(ctx, &merged_view(ctx, st));
    let info_bytes = serde_json::to_vec(&heartbeat)?;
    match transport.put_create(&info_path, &info_bytes).await {
        Ok(outcome) => {
            record_put(st, &info_path, &outcome);
            st.pending_heartbeat = false;
            st.last_heartbeat_at = Some(ctx.clock.now());
        }
        Err(TransportError::PreconditionFailed) => {
            adopt_own_file(transport, st, &info_path).await?;
        }
        Err(other) => return Err(other),
    }

    st.attached = true;
    Ok((true, st.docs.len(), import_rejected))
}

/// The adopting read (adoption arms): GET the own file and store its
/// etag — the token that guards the next rewrite. When the file is
/// appdata, decode it as the publish seed; a corrupt seed quarantines
/// the path (stop-write, fail closed).
async fn adopt_own_file<T: FilesTransport>(
    transport: &T,
    st: &mut LoopState,
    path: &str,
) -> Result<(), TransportError> {
    finding("OwnFileAdopted", path);
    match transport.get(path).await {
        Ok(content) => {
            record_get(st, path, &content);
            if path.ends_with(APPDATA_FILE_NAME) {
                match serde_json::from_slice::<UserDocument>(&content.bytes) {
                    Ok(document) if document.format <= DOCUMENT_FORMAT => {
                        st.publish_seed = Some(document);
                    }
                    _ => {
                        st.forked_paths.insert(path.to_owned());
                        st.parked.insert(path.to_owned(), ParkKind::Quarantined);
                        finding("OwnSeedQuarantined", path);
                    }
                }
            }
            Ok(())
        }
        Err(TransportError::NotFound) => {
            // The create raced a delete; the next cycle recreates via
            // create-once. Nothing is stored, so the cold-store rule
            // keeps guarding.
            Ok(())
        }
        Err(other) => Err(other),
    }
}

// ------------------------------------------------------------------
// The steady cycle.
// ------------------------------------------------------------------

async fn steady_cycle<T: FilesTransport>(
    transport: &T,
    ctx: &SyncContext,
    st: &mut LoopState,
) -> Result<(bool, usize, usize), TransportError> {
    // 1. PROPFIND devices/ → dirty / vanished sets.
    let listing = transport.propfind_devices().await?;
    for device in ingest_listing(&listing, ctx, st) {
        // The tree is the truth: a vanished device's rows leave the
        // merged view (its subtree was removed in the Files UI; the
        // trashbin is the user's recovery). Nothing on the server is
        // touched — readers never delete.
        st.docs.remove(&device);
        st.heartbeats.remove(&device);
    }
    // 2. changed-set re-read. Self is never dirty in steady state:
    // own files are output-only, and every self-read happens at
    // attach/adoption, feeding the seed (D-FT4).
    read_dirty_peers(transport, ctx, st).await?;

    // 3. fold over local + seed + peers.
    let merged = merged_view(ctx, st);
    // 3½. the import step (D-ES5): the merged rows that differ from
    // the local-derived rows cross to the engine before this cycle's
    // publish — fold-first is an ordering of intent. What the device
    // publishes is identical either way (`merged` already contains
    // local ∪ peers); the next cycle's local read then equals the
    // merged journal ⇒ empty delta ⇒ no report ⇒ no PUT.
    let import_rejected = import_step(ctx, st, &merged).await;
    // 4. the local-hash skip-compare (never a server etag, F5).
    let hash = StateHash::of(&merged);
    st.view = Some(merged.clone());
    let skip = st.published_hash.as_ref() == Some(&hash);
    let mut published = false;
    if !skip {
        // 5. conditional PUTs, each guarded by the fork set first.
        publish_appdata(transport, ctx, st, &merged).await?;
        published = true;
    }
    if st.pending_heartbeat {
        write_heartbeat(transport, ctx, st, &merged).await?;
    }
    Ok((published, st.docs.len(), import_rejected))
}

/// Updates the store from a listing; returns the devices that vanished
/// (stored in the docs map, absent from the listing).
fn ingest_listing(
    listing: &PropfindListing,
    ctx: &SyncContext,
    st: &mut LoopState,
) -> Vec<DeviceId> {
    let mut listed = BTreeSet::new();
    for member in &listing.members {
        // Raw href matching against the encoded forms we can compute:
        // a device directory's encoded form is its `Display` form
        // (UUIDs are URL-safe). Unmatched hrefs are ignored.
        if let Some(device) = href_device(&member.href) {
            listed.insert(device);
            if let Some(etag) = &member.etag {
                st.etags.record_device_dir(device, etag.clone());
            }
        }
    }
    let vanished: Vec<DeviceId> = st
        .docs
        .keys()
        .copied()
        .filter(|device| !listed.contains(device))
        .collect();
    st.etags
        .retain_device_dirs(|device| listed.contains(device));
    // The seed's retirement arm: the device whose adopted file seeded
    // the view has left the tree — its rows leave the merged view with
    // it (the tree is the truth).
    if !listed.contains(&ctx.identity.device_id) {
        st.publish_seed = None;
    }
    vanished
}

/// The raw-href → device mapping: the last path segment must parse as
/// a device id (everything else in the listing is not ours to read).
fn href_device(href: &str) -> Option<DeviceId> {
    let last = href.trim_end_matches('/').rsplit('/').next()?;
    DeviceId::parse(last)
}

/// Re-reads every dirty peer (dir etag differs from the store, or the
/// device was never seen), 404-tolerant, decode tolerant (D-SF15).
async fn read_dirty_peers<T: FilesTransport>(
    transport: &T,
    ctx: &SyncContext,
    st: &mut LoopState,
) -> Result<(), TransportError> {
    let dirty: Vec<DeviceId> = st
        .etags
        .dirty_devices()
        .into_iter()
        .filter(|device| *device != ctx.identity.device_id)
        .collect();
    for device in dirty {
        let appdata_path = rooted(&layout::device_appdata_path(device));
        let info_path = rooted(&layout::device_info_path(device));
        let appdata = match transport.get(&appdata_path).await {
            Ok(content) => Some(content),
            Err(TransportError::NotFound) => None,
            Err(other) => return Err(other),
        };
        if let Some(content) = &appdata {
            record_get(st, &appdata_path, content);
        }
        ingest_peer_document(
            st,
            &appdata_path,
            appdata.as_ref().map(|c| &c.bytes),
            device,
        );
        let info = match transport.get(&info_path).await {
            Ok(content) => Some(content),
            Err(TransportError::NotFound) => None,
            Err(other) => return Err(other),
        };
        if let Some(content) = &info {
            record_get(st, &info_path, content);
        }
        ingest_peer_heartbeat(st, &info_path, info.as_ref().map(|c| &c.bytes), device);
        st.etags.mark_device_read(device);
    }
    Ok(())
}

/// The tolerant peer-document decode (D-SF15): absent is fine; a
/// newer format parks (`NewerFormat` — fold nothing from it); garbage
/// quarantines (in-memory, the file stays untouched on the server); a
/// well-formed current-format document lands in the fold input.
fn ingest_peer_document(st: &mut LoopState, path: &str, bytes: Option<&Vec<u8>>, device: DeviceId) {
    let Some(bytes) = bytes else { return };
    match serde_json::from_slice::<UserDocument>(bytes) {
        Ok(document) if document.format <= DOCUMENT_FORMAT => {
            st.docs.insert(device, document);
            st.parked.remove(path);
        }
        Ok(document) => {
            st.parked.insert(
                path.to_owned(),
                ParkKind::NewerFormat {
                    format: document.format,
                },
            );
            st.docs.remove(&device);
        }
        Err(_) => {
            st.parked.insert(path.to_owned(), ParkKind::Quarantined);
            st.docs.remove(&device);
        }
    }
}

fn ingest_peer_heartbeat(
    st: &mut LoopState,
    path: &str,
    bytes: Option<&Vec<u8>>,
    device: DeviceId,
) {
    let Some(bytes) = bytes else { return };
    match serde_json::from_slice::<DeviceInfo>(bytes) {
        Ok(info) => {
            st.heartbeats.insert(device, info);
            st.parked.remove(path);
        }
        Err(_) => {
            st.parked.insert(path.to_owned(), ParkKind::Quarantined);
        }
    }
}

fn record_get(st: &mut LoopState, path: &str, content: &crate::transport::FileContent) {
    if let Some(etag) = &content.etag {
        st.etags.record_file(path, etag.clone());
    }
}

fn record_put(st: &mut LoopState, path: &str, outcome: &PutOutcome) {
    let etag = match outcome {
        PutOutcome::Created { etag } | PutOutcome::Updated { etag } => etag.clone(),
    };
    if let Some(etag) = etag {
        st.etags.record_file(path, etag);
    }
}

/// The merged journal rows that differ from the local-derived rows
/// (absent locally, or a different record) — the import step's input
/// (D-ES5).
fn journal_delta(
    merged: &UserDocument,
    local: &UserDocument,
) -> std::collections::BTreeMap<String, UserRecord> {
    let local_rows = local.sections.journal.as_ref();
    merged
        .sections
        .journal
        .as_ref()
        .map(|rows| {
            rows.iter()
                .filter(|(key, record)| {
                    local_rows.and_then(|local| local.get(*key)) != Some(record)
                })
                .map(|(key, record)| (key.clone(), record.clone()))
                .collect()
        })
        .unwrap_or_default()
}

/// The import step (D-ES5, D-ES7): the delta's rows decode into the
/// engine's report family; `Verbatim` rows and undecodable bodies are
/// dropped from the report (they still relay in the published view)
/// and counted. A fingerprint over the delta dedupes forever-different
/// views (absent-target tombstones, rejected rows) so no cycle re-sends
/// a no-op report. Returns the rejected count for the cycle outcome.
async fn import_step(ctx: &SyncContext, st: &mut LoopState, merged: &UserDocument) -> usize {
    let delta = journal_delta(merged, &ctx.local.document());
    if delta.is_empty() {
        st.last_import_rejected = 0;
        return 0;
    }
    let fingerprint = StateHash::of(&UserDocument {
        format: DOCUMENT_FORMAT,
        sections: UserSections {
            journal: Some(delta.clone()),
            unknown: BTreeMap::default(),
        },
        unknown: BTreeMap::default(),
    });
    if st.last_import_fingerprint.as_ref() == Some(&fingerprint) {
        return st.last_import_rejected;
    }
    let (rows, rejected) = decode_journal(&delta);
    if rejected > 0 {
        tracing::warn!(
            event = "files_sync.import_rejected",
            rejected,
            "files-sync finding"
        );
    }
    if !rows.is_empty()
        && ctx
            .report_tx
            .send(UserDataReport {
                journal: rows,
                settings: Vec::new(),
                proofs: Vec::new(),
            })
            .await
            .is_err()
    {
        // The engine's lane is closed; recorded once per delta by
        // the fingerprint below — the actor keeps syncing, and
        // publishing still relays everything.
        tracing::warn!(
            event = "files_sync.import_lane_closed",
            "files-sync finding"
        );
    }
    st.last_import_fingerprint = Some(fingerprint);
    st.last_import_rejected = rejected;
    rejected
}

/// The merged view: local contribution + the retained seed + every
/// peer document present in the latest listing.
fn merged_view(ctx: &SyncContext, st: &LoopState) -> UserDocument {
    fold(
        std::iter::once(&ctx.local.document())
            .chain(st.publish_seed.iter())
            .chain(st.docs.values())
            .collect::<Vec<_>>(),
    )
}

/// The cycle's own publish: guarded rewrite (or the cold-store
/// adopting GET first), with the 412 disambiguation on every failure
/// shape the guard can answer.
async fn publish_appdata<T: FilesTransport>(
    transport: &T,
    ctx: &SyncContext,
    st: &mut LoopState,
    merged: &UserDocument,
) -> Result<(), TransportError> {
    let path = rooted(&layout::device_appdata_path(ctx.identity.device_id));
    if st.forked_paths.contains(&path) {
        return Ok(());
    }
    let Some(token) = st.etags.file_etag(&path).cloned() else {
        // Cold store: the adopting GET first — never an unguarded PUT.
        return match transport.get(&path).await {
            Ok(content) => {
                record_get(st, &path, &content);
                if let Ok(document) = serde_json::from_slice::<UserDocument>(&content.bytes)
                    && document.format > DOCUMENT_FORMAT
                {
                    // A newer-format own file is not ours to overwrite
                    // blind: adopt it as the seed for the next publish
                    // (after re-folding), skip this one.
                    st.publish_seed = Some(document);
                    finding("OwnFileNewerFormat", &path);
                    return Ok(());
                }
                rewrite_own_appdata(transport, ctx, st, merged).await
            }
            Err(TransportError::NotFound) => {
                // Vanished since the store went cold: recreate via
                // create-once.
                create_own_appdata(transport, st, &path, merged).await
            }
            Err(other) => Err(other),
        };
    };
    let bytes = serde_json::to_vec(merged)?;
    match transport.put_if_match(&path, &bytes, &token).await {
        Ok(outcome) => {
            record_put(st, &path, &outcome);
            st.published_hash = Some(StateHash::of(merged));
            st.pending_heartbeat = true;
            Ok(())
        }
        Err(TransportError::NotFound) => create_own_appdata(transport, st, &path, merged).await,
        Err(TransportError::PreconditionFailed) => {
            disambiguate_own_412(transport, ctx, st, &path).await
        }
        Err(other) => Err(other),
    }
}

async fn rewrite_own_appdata<T: FilesTransport>(
    transport: &T,
    ctx: &SyncContext,
    st: &mut LoopState,
    merged: &UserDocument,
) -> Result<(), TransportError> {
    let path = rooted(&layout::device_appdata_path(ctx.identity.device_id));
    let bytes = serde_json::to_vec(merged)?;
    let token = st.etags.file_etag(&path).cloned();
    match token {
        Some(token) => match transport.put_if_match(&path, &bytes, &token).await {
            Ok(outcome) => {
                record_put(st, &path, &outcome);
                st.published_hash = Some(StateHash::of(merged));
                st.pending_heartbeat = true;
                Ok(())
            }
            Err(TransportError::PreconditionFailed) => {
                disambiguate_own_412(transport, ctx, st, &path).await
            }
            Err(other) => Err(other),
        },
        None => create_own_appdata(transport, st, &path, merged).await,
    }
}

async fn create_own_appdata<T: FilesTransport>(
    transport: &T,
    st: &mut LoopState,
    path: &str,
    merged: &UserDocument,
) -> Result<(), TransportError> {
    let bytes = serde_json::to_vec(merged)?;
    match transport.put_create(path, &bytes).await {
        Ok(outcome) => {
            record_put(st, path, &outcome);
            st.published_hash = Some(StateHash::of(merged));
            st.pending_heartbeat = true;
            Ok(())
        }
        Err(TransportError::PreconditionFailed) => {
            // Someone recreated the file between our 404 and this
            // create: the adoption arm.
            adopt_own_file(transport, st, path).await
        }
        Err(other) => Err(other),
    }
}

/// The fork disambiguation (D-FT6): one GET of the own path decides —
/// 404 ⇒ `OwnFileVanished` (drop the token; the next cycle recreates
/// via create-once), 200 ⇒ `ForkDetected` (stop-write that path,
/// reads continue, loud). Failing closed here is deliberate: a false
/// fork alarm over a user delete would cry wolf; a missed fork would
/// silently split a device's history.
async fn disambiguate_own_412<T: FilesTransport>(
    transport: &T,
    ctx: &SyncContext,
    st: &mut LoopState,
    path: &str,
) -> Result<(), TransportError> {
    match transport.get(path).await {
        Err(TransportError::NotFound) => {
            finding("OwnFileVanished", path);
            st.etags.forget_file(path);
            // The local view is no longer published anywhere: clear
            // the skip-hash so the next cycle recreates via
            // create-once (the seed's rows ride along — nothing is
            // dropped by a transient delete).
            st.published_hash = None;
            Ok(())
        }
        Ok(content) => {
            fork_alarm(path);
            st.forked_paths.insert(path.to_owned());
            st.publish_seed = None;
            // Our own subtree is read as peer data after the fork: the
            // sibling path (the clone's write) keeps participating in
            // PROPFIND/GET/fold, and forks on its own 412 if the clone
            // writes it — the set converges to full-subtree stop.
            if path.ends_with(APPDATA_FILE_NAME)
                && let Ok(document) = serde_json::from_slice::<UserDocument>(&content.bytes)
                && document.format <= DOCUMENT_FORMAT
            {
                st.docs.insert(ctx.identity.device_id, document);
            }
            Ok(())
        }
        Err(other) => Err(other),
    }
}

/// The heartbeat write (same guarded discipline as appdata); the
/// heartbeat's hash describes exactly the `merged` view passed in.
async fn write_heartbeat<T: FilesTransport>(
    transport: &T,
    ctx: &SyncContext,
    st: &mut LoopState,
    merged: &UserDocument,
) -> Result<(), TransportError> {
    let path = rooted(&layout::device_info_path(ctx.identity.device_id));
    if st.forked_paths.contains(&path) {
        return Ok(());
    }
    let heartbeat = heartbeat_for(ctx, merged);
    let bytes = serde_json::to_vec(&heartbeat)?;
    let Some(token) = st.etags.file_etag(&path).cloned() else {
        return match transport.get(&path).await {
            Ok(content) => {
                record_get(st, &path, &content);
                write_heartbeat_guarded(transport, ctx, st, &path, bytes).await
            }
            Err(TransportError::NotFound) => {
                create_own_heartbeat(transport, ctx, st, &path, bytes).await
            }
            Err(other) => Err(other),
        };
    };
    match transport.put_if_match(&path, &bytes, &token).await {
        Ok(outcome) => {
            record_put(st, &path, &outcome);
            st.pending_heartbeat = false;
            st.last_heartbeat_at = Some(ctx.clock.now());
            Ok(())
        }
        Err(TransportError::NotFound) => {
            create_own_heartbeat(transport, ctx, st, &path, bytes).await
        }
        Err(TransportError::PreconditionFailed) => {
            disambiguate_own_412(transport, ctx, st, &path).await
        }
        Err(other) => Err(other),
    }
}

async fn write_heartbeat_guarded<T: FilesTransport>(
    transport: &T,
    ctx: &SyncContext,
    st: &mut LoopState,
    path: &str,
    bytes: Vec<u8>,
) -> Result<(), TransportError> {
    match st.etags.file_etag(path).cloned() {
        Some(token) => match transport.put_if_match(path, &bytes, &token).await {
            Ok(outcome) => {
                record_put(st, path, &outcome);
                st.pending_heartbeat = false;
                st.last_heartbeat_at = Some(ctx.clock.now());
                Ok(())
            }
            Err(TransportError::PreconditionFailed) => {
                disambiguate_own_412(transport, ctx, st, path).await
            }
            Err(other) => Err(other),
        },
        None => create_own_heartbeat(transport, ctx, st, path, bytes).await,
    }
}

async fn create_own_heartbeat<T: FilesTransport>(
    transport: &T,
    ctx: &SyncContext,
    st: &mut LoopState,
    path: &str,
    bytes: Vec<u8>,
) -> Result<(), TransportError> {
    match transport.put_create(path, &bytes).await {
        Ok(outcome) => {
            record_put(st, path, &outcome);
            st.pending_heartbeat = false;
            st.last_heartbeat_at = Some(ctx.clock.now());
            Ok(())
        }
        Err(TransportError::PreconditionFailed) => {
            // A racing recreate: the adoption arm.
            adopt_own_file(transport, st, path).await
        }
        Err(other) => Err(other),
    }
}

/// Builds the heartbeat payload: the max clock over the merged rows
/// (falling back to the device's creation clock on an empty view).
fn heartbeat_for(ctx: &SyncContext, merged: &UserDocument) -> DeviceInfo {
    DeviceInfo {
        app_version: ctx.app_version.clone(),
        format_version: TREE_FORMAT_VERSION,
        last_clock: max_row_clock(merged).unwrap_or(ctx.identity.created_at),
        state_hash: StateHash::of(merged),
        written_at: ctx.clock.now(),
    }
}

/// Max clock over every clocked row in the merged view (`None` on an
/// empty view — the heartbeat falls back to the creation clock).
fn max_row_clock(document: &UserDocument) -> Option<DateTime<Utc>> {
    let mut max = None;
    for record in document
        .sections
        .journal
        .iter()
        .flatten()
        .map(|(_, record)| record)
    {
        let clock = match record {
            UserRecord::Entry { clock, .. } | UserRecord::Tombstone { clock, .. } => *clock,
            UserRecord::Verbatim(_) => continue,
        };
        max = Some(max.map_or(clock, |current: DateTime<Utc>| current.max(clock)));
    }
    max
}

#[cfg(test)]
mod divergence_rule {
    use super::*;
    use proptest::prelude::*;
    use serde_json::json;

    fn device(tag: u8) -> DeviceId {
        DeviceId::parse(&format!("0193c000-0000-7c89-ab01-2345abcdef{tag:02x}"))
            .expect("fixture device id")
    }

    fn doc_with(text: &str) -> UserDocument {
        let mut journal = BTreeMap::new();
        journal.insert(
            "0193c000-0000-7c89-ab01-2345abcdef99".to_owned(),
            UserRecord::Entry {
                body: BTreeMap::from([("text".to_owned(), json!(text))]),
                clock: "2026-11-05T08:00:00Z".parse().expect("clock"),
                author: None,
            },
        );
        UserDocument {
            format: DOCUMENT_FORMAT,
            sections: taskboard_sync_files::document::UserSections {
                journal: Some(journal),
                unknown: BTreeMap::new(),
            },
            unknown: BTreeMap::new(),
        }
    }

    fn info(hash: StateHash) -> DeviceInfo {
        DeviceInfo {
            app_version: "test".to_owned(),
            format_version: TREE_FORMAT_VERSION,
            last_clock: "2026-11-05T08:00:00Z".parse().expect("clock"),
            state_hash: hash,
            written_at: "2026-11-05T09:30:00Z".parse().expect("clock"),
        }
    }

    /// One cycle classification: a quiet cycle with the peer agreeing,
    /// a quiet cycle with the peer disagreeing, or a busy cycle (the
    /// disagreement bit is irrelevant while writes are in flight).
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Step {
        QuietAgree,
        QuietDisagree,
        Busy,
    }

    fn step_strategy() -> impl Strategy<Value = Step> {
        prop_oneof![
            Just(Step::QuietAgree),
            Just(Step::QuietDisagree),
            Just(Step::Busy)
        ]
    }

    fn evidence(
        step: Step,
    ) -> (
        BTreeMap<DeviceId, UserDocument>,
        BTreeMap<DeviceId, DeviceInfo>,
    ) {
        let peer = device(1);
        // The peer published exactly our view (it absorbed it); only
        // its heartbeat's claimed hash varies.
        let docs = BTreeMap::from([(peer, doc_with("view"))]);
        let heartbeats = match step {
            Step::QuietAgree => BTreeMap::from([(peer, info(StateHash::of(&doc_with("view"))))]),
            Step::QuietDisagree | Step::Busy => {
                BTreeMap::from([(peer, info(StateHash::of(&doc_with("other"))))])
            }
        };
        (docs, heartbeats)
    }

    /// The rule table, one row per test.
    #[test]
    fn a_first_quiet_disagreeing_cycle_does_not_fire() {
        let view = doc_with("view");
        let (docs, heartbeats) = evidence(Step::QuietDisagree);
        let (streak, diverged) = advance_divergence(0, true, &view, &docs, &heartbeats);
        assert_eq!(streak, 1);
        assert!(diverged.is_empty(), "the window is two cycles");
    }

    #[test]
    fn a_second_consecutive_quiet_disagreeing_cycle_fires_with_the_current_set() {
        let peer = device(1);
        let view = doc_with("view");
        let (docs, heartbeats) = evidence(Step::QuietDisagree);
        let (streak, diverged) = advance_divergence(1, true, &view, &docs, &heartbeats);
        assert_eq!(streak, QUIET_CYCLES_TO_FIRE);
        assert_eq!(diverged, BTreeSet::from([peer]));
    }

    #[test]
    fn a_third_quiet_disagreeing_cycle_stays_active_with_the_fresh_membership() {
        // Membership follows the latest observation: the same set here,
        // but recomputed every call (the set argument is only carried
        // through by the caller, never consulted).
        let peer = device(1);
        let view = doc_with("view");
        let (docs, heartbeats) = evidence(Step::QuietDisagree);
        let (streak, diverged) = advance_divergence(2, true, &view, &docs, &heartbeats);
        assert_eq!(streak, 3);
        assert_eq!(diverged, BTreeSet::from([peer]));
    }

    #[test]
    fn a_quiet_agreeing_cycle_resets_the_streak_and_clears_the_finding() {
        let view = doc_with("view");
        let (docs, heartbeats) = evidence(Step::QuietAgree);
        let (streak, diverged) = advance_divergence(2, true, &view, &docs, &heartbeats);
        assert_eq!(streak, 0);
        assert!(diverged.is_empty(), "agreement is evidence too");
    }

    #[test]
    fn a_busy_cycle_resets_the_streak_and_clears_the_finding() {
        let view = doc_with("view");
        let (docs, heartbeats) = evidence(Step::Busy);
        let (streak, diverged) = advance_divergence(1, false, &view, &docs, &heartbeats);
        assert_eq!(streak, 0);
        assert!(diverged.is_empty(), "local writes in flight: no firing");
    }

    #[test]
    fn a_parked_peer_is_excluded_no_document_no_comparison() {
        let view = doc_with("view");
        // The parked posture: a heartbeat exists but the document was
        // withheld (quarantine / newer format) ⇒ the peer is in
        // `heartbeats` only.
        let docs = BTreeMap::new();
        let heartbeats = BTreeMap::from([(device(1), info(StateHash::of(&doc_with("other"))))]);
        let (streak, diverged) = advance_divergence(1, true, &view, &docs, &heartbeats);
        assert_eq!(streak, 0, "no evidence to carry a streak over");
        assert!(diverged.is_empty(), "the park finding already says why");
    }

    /// A peer that has not absorbed our view (its published document
    /// lacks our newer row) is lagging: its older hash disagrees, yet
    /// no streak builds and nothing fires.
    #[test]
    fn a_peer_behind_our_view_is_lagging_not_diverged() {
        let mut view = doc_with("view");
        view.sections.journal.as_mut().expect("rows").insert(
            "0193c000-0000-7c89-ab01-2345abcdef98".to_owned(),
            UserRecord::Entry {
                body: BTreeMap::from([("text".to_owned(), json!("ours, newer"))]),
                clock: "2026-11-05T09:00:00Z".parse().expect("clock"),
                author: None,
            },
        );
        let peer = device(1);
        let docs = BTreeMap::from([(peer, doc_with("view"))]);
        let heartbeats = BTreeMap::from([(peer, info(StateHash::of(&doc_with("view"))))]);
        let (streak, diverged) = advance_divergence(1, true, &view, &docs, &heartbeats);
        assert_eq!(streak, 0, "lag is not evidence of divergence");
        assert!(diverged.is_empty());
    }

    #[test]
    fn a_heartbeat_less_peer_is_excluded() {
        let view = doc_with("view");
        let peer = device(1);
        let docs = BTreeMap::from([(peer, doc_with("peer"))]);
        let heartbeats = BTreeMap::new();
        let (streak, diverged) = advance_divergence(1, true, &view, &docs, &heartbeats);
        assert_eq!(streak, 0);
        assert!(diverged.is_empty(), "no evidence, no finding");
    }

    // The interleaving property: the window is local — across random
    // classification sequences the finding can only be active when
    // the last two cycles were both quiet-and-disagreeing (no streak
    // leakage across an intervening busy or agreeing cycle), and it
    // must be active whenever they were.
    proptest! {
        #[test]
        fn the_window_never_fires_without_two_consecutive_quiet_disagreeing_cycles(
            steps in prop::collection::vec(step_strategy(), 0..40),
        ) {
            let view = doc_with("view");
            let mut streak = 0u32;
            let mut history: Vec<Step> = Vec::new();
            for step in steps {
                let (docs, heartbeats) = evidence(step);
                let quiet = step != Step::Busy;
                let (s, finding) = advance_divergence(streak, quiet, &view, &docs, &heartbeats);
                streak = s;
                history.push(step);
                let two_consecutive = history.len() >= 2
                    && history[history.len() - 1] == Step::QuietDisagree
                    && history[history.len() - 2] == Step::QuietDisagree;
                prop_assert!(finding.is_empty() || two_consecutive);
                if two_consecutive {
                    prop_assert_eq!(finding.len(), 1, "fires exactly when the window closes");
                }
            }
        }
    }
}
