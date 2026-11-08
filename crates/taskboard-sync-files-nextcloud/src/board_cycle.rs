// SPDX-License-Identifier: AGPL-3.0-only
//! The board lanes (D-BF8–D-BF10): per owned files-board attach, pull,
//! and drain — added beside the per-user cycle in the same actor (one
//! credential, one transport, one poll cadence; a lane's failure is
//! typed and retried per board without disturbing the per-user
//! verdict).
//!
//! The laws this module is shaped by:
//! - **The outbox is the routing fabric**: the lane serves exactly the
//!   boards the chunk-3 ownership map binds to this connector.
//! - **Materialize at drain, never op replay**: every dirty entity is
//!   re-materialized from the published state (the `CardShape`
//!   precedent), so op arrival order cannot change the wire bytes.
//! - **Honest accounting**: an op reports `Applied` only after its
//!   chunk's create-once PUT succeeded; a failed PUT leaves every op
//!   queued (a pre-emptive Applied is data loss under crash).
//! - **No sidecar**: a files-board never produces a `Document` group —
//!   the planner routes its app-data ops as entity touches (the
//!   plan-shape pin), app-only fields are entity slots, sections ride
//!   the board documents.
//! - **Fold-first discovery**: a pre-existing subtree (identity 412)
//!   publishes nothing until its first pull folded (publish-after-
//!   import, the DR-8 journal precedent).
//!
//! GC duty note: [`taskboard_sync_files::board_gc::gc_eligible`] names
//! the owner's deletable chunk seqs, but the transport's closed verb
//! set carries no DELETE yet — the deletion rides the verb's backlog
//! issue, the watermarks and snapshot floor are already GC-shaped.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

use taskboard_domain::OpId;
use taskboard_domain::ids::{AuthorId, BoardId};
use taskboard_domain::messages::SyncReport;
use taskboard_domain::ownership::BoardOwnershipMap;
use taskboard_domain::persistence::PersistedState;

use taskboard_domain::push::{EntityTables, PushTarget, plan_pushes};
use taskboard_domain::remote::{FilesBoardSnapshot, PushOutcome, PushResult};
use taskboard_sync_files::board_chunk::BoardChunk;
use taskboard_sync_files::board_fold::{fold_board, fold_sections};
use taskboard_sync_files::board_gc::{BOARD_CHUNK_FORMAT, next_chunk_range, snapshot_due};
use taskboard_sync_files::board_identity::BoardIdentity;
use taskboard_sync_files::board_payload::{EntityEntry, EntityKind, MaterializedEntity};
use taskboard_sync_files::board_snapshot::{BoardSnapshot, SnapshotState};
use taskboard_sync_files::ids::DeviceId;
use taskboard_sync_files::layout::{
    self, BOARDS_DIR_NAME, DEVICES_DIR_NAME, TREE_FORMAT_VERSION, board_identity_path,
    board_ops_path, board_snapshot_path, classify, device_dir_name,
};

use crate::cycle::SyncContext;
use crate::error::TransportError;
use crate::etag::Etag;
use crate::transport::{FilesTransport, PutOutcome};

/// One board lane's cycle-bookkeeping (in-memory; a restart re-reads
/// honestly once — the fold is idempotent, the cursor is an
/// optimization, never a correctness load-bearer).
#[derive(Debug)]
pub struct Lane {
    /// The subtree's directories + identity exist (or were adopted).
    pub attached: bool,
    /// A pre-existing identity (412): publish nothing until a pull
    /// folded once (fold-first discovery, D-SF13's board lane).
    folded_once: bool,
    /// This device's last published chunk seq (gap-free `+1` from it).
    last_published_seq: Option<u64>,
    /// Chunks published since the last snapshot (the cadence counter).
    chunks_since_snapshot: u64,
    /// The boot leg of the snapshot cadence.
    pending_boot_snapshot: bool,
    /// Per-peer ingest cursor: the highest chunk seq folded from that
    /// device's ops/ (per-device seq spaces).
    watermarks: BTreeMap<DeviceId, u64>,
    /// Member relative path → served etag at last read (the F6
    /// change filter).
    member_etags: BTreeMap<String, Etag>,
    /// Own snapshot file's etag (the If-Match guard).
    snapshot_etag: Option<Etag>,
    /// Per-peer device-dir etag (the F6 filter one level up: the
    /// per-device subtree is re-listed only when its dir changed).
    device_dir_etags: BTreeMap<DeviceId, Etag>,
}

impl Default for Lane {
    fn default() -> Self {
        Self {
            attached: false,
            folded_once: false,
            last_published_seq: None,
            chunks_since_snapshot: 0,
            pending_boot_snapshot: true,
            watermarks: BTreeMap::new(),
            member_etags: BTreeMap::new(),
            snapshot_etag: None,
            device_dir_etags: BTreeMap::new(),
        }
    }
}

/// All lanes, keyed by local board id.
#[derive(Debug, Default)]
pub struct Lanes {
    pub boards: BTreeMap<BoardId, Lane>,
    /// Foreign subtrees already folded once (discovery door 2): the
    /// binding mints via the engine's ingestion; once bound, the board
    /// joins `boards` with a real lane (a fresh one re-folds once —
    /// the fold is idempotent).
    discovered: std::collections::BTreeSet<uuid::Uuid>,
}

/// The one board-cycle tick: every files-owned board's lane, in
/// board-id order. Returns the number of lanes that ran to completion
/// (diagnostics only — failures are logged, never thrown across the
/// per-user verdict).
pub async fn run_board_lanes<T: FilesTransport>(
    transport: &T,
    ctx: &SyncContext,
    lanes: &mut Lanes,
) -> usize {
    let Ok(persisted) = ctx.state_reader.read_state().await else {
        tracing::warn!("board lanes skipped: the engine is unreachable");
        return 0;
    };
    let ownership = BoardOwnershipMap::from_boards(&persisted.boards);
    let owned: Vec<(BoardId, uuid::Uuid)> = ownership
        .bindings()
        .filter(|(_, connector)| **connector == ctx.connector)
        .filter_map(|(board, _)| {
            let remote = persisted
                .boards
                .get(&board)?
                .remote
                .as_ref()?
                .board
                .as_files()?;
            Some((board, remote))
        })
        .collect();
    let mut completed = 0usize;
    for (board, remote) in owned {
        let lane = lanes.boards.entry(board).or_default();
        match run_lane(transport, ctx, lane, board, remote, &persisted).await {
            Ok(()) => completed += 1,
            Err(err) => {
                tracing::warn!(?err, board = ?board, "board lane failed; retried next tick");
            }
        }
    }
    completed += discover_subtrees(transport, ctx, lanes, &persisted).await;
    completed
}

/// Discovery door 2 (D-SF13's board lane): a foreign
/// `boards/<uuid>/` subtree — no local binding, never discovered
/// before — folds once (pull-only: the fold-first law publishes
/// nothing) and reports; the engine's Files ingestion mints the
/// binding, and the next tick the board is owned.
async fn discover_subtrees<T: FilesTransport>(
    transport: &T,
    ctx: &SyncContext,
    lanes: &mut Lanes,
    persisted: &PersistedState,
) -> usize {
    let boards_dir = crate::cycle::rooted(BOARDS_DIR_NAME);
    let Ok(listing) = transport.propfind_tree(&boards_dir).await else {
        return 0; // no boards dir yet: nothing to discover
    };
    let bound: std::collections::BTreeSet<uuid::Uuid> = persisted
        .boards
        .values()
        .filter_map(|board| board.remote.as_ref().and_then(|r| r.board.as_files()))
        .collect();
    let mut folded = 0usize;
    for member in &listing.members {
        let Some(relative) = member
            .href
            .rsplit_once("taskboard/")
            .map(|(_, rest)| rest.trim_end_matches('/'))
        else {
            continue;
        };
        let Ok(uuid) = uuid::Uuid::parse_str(relative.rsplit('/').next().unwrap_or(relative))
        else {
            continue; // a foreign name is not ours to read
        };
        if bound.contains(&uuid) || !lanes.discovered.insert(uuid) {
            continue;
        }
        if persisted
            .boards
            .values()
            .any(|board| board.id.as_uuid() == uuid)
        {
            // A local board of ours with no files binding (a Deck
            // orphan, or an unbound local row): adoption is a user
            // decision (the opt-in door), never a silent scan claim.
            tracing::info!(
                ?uuid,
                "foreign subtree for an unbound local board; adoption deferred to the opt-in door"
            );
            continue;
        }
        // Fold-first: pull-only, publish nothing (the provisional
        // lane's law).
        let board = BoardId::from_uuid(uuid);
        let started = Instant::now();
        let mut lane = Lane {
            attached: true, // the subtree predates us; never write
            ..Lane::default()
        };
        match pull_board(transport, ctx, &mut lane, board, uuid, started).await {
            Ok(_) => folded += 1,
            Err(err) => {
                tracing::warn!(?err, ?uuid, "discovery fold failed; retried next tick");
                lanes.discovered.remove(&uuid);
            }
        }
    }
    folded
}

/// What the files tier holds at a move target's subtree, as this device
/// reads it (the move ceremony's resume evidence, D-BF11).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoveSubtreeProbe {
    /// No identity: nothing was published.
    Absent,
    /// The identity and this device's directory exist, the floor does
    /// not: a publish died midway and resumes.
    Partial,
    /// This device's snapshot floor exists.
    Published,
    /// An identity this device never attached to.
    Foreign,
}

/// Reads the move target's subtree; see [`MoveSubtreeProbe`].
///
/// # Errors
///
/// Any transport failure other than the absences the probe classifies.
pub async fn probe_move_subtree<T: FilesTransport>(
    transport: &T,
    device: DeviceId,
    remote: uuid::Uuid,
) -> Result<MoveSubtreeProbe, TransportError> {
    let board = BoardId::from_uuid(remote);
    let identity = crate::cycle::rooted(&board_identity_path(board));
    match transport.get(&identity).await {
        Ok(_) => {}
        Err(TransportError::NotFound) => return Ok(MoveSubtreeProbe::Absent),
        Err(err) => return Err(err),
    }
    let floor = crate::cycle::rooted(&board_snapshot_path(board, device, 0));
    match transport.get(&floor).await {
        Ok(_) => return Ok(MoveSubtreeProbe::Published),
        Err(TransportError::NotFound) => {}
        Err(err) => return Err(err),
    }
    let devices = crate::cycle::rooted(&format!("{BOARDS_DIR_NAME}/{remote}/{DEVICES_DIR_NAME}"));
    let own = device_dir_name(device);
    let listing = match transport.propfind_tree(&devices).await {
        Ok(listing) => listing,
        Err(TransportError::NotFound) => return Ok(MoveSubtreeProbe::Foreign),
        Err(err) => return Err(err),
    };
    let attached = listing
        .members
        .iter()
        .any(|member| member.href.trim_end_matches('/').ends_with(&own));
    Ok(if attached {
        MoveSubtreeProbe::Partial
    } else {
        MoveSubtreeProbe::Foreign
    })
}

/// Publishes a board's converged state as its late-join floor without a
/// binding existing yet (the move ceremony's step 2: publish precedes
/// retirement).
///
/// # Errors
///
/// The attach or snapshot PUT's transport failure.
pub async fn publish_move_floor<T: FilesTransport>(
    transport: &T,
    ctx: &SyncContext,
    board: BoardId,
    remote: uuid::Uuid,
    persisted: &PersistedState,
) -> Result<(), TransportError> {
    let mut lane = Lane::default();
    attach_board(transport, ctx, &mut lane, remote).await?;
    snapshot_duty(transport, ctx, &mut lane, board, remote, persisted).await
}

async fn run_lane<T: FilesTransport>(
    transport: &T,
    ctx: &SyncContext,
    lane: &mut Lane,
    board: BoardId,
    remote: uuid::Uuid,
    persisted: &PersistedState,
) -> Result<(), TransportError> {
    let started = Instant::now();
    if !lane.attached {
        attach_board(transport, ctx, lane, remote).await?;
    }
    pull_board(transport, ctx, lane, board, remote, started).await?;
    if !lane.folded_once {
        // The first pull failed halfway on a discovered subtree:
        // publish nothing (publish-after-import ordering, pinned).
        return Ok(());
    }
    drain_board(transport, ctx, lane, board, remote, persisted).await?;
    snapshot_duty(transport, ctx, lane, board, remote, persisted).await?;
    Ok(())
}

/// The board subtree's MKCOL ladder + create-once identity. A 412 on
/// the identity means the subtree predates this device (another
/// device's adoption, or the move ceremony's step 2): the lane adopts
/// fold-first.
async fn attach_board<T: FilesTransport>(
    transport: &T,
    ctx: &SyncContext,
    lane: &mut Lane,
    remote: uuid::Uuid,
) -> Result<(), TransportError> {
    let boards_dir = format!("{}/", crate::cycle::rooted(BOARDS_DIR_NAME));
    crate::cycle::mkcol_exists_ok(transport, boards_dir.trim_end_matches('/')).await?;
    let subtree_dir = format!("{boards_dir}{remote}");
    crate::cycle::mkcol_exists_ok(transport, &subtree_dir).await?;
    let devices_dir = format!("{subtree_dir}/{DEVICES_DIR_NAME}");
    crate::cycle::mkcol_exists_ok(transport, &devices_dir).await?;
    let self_dir = format!("{devices_dir}/{}", device_dir_name(ctx.identity.device_id));
    crate::cycle::mkcol_exists_ok(transport, &self_dir).await?;

    let identity_path = crate::cycle::rooted(&board_identity_path(BoardId::from_uuid(remote)));
    let identity = BoardIdentity {
        board_id: remote,
        format_version: TREE_FORMAT_VERSION,
        created_at: ctx.clock.now(),
    };
    let body = serde_json::to_vec(&identity).expect("identity serializes");
    match transport.put_create(&identity_path, &body).await {
        Ok(_) => lane.attached = true,
        Err(TransportError::PreconditionFailed) => {
            // Pre-existing subtree: adopt fold-first (discovery door 2).
            tracing::info!(?remote, "board subtree adopted fold-first");
            lane.attached = true;
        }
        Err(err) => return Err(err),
    }
    Ok(())
}

/// Everything one lane's pull folded, ready to report and republish.
struct PulledBoard {
    entities: Vec<EntityEntry>,
    sections: Option<taskboard_domain::appdata::AppDataDocument>,
    up_to_seq: u64,
}

/// The pull (D-BF9): list the subtree level by level (depth-1
/// reality), fetch changed members (F6 etag compare; a redundant
/// re-fetch is harmless — the fold is idempotent), fold entities +
/// sections per the chunk-4 machinery, and deliver the folded board
/// as a `Files`-tagged `Completed`.
async fn pull_board<T: FilesTransport>(
    transport: &T,
    ctx: &SyncContext,
    lane: &mut Lane,
    board: BoardId,
    remote: uuid::Uuid,
    started: Instant,
) -> Result<PulledBoard, TransportError> {
    let subtree = crate::cycle::rooted(&format!("{BOARDS_DIR_NAME}/{remote}"));
    let mut pulled = PulledBoard {
        entities: Vec::new(),
        sections: None,
        up_to_seq: 0,
    };
    let mut rejected_rows = 0usize;
    let mut section_docs: Vec<serde_json::Value> = Vec::new();
    // Level 1: the subtree lists the devices dir (and identity.json).
    let devices_dir = format!("{subtree}/{DEVICES_DIR_NAME}");
    for member in &transport.propfind_tree(&devices_dir).await?.members {
        let Some(relative) = member
            .href
            .rsplit_once("taskboard/")
            .map(|(_, rest)| rest.trim_end_matches('/'))
        else {
            continue;
        };
        let Some(peer) = DeviceId::parse(relative.rsplit('/').next().unwrap_or(relative)) else {
            continue;
        };
        // F6's dir-etag filter one level up: an unchanged device dir
        // has already been folded.
        let Some(etag) = &member.etag else { continue };
        if lane.device_dir_etags.get(&peer) == Some(etag) {
            continue;
        }
        // The cursor records only AFTER the fold succeeded: a
        // failed/garbled fold must re-list the dir next tick (the
        // cursor is an optimization, never a correctness load-bearer).
        fold_peer_subtree(
            transport,
            lane,
            peer,
            &format!("{devices_dir}/{peer}"),
            &mut pulled.entities,
            &mut section_docs,
            &mut pulled.up_to_seq,
            &mut rejected_rows,
        )
        .await?;
        lane.device_dir_etags.insert(peer, etag.clone());
    }
    if rejected_rows > 0 {
        tracing::debug!(?board, rejected_rows, "board pull relayed with findings");
    }
    pulled.sections = fold_sections(&section_docs).merged;
    lane.folded_once = true;
    report_pull(ctx, board, remote, &pulled, started).await;
    Ok(pulled)
}

/// One peer device subtree's fold: the device dir's snapshot files,
/// plus its ops/ collection's chunk files (depth-1 all the way down),
/// each gated by its served etag.
#[allow(clippy::too_many_arguments)] // the fold's accumulator tuple, spelled out
async fn fold_peer_subtree<T: FilesTransport>(
    transport: &T,
    lane: &mut Lane,
    peer: DeviceId,
    device_dir: &str,
    entities: &mut Vec<EntityEntry>,
    section_docs: &mut Vec<serde_json::Value>,
    up_to_seq: &mut u64,
    rejected_rows: &mut usize,
) -> Result<(), TransportError> {
    for member in &transport.propfind_tree(device_dir).await?.members {
        let Some(relative) = member
            .href
            .rsplit_once("taskboard/")
            .map(|(_, rest)| rest.trim_end_matches('/'))
        else {
            continue;
        };
        let unchanged = member
            .etag
            .as_ref()
            .is_some_and(|etag| lane.member_etags.get(relative) == Some(etag));
        match classify(relative) {
            layout::DeviceFileKind::BoardSnapshot { .. } => {
                if unchanged {
                    continue;
                }
                let Ok(content) = read_member(transport, relative).await else {
                    continue;
                };
                match serde_json::from_slice::<BoardSnapshot>(&content.bytes) {
                    Ok(snapshot) => {
                        record_etag(lane, relative, &content);
                        if snapshot.device_id == peer {
                            entities.extend(snapshot.state.entities.iter().cloned());
                            if let Some(sections) = snapshot.sections {
                                section_docs.push(sections);
                            }
                            *up_to_seq = (*up_to_seq).max(snapshot.seq);
                        } else {
                            *rejected_rows += 1; // the path owns identity
                        }
                    }
                    Err(_) => *rejected_rows += 1, // uncached: re-read next tick
                }
            }
            layout::DeviceFileKind::Ops { .. } | layout::DeviceFileKind::BoardOps { .. } => {}
            _ if relative.rsplit('/').next() == Some(layout::OPS_DIR_NAME) => {
                // The ops/ collection: its chunk files sit one level
                // deeper; each changed chunk folds the same way.
                let ops_dir = format!("{device_dir}/{}", layout::OPS_DIR_NAME);
                for chunk_member in &transport.propfind_tree(&ops_dir).await?.members {
                    let Some(chunk_relative) = chunk_member
                        .href
                        .rsplit_once("taskboard/")
                        .map(|(_, rest)| rest.trim_end_matches('/'))
                    else {
                        continue;
                    };
                    let layout::DeviceFileKind::BoardOps { seq, .. } = classify(chunk_relative)
                    else {
                        continue;
                    };
                    let chunk_unchanged = chunk_member
                        .etag
                        .as_ref()
                        .is_some_and(|etag| lane.member_etags.get(chunk_relative) == Some(etag));
                    if chunk_unchanged || seq <= lane.watermarks.get(&peer).copied().unwrap_or(0) {
                        continue;
                    }
                    let Ok(content) = read_member(transport, chunk_relative).await else {
                        continue;
                    };
                    match serde_json::from_slice::<BoardChunk>(&content.bytes) {
                        Ok(chunk) => {
                            record_etag(lane, chunk_relative, &content);
                            if chunk.device_id == peer {
                                entities.extend(chunk.entities.iter().cloned());
                                if let Some(sections) = chunk.sections {
                                    section_docs.push(sections);
                                }
                                lane.watermarks
                                    .entry(peer)
                                    .and_modify(|cursor| *cursor = (*cursor).max(seq))
                                    .or_insert(seq);
                                *up_to_seq = (*up_to_seq).max(seq);
                            } else {
                                *rejected_rows += 1; // the path owns identity
                            }
                        }
                        // A torn chunk heals by re-read (its etag is
                        // not cached on failure), else via the snapshot.
                        Err(_) => *rejected_rows += 1,
                    }
                }
            }
            _ => {} // identity, info, foreign, temporary: not this lane's data
        }
    }
    Ok(())
}

async fn read_member<T: FilesTransport>(
    transport: &T,
    relative: &str,
) -> Result<crate::transport::FileContent, TransportError> {
    transport.get(&crate::cycle::rooted(relative)).await
}

fn record_etag(lane: &mut Lane, relative: &str, content: &crate::transport::FileContent) {
    if let Some(etag) = &content.etag {
        lane.member_etags.insert(relative.to_owned(), etag.clone());
    }
}

/// The drain (D-BF8): the board's dirty entities materialize into one
/// immutable chunk; every op in it reports `Applied` only after the
/// create-once PUT succeeded.
#[allow(clippy::too_many_lines)] // one stage per step: plan, collect, materialize, publish, account
async fn drain_board<T: FilesTransport>(
    transport: &T,
    ctx: &SyncContext,
    lane: &mut Lane,
    board: BoardId,
    remote: uuid::Uuid,
    persisted: &PersistedState,
) -> Result<(), TransportError> {
    let ownership = BoardOwnershipMap::from_boards(&persisted.boards);
    let tables = EntityTables {
        tasks: &persisted.tasks,
        stacks: &persisted.stacks,
        labels: &persisted.labels,
        boards: &persisted.boards,
    };
    let board_ops: Vec<&taskboard_domain::PendingOp> = persisted
        .outbox
        .iter()
        .filter(|entry| entry.op.owning_board(&tables) == Some(board))
        .collect();
    if board_ops.is_empty() {
        return Ok(());
    }
    let owned_ops: Vec<taskboard_domain::PendingOp> = board_ops.into_iter().cloned().collect();
    let groups = plan_pushes(&owned_ops, tables, &ownership);
    // The no-sidecar shape: no Document group exists for this board
    // (the planner pin); the dirty set is the group targets.
    let mut dirty_tasks = BTreeSet::new();
    let mut dirty_stacks = BTreeSet::new();
    let mut dirty_labels = BTreeSet::new();
    let mut dirty_board = false;
    let mut chunk_ops: Vec<OpId> = Vec::new();
    for group in &groups {
        match group.target {
            PushTarget::Task(task) => {
                dirty_tasks.insert(task);
            }
            PushTarget::Stack(stack) => {
                dirty_stacks.insert(stack);
            }
            PushTarget::Label(label) => {
                dirty_labels.insert(label);
            }
            PushTarget::Board(_) => dirty_board = true,
            PushTarget::Document(_) | PushTarget::RemoteCard(_) | PushTarget::RemoteStack(_) => {
                tracing::warn!(
                    board = ?board,
                    "plan produced a non-entity target on a files board; op stays queued"
                );
                continue;
            }
        }
        chunk_ops.push(group.primary);
        chunk_ops.extend(group.subsumed.iter().copied());
    }
    let mut entries: Vec<EntityEntry> = Vec::new();
    if dirty_board && let Some(row) = persisted.boards.get(&board) {
        entries.push(EntityEntry::Modeled(MaterializedEntity::from_board(row)));
    }
    for stack in &dirty_stacks {
        if let Some(row) = persisted.stacks.get(stack) {
            entries.push(EntityEntry::Modeled(MaterializedEntity::from_stack(row)));
        }
    }
    for label in &dirty_labels {
        if let Some(row) = persisted.labels.get(label) {
            entries.push(EntityEntry::Modeled(MaterializedEntity::from_label(row)));
        }
    }
    for task in &dirty_tasks {
        if let Some(row) = persisted.tasks.get(task) {
            entries.push(EntityEntry::Modeled(MaterializedEntity::from_task(row)));
        }
    }
    // Every op must have found an entity to materialize, or it is not
    // publishable — honesty over accounting pressure (an unpublished
    // op stays queued and retries; a pre-emptive Applied is data loss).
    if entries.is_empty() {
        tracing::debug!(board = ?board, "drain found nothing publishable; ops stay queued");
        return Ok(());
    }
    let Some((first_seq, last_seq)) = next_chunk_range(lane.last_published_seq, entries.len())
    else {
        tracing::warn!(board = ?board, "chunk range unrepresentable; ops stay queued");
        return Ok(());
    };
    let chunk = BoardChunk {
        format_version: BOARD_CHUNK_FORMAT,
        device_id: ctx.identity.device_id,
        first_seq,
        last_seq,
        written_at: ctx.clock.now(),
        entities: entries,
        sections: Some(
            serde_json::to_value(sections_document(ctx, persisted, board))
                .expect("sections serialize"),
        ),
        unknown: BTreeMap::new(),
    };
    let body = serde_json::to_vec(&chunk).expect("chunk serializes");
    let path = crate::cycle::rooted(&board_ops_path(
        BoardId::from_uuid(remote),
        ctx.identity.device_id,
        last_seq,
    ));
    match transport.put_create(&path, &body).await {
        Ok(_) => {
            lane.last_published_seq = Some(last_seq);
            lane.chunks_since_snapshot += 1;
            publish_drain_report(ctx, board, remote, &chunk_ops).await;
        }
        Err(TransportError::PreconditionFailed) => {
            // Crash-window reconcile: the seq space is this device's
            // own, so a create-once 412 means THIS chunk published in
            // a crashed attempt. Fetch it; an own-device chunk at the
            // same last_seq IS the publication — the ops report
            // Applied (the idempotence pin: exactly once).
            match transport.get(&path).await {
                Ok(content) => {
                    let published = serde_json::from_slice::<BoardChunk>(&content.bytes).is_ok_and(
                        |existing| {
                            existing.device_id == ctx.identity.device_id
                                && existing.last_seq == last_seq
                        },
                    );
                    if published {
                        tracing::info!(
                            ?board,
                            seq = last_seq,
                            "own chunk reconciled after a crash"
                        );
                        lane.last_published_seq = Some(last_seq);
                        lane.chunks_since_snapshot += 1;
                        publish_drain_report(ctx, board, remote, &chunk_ops).await;
                    } else {
                        tracing::error!(
                            ?board,
                            seq = last_seq,
                            "create-once 412 reconciled to a foreign chunk; ops stay queued"
                        );
                    }
                }
                Err(_) => {
                    tracing::error!(
                        ?board,
                        seq = last_seq,
                        "create-once 412 without the file; ops stay queued"
                    );
                }
            }
        }
        Err(err) => return Err(err),
    }
    Ok(())
}

/// The snapshot duty (D-BF7's cadence, chunk 5's publisher): on boot
/// and every [`SNAPSHOT_EVERY_N_CHUNKS`] chunks the owner folds its
/// view into `snapshot-<seq>.json` (guarded rewrite; first publish
/// create-once).
async fn snapshot_duty<T: FilesTransport>(
    transport: &T,
    ctx: &SyncContext,
    lane: &mut Lane,
    board: BoardId,
    remote: uuid::Uuid,
    persisted: &PersistedState,
) -> Result<(), TransportError> {
    if !snapshot_due(lane.chunks_since_snapshot, lane.pending_boot_snapshot) {
        return Ok(());
    }
    let seq = lane.last_published_seq.unwrap_or(0);
    let entries: Vec<EntityEntry> = persisted
        .boards
        .get(&board)
        .map(|row| EntityEntry::Modeled(MaterializedEntity::from_board(row)))
        .into_iter()
        .chain(
            persisted
                .stacks
                .values()
                .filter(|stack| stack.board == board)
                .map(|stack| EntityEntry::Modeled(MaterializedEntity::from_stack(stack))),
        )
        .chain(
            persisted
                .labels
                .values()
                .filter(|label| label.board == board)
                .map(|label| EntityEntry::Modeled(MaterializedEntity::from_label(label))),
        )
        .chain(
            persisted
                .tasks
                .values()
                .filter(|task| {
                    persisted
                        .stacks
                        .get(&task.stack)
                        .is_some_and(|stack| stack.board == board)
                })
                .map(|task| EntityEntry::Modeled(MaterializedEntity::from_task(task))),
        )
        .collect();
    let folded = fold_board(&entries);
    let snapshot = BoardSnapshot {
        format_version: TREE_FORMAT_VERSION,
        device_id: ctx.identity.device_id,
        seq,
        written_at: ctx.clock.now(),
        state: SnapshotState {
            entities: folded
                .entities
                .values()
                .map(|entity| EntityEntry::Modeled(entity.clone()))
                .collect(),
            unknown: folded.verbatim,
        },
        sections: Some(
            serde_json::to_value(sections_document(ctx, persisted, board))
                .expect("sections serialize"),
        ),
        unknown: BTreeMap::new(),
    };
    let body = serde_json::to_vec(&snapshot).expect("snapshot serializes");
    let path = crate::cycle::rooted(&board_snapshot_path(
        BoardId::from_uuid(remote),
        ctx.identity.device_id,
        seq,
    ));
    let outcome = match &lane.snapshot_etag {
        Some(etag) => transport.put_if_match(&path, &body, etag).await,
        None => transport.put_create(&path, &body).await,
    };
    match outcome {
        Ok(PutOutcome::Created { etag } | PutOutcome::Updated { etag }) => {
            lane.snapshot_etag = etag;
            lane.pending_boot_snapshot = false;
            lane.chunks_since_snapshot = 0;
            tracing::debug!(board = ?board, seq, "board snapshot published");
        }
        Err(TransportError::PreconditionFailed) => {
            // The lost-update guard: our snapshot token is stale — the
            // fork posture logs loudly and skips this cadence; the next
            // snapshot re-reads the file's fresh etag off the listing.
            tracing::warn!(board = ?board, "snapshot If-Match 412; cadence skipped");
            lane.snapshot_etag = None;
        }
        Err(err) => return Err(err),
    }
    Ok(())
}

/// The drain's report (D-BF8): the echo-less `Applied` settles every
/// published op through the engine's ordinary accounting; the (empty)
/// files snapshot merges nothing.
async fn publish_drain_report(ctx: &SyncContext, board: BoardId, remote: uuid::Uuid, ops: &[OpId]) {
    let pushes: Vec<PushOutcome> = ops
        .iter()
        .map(|op_id| PushOutcome {
            op: *op_id,
            result: PushResult::Applied { echo: None },
        })
        .collect();
    let identity = taskboard_domain::RemoteBoardId::files(remote);
    let report = SyncReport::Completed {
        connector: ctx.connector.clone(),
        snapshot: Box::new(taskboard_domain::remote::RemoteBoardSnapshot::Files(
            FilesBoardSnapshot::identity_only(identity),
        )),
        validators: taskboard_domain::BoardPullValidators {
            boards: taskboard_domain::SyncValidators::default(),
            stacks: taskboard_domain::SyncValidators::default(),
            archived_stacks: taskboard_domain::SyncValidators::default(),
        },
        pushes,
        read_at: ctx.clock.now(),
        duration: std::time::Duration::ZERO,
        app_data: taskboard_domain::AppDataEvidence::Absent,
        features: taskboard_domain::DeckFeatures::default(),
    };
    if ctx.board_reports.send(report).await.is_err() {
        // The engine is gone; nothing to account to. The chunk is
        // published (create-once), so the data is on the wire — the
        // ops re-report on a later cycle only if still queued (the
        // honest at-least-once posture).
        tracing::warn!(board = ?board, "board report lane closed; ops re-report next cycle");
    }
}

/// The pull's report (D-BF9): the folded board decoded back to domain
/// rows (chunk 4's codecs — one mapping, no fork), delivered as a
/// `Files`-tagged `Completed` the engine ingests through the ordinary
/// pipeline.
async fn report_pull(
    ctx: &SyncContext,
    board: BoardId,
    remote: uuid::Uuid,
    pulled: &PulledBoard,
    started: Instant,
) {
    let remote_board = taskboard_domain::RemoteBoardId::files(remote);
    let mut board_row = None;
    let mut stacks = Vec::new();
    let mut tasks = Vec::new();
    let mut labels = Vec::new();
    let mut undecodable = 0usize;
    for entry in &pulled.entities {
        let EntityEntry::Modeled(entity) = entry else {
            undecodable += 1; // a verbatim relay this decode cannot carry
            continue;
        };
        let decoded = match entity.kind {
            EntityKind::Board => entity.to_board().map(|row| board_row = Some(row)),
            EntityKind::Stack => entity.to_stack(board).map(|row| stacks.push(row)),
            EntityKind::Task => entity.to_task().map(|row| tasks.push(row)),
            EntityKind::Label => entity.to_label(board).map(|row| labels.push(row)),
        };
        if decoded.is_none() {
            undecodable += 1;
        }
    }
    if undecodable > 0 {
        tracing::debug!(board = ?board, undecodable, "fold rows this decode cannot carry");
    }
    let report = SyncReport::Completed {
        connector: ctx.connector.clone(),
        snapshot: Box::new(taskboard_domain::remote::RemoteBoardSnapshot::Files(
            FilesBoardSnapshot {
                board: remote_board,
                up_to_seq: pulled.up_to_seq,
                board_row,
                stacks,
                tasks,
                labels,
                sections: pulled.sections.clone().map(Box::new),
            },
        )),
        validators: taskboard_domain::BoardPullValidators {
            boards: taskboard_domain::SyncValidators::default(),
            stacks: taskboard_domain::SyncValidators::default(),
            archived_stacks: taskboard_domain::SyncValidators::default(),
        },
        pushes: Vec::new(),
        read_at: ctx.clock.now(),
        duration: started.elapsed(),
        app_data: taskboard_domain::AppDataEvidence::Absent,
        features: taskboard_domain::DeckFeatures::default(),
    };
    if ctx.board_reports.send(report).await.is_err() {
        tracing::warn!(board = ?board, "board report lane closed; pull dropped");
    }
}

/// The board document's section payload: the sidecar's row codecs and
/// retention windows verbatim, keyed by domain UUIDs (D-BF9's sections
/// leg; the fold unions across publishers by row clock). The per-card
/// `tasks` section has no files home — app-only fields are entity
/// slots there (the one lane).
fn sections_document(
    ctx: &SyncContext,
    persisted: &PersistedState,
    board: BoardId,
) -> taskboard_domain::appdata::AppDataDocument {
    let author = AuthorId::from_uuid(ctx.identity.device_id.as_uuid());
    let now = ctx.clock.now();
    let today = now.date_naive();
    let board_tasks: Vec<_> = persisted
        .tasks
        .values()
        .filter(|task| {
            persisted
                .stacks
                .get(&task.stack)
                .is_some_and(|stack| stack.board == board)
        })
        .collect();
    let mut reminder_rules = BTreeMap::new();
    let mut dismissals = BTreeMap::new();
    for task in &board_tasks {
        let key = task.id.as_uuid().to_string();
        let rules = taskboard_domain::appdata::rules_entry_for(
            &persisted.reminders.rules,
            task.id,
            Some(author),
        );
        if !rules.rules.is_empty() || !rules.unknown.is_empty() {
            reminder_rules.insert(key.clone(), rules);
        }
        let task_dismissals = taskboard_domain::appdata::dismissals_entry_for(
            &persisted.reminders.dismissals,
            &persisted.reminders.rules,
            task.id,
            Some(author),
        );
        if !task_dismissals.dismissals.is_empty() || !task_dismissals.unknown.is_empty() {
            dismissals.insert(key, task_dismissals);
        }
    }
    let time_entries = taskboard_domain::appdata::files_time_entries_for(
        &persisted.tracking.entries,
        &persisted.tasks,
        &persisted.stacks,
        board,
        now,
        Some(author),
    );
    let habits =
        taskboard_domain::appdata::habits_for(&persisted.habits.habits, board, Some(author));
    let habit_log = taskboard_domain::appdata::habit_log_for(
        &persisted.habits.habits,
        &persisted.habits.log,
        board,
        today,
        Some(author),
    );
    let break_rules = taskboard_domain::appdata::break_rules_for(
        &persisted.reminders.break_rules,
        board,
        Some(author),
    );
    let sections = taskboard_domain::appdata::AppDataSections {
        tasks: None,
        reminder_rules: if reminder_rules.is_empty() {
            None
        } else {
            Some(reminder_rules)
        },
        dismissals: if dismissals.is_empty() {
            None
        } else {
            Some(dismissals)
        },
        time_entries: if time_entries.is_empty() {
            None
        } else {
            Some(time_entries)
        },
        habits: if habits.is_empty() {
            None
        } else {
            Some(habits)
        },
        habit_log: if habit_log.is_empty() {
            None
        } else {
            Some(habit_log)
        },
        break_rules: if break_rules.is_empty() {
            None
        } else {
            Some(break_rules)
        },
        unknown: BTreeMap::new(),
    };
    taskboard_domain::appdata::AppDataDocument {
        format: taskboard_domain::appdata::APPDATA_FORMAT,
        board: taskboard_domain::RemoteBoardId::files(board.as_uuid()),
        sections,
        unknown: BTreeMap::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::FakeFilesTransport;
    use std::sync::Arc;
    use taskboard_domain::ids::{RemoteBoardId, StackId, TaskId};
    use taskboard_domain::persistence::PersistedState;
    use taskboard_domain::{Board, LocalOp, PendingOp, Stack, Task};
    use taskboard_sync_files::ids::DeviceId;
    use tokio::sync::mpsc;

    const SELF: &str = "0193a1b2-c3d4-7cc9-8a2e-5f1e2d3c4b5a";
    const BOARD_UUID: uuid::Uuid = uuid::Uuid::from_u128(0xB0A1D);
    const STACK_UUID: uuid::Uuid = uuid::Uuid::from_u128(0x57AC);
    const TASK_UUID: uuid::Uuid = uuid::Uuid::from_u128(0xCA2D);

    fn device() -> DeviceId {
        DeviceId::parse(SELF).expect("parses")
    }

    fn board() -> BoardId {
        BoardId::from_uuid(BOARD_UUID)
    }

    /// A context over an in-memory state: the boards the state carries
    /// bound to "files-sync" are this actor's lanes.
    struct FixedState(PersistedState);

    impl std::fmt::Debug for FixedState {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("FixedState").finish()
        }
    }

    impl taskboard_domain::persistence::SyncStateReader for FixedState {
        fn read_state(
            &self,
        ) -> taskboard_domain::BoxFuture<
            '_,
            Result<PersistedState, taskboard_domain::RepositoryError>,
        > {
            Box::pin(async { Ok(self.0.clone()) })
        }
        fn read_settings(&self) -> taskboard_domain::SettingsSnapshot {
            taskboard_domain::SettingsSnapshot::default()
        }
    }

    fn files_bound_board() -> Board {
        Board {
            id: board(),
            remote: Some(taskboard_domain::ids::RemoteBoardBinding {
                connector: taskboard_domain::ids::ConnectorId::from("files-sync"),
                board: RemoteBoardId::files(BOARD_UUID),
            }),
            title: "files board".into(),
            color: taskboard_domain::Color::new("2a9d8f"),
            archived: false,
            deleted: false,
            clocks: taskboard_domain::BoardClocks {
                title: chrono::DateTime::<chrono::Utc>::MIN_UTC,
                color: chrono::DateTime::<chrono::Utc>::MIN_UTC,
                archived: chrono::DateTime::<chrono::Utc>::MIN_UTC,
                deleted: chrono::DateTime::<chrono::Utc>::MIN_UTC,
            },
            authors: taskboard_domain::entities::BoardAuthors::default(),
            remote_seen: None,
        }
    }

    fn stack() -> Stack {
        Stack {
            id: StackId::from_uuid(STACK_UUID),
            board: board(),
            remote: None,
            title: "column".into(),
            order: 1,
            archived: false,
            deleted: false,
            clocks: taskboard_domain::StackClocks {
                title: chrono::DateTime::<chrono::Utc>::MIN_UTC,
                order: chrono::DateTime::<chrono::Utc>::MIN_UTC,
                deleted: chrono::DateTime::<chrono::Utc>::MIN_UTC,
            },
            authors: taskboard_domain::entities::StackAuthors::default(),
            remote_seen: None,
        }
    }

    fn task() -> Task {
        Task {
            id: TaskId::from_uuid(TASK_UUID),
            stack: StackId::from_uuid(STACK_UUID),
            remote: None,
            title: "seed card".into(),
            description: String::new(),
            duedate: None,
            scheduled: None,
            done: None,
            order: 1,
            label_memberships: BTreeMap::new(),
            effort: None,
            importance: None,
            urgency: None,
            recur: None,
            kind: taskboard_domain::entities::TaskKind::Standard,
            archived: false,
            deleted: false,
            clocks: taskboard_domain::TaskClocks {
                title: chrono::DateTime::<chrono::Utc>::MIN_UTC,
                description: chrono::DateTime::<chrono::Utc>::MIN_UTC,
                duedate: chrono::DateTime::<chrono::Utc>::MIN_UTC,
                scheduled: chrono::DateTime::<chrono::Utc>::MIN_UTC,
                done: chrono::DateTime::<chrono::Utc>::MIN_UTC,
                position: chrono::DateTime::<chrono::Utc>::MIN_UTC,
                effort: chrono::DateTime::<chrono::Utc>::MIN_UTC,
                importance: chrono::DateTime::<chrono::Utc>::MIN_UTC,
                urgency: chrono::DateTime::<chrono::Utc>::MIN_UTC,
                recur: chrono::DateTime::<chrono::Utc>::MIN_UTC,
                kind: chrono::DateTime::<chrono::Utc>::MIN_UTC,
                archived: chrono::DateTime::<chrono::Utc>::MIN_UTC,
                deleted: chrono::DateTime::<chrono::Utc>::MIN_UTC,
            },
            authors: taskboard_domain::entities::TaskAuthors::default(),
            remote_seen: None,
            remote_baseline: None,
        }
    }

    fn state_with(outbox: Vec<PendingOp>) -> PersistedState {
        let mut state = PersistedState::default();
        state.boards.insert(board(), files_bound_board());
        state.stacks.insert(StackId::from_uuid(STACK_UUID), stack());
        state.tasks.insert(TaskId::from_uuid(TASK_UUID), task());
        state.outbox = outbox;
        state
    }

    fn ctx(
        state: Arc<FixedState>,
        board_reports: mpsc::Sender<SyncReport>,
    ) -> crate::cycle::SyncContext {
        crate::cycle::SyncContext {
            identity: taskboard_sync_files::identity::DeviceIdentity {
                device_id: device(),
                created_at: chrono::DateTime::<chrono::Utc>::MIN_UTC,
            },
            app_version: "test".into(),
            clock: Arc::new(FixedClock),
            local: Arc::new(crate::cycle::EmptyDocument),
            report_tx: mpsc::channel(1).0,
            connector: taskboard_domain::ids::ConnectorId::from("files-sync"),
            state_reader: state,
            board_reports,
        }
    }

    #[derive(Debug)]
    struct FixedClock;

    impl taskboard_domain::Clock for FixedClock {
        fn now(&self) -> chrono::DateTime<chrono::Utc> {
            use chrono::TimeZone as _;
            chrono::Utc
                .timestamp_opt(1_000_000, 0)
                .single()
                .expect("ts")
        }
    }

    fn queued_update(op_uuid: u128, task: TaskId) -> PendingOp {
        PendingOp {
            op_id: taskboard_domain::OpId(uuid::Uuid::from_u128(op_uuid)),
            op: LocalOp::UpdateTask(task),
            queued_at: chrono::DateTime::<chrono::Utc>::MIN_UTC,
        }
    }

    /// The drain journey: attach (ladder + identity), one chunk
    /// published create-once at `ops/000001.json`, the op reported
    /// `Applied { echo: None }` AFTER the PUT (the honesty pin: a
    /// failing PUT reports nothing).
    #[tokio::test]
    async fn a_files_lane_drains_dirty_entities_into_a_create_once_chunk() {
        let fake = FakeFilesTransport::new();
        fake.seed_dirs(&["taskboard"]);
        let state = Arc::new(FixedState(state_with(vec![queued_update(
            0xE1,
            TaskId::from_uuid(TASK_UUID),
        )])));
        let context = ctx(Arc::clone(&state), mpsc::channel(8).0);
        let mut lanes = Lanes::default();

        // First tick: attach + pull (nothing on the wire) + drain.
        let ran = run_board_lanes(&fake, &context, &mut lanes).await;
        assert_eq!(ran, 1, "the owned board's lane ran");
        let ops_path = crate::cycle::rooted(&taskboard_sync_files::layout::board_ops_path(
            board(),
            device(),
            1,
        ));
        let chunk_bytes = fake.file(&ops_path).expect("the chunk published");
        let chunk: BoardChunk = serde_json::from_slice(&chunk_bytes).expect("a valid chunk");
        assert_eq!(chunk.device_id, device());
        assert_eq!(chunk.first_seq, 1);
        assert_eq!(chunk.last_seq, 1);
        let entities: Vec<&taskboard_sync_files::board_payload::MaterializedEntity> = chunk
            .entities
            .iter()
            .filter_map(|entry| match entry {
                EntityEntry::Modeled(entity) => Some(entity),
                EntityEntry::Verbatim(_) => None,
            })
            .collect();
        assert!(
            entities
                .iter()
                .any(|entity| entity.kind == EntityKind::Task && entity.id == TASK_UUID),
            "the dirty task materialized into the chunk"
        );
        assert!(
            chunk.sections.is_some(),
            "every chunk carries the publisher's whole section view"
        );

        // The honest accounting: re-run over a fresh report lane; the
        // drain re-publishes at the next seq (the still-queued op's
        // at-least-once posture — the engine settles via the report).
        let (board_tx, mut board_rx) = mpsc::channel(8);
        let context2 = ctx(Arc::clone(&state), board_tx);
        // Tick 2 over FRESH lane state — the crash-window model: the
        // device forgot its seq cursor and re-pushes seq 1. The
        // create-once 412 reconciles to the own-device chunk and the
        // op reports Applied exactly once (the idempotence pin).
        let mut lanes2 = Lanes::default();
        let _ = run_board_lanes(&fake, &context2, &mut lanes2).await;
        let report = board_rx.recv().await.expect("the pull report");
        let SyncReport::Completed { pushes, .. } = report else {
            panic!("completed");
        };
        assert!(pushes.is_empty(), "the pull carries no outcomes");
        let report = board_rx.recv().await.expect("the drain report");
        let SyncReport::Completed { pushes, .. } = report else {
            panic!("completed");
        };
        assert_eq!(pushes.len(), 1, "the reconciled op settles exactly once");
        assert!(matches!(
            pushes[0].result,
            taskboard_domain::PushResult::Applied { echo: None }
        ));
        // Tick 3 with the reconciled cursor: the still-queued op
        // (this harness stands in for the engine's outbox) drains at
        // the next gap-free seq.
        let _ = run_board_lanes(&fake, &context2, &mut lanes2).await;
        let seq2 = crate::cycle::rooted(&taskboard_sync_files::layout::board_ops_path(
            board(),
            device(),
            2,
        ));
        assert!(
            fake.file(&seq2).is_some(),
            "the still-queued op drains again at the next seq"
        );
    }

    /// The honesty pin's negative leg: a failing create-once PUT
    /// reports NOTHING — no `Applied` ever flies for an unpublished
    /// chunk.
    #[tokio::test]
    async fn a_failing_put_reports_nothing() {
        let fake = FakeFilesTransport::new();
        fake.seed_dirs(&["taskboard"]);
        // The staged verdicts land in call order: the attach ladder
        // (4 MKCOL + 1 create-once identity) and the pull's listings
        // (the devices dir, the self device dir) pass through to the
        // model; the drain's chunk PUT answers 503 — nothing
        // publishes, nothing reports Applied.
        for _ in 0..7 {
            fake.stage(crate::test_support::Effect::Pass);
        }
        fake.stage(crate::test_support::Effect::Status(503));
        let state = Arc::new(FixedState(state_with(vec![queued_update(
            0xE2,
            TaskId::from_uuid(TASK_UUID),
        )])));
        let (board_tx, mut board_rx) = mpsc::channel(8);
        let context = ctx(Arc::clone(&state), board_tx);
        let mut lanes = Lanes::default();
        let _ = run_board_lanes(&fake, &context, &mut lanes).await;
        let _ = board_rx.try_recv().expect("the pull report still flies");
        assert!(
            board_rx.try_recv().is_err(),
            "no drain report: the chunk never published"
        );
        let ops_path = crate::cycle::rooted(&taskboard_sync_files::layout::board_ops_path(
            board(),
            device(),
            1,
        ));
        assert!(fake.file(&ops_path).is_none(), "nothing published");
    }

    /// The §9.5 plan-shape impossibility's drain-side pin: a ref-delete
    /// op (a Deck-collapse artifact) attributing to a files board forms
    /// a non-entity plan target — the drain publishes nothing for it
    /// and reports no `Applied` (the op stays queued; a Deck actor
    /// drains it, never this lane).
    #[tokio::test]
    async fn a_ref_delete_op_on_a_files_board_stays_queued() {
        let fake = FakeFilesTransport::new();
        fake.seed_dirs(&["taskboard"]);
        let op = PendingOp {
            op_id: taskboard_domain::OpId(uuid::Uuid::from_u128(0xE3)),
            op: taskboard_domain::LocalOp::DeleteRemoteCard {
                board: board(),
                card: taskboard_domain::ids::RemoteCardRef {
                    board: RemoteBoardId::files(BOARD_UUID),
                    stack: taskboard_domain::ids::RemoteStackId::files(STACK_UUID),
                    card: taskboard_domain::ids::RemoteCardId::files(TASK_UUID),
                },
            },
            queued_at: chrono::DateTime::<chrono::Utc>::MIN_UTC,
        };
        let state = Arc::new(FixedState(state_with(vec![op])));
        let (board_tx, mut board_rx) = mpsc::channel(8);
        let context = ctx(Arc::clone(&state), board_tx);
        let mut lanes = Lanes::default();
        let _ = run_board_lanes(&fake, &context, &mut lanes).await;
        let _ = board_rx.try_recv().expect("the pull report still flies");
        assert!(
            board_rx.try_recv().is_err(),
            "no drain report: the ref-delete never publishes"
        );
        let ops_path = crate::cycle::rooted(&taskboard_sync_files::layout::board_ops_path(
            board(),
            device(),
            1,
        ));
        assert!(fake.file(&ops_path).is_none(), "nothing published");
    }

    /// Fold-first discovery: a pre-existing subtree (identity 412)
    /// publishes nothing on its first tick — the pull folds first, the
    /// peer's entities arrive as the Files report.
    #[tokio::test]
    async fn a_discovered_subtree_folds_first_and_publishes_after() {
        let peer: DeviceId =
            DeviceId::parse("0193a1b2-c3d4-7cc9-8a2e-5f1e2d3c4b6b").expect("parses");
        let fake = FakeFilesTransport::new();
        fake.seed_dirs(&[
            "taskboard",
            "taskboard/boards",
            "taskboard/boards/00000000-0000-0000-0000-0000000b0a1d",
            "taskboard/boards/00000000-0000-0000-0000-0000000b0a1d/devices",
            "taskboard/boards/00000000-0000-0000-0000-0000000b0a1d/devices/0193a1b2-c3d4-7cc9-8a2e-5f1e2d3c4b6b",
            "taskboard/boards/00000000-0000-0000-0000-0000000b0a1d/devices/0193a1b2-c3d4-7cc9-8a2e-5f1e2d3c4b6b/ops",
        ]);
        // The peer's subtree already exists with one chunk.
        let identity_path =
            crate::cycle::rooted(&taskboard_sync_files::layout::board_identity_path(board()));
        fake.seed_file(
            &identity_path,
            br#"{"board_id":"x","format_version":1,"created_at":"2026-11-08T00:00:00Z"}"#.to_vec(),
        );
        let peer_chunk = BoardChunk {
            format_version: BOARD_CHUNK_FORMAT,
            device_id: peer,
            first_seq: 1,
            last_seq: 1,
            written_at: {
                use chrono::TimeZone as _;
                chrono::Utc
                    .timestamp_opt(1_000_000, 0)
                    .single()
                    .expect("ts")
            },
            entities: vec![EntityEntry::Modeled(MaterializedEntity::from_task(&{
                let mut t = task();
                t.title = "peer card".into();
                t
            }))],
            sections: None,
            unknown: BTreeMap::default(),
        };
        let peer_ops = crate::cycle::rooted(&taskboard_sync_files::layout::board_ops_path(
            board(),
            peer,
            1,
        ));
        fake.seed_file(
            &peer_ops,
            serde_json::to_vec(&peer_chunk).expect("chunk serializes"),
        );

        let state = Arc::new(FixedState(state_with(vec![])));
        let (board_tx, mut board_rx) = mpsc::channel(8);
        let context = ctx(Arc::clone(&state), board_tx);
        let mut lanes = Lanes::default();
        let _ = run_board_lanes(&fake, &context, &mut lanes).await;

        let report = board_rx.try_recv().expect("the pull report flies");
        let SyncReport::Completed { snapshot, .. } = report else {
            panic!("completed");
        };
        let taskboard_domain::remote::RemoteBoardSnapshot::Files(files) = *snapshot else {
            panic!("files payload");
        };
        assert_eq!(files.tasks.len(), 1, "the peer's chunk folded");
        assert_eq!(files.tasks[0].title, "peer card");
        assert_eq!(files.up_to_seq, 1);

        // Fold-first: no local chunk exists even though the fixture
        // state carries a queued op.
        let local_ops = crate::cycle::rooted(&taskboard_sync_files::layout::board_ops_path(
            board(),
            device(),
            1,
        ));
        assert!(fake.file(&local_ops).is_none(), "publish-after-import");
    }

    /// The snapshot cadence's boot leg: the first tick publishes the
    /// owner's fold at the seq floor, create-once.
    #[tokio::test]
    async fn the_boot_snapshot_publishes_the_owners_fold() {
        let fake = FakeFilesTransport::new();
        fake.seed_dirs(&["taskboard"]);
        let state = Arc::new(FixedState(state_with(vec![])));
        let context = ctx(Arc::clone(&state), mpsc::channel(8).0);
        let mut lanes = Lanes::default();
        let _ = run_board_lanes(&fake, &context, &mut lanes).await;
        let snap_path = crate::cycle::rooted(&taskboard_sync_files::layout::board_snapshot_path(
            board(),
            device(),
            0,
        ));
        let bytes = fake.file(&snap_path).expect("the boot snapshot published");
        let snapshot: BoardSnapshot = serde_json::from_slice(&bytes).expect("valid snapshot");
        assert_eq!(snapshot.device_id, device());
        assert_eq!(snapshot.seq, 0);
        let modeled = snapshot
            .state
            .entities
            .iter()
            .filter(|entry| matches!(entry, EntityEntry::Modeled(_)))
            .count();
        assert!(modeled >= 2, "the board's own rows fold in: {modeled}");
    }

    fn snapshot_at_seq_zero() -> String {
        crate::cycle::rooted(&board_snapshot_path(board(), device(), 0))
    }

    #[tokio::test]
    async fn the_move_floor_publishes_without_a_binding_and_is_probed() {
        let fake = FakeFilesTransport::new();
        fake.seed_dirs(&["taskboard"]);
        let mut persisted = state_with(vec![]);
        for row in persisted.boards.values_mut() {
            row.remote = None;
        }
        let state = Arc::new(FixedState(persisted.clone()));
        let context = ctx(state, mpsc::channel(8).0);
        let remote = board().as_uuid();
        assert_eq!(
            probe_move_subtree(&fake, device(), remote)
                .await
                .expect("probe"),
            MoveSubtreeProbe::Absent
        );

        publish_move_floor(&fake, &context, board(), remote, &persisted)
            .await
            .expect("publish");

        assert!(fake.file(&snapshot_at_seq_zero()).is_some());
        assert_eq!(
            probe_move_subtree(&fake, device(), remote)
                .await
                .expect("probe"),
            MoveSubtreeProbe::Published
        );
    }

    #[tokio::test]
    async fn a_torn_publish_probes_partial_and_a_stranger_foreign() {
        let fake = FakeFilesTransport::new();
        fake.seed_dirs(&["taskboard"]);
        let persisted = state_with(vec![]);
        let context = ctx(Arc::new(FixedState(persisted.clone())), mpsc::channel(8).0);
        let remote = board().as_uuid();
        publish_move_floor(&fake, &context, board(), remote, &persisted)
            .await
            .expect("publish");
        fake.remove_file(&snapshot_at_seq_zero());
        assert_eq!(
            probe_move_subtree(&fake, device(), remote)
                .await
                .expect("probe"),
            MoveSubtreeProbe::Partial
        );

        let stranger = DeviceId::mint();
        assert_eq!(
            probe_move_subtree(&fake, stranger, remote)
                .await
                .expect("probe"),
            MoveSubtreeProbe::Foreign
        );
    }
}
