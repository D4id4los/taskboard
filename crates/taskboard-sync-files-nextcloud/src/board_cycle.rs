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
//! GC duty note: the owner's post-snapshot GC pass runs inside
//! [`snapshot_duty`]'s publish-success arm — the just-published
//! snapshot's `seq` **is** the GC watermark. Publish-first is the only
//! safe order: a snapshot subsumes chunks `≤` its seq, so the floor
//! must exist on the server before any chunk goes. The pass re-derives
//! the own-chunk seq set from one `propfind_tree` over this device's
//! own `ops/` directory every cadence (no persistent seq set — a
//! restart loses in-memory state, and the module doc forbids
//! correctness load on in-memory cursors), feeds it plus the watermark
//! into [`taskboard_sync_files::board_gc::gc_eligible`], and deletes
//! each eligible file. Owner scope is structural: the listing root is
//! hard-pinned to the device dir built from the context identity — no
//! foreign device's path is ever constructed, let alone deleted (the
//! D-BF7 owner leg; test-pinned). Tombstones are forever: GC removes
//! whole ops files at/below the watermark only — tombstone *fields*
//! live on in the snapshots. The unbind retirement (the operator's
//! GC-on-unbind decision, as amended 2026-11-19: device-scoped — only
//! THIS device's files under the remote board go; the board's shared
//! subtree stays because another client may still be connected) is the
//! delete verb's only other caller.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

use taskboard_domain::OpId;
use taskboard_domain::SyncErrorKind;
use taskboard_domain::board_move::AdoptionRefusal;
use taskboard_domain::ids::{AuthorId, BoardId, StackId, TaskId};
use taskboard_domain::messages::SyncReport;
use taskboard_domain::ownership::BoardOwnershipMap;
use taskboard_domain::persistence::PersistedState;

use taskboard_domain::push::{EntityTables, PushTarget, plan_pushes};
use taskboard_domain::remote::{FilesBoardSnapshot, PushOutcome, PushResult};
use taskboard_sync_files::board_chunk::BoardChunk;
use taskboard_sync_files::board_fold::{fold_board, fold_sections};
use taskboard_sync_files::board_gc::{
    BOARD_CHUNK_FORMAT, gc_eligible, next_chunk_range, snapshot_due,
};
use taskboard_sync_files::board_identity::BoardIdentity;
use taskboard_sync_files::board_payload::{EntityEntry, EntityKind, MaterializedEntity};
use taskboard_sync_files::board_snapshot::{BoardSnapshot, SnapshotState};
use taskboard_sync_files::ids::DeviceId;
use taskboard_sync_files::layout::{
    self, BOARDS_DIR_NAME, DEVICES_DIR_NAME, OPS_DIR_NAME, TREE_FORMAT_VERSION,
    board_identity_path, board_ops_path, board_snapshot_path, classify, device_dir_name,
};

use taskboard_sync_files::board_move::{MoveRecord, MoveRecordError, MovedFrom};

use crate::cycle::SyncContext;
use crate::error::TransportError;
use crate::etag::Etag;
use crate::status::{BoardMoveFinding, BoardMoveRefusal};
use crate::transport::{DeleteOutcome, FilesTransport, PutOutcome};

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
    /// The board's files identity last seen while bound. Unbind is
    /// instantaneous at the state level and erases the binding, but
    /// the retirement duty still needs the remote uuid to address
    /// this device's own files — the last-seen value rides the lane.
    pub remote_uuid: Option<uuid::Uuid>,
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
            remote_uuid: None,
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
    /// Boards moved here from Deck, surfaced on the status watch.
    pub moved: BTreeMap<uuid::Uuid, BoardMoveFinding>,
    /// The lanes' last-tick outcomes, keyed by the board's files
    /// identity — rebuilt every tick over the boards the lanes serve
    /// (a success clears a previous tick's failure; the watch is the
    /// whole truth of the last tick, not an accumulating alarm).
    /// Surfaced so a failing lane never reads neutral Ok (#941's
    /// honesty leg).
    pub outcomes: BTreeMap<uuid::Uuid, crate::status::LaneOutcome>,
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
    let mut outcomes = BTreeMap::new();
    let owned_ids: BTreeSet<BoardId> = owned.iter().map(|(board, _)| *board).collect();
    // The orphan law (plan `.artifacts/plans/2026-11-24-sync-orphan-lanes-plan.md` §1):
    // board-less outbox ops (owning entity rows wiped) are cycle-level
    // bookkeeping — the planner synthesizes their completions, so they
    // drain every cycle: riding the FIRST board lane's pass below, or
    // (zero owned boards) through the standalone drain.
    let tables = EntityTables {
        tasks: &persisted.tasks,
        stacks: &persisted.stacks,
        labels: &persisted.labels,
        boards: &persisted.boards,
    };
    let orphan_ops: Vec<taskboard_domain::PendingOp> = persisted
        .outbox
        .iter()
        .filter(|entry| entry.op.owning_board(&tables).is_none())
        .cloned()
        .collect();
    if owned.is_empty() {
        // #1255: the zero-board leg — no lane exists, so the orphans
        // drain standalone instead of stranding behind the empty set.
        drain_orphans_only(ctx, &persisted, &orphan_ops).await;
    }
    for (index, (board, remote)) in owned.into_iter().enumerate() {
        let lane = lanes.boards.entry(board).or_default();
        lane.remote_uuid = Some(remote);
        // `if index == 0`, exactly like the Deck connector's ride: the
        // orphans carry once per cycle, on the first board pass.
        let carried_orphans: &[taskboard_domain::PendingOp] =
            if index == 0 { &orphan_ops } else { &[] };
        match run_lane(
            transport,
            ctx,
            lane,
            board,
            remote,
            &persisted,
            carried_orphans,
        )
        .await
        {
            Ok(()) => {
                completed += 1;
                outcomes.insert(remote, crate::status::LaneOutcome::Ok);
            }
            Err(err) => {
                tracing::warn!(?err, board = ?board, "board lane failed; retried next tick");
                outcomes.insert(remote, crate::status::LaneOutcome::Failed((&err).into()));
            }
        }
    }
    retire_unbound_lanes(transport, ctx, lanes, &owned_ids, &mut outcomes).await;
    lanes.outcomes = outcomes;
    completed += discover_subtrees(transport, ctx, lanes, &persisted).await;
    completed
}

/// The unbind retirement's typed outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RetireOutcome {
    /// This device's files were deleted this tick.
    Retired,
    /// They were already gone (the delete raced a previous tick's
    /// success, or a crash landed between the delete and the prune).
    AlreadyRetired,
}

/// The unbind trigger (leg 2, the operator's GC-on-unbind decision as
/// amended 2026-11-19 — **device-scoped**): a lane whose board stopped
/// being files-bound is retiring. The actor owns the transport and
/// remembers the remote uuid on the lane, so the retirement runs per
/// tick until complete: delete THIS device's own directory under the
/// remote board (`boards/<remote>/devices/<own>/`) — never the board's
/// shared subtree, because a different client may still be connected
/// to that board. On completion the lane and its discovery entry are
/// pruned (the stale-lane leak, §3 of the chunk plan); on a transport
/// error the retirement is deferred and surfaces typed on the outcomes
/// watch — never silent. The engine's `UnbindBoard` is untouched:
/// unbind is instantaneous at the state level, and this cleanup
/// finding never blocks or refuses it.
async fn retire_unbound_lanes<T: FilesTransport>(
    transport: &T,
    ctx: &SyncContext,
    lanes: &mut Lanes,
    owned: &BTreeSet<BoardId>,
    outcomes: &mut BTreeMap<uuid::Uuid, crate::status::LaneOutcome>,
) {
    let retiring: Vec<(BoardId, uuid::Uuid)> = lanes
        .boards
        .iter()
        .filter(|(board, _)| !owned.contains(board))
        .filter_map(|(board, lane)| lane.remote_uuid.map(|remote| (*board, remote)))
        .collect();
    for (board, remote) in retiring {
        match retire_own_device_files(transport, ctx, remote).await {
            Ok(RetireOutcome::Retired | RetireOutcome::AlreadyRetired) => {
                tracing::info!(?remote, "unbind retirement complete; lane pruned");
                lanes.boards.remove(&board);
                lanes.discovered.remove(&remote);
                outcomes.insert(remote, crate::status::LaneOutcome::Ok);
            }
            Err(err) => {
                tracing::warn!(
                    ?err,
                    ?remote,
                    "unbind retirement deferred; retried next tick"
                );
                outcomes.insert(remote, crate::status::LaneOutcome::Failed((&err).into()));
            }
        }
    }
}

/// Deletes this device's own directory under the remote board — one
/// collection DELETE, recursive per RFC 4918 Depth semantics. The
/// target is derived entirely from `ctx.identity.device_id`
/// (device-scoped by construction): the board's shared bytes
/// (`identity.json`, `move.json`) and any other device's directory are
/// never constructed here, let alone deleted. Idempotent: the
/// already-absent mapping covers the crash-between-delete-and-prune
/// window.
async fn retire_own_device_files<T: FilesTransport>(
    transport: &T,
    ctx: &SyncContext,
    remote: uuid::Uuid,
) -> Result<RetireOutcome, TransportError> {
    let device_dir = crate::cycle::rooted(&format!(
        "{BOARDS_DIR_NAME}/{remote}/{DEVICES_DIR_NAME}/{}",
        device_dir_name(ctx.identity.device_id)
    ));
    match transport.delete(&device_dir).await? {
        DeleteOutcome::Deleted => Ok(RetireOutcome::Retired),
        DeleteOutcome::AlreadyAbsent => Ok(RetireOutcome::AlreadyRetired),
    }
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
        if bound.contains(&uuid) {
            settle_adopted(lanes, uuid);
            continue;
        }
        if lanes.discovered.contains(&uuid) {
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
        if !may_discover(transport, ctx, lanes, persisted, uuid).await {
            continue;
        }
        lanes.discovered.insert(uuid);
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
    moved_from: &MovedFrom,
) -> Result<(), TransportError> {
    let mut lane = Lane::default();
    attach_board(transport, ctx, &mut lane, remote, Some(moved_from)).await?;
    let floor = crate::cycle::rooted(&board_snapshot_path(
        BoardId::from_uuid(remote),
        ctx.identity.device_id,
        0,
    ));
    match transport.get(&floor).await {
        Ok(existing) => lane.snapshot_etag = existing.etag,
        Err(TransportError::NotFound) => {}
        Err(err) => return Err(err),
    }
    snapshot_duty(transport, ctx, &mut lane, board, remote, persisted).await
}

/// The move target's `moved_from`, once published.
///
/// # Errors
///
/// A transport or decode failure other than an absent identity.
pub async fn read_moved_from<T: FilesTransport>(
    transport: &T,
    remote: uuid::Uuid,
) -> Result<Option<MovedFrom>, TransportError> {
    match read_identity_provenance(transport, remote).await {
        Err(TransportError::NotFound) => Ok(None),
        other => other,
    }
}

/// Whether the commit record exists.
///
/// # Errors
///
/// A transport failure other than its absence.
pub async fn move_record_exists<T: FilesTransport>(
    transport: &T,
    remote: uuid::Uuid,
) -> Result<bool, TransportError> {
    let path = crate::cycle::rooted(&layout::board_move_path(BoardId::from_uuid(remote)));
    match transport.get(&path).await {
        Ok(_) => Ok(true),
        Err(TransportError::NotFound) => Ok(false),
        Err(err) => Err(err),
    }
}

/// Writes the commit record once; an existing record means an earlier
/// run already committed.
///
/// # Errors
///
/// The PUT's transport failure.
pub async fn publish_move_record<T: FilesTransport>(
    transport: &T,
    remote: uuid::Uuid,
    record: &MoveRecord,
) -> Result<(), TransportError> {
    let path = crate::cycle::rooted(&layout::board_move_path(BoardId::from_uuid(remote)));
    let body = serde_json::to_vec(record)?;
    match transport.put_create(&path, &body).await {
        Ok(_) | Err(TransportError::PreconditionFailed) => Ok(()),
        Err(err) => Err(err),
    }
}

fn settle_adopted(lanes: &mut Lanes, uuid: uuid::Uuid) {
    if let Some(finding) = lanes.moved.get_mut(&uuid)
        && let BoardMoveFinding::Adopting {
            moved_by,
            committed_at,
        } = *finding
    {
        *finding = BoardMoveFinding::Adopted {
            moved_by,
            committed_at,
        };
    }
}

/// Whether an unbound subtree may be folded as a new board. Anything
/// short of positive evidence holds: a duplicate board is not undoable,
/// a held subtree is retried next tick.
async fn may_discover<T: FilesTransport>(
    transport: &T,
    ctx: &SyncContext,
    lanes: &mut Lanes,
    persisted: &PersistedState,
    uuid: uuid::Uuid,
) -> bool {
    let moved_from = match read_identity_provenance(transport, uuid).await {
        Ok(Some(moved_from)) => moved_from,
        Ok(None) => return true,
        Err(_) => return false,
    };
    let path = crate::cycle::rooted(&layout::board_move_path(BoardId::from_uuid(uuid)));
    let bytes = match transport.get(&path).await {
        Ok(content) => content.bytes,
        // In flight or undone: nobody adopts and nobody mints.
        Err(_) => return false,
    };
    let record = match MoveRecord::decode(&bytes) {
        Ok(record) => record,
        Err(err) => {
            let refusal = match err {
                MoveRecordError::Malformed => BoardMoveRefusal::RecordMalformed,
                MoveRecordError::NewerFormat { format } => {
                    BoardMoveRefusal::RecordNewerFormat { format }
                }
            };
            tracing::warn!(?uuid, ?refusal, "move record unusable; board held");
            lanes.moved.insert(uuid, BoardMoveFinding::Refused(refusal));
            return false;
        }
    };
    let Some(local) = deck_holder(ctx, persisted, &moved_from) else {
        return true; // a device with no copy: the floor is its copy
    };
    adopt(ctx, lanes, persisted, uuid, local, &record).await;
    false
}

/// The identity's `moved_from`; `Ok(None)` for an ordinary subtree. A
/// missing identity is an error here: the subtree is still being built.
async fn read_identity_provenance<T: FilesTransport>(
    transport: &T,
    uuid: uuid::Uuid,
) -> Result<Option<MovedFrom>, TransportError> {
    let path = crate::cycle::rooted(&board_identity_path(BoardId::from_uuid(uuid)));
    let content = transport.get(&path).await?;
    let identity: BoardIdentity = serde_json::from_slice(&content.bytes)?;
    Ok(identity.moved_from)
}

fn deck_holder(
    ctx: &SyncContext,
    persisted: &PersistedState,
    moved_from: &MovedFrom,
) -> Option<BoardId> {
    let deck = taskboard_domain::RemoteBoardId::deck(moved_from.deck_board);
    persisted.boards.values().find_map(|board| {
        let binding = board.remote.as_ref().filter(|_| !board.deleted)?;
        let origin = ctx.deck_origins.get(&binding.connector)?;
        (binding.board == deck && *origin == moved_from.deck_server).then_some(board.id)
    })
}

/// Loud first: the finding is recorded before anything can change.
async fn adopt(
    ctx: &SyncContext,
    lanes: &mut Lanes,
    persisted: &PersistedState,
    uuid: uuid::Uuid,
    local: BoardId,
    record: &MoveRecord,
) {
    let adoption = record.adoption(uuid);
    let tables = EntityTables {
        tasks: &persisted.tasks,
        stacks: &persisted.stacks,
        labels: &persisted.labels,
        boards: &persisted.boards,
    };
    let plan = taskboard_domain::board_move::plan_board_adoption(
        &tables,
        &persisted.outbox,
        local,
        &adoption,
        &ctx.connector,
    );
    if let Err(refusal) = plan {
        let refusal = match refusal {
            AdoptionRefusal::Collision => BoardMoveRefusal::Collision,
            AdoptionRefusal::NotBoundToMovedDeck => BoardMoveRefusal::NotBoundToMovedDeck,
        };
        tracing::warn!(?uuid, ?refusal, "moved board not adopted");
        lanes.moved.insert(uuid, BoardMoveFinding::Refused(refusal));
        return;
    }
    tracing::warn!(?uuid, moved_by = %record.moved_by, "board moved to files; adopting");
    lanes.moved.insert(
        uuid,
        BoardMoveFinding::Adopting {
            moved_by: record.moved_by,
            committed_at: record.committed_at,
        },
    );
    let report = SyncReport::BoardMoved {
        connector: ctx.connector.clone(),
        board: local,
        adoption: Box::new(adoption),
    };
    if ctx.board_reports.send(report).await.is_err() {
        tracing::warn!(?uuid, "board report lane closed; adoption retried");
    }
}

async fn run_lane<T: FilesTransport>(
    transport: &T,
    ctx: &SyncContext,
    lane: &mut Lane,
    board: BoardId,
    remote: uuid::Uuid,
    persisted: &PersistedState,
    orphan_ops: &[taskboard_domain::PendingOp],
) -> Result<(), TransportError> {
    let started = Instant::now();
    if !lane.attached {
        attach_board(transport, ctx, lane, remote, None).await?;
    }
    pull_board(transport, ctx, lane, board, remote, started).await?;
    if !lane.folded_once {
        // The first pull failed halfway on a discovered subtree:
        // publish nothing (publish-after-import ordering, pinned).
        return Ok(());
    }
    drain_board(transport, ctx, lane, board, remote, persisted, orphan_ops).await?;
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
    moved_from: Option<&MovedFrom>,
) -> Result<(), TransportError> {
    let boards_dir = format!("{}/", crate::cycle::rooted(BOARDS_DIR_NAME));
    crate::cycle::mkcol_exists_ok(transport, boards_dir.trim_end_matches('/')).await?;
    let subtree_dir = format!("{boards_dir}{remote}");
    crate::cycle::mkcol_exists_ok(transport, &subtree_dir).await?;
    let devices_dir = format!("{subtree_dir}/{DEVICES_DIR_NAME}");
    crate::cycle::mkcol_exists_ok(transport, &devices_dir).await?;
    let self_dir = format!("{devices_dir}/{}", device_dir_name(ctx.identity.device_id));
    crate::cycle::mkcol_exists_ok(transport, &self_dir).await?;
    // The drain's create-once chunk PUT needs its parent collection
    // (#941): a real Sabre/DAV server 404s a PUT into a missing
    // `ops/`, which stranded every queued op forever — the ladder
    // owns the directory, the drain owns only the file.
    let ops_dir = format!("{self_dir}/{}", layout::OPS_DIR_NAME);
    crate::cycle::mkcol_exists_ok(transport, &ops_dir).await?;

    let identity_path = crate::cycle::rooted(&board_identity_path(BoardId::from_uuid(remote)));
    let identity = BoardIdentity {
        board_id: remote,
        format_version: TREE_FORMAT_VERSION,
        created_at: ctx.clock.now(),
        moved_from: moved_from.cloned(),
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

/// The ref-delete family's tombstone (the N-19 repair): a files-bound
/// board has no later Deck actor to serve its #150 collapse artifacts,
/// so the drain relays the deletion itself. The local row resolves
/// first (the ref's remote binding), else a tombstone mints from the
/// ref's own files UUID; `deleted` flips with a fresh clock so every
/// peer's field duel folds it. `None`: a Deck-tagged ref with no
/// local row — genuinely not this tier's data.
fn task_tombstone(
    card_ref: &taskboard_domain::ids::RemoteCardRef,
    tasks: &BTreeMap<TaskId, taskboard_domain::entities::Task>,
    now: chrono::DateTime<chrono::Utc>,
) -> Option<taskboard_domain::entities::Task> {
    if let Some(local) = tasks.values().find(|task| {
        task.remote
            .as_ref()
            .is_some_and(|r| r.card == card_ref.card)
    }) {
        let mut tombstone = local.clone();
        tombstone.deleted = true;
        tombstone.clocks.deleted = now;
        return Some(tombstone);
    }
    let uuid = card_ref.card.as_files()?;
    Some(taskboard_domain::entities::Task {
        id: TaskId::from_uuid(uuid),
        stack: card_ref.stack.as_files().map_or(
            taskboard_domain::ids::StackId::from_uuid(uuid),
            taskboard_domain::ids::StackId::from_uuid,
        ),
        remote: None,
        title: String::new(),
        description: String::new(),
        duedate: None,
        scheduled: None,
        done: None,
        order: 0,
        label_memberships: BTreeMap::new(),
        effort: None,
        importance: None,
        urgency: None,
        recur: None,
        kind: taskboard_domain::entities::TaskKind::Standard,
        archived: false,
        deleted: true,
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
            deleted: now,
        },
        authors: taskboard_domain::entities::TaskAuthors::default(),
        remote_seen: None,
        remote_baseline: None,
    })
}

/// The stack twin of [`task_tombstone`].
fn stack_tombstone(
    stack_ref: &taskboard_domain::ids::RemoteStackRef,
    stacks: &BTreeMap<StackId, taskboard_domain::entities::Stack>,
    now: chrono::DateTime<chrono::Utc>,
) -> Option<taskboard_domain::entities::Stack> {
    if let Some(local) = stacks.values().find(|stack| {
        stack
            .remote
            .as_ref()
            .is_some_and(|r| r.stack == stack_ref.stack)
    }) {
        let mut tombstone = local.clone();
        tombstone.deleted = true;
        tombstone.clocks.deleted = now;
        return Some(tombstone);
    }
    let uuid = stack_ref.stack.as_files()?;
    let board_uuid = stack_ref.board.as_files()?;
    Some(taskboard_domain::entities::Stack {
        id: StackId::from_uuid(uuid),
        board: BoardId::from_uuid(board_uuid),
        remote: None,
        title: String::new(),
        order: 0,
        archived: false,
        deleted: true,
        clocks: taskboard_domain::StackClocks {
            title: chrono::DateTime::<chrono::Utc>::MIN_UTC,
            order: chrono::DateTime::<chrono::Utc>::MIN_UTC,
            deleted: now,
        },
        authors: taskboard_domain::entities::StackAuthors::default(),
        remote_seen: None,
    })
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
    orphan_ops: &[taskboard_domain::PendingOp],
) -> Result<(), TransportError> {
    let ownership = BoardOwnershipMap::from_boards(&persisted.boards);
    let tables = EntityTables {
        tasks: &persisted.tasks,
        stacks: &persisted.stacks,
        labels: &persisted.labels,
        boards: &persisted.boards,
    };
    // The orphan ride (the orphan law, plan §1): the carried ops append
    // BEFORE the empty guard — a first board with no own ops must still
    // drain them (the #1255 early-return trap).
    let mut owned_ops: Vec<taskboard_domain::PendingOp> = persisted
        .outbox
        .iter()
        .filter(|entry| entry.op.owning_board(&tables) == Some(board))
        .cloned()
        .collect();
    owned_ops.extend(orphan_ops.iter().cloned());
    if owned_ops.is_empty() {
        return Ok(());
    }
    let groups = plan_pushes(&owned_ops, tables, &ownership);
    // The no-sidecar shape: no Document group exists for this board
    // (the planner pin); the dirty set is the group targets.
    let mut dirty_tasks = BTreeSet::new();
    let mut dirty_stacks = BTreeSet::new();
    let mut dirty_labels = BTreeSet::new();
    let mut dirty_board = false;
    let mut ref_tombstone_tasks: Vec<taskboard_domain::entities::Task> = Vec::new();
    let mut ref_tombstone_stacks: Vec<taskboard_domain::entities::Stack> = Vec::new();
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
            PushTarget::RemoteCard(card_ref) => {
                // The ref-delete family drains here too (the N-19
                // repair): no Deck actor ever serves a files-bound
                // board again, so the collapse artifact's deletion
                // relays as this tier's tombstone.
                match task_tombstone(&card_ref, &persisted.tasks, ctx.clock.now()) {
                    Some(tombstone) => {
                        ref_tombstone_tasks.push(tombstone);
                        chunk_ops.push(group.primary);
                        chunk_ops.extend(group.subsumed.iter().copied());
                        continue;
                    }
                    None => {
                        tracing::warn!(
                            board = ?board,
                            "a deck-tagged ref-delete with no local row stays queued"
                        );
                    }
                }
            }
            PushTarget::RemoteStack(stack_ref) => {
                match stack_tombstone(&stack_ref, &persisted.stacks, ctx.clock.now()) {
                    Some(tombstone) => {
                        ref_tombstone_stacks.push(tombstone);
                        chunk_ops.push(group.primary);
                        chunk_ops.extend(group.subsumed.iter().copied());
                        continue;
                    }
                    None => {
                        tracing::warn!(
                            board = ?board,
                            "a deck-tagged ref-delete with no local row stays queued"
                        );
                    }
                }
            }
            PushTarget::Document(_) => {
                // The planner cannot produce a Document group for a
                // files-owned board (the plan-shape pin); reaching
                // here means the ownership map and the binding
                // disagree — the op stays queued, loudly.
                tracing::warn!(
                    board = ?board,
                    "plan produced a sidecar target on a files board; op stays queued"
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
    // The ref-delete family's tombstones (the N-19 repair): already
    // materialized against the ref during the group walk.
    for tombstone in &ref_tombstone_stacks {
        entries.push(EntityEntry::Modeled(MaterializedEntity::from_stack(
            tombstone,
        )));
    }
    for tombstone in &ref_tombstone_tasks {
        entries.push(EntityEntry::Modeled(MaterializedEntity::from_task(
            tombstone,
        )));
    }
    // Every op must have found an entity to materialize, or it is not
    // publishable — honesty over accounting pressure (an unpublished
    // op stays queued and retries; a pre-emptive Applied is data loss).
    // The exception is the orphan ride's synthetic completions (the
    // orphan law): a group whose target entity row is wiped
    // materializes nothing, so nothing goes on the wire — its ops
    // account alone, without a chunk (a synthetic completion never
    // addresses the server; only a materialized write needs the
    // create-once PUT). An unresolved ref-delete is not synthetic —
    // it stays queued.
    if entries.is_empty() {
        let synthetic: Vec<OpId> = groups
            .iter()
            .filter(|group| match group.target {
                PushTarget::Task(id) => !tables.tasks.contains_key(&id),
                PushTarget::Stack(id) => !tables.stacks.contains_key(&id),
                PushTarget::Label(id) => !tables.labels.contains_key(&id),
                PushTarget::Board(id) => !tables.boards.contains_key(&id),
                PushTarget::RemoteCard(_)
                | PushTarget::RemoteStack(_)
                | PushTarget::Document(_) => false,
            })
            .flat_map(|group| std::iter::once(group.primary).chain(group.subsumed.iter().copied()))
            .collect();
        if synthetic.is_empty() {
            tracing::debug!(board = ?board, "drain found nothing publishable; ops stay queued");
            return Ok(());
        }
        publish_drain_report(ctx, board, remote, &synthetic).await;
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

/// The files tier's zero-board leg of the orphan law (plan
/// `.artifacts/plans/2026-11-24-sync-orphan-lanes-plan.md` §2 D2 — the
/// one recorded mechanism divergence): `drain_board` needs a bound
/// (board, remote) pair — a chunk document publishes into a board
/// subtree, and an orphan-only drain has neither. So the planner's
/// synthetic completions (the entity row a group targets is absent
/// from the tables — the same materialization check `drain_board`
/// performs per entity) account through the board-less push-only
/// report path (`Failed { board: None }` — the same accounting shape
/// the Deck connector's standalone drain uses; no board exists to
/// attribute a `Completed` to, and a fabricated identity would mint a
/// phantom board row). A group that would need a lane — a
/// materialized entity write or a ref-tombstone relay, both of which
/// publish only as chunk entries — has no destination here and stays
/// queued loudly until a board binds (mechanism, not semantics: the
/// Deck connector drains the wire-shaped orphans; this tier drains
/// everything publishable without a board, every cycle).
async fn drain_orphans_only(
    ctx: &SyncContext,
    persisted: &PersistedState,
    orphan_ops: &[taskboard_domain::PendingOp],
) {
    if orphan_ops.is_empty() {
        // Zero-cost guard first: the empty lane publishes nothing.
        return;
    }
    let tables = EntityTables {
        tasks: &persisted.tasks,
        stacks: &persisted.stacks,
        labels: &persisted.labels,
        boards: &persisted.boards,
    };
    let ownership = BoardOwnershipMap::from_boards(&persisted.boards);
    let mut completions: Vec<OpId> = Vec::new();
    for group in plan_pushes(orphan_ops, tables, &ownership) {
        let row_present = match group.target {
            PushTarget::Task(id) => tables.tasks.contains_key(&id),
            PushTarget::Stack(id) => tables.stacks.contains_key(&id),
            PushTarget::Label(id) => tables.labels.contains_key(&id),
            PushTarget::Board(id) => tables.boards.contains_key(&id),
            // No board subtree exists to relay a ref-tombstone into,
            // and the flush needs a board's sidecar lane: these targets
            // are never publishable without a board.
            PushTarget::RemoteCard(_) | PushTarget::RemoteStack(_) | PushTarget::Document(_) => {
                false
            }
        };
        if row_present {
            tracing::warn!(
                connector = %ctx.connector,
                "an orphan group needs a board lane; op stays queued until a board binds"
            );
            continue;
        }
        completions.push(group.primary);
        completions.extend(group.subsumed.iter().copied());
    }
    if completions.is_empty() {
        return;
    }
    let pushes: Vec<PushOutcome> = completions
        .iter()
        .map(|op_id| PushOutcome {
            op: *op_id,
            result: PushResult::Applied { echo: None },
        })
        .collect();
    tracing::debug!(
        connector = %ctx.connector,
        ops = pushes.len(),
        "zero-board orphan drain"
    );
    let report = SyncReport::Failed {
        connector: ctx.connector.clone(),
        kind: SyncErrorKind::NoBoard,
        pushes,
        read_at: ctx.clock.now(),
        board: None,
        duration: std::time::Duration::ZERO,
        features: taskboard_domain::DeckFeatures::default(),
    };
    if ctx.board_reports.send(report).await.is_err() {
        // The engine is gone; nothing to account to. The completions
        // were local, so nothing is lost — the ops re-report on a
        // later cycle only if still queued (the honest at-least-once
        // posture).
        tracing::warn!("board report lane closed; orphan completions re-report next cycle");
    }
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
            // The owner's GC pass (D-BF7), strictly after publish
            // success: the snapshot just published IS the watermark —
            // publish-first is the only safe order (a snapshot
            // subsumes chunks ≤ its seq, so the floor must exist on
            // the server before any chunk goes). The 412 arm skips the
            // pass entirely: no published snapshot ⇒ no watermark
            // advance ⇒ nothing is GC-eligible.
            if let Err(err) = gc_own_chunks(transport, ctx, remote, seq).await {
                tracing::warn!(
                    ?err, board = ?board, seq,
                    "post-snapshot GC skipped; eligibility is recomputed next cadence"
                );
            }
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

/// The owner's post-snapshot GC pass: one PROPFIND over this device's
/// own `ops/` directory (the listing root is hard-pinned to the device
/// dir built from the context identity — owner scope is structural),
/// the seq parsed out of each `NNNNNN.json` member name (non-parsing
/// names are skipped — a foreign name is not ours to read), then
/// `gc_eligible` against the just-published watermark and one DELETE
/// per eligible file. A per-file delete failure logs at warn and
/// continues; the pass never fails the lane (eligibility is recomputed
/// from the listing next cadence and the leftover file is retried
/// then). A transport-class error on the PROPFIND aborts the pass
/// silently-for-this-cadence (same retry story).
async fn gc_own_chunks<T: FilesTransport>(
    transport: &T,
    ctx: &SyncContext,
    remote: uuid::Uuid,
    watermark: u64,
) -> Result<(), TransportError> {
    let device = ctx.identity.device_id;
    let ops_dir = crate::cycle::rooted(&format!(
        "{BOARDS_DIR_NAME}/{remote}/{DEVICES_DIR_NAME}/{}/{OPS_DIR_NAME}",
        device_dir_name(device)
    ));
    let listing = match transport.propfind_tree(&ops_dir).await {
        Ok(listing) => listing,
        // No ops dir (yet/anymore): nothing to GC.
        Err(TransportError::NotFound) => return Ok(()),
        Err(err) => return Err(err),
    };
    let own_seqs: BTreeSet<u64> = listing
        .members
        .iter()
        .filter_map(|member| {
            let name = member.href.trim_end_matches('/').rsplit('/').next()?;
            name.strip_suffix(".json")?.parse().ok()
        })
        .collect();
    for seq in gc_eligible(&own_seqs, Some(watermark)) {
        let path = crate::cycle::rooted(&board_ops_path(BoardId::from_uuid(remote), device, seq));
        match transport.delete(&path).await {
            // Both outcomes are the idempotent re-run's success.
            Ok(DeleteOutcome::Deleted | DeleteOutcome::AlreadyAbsent) => {}
            Err(err) => {
                tracing::warn!(?seq, ?err, "chunk delete failed; retried next cadence");
            }
        }
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
            clock: Arc::new(taskboard_domain::test_support::FrozenClock::from_secs(
                1_000_000,
            )),
            local: Arc::new(crate::cycle::EmptyDocument),
            report_tx: mpsc::channel(1).0,
            connector: taskboard_domain::ids::ConnectorId::from("files-sync"),
            state_reader: state,
            board_reports,
            deck_origins: BTreeMap::from([(
                taskboard_domain::ids::ConnectorId::from("nextcloud"),
                DECK_ORIGIN.to_owned(),
            )]),
            cancel: crate::cycle::CycleCancel::new(),
        }
    }

    fn queued_update(op_uuid: u128, task: TaskId) -> PendingOp {
        PendingOp {
            op_id: taskboard_domain::OpId(uuid::Uuid::from_u128(op_uuid)),
            op: LocalOp::UpdateTask(task),
            queued_at: chrono::DateTime::<chrono::Utc>::MIN_UTC,
        }
    }

    /// The last PUT's body recorded for `path`. After the owner's
    /// post-snapshot GC pass an own chunk at/below the snapshot's seq
    /// no longer lingers as a model file — the request log is the
    /// durable observable of what was published (never a transient
    /// intermediate).
    fn last_put_body(fake: &FakeFilesTransport, path: &str) -> Vec<u8> {
        fake.requests()
            .iter()
            .rev()
            .find(|req| req.method == "PUT" && req.path == path)
            .unwrap_or_else(|| panic!("no PUT recorded for {path}"))
            .body
            .clone()
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
        let chunk_bytes = last_put_body(&fake, &ops_path);
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
        // device forgot its seq cursor and re-pushes seq 1. The chunk
        // file is seeded back (the crash happened after the chunk
        // publish but before the snapshot cadence GC'd it), so the
        // create-once 412 reconciles to the own-device chunk and the
        // op reports Applied exactly once (the idempotence pin).
        fake.seed_file(&ops_path, chunk_bytes.clone());
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
            fake.count("PUT", Some(&seq2)) >= 1,
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
        // (5 MKCOL + 1 create-once identity) and the pull's listings
        // (the devices dir, the self device dir, its ops dir) pass
        // through to the model; the drain's chunk PUT answers 503 —
        // nothing publishes, nothing reports Applied.
        for _ in 0..9 {
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

    /// The incident scenario (#941): a fresh device adopting a former
    /// install's subtree (the identity PUT answers 412) — the old tree
    /// has no `ops/` under this device, yet the first tick's drain
    /// must land its chunk: the attach ladder owns the drain's parent
    /// collection, so the lane cannot 404 forever.
    #[tokio::test]
    async fn a_fresh_device_on_an_adopted_subtree_drains_successfully() {
        let fake = FakeFilesTransport::new();
        fake.seed_dirs(&[
            "taskboard",
            "taskboard/boards",
            &format!("taskboard/boards/{BOARD_UUID}"),
            &format!("taskboard/boards/{BOARD_UUID}/devices"),
        ]);
        let identity_path =
            crate::cycle::rooted(&taskboard_sync_files::layout::board_identity_path(board()));
        fake.seed_file(
            &identity_path,
            br#"{"board_id":"00000000-0000-0000-0000-0000000b0a1d","format_version":1,"created_at":"2026-11-08T00:00:00Z"}"#.to_vec(),
        );
        let state = Arc::new(FixedState(state_with(vec![queued_update(
            0xE1,
            TaskId::from_uuid(TASK_UUID),
        )])));
        let context = ctx(Arc::clone(&state), mpsc::channel(8).0);
        let mut lanes = Lanes::default();

        let ran = run_board_lanes(&fake, &context, &mut lanes).await;
        assert_eq!(ran, 1, "the adopted board's lane ran");
        let ops_path = crate::cycle::rooted(&taskboard_sync_files::layout::board_ops_path(
            board(),
            device(),
            1,
        ));
        assert!(
            fake.count("PUT", Some(&ops_path)) >= 1,
            "the drain published despite the adopted (ops-less) tree"
        );
    }

    /// The ladder-completeness pin (#941): after a full tick (attach,
    /// pull, drain, snapshot) every collection any PUT wrote into was
    /// MKCOL'd in that same tick's ladder — a future path addition
    /// that skips its parent fails here against the fake's missing-
    /// parent posture, not on the real server's 404.
    #[tokio::test]
    async fn the_attach_ladder_mkcols_every_collection_the_puts_need() {
        let fake = FakeFilesTransport::new();
        fake.seed_dirs(&["taskboard"]);
        let state = Arc::new(FixedState(state_with(vec![queued_update(
            0xE1,
            TaskId::from_uuid(TASK_UUID),
        )])));
        let context = ctx(Arc::clone(&state), mpsc::channel(8).0);
        let mut lanes = Lanes::default();
        let ran = run_board_lanes(&fake, &context, &mut lanes).await;
        assert_eq!(ran, 1, "the lane ran to completion");

        let mkcols: BTreeSet<String> = fake
            .requests()
            .iter()
            .filter(|req| req.method == "MKCOL")
            .map(|req| req.path.clone())
            .collect();
        for put in fake.requests().iter().filter(|req| req.method == "PUT") {
            let mut dir = put.path.as_str();
            while let Some((parent, _)) = dir.rsplit_once('/') {
                dir = parent;
                if dir == "taskboard" || dir.is_empty() {
                    break; // the per-user ladder owns the root
                }
                assert!(
                    mkcols.contains(dir),
                    "PUT {}/ has no MKCOL for its ancestor {dir}/",
                    put.path
                );
            }
        }
    }

    /// The ref-delete pin, re-authored to the N-19 repair's contract
    /// (the investigation's amended delta): a files-tagged ref-delete
    /// materializes as a tombstone chunk and settles `Applied` — no
    /// Deck actor ever serves a files-bound board again — while a
    /// deck-tagged ref with no local row stays queued (not this
    /// tier's data).
    #[tokio::test]
    async fn a_files_tagged_ref_delete_materializes_its_tombstone() {
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
        let report = board_rx.try_recv().expect("the drain report");
        let SyncReport::Completed { pushes, .. } = report else {
            panic!("completed");
        };
        assert_eq!(pushes.len(), 1, "the ref-delete settled");
        assert!(matches!(
            pushes[0].result,
            taskboard_domain::PushResult::Applied { .. }
        ));
        let ops_path = crate::cycle::rooted(&taskboard_sync_files::layout::board_ops_path(
            board(),
            device(),
            1,
        ));
        let bytes = last_put_body(&fake, &ops_path);
        let chunk: BoardChunk = serde_json::from_slice(&bytes).expect("a valid chunk");
        let tombstone = chunk
            .entities
            .iter()
            .filter_map(|entry| match entry {
                EntityEntry::Modeled(entity) => Some(entity),
                EntityEntry::Verbatim(_) => None,
            })
            .find(|entity| entity.kind == EntityKind::Task && entity.id == TASK_UUID)
            .expect("the deleted task relays");
        let row = tombstone.to_task().expect("a decodable tombstone");
        assert!(row.deleted, "the tombstone carries the deletion");
    }

    /// The pinned refusal's surviving arm: a deck-tagged ref-delete
    /// with no local row stays queued (the files tier cannot address
    /// a Deck namespace id).
    #[tokio::test]
    async fn a_deck_tagged_ref_delete_with_no_local_row_stays_queued() {
        let fake = FakeFilesTransport::new();
        fake.seed_dirs(&["taskboard"]);
        let op = PendingOp {
            op_id: taskboard_domain::OpId(uuid::Uuid::from_u128(0xE3)),
            op: taskboard_domain::LocalOp::DeleteRemoteCard {
                board: board(),
                card: taskboard_domain::ids::RemoteCardRef {
                    board: RemoteBoardId::deck(42),
                    stack: taskboard_domain::ids::RemoteStackId::deck(7),
                    card: taskboard_domain::ids::RemoteCardId::deck(9),
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
            "no drain report: the deck-tagged ref never publishes"
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
            br#"{"board_id":"00000000-0000-0000-0000-0000000b0a1d","format_version":1,"created_at":"2026-11-08T00:00:00Z"}"#.to_vec(),
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

    // ---- The owner's post-snapshot GC pass (D-BF7) -------------------
    //
    // The eligibility law, enforced by construction and pinned here:
    // watermark-bounded (the just-published snapshot's seq), owner-
    // scoped (only paths derived from ctx.identity.device_id), and
    // tombstones-are-forever (whole ops files go; the tombstone fields
    // ride the snapshots).

    fn own_ops_path(seq: u64) -> String {
        crate::cycle::rooted(&taskboard_sync_files::layout::board_ops_path(
            board(),
            device(),
            seq,
        ))
    }

    fn gc_ctx() -> crate::cycle::SyncContext {
        ctx(Arc::new(FixedState(state_with(vec![]))), mpsc::channel(1).0)
    }

    /// The watermark bound, lane-level: chunk 1 published on tick 1 is
    /// subsumed by the boot snapshot (seq 1) and GC'd; the chunk
    /// published on tick 2 (after the snapshot, no cadence due) is
    /// still a live ops file — a test asserting its survival passes,
    /// deleting it would be the bug.
    #[tokio::test]
    async fn the_gc_pass_deletes_watermark_bounded_chunks_and_spares_unpublished_ones() {
        let fake = FakeFilesTransport::new();
        fake.seed_dirs(&["taskboard"]);
        let state = Arc::new(FixedState(state_with(vec![queued_update(
            0xE1,
            TaskId::from_uuid(TASK_UUID),
        )])));
        let context = ctx(Arc::clone(&state), mpsc::channel(8).0);
        let mut lanes = Lanes::default();
        run_board_lanes(&fake, &context, &mut lanes).await;
        assert_eq!(
            fake.count("DELETE", Some(&own_ops_path(1))),
            1,
            "chunk 1 is GC'd behind the snapshot that subsumes it"
        );
        assert!(fake.file(&own_ops_path(1)).is_none());

        // Tick 2 over the same lane: the still-queued op drains at
        // seq 2; one chunk since the snapshot ⇒ no snapshot due ⇒ no
        // watermark ⇒ nothing eligible.
        run_board_lanes(&fake, &context, &mut lanes).await;
        assert_eq!(
            fake.count("DELETE", Some(&own_ops_path(2))),
            0,
            "the chunk published after the snapshot survives"
        );
        assert!(
            fake.file(&own_ops_path(2)).is_some(),
            "the unpublished chunk is still a live ops file"
        );
    }

    /// Owner scope, lane-level: a foreign device's chunk in its own
    /// device dir is never deleted, across cadences — and every DELETE
    /// request this device ever issues sits under its own ops dir
    /// prefix (the structural scope pin: no foreign path is ever
    /// constructed).
    #[tokio::test]
    async fn the_gc_pass_never_touches_a_foreign_device_dir() {
        let peer = DeviceId::parse("0193a1b2-c3d4-7cc9-8a2e-5f1e2d3c4b6b").expect("parses");
        let fake = FakeFilesTransport::new();
        fake.seed_dirs(&["taskboard"]);
        let peer_chunk = crate::cycle::rooted(&taskboard_sync_files::layout::board_ops_path(
            board(),
            peer,
            1,
        ));
        fake.seed_file(&peer_chunk, b"peer bytes".to_vec());

        let state = Arc::new(FixedState(state_with(vec![queued_update(
            0xE1,
            TaskId::from_uuid(TASK_UUID),
        )])));
        let context = ctx(Arc::clone(&state), mpsc::channel(8).0);
        let mut lanes = Lanes::default();
        run_board_lanes(&fake, &context, &mut lanes).await;
        run_board_lanes(&fake, &context, &mut lanes).await;

        assert!(
            fake.file(&peer_chunk).is_some(),
            "the foreign device's chunk is never deleted"
        );
        let own_prefix = crate::cycle::rooted(&format!(
            "boards/{BOARD_UUID}/devices/{}/ops/",
            device_dir_name(device())
        ));
        let foreign_deletes = fake
            .requests()
            .iter()
            .filter(|req| req.method == "DELETE" && !req.path.starts_with(&own_prefix))
            .count();
        assert_eq!(foreign_deletes, 0, "every DELETE is owner-scoped");
        assert_eq!(
            fake.count("DELETE", Some(&own_prefix)),
            1,
            "only own chunk 1 was eligible"
        );
    }

    /// The `None`-watermark leg at the lane level: a snapshot cadence
    /// that never publishes (the PUT fails typed) runs no GC pass —
    /// zero DELETE requests, exactly the pinned `gc_eligible(.., None)`
    /// law (nothing is subsumed, so nothing may go).
    #[tokio::test]
    async fn a_snapshot_cadence_that_never_publishes_records_no_deletes() {
        let fake = FakeFilesTransport::new();
        fake.seed_dirs(&["taskboard"]);
        // The staged verdicts land in call order: the attach ladder
        // (5 MKCOL + 1 create-once identity) and the pull's listings
        // (the devices dir, the self device dir, its ops dir) plus the
        // drain's chunk PUT pass through; the snapshot PUT answers 500.
        for _ in 0..10 {
            fake.stage(crate::test_support::Effect::Pass);
        }
        fake.stage(crate::test_support::Effect::Status(500));
        let state = Arc::new(FixedState(state_with(vec![queued_update(
            0xE1,
            TaskId::from_uuid(TASK_UUID),
        )])));
        let context = ctx(Arc::clone(&state), mpsc::channel(8).0);
        let mut lanes = Lanes::default();
        let _ = run_board_lanes(&fake, &context, &mut lanes).await;
        assert_eq!(
            fake.count("DELETE", None),
            0,
            "no published snapshot ⇒ no GC watermark ⇒ no deletions"
        );
    }

    /// The tombstone law's whole-file leg: a chunk whose payload
    /// carries `deleted` fields is GC'd exactly like any other ops
    /// file behind its subsuming snapshot (the tombstone *fields*
    /// live on in the snapshot payload). The inverse pin — a test
    /// asserting the tombstone chunk survives — fails by
    /// construction; that is the documented D-BF7 semantics, not a
    /// bug.
    #[tokio::test]
    async fn a_tombstone_bearing_chunk_is_gcd_like_any_other() {
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
        let context = ctx(Arc::clone(&state), mpsc::channel(8).0);
        let mut lanes = Lanes::default();
        run_board_lanes(&fake, &context, &mut lanes).await;
        assert_eq!(
            fake.count("DELETE", Some(&own_ops_path(1))),
            1,
            "the tombstone-bearing chunk is GC'd behind its snapshot"
        );
    }

    // The GC pass in isolation (deterministic sequencing, no lane
    // cadence): per-file failure semantics and the idempotent re-run.

    /// A per-file delete failure (typed) leaves the file in place, the
    /// pass continues to the next seq, and the lane stays Ok; the next
    /// cadence's pass retries the leftover (two-cadence sequence).
    #[tokio::test]
    async fn a_failed_chunk_delete_continues_and_retries_next_cadence() {
        let fake = FakeFilesTransport::new();
        fake.seed_dirs(&[
            "taskboard",
            "taskboard/boards",
            &format!("taskboard/boards/{BOARD_UUID}"),
            &format!("taskboard/boards/{BOARD_UUID}/devices"),
            &format!("taskboard/boards/{BOARD_UUID}/devices/{SELF}"),
            &format!("taskboard/boards/{BOARD_UUID}/devices/{SELF}/ops"),
        ]);
        fake.seed_file(&own_ops_path(1), b"chunk one".to_vec());
        fake.seed_file(&own_ops_path(2), b"chunk two".to_vec());
        // The staged verdicts land in call order: the listing passes
        // through, the first chunk DELETE answers 503 (the injection
        // targets the first per-file delete, not the listing).
        fake.stage(crate::test_support::Effect::Pass);
        fake.stage(crate::test_support::Effect::Status(503));

        let outcome = gc_own_chunks(&fake, &gc_ctx(), BOARD_UUID, 2).await;
        assert!(outcome.is_ok(), "a per-file failure never fails the pass");
        assert!(
            fake.file(&own_ops_path(1)).is_some(),
            "the failed delete leaves the file in place"
        );
        assert!(
            fake.file(&own_ops_path(2)).is_none(),
            "the pass continued to the next seq"
        );

        // The next cadence: eligibility recomputed from the listing,
        // the leftover retried — the idempotent re-run converges.
        let outcome = gc_own_chunks(&fake, &gc_ctx(), BOARD_UUID, 2).await;
        assert!(outcome.is_ok());
        assert!(fake.file(&own_ops_path(1)).is_none());
    }

    /// Already-absent: a listing naming a file the model no longer
    /// holds (the crash-between-publish-and-GC analog) deletes as the
    /// idempotent no-op and the pass stays Ok.
    #[tokio::test]
    async fn an_already_absent_chunk_deletes_as_the_idempotent_noop() {
        let fake = FakeFilesTransport::new();
        fake.seed_dirs(&[
            "taskboard",
            "taskboard/boards",
            &format!("taskboard/boards/{BOARD_UUID}"),
            &format!("taskboard/boards/{BOARD_UUID}/devices"),
            &format!("taskboard/boards/{BOARD_UUID}/devices/{SELF}"),
            &format!("taskboard/boards/{BOARD_UUID}/devices/{SELF}/ops"),
        ]);
        fake.stage(crate::test_support::Effect::Listing(vec![
            crate::transport::PropfindMember {
                href: format!("taskboard/boards/{BOARD_UUID}/devices/{SELF}/ops/000001.json"),
                etag: None,
            },
        ]));
        let outcome = gc_own_chunks(&fake, &gc_ctx(), BOARD_UUID, 1).await;
        assert!(outcome.is_ok(), "already-absent is a no-op, not an error");
        assert_eq!(fake.count("DELETE", Some(&own_ops_path(1))), 1);
    }

    /// A transport-class error on the listing aborts the pass
    /// silently-for-this-cadence: no DELETE is ever issued on a failed
    /// listing (the retry story is next cadence's re-derivation).
    #[tokio::test]
    async fn a_failed_listing_aborts_the_pass_without_deleting() {
        let fake = FakeFilesTransport::new();
        fake.seed_dirs(&[
            "taskboard",
            "taskboard/boards",
            &format!("taskboard/boards/{BOARD_UUID}"),
            &format!("taskboard/boards/{BOARD_UUID}/devices"),
            &format!("taskboard/boards/{BOARD_UUID}/devices/{SELF}"),
            &format!("taskboard/boards/{BOARD_UUID}/devices/{SELF}/ops"),
        ]);
        fake.seed_file(&own_ops_path(1), b"chunk one".to_vec());
        fake.stage(crate::test_support::Effect::Status(503));
        let outcome = gc_own_chunks(&fake, &gc_ctx(), BOARD_UUID, 1).await;
        assert!(outcome.is_err(), "the listing failure aborts the pass");
        assert_eq!(
            fake.count("DELETE", None),
            0,
            "no deletion on an aborted listing"
        );
        assert!(
            fake.file(&own_ops_path(1)).is_some(),
            "the chunk survives the aborted cadence"
        );
    }

    const DECK_ORIGIN: &str = "https://cloud.example";

    fn moved_from() -> MovedFrom {
        MovedFrom {
            deck_server: DECK_ORIGIN.to_owned(),
            deck_board: 42,
        }
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

        publish_move_floor(&fake, &context, board(), remote, &persisted, &moved_from())
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
        publish_move_floor(&fake, &context, board(), remote, &persisted, &moved_from())
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

    // -- the N-19 investigation (the files-sync-status UX plan US-2,
    // D-2): the hermetic five-shape fixture and the reproduction test.
    // The operator's posture: a files-bound board, the per-board
    // pending count only rising, and only `identity.json` in the
    // remote subtree. The fixture seeds one op of every shape the
    // planner can attribute to the board and asserts the cycle's
    // contract: every op settles `Applied` on the report lane and an
    // ops/ chunk lands beside identity.json.

    const STACK2_UUID: uuid::Uuid = uuid::Uuid::from_u128(0x57AD);
    const LABEL_UUID: uuid::Uuid = uuid::Uuid::from_u128(0x1A81E);

    fn label_row() -> taskboard_domain::Label {
        taskboard_domain::Label {
            id: taskboard_domain::ids::LabelId::from_uuid(LABEL_UUID),
            board: board(),
            remote: None,
            title: "urgent".into(),
            color: taskboard_domain::Color::new("e76f51"),
            deleted: false,
            clocks: taskboard_domain::LabelClocks {
                title: chrono::DateTime::<chrono::Utc>::MIN_UTC,
                color: chrono::DateTime::<chrono::Utc>::MIN_UTC,
                deleted: chrono::DateTime::<chrono::Utc>::MIN_UTC,
            },
            authors: taskboard_domain::entities::LabelAuthors::default(),
            remote_seen: None,
        }
    }

    fn stack2() -> Stack {
        Stack {
            id: StackId::from_uuid(STACK2_UUID),
            board: board(),
            remote: None,
            title: "second column".into(),
            order: 2,
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

    /// One op of every attributed shape (the engine queues these
    /// optimistically, so the entity rows already exist locally):
    /// task edit, stack add, label op, board rename, sections touch.
    fn five_shape_outbox() -> Vec<PendingOp> {
        let task = TaskId::from_uuid(TASK_UUID);
        let stack = StackId::from_uuid(STACK2_UUID);
        let label = taskboard_domain::ids::LabelId::from_uuid(LABEL_UUID);
        let op = |n: u128, op: LocalOp| PendingOp {
            op_id: taskboard_domain::OpId(uuid::Uuid::from_u128(n)),
            op,
            queued_at: chrono::DateTime::<chrono::Utc>::MIN_UTC,
        };
        vec![
            op(0xE10, LocalOp::UpdateTask(task)),
            op(0xE11, LocalOp::CreateStack(stack)),
            op(0xE12, LocalOp::CreateLabel(label)),
            op(0xE13, LocalOp::UpdateBoard(board())),
            op(0xE14, LocalOp::UpdateAppData(task)),
        ]
    }

    fn five_shape_state() -> PersistedState {
        let mut state = state_with(five_shape_outbox());
        state
            .stacks
            .insert(StackId::from_uuid(STACK2_UUID), stack2());
        state.labels.insert(
            taskboard_domain::ids::LabelId::from_uuid(LABEL_UUID),
            label_row(),
        );
        state
    }

    /// US-2 AC-1 — the reproduction: after a cycle runs without a
    /// transport error, every attributed op settles `Applied` on the
    /// report lane (the pending count's outbox source drains to zero)
    /// and an `ops/NNNN.json` chunk exists beside `identity.json`.
    #[tokio::test]
    async fn n19_ac1_pending_falls_after_a_cycle() {
        let fake = FakeFilesTransport::new();
        fake.seed_dirs(&["taskboard"]);
        let state = Arc::new(FixedState(five_shape_state()));
        let (board_tx, mut board_rx) = mpsc::channel(16);
        let context = ctx(Arc::clone(&state), board_tx);
        let mut lanes = Lanes::default();
        let ran = run_board_lanes(&fake, &context, &mut lanes).await;
        assert_eq!(ran, 1, "the owned board's lane ran to completion");

        // (c) The transport listing shows the ops/ chunk beside
        // identity.json (structural fact, not a log match).
        let identity =
            crate::cycle::rooted(&taskboard_sync_files::layout::board_identity_path(board()));
        assert!(fake.file(&identity).is_some(), "identity.json lands");
        let ops_prefix = crate::cycle::rooted(&format!(
            "boards/{BOARD_UUID}/devices/{}/ops/",
            device_dir_name(device())
        ));
        assert!(
            fake.count("PUT", Some(&ops_prefix)) >= 1,
            "an ops chunk published beside identity.json"
        );

        // (a)+(b) Every op in the outbox settled Applied — the typed
        // pending-falls proof (the engine's outbox is the single
        // source of the pending count).
        let mut settled: Vec<taskboard_domain::OpId> = Vec::new();
        while let Ok(report) = board_rx.try_recv() {
            let SyncReport::Completed { pushes, .. } = report else {
                continue;
            };
            for outcome in pushes {
                let taskboard_domain::PushResult::Applied { .. } = outcome.result else {
                    continue;
                };
                settled.push(outcome.op);
            }
        }
        let outbox = five_shape_outbox();
        assert_eq!(
            settled.len(),
            outbox.len(),
            "every attributed op settled Applied"
        );
        for entry in &outbox {
            assert!(
                settled.contains(&entry.op_id),
                "op {} settled",
                entry.op_id.0
            );
        }
    }

    /// The stranding reproduction (the investigation's ac2, US-2),
    /// green since the drain repair: a ref-delete op — the #150
    /// Deck-collapse artifact — attributes to a files-bound board and
    /// now settles as a tombstone chunk; the pending count falls.
    #[tokio::test]
    async fn n19_ac2_a_ref_delete_no_longer_strands_on_a_files_board() {
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
        let (board_tx, mut board_rx) = mpsc::channel(16);
        let context = ctx(Arc::clone(&state), board_tx);
        let mut lanes = Lanes::default();
        let ran = run_board_lanes(&fake, &context, &mut lanes).await;
        assert_eq!(ran, 1, "the owned board's lane ran");
        let ops_prefix = crate::cycle::rooted(&format!(
            "boards/{BOARD_UUID}/devices/{}/ops/",
            device_dir_name(device())
        ));
        assert!(
            fake.count("PUT", Some(&ops_prefix)) >= 1,
            "the ref-delete's tombstone chunk published"
        );
        let mut settled = 0usize;
        while let Ok(SyncReport::Completed { pushes, .. }) = board_rx.try_recv() {
            settled += pushes
                .iter()
                .filter(|outcome| {
                    matches!(outcome.result, taskboard_domain::PushResult::Applied { .. })
                })
                .count();
        }
        assert_eq!(settled, 1, "the ref-delete settled Applied");
    }

    /// The pull-loop instrument (H3, D-2): a transport failure in the
    /// pull stage holds the lane's publish back for that tick only —
    /// the next tick recovers, drains, and settles the op. No
    /// permanent silent stall lives in the lane runner.
    #[tokio::test]
    async fn n19_h3_a_failed_pull_recovers_on_the_next_tick() {
        let fake = FakeFilesTransport::new();
        fake.seed_dirs(&["taskboard"]);
        // The attach ladder (4 MKCOL + 1 create-once identity) passes;
        // the pull's first listing answers 503 — nothing publishes
        // this tick (publish-after-import).
        for _ in 0..5 {
            fake.stage(crate::test_support::Effect::Pass);
        }
        fake.stage(crate::test_support::Effect::Status(503));
        let state = Arc::new(FixedState(state_with(vec![queued_update(
            0xE5,
            TaskId::from_uuid(TASK_UUID),
        )])));
        let (board_tx, mut board_rx) = mpsc::channel(16);
        let context = ctx(Arc::clone(&state), board_tx);
        let mut lanes = Lanes::default();
        let ran = run_board_lanes(&fake, &context, &mut lanes).await;
        assert_eq!(ran, 0, "the pull failure holds the lane open");
        assert!(
            board_rx.try_recv().is_err(),
            "no report flies for a failed pull"
        );

        // The next tick: the same lane recovers, folds, drains.
        let ran = run_board_lanes(&fake, &context, &mut lanes).await;
        assert_eq!(ran, 1, "the lane recovered");
        let mut settled = 0usize;
        while let Ok(SyncReport::Completed { pushes, .. }) = board_rx.try_recv() {
            settled += pushes
                .iter()
                .filter(|outcome| {
                    matches!(outcome.result, taskboard_domain::PushResult::Applied { .. })
                })
                .count();
        }
        assert_eq!(settled, 1, "the op settled on the recovering tick");
    }

    /// A moved subtree on the server, with `this_device`'s Deck copy.
    fn moved_world(record: Option<Vec<u8>>) -> (FakeFilesTransport, PersistedState, uuid::Uuid) {
        let target = uuid::Uuid::from_u128(0x7777);
        let fake = FakeFilesTransport::new();
        let dir = format!("taskboard/boards/{target}");
        fake.seed_dirs(&["taskboard", "taskboard/boards", &dir]);
        let identity = BoardIdentity {
            board_id: target,
            format_version: TREE_FORMAT_VERSION,
            created_at: chrono::DateTime::UNIX_EPOCH,
            moved_from: Some(moved_from()),
        };
        fake.seed_file(
            &format!("{dir}/identity.json"),
            serde_json::to_vec(&identity).expect("serializes"),
        );
        if let Some(bytes) = record {
            fake.seed_file(&format!("{dir}/move.json"), bytes);
        }
        let mut persisted = state_with(vec![]);
        for row in persisted.boards.values_mut() {
            row.remote = Some(taskboard_domain::ids::RemoteBoardBinding {
                connector: taskboard_domain::ids::ConnectorId::from("nextcloud"),
                board: taskboard_domain::RemoteBoardId::deck(42),
            });
        }
        (fake, persisted, target)
    }

    fn record_bytes() -> Vec<u8> {
        serde_json::to_vec(&MoveRecord {
            format_version: taskboard_sync_files::board_move::MOVE_RECORD_FORMAT,
            moved_from: moved_from(),
            moved_by: DeviceId::mint(),
            committed_at: chrono::DateTime::UNIX_EPOCH,
            stacks: BTreeMap::new(),
            cards: BTreeMap::new(),
            labels: BTreeMap::new(),
        })
        .expect("serializes")
    }

    async fn tick(
        fake: &FakeFilesTransport,
        persisted: PersistedState,
        reports: mpsc::Sender<SyncReport>,
    ) -> Lanes {
        let context = ctx(Arc::new(FixedState(persisted)), reports);
        let mut lanes = Lanes::default();
        let _ = run_board_lanes(fake, &context, &mut lanes).await;
        lanes
    }

    #[tokio::test]
    async fn an_uncommitted_move_neither_adopts_nor_mints() {
        let (fake, persisted, target) = moved_world(None);
        let (tx, mut rx) = mpsc::channel(8);
        let lanes = tick(&fake, persisted, tx).await;
        assert!(rx.try_recv().is_err(), "nothing reported");
        assert!(!lanes.discovered.contains(&target));
        assert!(lanes.moved.is_empty());
    }

    #[tokio::test]
    async fn a_committed_move_reports_adoption_and_never_mints() {
        let (fake, persisted, target) = moved_world(Some(record_bytes()));
        let (tx, mut rx) = mpsc::channel(8);
        let lanes = tick(&fake, persisted, tx).await;
        assert!(matches!(rx.try_recv(), Ok(SyncReport::BoardMoved { .. })));
        assert!(rx.try_recv().is_err(), "no discovery fold beside it");
        assert!(matches!(
            lanes.moved.get(&target),
            Some(BoardMoveFinding::Adopting { .. })
        ));
    }

    #[tokio::test]
    async fn the_finding_exists_even_when_the_report_cannot_be_delivered() {
        let (fake, persisted, target) = moved_world(Some(record_bytes()));
        let (tx, rx) = mpsc::channel(1);
        drop(rx);
        let lanes = tick(&fake, persisted, tx).await;
        assert!(matches!(
            lanes.moved.get(&target),
            Some(BoardMoveFinding::Adopting { .. })
        ));
    }

    #[tokio::test]
    async fn a_device_with_no_copy_discovers_the_committed_subtree() {
        let (fake, mut persisted, target) = moved_world(Some(record_bytes()));
        for row in persisted.boards.values_mut() {
            row.remote = None;
        }
        let (tx, _rx) = mpsc::channel(8);
        let lanes = tick(&fake, persisted, tx).await;
        assert!(lanes.discovered.contains(&target));
        assert!(lanes.moved.is_empty());
    }

    #[tokio::test]
    async fn a_malformed_record_is_a_refusal_and_never_mints() {
        let (fake, persisted, target) = moved_world(Some(b"{}".to_vec()));
        let (tx, mut rx) = mpsc::channel(8);
        let lanes = tick(&fake, persisted, tx).await;
        assert!(rx.try_recv().is_err());
        assert!(!lanes.discovered.contains(&target));
        assert_eq!(
            lanes.moved.get(&target),
            Some(&BoardMoveFinding::Refused(
                BoardMoveRefusal::RecordMalformed
            ))
        );
    }

    #[tokio::test]
    async fn a_subtree_without_a_readable_identity_is_held() {
        let (fake, persisted, target) = moved_world(None);
        fake.remove_file(&format!("taskboard/boards/{target}/identity.json"));
        let (tx, mut rx) = mpsc::channel(8);
        let lanes = tick(&fake, persisted, tx).await;
        assert!(rx.try_recv().is_err());
        assert!(!lanes.discovered.contains(&target));
    }

    // ---- The unbind retirement (leg 2, the operator's GC-on-unbind
    // decision as amended 2026-11-19: device-scoped) -------------------

    const PEER_DEVICE: &str = "0193a1b2-c3d4-7cc9-8a2e-5f1e2d3c4b6b";

    fn peer_id() -> DeviceId {
        DeviceId::parse(PEER_DEVICE).expect("parses")
    }

    /// A bound subtree with this device's chunk + snapshot-bearing
    /// device dir, another device's chunk, and the board's shared
    /// identity — everything the retirement's scope law can be
    /// asserted against.
    fn seed_bound_subtree(fake: &FakeFilesTransport) -> (String, String, String) {
        let self_dir = format!(
            "taskboard/boards/{BOARD_UUID}/devices/{}",
            device_dir_name(device())
        );
        let peer_dir = format!(
            "taskboard/boards/{BOARD_UUID}/devices/{}",
            device_dir_name(peer_id())
        );
        fake.seed_dirs(&[
            "taskboard",
            "taskboard/boards",
            &format!("taskboard/boards/{BOARD_UUID}"),
            &format!("taskboard/boards/{BOARD_UUID}/devices"),
            &self_dir,
            &format!("{self_dir}/ops"),
            &peer_dir,
            &format!("{peer_dir}/ops"),
        ]);
        let identity_path =
            crate::cycle::rooted(&taskboard_sync_files::layout::board_identity_path(board()));
        fake.seed_file(
            &identity_path,
            br#"{"board_id":"00000000-0000-0000-0000-0000000b0a1d","format_version":1,"created_at":"2026-11-08T00:00:00Z"}"#.to_vec(),
        );
        let own_chunk = own_ops_path(1);
        fake.seed_file(&own_chunk, b"own chunk".to_vec());
        let peer_chunk = crate::cycle::rooted(&taskboard_sync_files::layout::board_ops_path(
            board(),
            peer_id(),
            1,
        ));
        fake.seed_file(&peer_chunk, b"peer chunk".to_vec());
        (identity_path, own_chunk, peer_chunk)
    }

    fn unbound_state() -> PersistedState {
        let mut state = state_with(vec![]);
        let mut row = files_bound_board();
        row.remote = None;
        state.boards.insert(board(), row);
        state
    }

    /// The amended device-scope law (fail by construction): the
    /// retirement deletes exactly this device's own directory under
    /// the remote board and nothing else — a foreign device's chunk
    /// and the board's shared identity.json survive, every recorded
    /// DELETE sits under this device's device-dir prefix, and the
    /// assertion set fails on any whole-subtree (Option A unamended)
    /// implementation.
    #[tokio::test]
    async fn an_unbind_retires_only_this_devices_files_under_the_remote_board() {
        let fake = FakeFilesTransport::new();
        let (identity_path, own_chunk, peer_chunk) = seed_bound_subtree(&fake);
        let context = ctx(Arc::new(FixedState(state_with(vec![]))), mpsc::channel(8).0);
        let mut lanes = Lanes::default();
        // Tick 1 while bound: the lane runs and remembers the remote
        // uuid (the boot snapshot's seq-0 watermark leaves the seeded
        // chunk 1 alone — nothing is subsumed yet).
        run_board_lanes(&fake, &context, &mut lanes).await;
        assert!(
            fake.file(&own_chunk).is_some(),
            "precondition: nothing GC'd at watermark 0"
        );

        // Tick 2 after the state-level unbind: the lane is retiring.
        let context = ctx(Arc::new(FixedState(unbound_state())), mpsc::channel(8).0);
        run_board_lanes(&fake, &context, &mut lanes).await;

        assert!(
            fake.file(&own_chunk).is_none(),
            "this device's files are gone"
        );
        assert!(
            fake.file(&identity_path).is_some(),
            "the board's shared subtree is never touched (another client may still be connected)"
        );
        assert!(
            fake.file(&peer_chunk).is_some(),
            "another device's files are never touched"
        );
        let own_prefix = crate::cycle::rooted(&format!(
            "boards/{BOARD_UUID}/devices/{}",
            device_dir_name(device())
        ));
        let deletes: Vec<String> = fake
            .requests()
            .into_iter()
            .filter(|req| req.method == "DELETE")
            .map(|req| req.path)
            .collect();
        assert_eq!(deletes.len(), 1, "one collection delete, device-scoped");
        assert!(
            deletes[0].starts_with(&own_prefix),
            "the delete target is this device's own directory: {:?}",
            deletes[0]
        );
        // The stale-lane leak fix: the lane and its discovery entry
        // are pruned, and the retirement is visible as Ok, not silent.
        assert!(lanes.boards.is_empty(), "the retiring lane is pruned");
        assert!(!lanes.discovered.contains(&BOARD_UUID));
        assert_eq!(
            lanes.outcomes.get(&BOARD_UUID),
            Some(&crate::status::LaneOutcome::Ok)
        );
    }

    /// A transport failure defers the retirement: the files stay, the
    /// lane stays, the outcome is a typed failure on the watch (never
    /// silent, never blocking the engine's unbind) — and the next
    /// tick's retry completes idempotently.
    #[tokio::test]
    async fn a_deferred_retirement_stays_visible_and_completes_next_tick() {
        let fake = FakeFilesTransport::new();
        let (identity_path, own_chunk, peer_chunk) = seed_bound_subtree(&fake);
        let context = ctx(Arc::new(FixedState(state_with(vec![]))), mpsc::channel(8).0);
        let mut lanes = Lanes::default();
        run_board_lanes(&fake, &context, &mut lanes).await;

        // Tick 2: the retirement DELETE (the tick's first request)
        // answers 503 — deferred.
        fake.stage(crate::test_support::Effect::Status(503));
        let context = ctx(Arc::new(FixedState(unbound_state())), mpsc::channel(8).0);
        run_board_lanes(&fake, &context, &mut lanes).await;
        assert!(
            fake.file(&own_chunk).is_some(),
            "the failed delete left the files"
        );
        assert!(
            lanes.boards.contains_key(&board()),
            "the lane stays for the retry"
        );
        assert!(fake.file(&identity_path).is_some());
        assert!(fake.file(&peer_chunk).is_some());
        assert_eq!(
            lanes.outcomes.get(&BOARD_UUID),
            Some(&crate::status::LaneOutcome::Failed(
                crate::status::LaneFailure::Unavailable
            )),
            "the stuck retirement is visible, never silent"
        );

        // Tick 3: the retry completes.
        run_board_lanes(&fake, &context, &mut lanes).await;
        assert!(fake.file(&own_chunk).is_none());
        assert!(lanes.boards.is_empty());
        assert_eq!(
            lanes.outcomes.get(&BOARD_UUID),
            Some(&crate::status::LaneOutcome::Ok)
        );
        assert!(fake.file(&peer_chunk).is_some());
    }

    // ---- C2: the orphan law in the files tier (#1255) ------------------

    const WIPED_BOARD_UUID: uuid::Uuid = uuid::Uuid::from_u128(0xDEAD0);
    const ORPHAN_OP: u128 = 0xB1A0;

    /// An orphan op: its owning board row is wiped, so `owning_board`
    /// is `None` and no board lane's per-board filter ever picks it.
    /// A zero-board state carrying only the orphan op.
    fn orphan_only_state() -> PersistedState {
        PersistedState {
            outbox: vec![orphan_op()],
            ..PersistedState::default()
        }
    }

    fn orphan_op() -> PendingOp {
        PendingOp {
            op_id: taskboard_domain::OpId(uuid::Uuid::from_u128(ORPHAN_OP)),
            op: LocalOp::UpdateBoard(BoardId::from_uuid(WIPED_BOARD_UUID)),
            queued_at: chrono::DateTime::<chrono::Utc>::MIN_UTC,
        }
    }

    /// Every `Completed`/`Failed` report's op ids, flattened.
    fn reported_op_ids(reports: &[SyncReport]) -> Vec<taskboard_domain::OpId> {
        reports
            .iter()
            .flat_map(|report| match report {
                SyncReport::Completed { pushes, .. } | SyncReport::Failed { pushes, .. } => {
                    pushes.iter().map(|outcome| outcome.op).collect::<Vec<_>>()
                }
                SyncReport::BoardMoved { .. } => Vec::new(),
            })
            .collect()
    }

    /// #1255 — the ride: an orphan op rides the first (and only) bound
    /// board's drain and completes with it.
    #[tokio::test]
    async fn files_lane_drains_orphan_ops_on_first_board_pass() {
        let fake = FakeFilesTransport::new();
        fake.seed_dirs(&["taskboard"]);
        let state = Arc::new(FixedState(state_with(vec![
            queued_update(0xE1, TaskId::from_uuid(TASK_UUID)),
            orphan_op(),
        ])));
        let (board_tx, mut board_rx) = mpsc::channel(8);
        let context = ctx(state, board_tx);
        let mut lanes = Lanes::default();
        let ran = run_board_lanes(&fake, &context, &mut lanes).await;
        assert_eq!(ran, 1, "the owned board's lane ran");

        let mut reports = Vec::new();
        while let Ok(report) = board_rx.try_recv() {
            reports.push(report);
        }
        assert!(
            reported_op_ids(&reports)
                .contains(&taskboard_domain::OpId(uuid::Uuid::from_u128(ORPHAN_OP))),
            "the orphan op completed with the first board's drain"
        );
    }

    /// #1255 — the early-return trap: the first board has no own ops,
    /// so the per-board filter is empty — the orphan still drains (the
    /// append must precede the guard). Nothing materializes, so no
    /// chunk goes on the wire; the completions report alone.
    #[tokio::test]
    async fn files_first_board_with_no_own_ops_still_drains_orphans() {
        let fake = FakeFilesTransport::new();
        fake.seed_dirs(&["taskboard"]);
        let state = Arc::new(FixedState(state_with(vec![orphan_op()])));
        let (board_tx, mut board_rx) = mpsc::channel(8);
        let context = ctx(state, board_tx);
        let mut lanes = Lanes::default();
        run_board_lanes(&fake, &context, &mut lanes).await;

        let mut reports = Vec::new();
        while let Ok(report) = board_rx.try_recv() {
            reports.push(report);
        }
        assert!(
            reported_op_ids(&reports)
                .contains(&taskboard_domain::OpId(uuid::Uuid::from_u128(ORPHAN_OP))),
            "the orphan drained despite the empty own-op set"
        );
        assert!(
            fake.requests().iter().all(|req| {
                // A chunk file is an all-numeric name under `ops/` (the
                // snapshot duty's `snapshot-<seq>.json` is a legitimate
                // non-chunk write into the same collection).
                let file = req.path.rsplit('/').next().unwrap_or(&req.path);
                !(req.method == "PUT"
                    && req
                        .path
                        .contains(taskboard_sync_files::layout::OPS_DIR_NAME)
                    && file.parse::<u64>().is_ok())
            }),
            "a synthetic completion publishes no chunk"
        );
    }

    /// #1255 — the zero-board leg (the recorded mechanism divergence):
    /// no lane exists, so the synthetic orphan completions run through
    /// the board-less push-only report path.
    #[tokio::test]
    async fn files_lanes_drain_orphans_when_zero_boards_bound() {
        let fake = FakeFilesTransport::new();
        let (board_tx, mut board_rx) = mpsc::channel(8);
        let context = ctx(Arc::new(FixedState(orphan_only_state())), board_tx);
        let mut lanes = Lanes::default();
        run_board_lanes(&fake, &context, &mut lanes).await;

        let mut reports = Vec::new();
        while let Ok(report) = board_rx.try_recv() {
            reports.push(report);
        }
        assert!(
            reports.iter().any(
                |report| matches!(report, SyncReport::Failed { board: None, .. })
                    && reported_op_ids(std::slice::from_ref(report))
                        .contains(&taskboard_domain::OpId(uuid::Uuid::from_u128(ORPHAN_OP)))
            ),
            "the zero-board drain accounted the orphan through the board-less path"
        );
    }

    /// #1255 — the zero-board drain's synthetic completions contact no
    /// write endpoint (the fake's request log is the whole truth of
    /// the wire).
    #[tokio::test]
    async fn files_orphan_drain_contacts_no_server_for_noop_groups() {
        let fake = FakeFilesTransport::new();
        let context = ctx(
            Arc::new(FixedState(orphan_only_state())),
            mpsc::channel(8).0,
        );
        let mut lanes = Lanes::default();
        run_board_lanes(&fake, &context, &mut lanes).await;

        assert!(
            fake.requests()
                .iter()
                .all(|req| matches!(req.method.as_str(), "PROPFIND" | "MKCOL" | "GET")),
            "a synthetic-completion drain never addresses a write endpoint"
        );
        // And the noop op actually drained (it is not stranded behind
        // the zero-owned guard either).
        let (board_tx, mut board_rx) = mpsc::channel(8);
        let context = ctx(Arc::new(FixedState(orphan_only_state())), board_tx);
        let mut lanes = Lanes::default();
        run_board_lanes(&fake, &context, &mut lanes).await;
        let mut reports = Vec::new();
        while let Ok(report) = board_rx.try_recv() {
            reports.push(report);
        }
        assert!(
            reported_op_ids(&reports)
                .contains(&taskboard_domain::OpId(uuid::Uuid::from_u128(ORPHAN_OP))),
            "the zero-board orphan drain completed the op"
        );
    }
}
