// SPDX-License-Identifier: MIT OR Apache-2.0
//! The sync actor: one scheduled cycle — read (via the engine's
//! `SyncStateReader` port) → push (coalesced, dependency-ordered,
//! fetch-before-write) → pull (three conditional reads into a complete
//! `RemoteBoardSnapshot`) → report (evidence-then-verdict), with the
//! offline finite state machine around it (ADR 0007).
//!
//! The actor is stateless across restarts except the injected client: all
//! durable facts arrive via `read_state`; the in-memory pull cache is a
//! rebuildable optimization. Its board set is *derived*: every cycle the
//! actor syncs exactly the persisted bindings tagged with its own
//! connector id — a newly bound board is picked up by the next read with
//! no message at all (the `SetBoard` era is over). This actor reports
//! only *its own* connector's network episodes on the injected broadcast
//! (`SystemEvent::ConnectorLost`/`ConnectorRestored`, exactly once per
//! offline episode); counting those episodes into the global
//! `NetworkLost`/`NetworkRestored` is the app-layer authority's job
//! (`taskboard-app/src/authority.rs`) — no actor is the network
//! authority. `Shutdown` is the only event consumed from the broadcast.

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{broadcast, mpsc};
use tokio::task::JoinHandle;

use taskboard_domain::{
    AppDataEvidence, AppDataUnreadable, BoardPullValidators, Clock, DeckFeatures, EntityTables,
    PushOutcome, PushResult, RemoteBoard, RemoteBoardId, RemoteBoardSnapshot, RemoteCardRef,
    RemoteLabel, RemoteStack, RemoteStackId, RemoteTask, SyncCommand, SyncErrorKind, SyncReport,
    SyncStateReader, SyncValidators, SystemEvent, ValidatorKey, flags_for, plan_pushes,
};

use crate::anchor;
use crate::client::{DeckClient, Fetch, Validators};
use crate::error::DeckError;
use crate::flush;
use crate::mapping;
use crate::mapping::classify_read;
use crate::model::StackFilter;
use crate::model::{Board, Label, Stack};
use crate::poll::poll_backoff;
use crate::push_exec::{BindingOverlay, GroupOutcome, PushExecutor};

/// Knobs the Phase 5 bootstrap wires from figment's `[sync]` table; tests
/// inject tiny/paused values.
#[derive(Debug, Clone)]
pub struct SyncActorConfig {
    /// Idle wait between cycles after a success.
    pub poll_interval: Duration,
    /// First failure backoff (`initial * 2^(streak-1)`).
    pub backoff_initial: Duration,
    /// Backoff saturation.
    pub backoff_max: Duration,
    /// Wall-clock seam for every stamp the actor mints (`read_at`,
    /// dead-board tombstones, archive-mismatch corrections) — the same
    /// seam the engine runs on. Production injects `SystemClock`; the
    /// hermetic suites a fixed clock.
    pub clock: Arc<dyn Clock>,
}

/// Spawns the sync actor onto the current runtime.
///
/// All channel halves are injected (the bootstrap owns the graph, mirroring
/// [`taskboard_domain::TaskRepository`]-style seams): `commands` is the
/// engine's nudge inbox (`SyncNow`/`Detach`), `reports` carries cycle
/// reports to the engine, `system` is the broadcast this actor publishes
/// network events on (and subscribes to for `Shutdown`). `id` is the
/// config-minted connector name: the actor syncs every persisted binding
/// tagged with it, and nothing else.
///
/// # Examples
///
/// Wiring the actor over the Phase 5 channel graph (the bootstrap owns
/// every half; the engine handle implements [`taskboard_domain::
/// SyncStateReader`], and the actor publishes `NetworkLost`/`Restored`
/// next to the engine's own system feed):
///
/// ```no_run
/// use std::sync::Arc;
/// use std::time::Duration;
///
/// use taskboard_domain::{
///     PersistedState, RepositoryError, SyncCommand, SyncReport, SyncStateReader,
///     SystemClock, SystemEvent,
/// };
/// use taskboard_sync_nextcloud::{DeckClient, SyncActorConfig, spawn_sync_actor};
/// use tokio::sync::{broadcast, mpsc};
///
/// // In production: `taskboard_state::EngineHandle` implements the port.
/// #[derive(Debug)]
/// struct EngineView;
///
/// impl SyncStateReader for EngineView {
///     fn read_state(
///         &self,
///     ) -> taskboard_domain::BoxFuture<'_, Result<PersistedState, RepositoryError>> {
///         Box::pin(async { Ok(PersistedState::default()) })
///     }
/// }
///
/// # fn main() {
/// let client = DeckClient::new("https://cloud.example.com", "user", "app-password")
///     .expect("base URL must be valid");
/// let (nudges_tx, nudges_rx) = mpsc::channel::<SyncCommand>(16);
/// let (report_tx, report_rx) = mpsc::channel::<SyncReport>(16);
/// let (system_tx, _system_rx) = broadcast::channel::<SystemEvent>(16);
///
/// let cfg = SyncActorConfig {
///     poll_interval: Duration::from_secs(30),
///     backoff_initial: Duration::from_secs(5),
///     backoff_max: Duration::from_secs(300),
///     clock: Arc::new(SystemClock),
/// };
/// let actor = spawn_sync_actor(
///     taskboard_domain::ConnectorId::from("nextcloud"),
///     client,
///     Arc::new(EngineView),
///     nudges_rx,
///     report_tx,
///     system_tx,
///     cfg,
/// );
/// // The engine consumes `report_rx`, sends `SyncNow` over `nudges_tx`,
/// // and `actor` is aborted last at shutdown.
/// # let _ = (nudges_tx, report_rx, _system_rx, actor);
/// # }
#[allow(clippy::too_many_arguments)] // the injected seams ARE the constructor
#[must_use]
pub fn spawn_sync_actor(
    id: taskboard_domain::ConnectorId,
    client: DeckClient,
    state: Arc<dyn SyncStateReader>,
    commands: mpsc::Receiver<SyncCommand>,
    reports: mpsc::Sender<SyncReport>,
    system: broadcast::Sender<SystemEvent>,
    cfg: SyncActorConfig,
) -> JoinHandle<()> {
    tokio::spawn(run(id, client, state, commands, reports, system, cfg))
}

/// Local stamps of the bound cards (archived flag + `remote_seen`), the
/// detail refresh's comparison baseline keyed by remote ref.
type LocalCardStamps = std::collections::HashMap<
    (RemoteBoardId, taskboard_domain::RemoteCardId),
    (bool, Option<chrono::DateTime<chrono::Utc>>),
>;

/// In-memory cache of the last decoded listings, fill for `304`s. The
/// boards listing is shared across the connector's boards; the stack
/// listings key per remote board, so a rebind is a non-event — a newly
/// bound board has (or lacks) its own slot, and boards *in* the derived
/// set never lose their slot to set changes (no cache-clear-on-retarget;
/// slots for boards that left the set are pruned at cycle start). A cold
/// cache with warm validators forces one unconditional refetch — never a
/// partial snapshot (decision 9).
#[derive(Debug, Default)]
struct PullCache {
    boards: Option<Vec<Board>>,
    stacks_active: std::collections::BTreeMap<RemoteBoardId, Option<Vec<Stack>>>,
    stacks_archived: std::collections::BTreeMap<RemoteBoardId, Option<Vec<Stack>>>,
}

/// The loop's mutable state (ADR 0007 decision 12). No sticky target
/// exists: the board set derives from the persisted bindings every cycle.
struct LoopState {
    cache: PullCache,
    rerun_requested: bool,
    failure_streak: u32,
    offline: bool,
    /// The freshest advertised `Retry-After` delay; a floor on the
    /// autonomous poll backoff. Overwritten by every failure observation,
    /// ignored on success (the streak resets, so `poll_interval` wins).
    retry_after_floor: Duration,
    /// The connector's derived capability flags (D18): probed once per
    /// actor lifetime before the select loop, [`DeckFeatures::CONSERVATIVE`]
    /// on any probe failure. In-memory like everything else here; an
    /// actor respawn (reconnect / re-attach) re-probes.
    features: DeckFeatures,
    /// Per-board sidecar document cache: the attachment metadata pair of
    /// the last successful ingest plus the parsed document (in-memory
    /// only — D-S4 item 3; no new `ValidatorKey` kind: attachment
    /// content has no HTTP validators, so the persisted validator table
    /// is the wrong home). Retention mirrors the pull cache.
    documents: std::collections::BTreeMap<RemoteBoardId, CachedAppData>,
}

/// One board's last successful document ingest (D11). Only a
/// `Document`-class evidence inserts or refreshes an entry; every
/// failure form leaves the cache untouched — the next cycle re-GETs,
/// the honest retry.
#[derive(Debug, Clone)]
struct CachedAppData {
    last_modified: i64,
    filesize: i64,
    document: taskboard_domain::AppDataDocument,
}

#[allow(
    clippy::single_match_else, // select! arms read best as match arms
    clippy::too_many_lines // one biased loop; the arms ARE the design
)]
async fn run(
    id: taskboard_domain::ConnectorId,
    client: DeckClient,
    state: Arc<dyn SyncStateReader>,
    mut commands: mpsc::Receiver<SyncCommand>,
    reports: mpsc::Sender<SyncReport>,
    system: broadcast::Sender<SystemEvent>,
    cfg: SyncActorConfig,
) {
    tracing::info!(connector = %id, "sync actor started");
    let mut sys_rx = system.subscribe();
    let mut sys_open = true;
    let mut st = LoopState {
        cache: PullCache::default(),
        rerun_requested: false,
        failure_streak: 0,
        offline: false,
        retry_after_floor: Duration::ZERO,
        features: probe_features(&client).await,
        documents: std::collections::BTreeMap::new(),
    };
    // First cycle fires immediately; an empty derived set is a silent
    // skip on poll ticks (no `NoBoard` report — decision 10).
    let mut deadline = tokio::time::Instant::now();

    loop {
        tokio::select! {
            biased;

            cmd = commands.recv() => match cmd {
                Some(SyncCommand::SyncNow) => {
                    cycle(&id, &client, &state, &reports, &system, &mut st, &cfg, true).await;
                    deadline = reschedule(&st, &cfg);
                }
                Some(SyncCommand::Detach) => {
                    // Connector-only detach (WI-H): the credential dies
                    // with this task; the engine sees the reports sender
                    // drop and continues without sync.
                    tracing::info!("sync actor stopping: connector detached");
                    return;
                }
                None => {
                    tracing::info!("sync actor stopping: command inbox closed");
                    return;
                }
            },

            () = tokio::time::sleep_until(deadline) => {
                cycle(&id, &client, &state, &reports, &system, &mut st, &cfg, false).await;
                deadline = reschedule(&st, &cfg);
            },

            event = sys_rx.recv(), if sys_open => match event {
                Ok(SystemEvent::Shutdown) => {
                    tracing::info!("sync actor stopping: shutdown event");
                    return;
                }
                Ok(
                    SystemEvent::StorageFailed
                        | SystemEvent::TaskResurrected(_)
                        | SystemEvent::NetworkLost
                        | SystemEvent::NetworkRestored
                        | SystemEvent::ConnectorLost(_)
                        | SystemEvent::ConnectorRestored(_),
                ) => {
                    // The engine's persistence signal is no sync concern;
                    // the whole network vocabulary belongs to the app-layer
                    // authority — this actor reports only its own
                    // connector's episodes and counts nothing, not even
                    // its own events.
                }
                Err(broadcast::error::RecvError::Closed) => {
                    tracing::warn!("system broadcast closed; continuing without shutdown watch");
                    sys_open = false;
                }
                Err(broadcast::error::RecvError::Lagged(missed)) => {
                    tracing::warn!(missed, "system event backlog overflowed");
                }
            },
        }

        // Nudge absorber (ADR 0006): whatever accumulated in the inbox
        // during the cycle collapses into one immediate rerun.
        while let Ok(cmd) = commands.try_recv() {
            match cmd {
                SyncCommand::SyncNow => st.rerun_requested = true,
                SyncCommand::Detach => {
                    // Queued while a cycle ran: still a detach — processed
                    // right after the cycle, never absorbed.
                    tracing::info!("sync actor stopping: connector detached");
                    return;
                }
            }
        }
        if st.rerun_requested {
            st.rerun_requested = false;
            deadline = tokio::time::Instant::now();
        }
    }
}

/// One capabilities probe per actor lifetime (D18): `Ok` + a Deck
/// version ⇒ the derived flags; `Ok` without a deck section, or any
/// typed error, ⇒ the conservative arm with one typed warning — the
/// degraded lane is *correct*, never a surfaced error. Awaited once at
/// actor start, before the select loop; an actor respawn re-probes.
async fn probe_features(client: &DeckClient) -> DeckFeatures {
    match client.capabilities().await {
        Ok(caps) => {
            if let Some(version) = caps.deck_version {
                let features = flags_for(&version);
                tracing::info!(?features, "deck capability flags derived");
                features
            } else {
                tracing::warn!(
                    "capabilities carry no deck section: deriving conservative feature flags"
                );
                DeckFeatures::CONSERVATIVE
            }
        }
        Err(err) => {
            tracing::warn!(
                ?err,
                "capabilities probe failed: deriving conservative feature flags"
            );
            DeckFeatures::CONSERVATIVE
        }
    }
}

/// The next wake-up: `poll_interval` after success, the failure backoff
/// after a failure streak.
fn reschedule(st: &LoopState, cfg: &SyncActorConfig) -> tokio::time::Instant {
    let delay = if st.failure_streak == 0 {
        cfg.poll_interval
    } else {
        poll_backoff(
            st.failure_streak,
            cfg.backoff_initial,
            cfg.backoff_max,
            st.retry_after_floor,
        )
    };
    tokio::time::Instant::now() + delay
}

/// The boards this connector's actor owns: persisted bindings tagged
/// with this actor's id, live and tombstoned alike (R9c: a deleting
/// board's finalizing push is a sync fact), in board-id order. Recomputed
/// from the persisted state every cycle — the derived set is the only
/// target source (D3-2's semantics live here now: a finalized tombstone's
/// cleared binding drops the board; a fresh actor derives from persisted
/// bindings; boards tagged with *another* connector are excluded).
fn owned_boards(
    id: &taskboard_domain::ConnectorId,
    boards: &std::collections::BTreeMap<taskboard_domain::BoardId, taskboard_domain::Board>,
) -> Vec<(
    taskboard_domain::BoardId,
    taskboard_domain::RemoteBoardBinding,
)> {
    boards
        .iter()
        .filter_map(|(board_id, board)| {
            let binding = board.remote.as_ref()?;
            (binding.connector == *id).then(|| (*board_id, binding.clone()))
        })
        .collect()
}

/// One cycle. `commanded` distinguishes nudges (which report `NoBoard` on
/// an empty derived set) from poll ticks (which silently skip). One read,
/// then a per-board loop: push (that board's ops only) → pull (its own
/// cache slots) → one report per board. A board's failure continues the
/// loop; a Network-classified failure ends the cycle early (the transport
/// is shared and down; `go_offline` stamps the unattempted boards' rows
/// anyway — D-12). Backoff stays actor-global.
#[allow(clippy::too_many_lines, clippy::too_many_arguments)] // the cycle IS the design; the seams ARE the constructor
async fn cycle(
    id: &taskboard_domain::ConnectorId,
    client: &DeckClient,
    state: &Arc<dyn SyncStateReader>,
    reports: &mpsc::Sender<SyncReport>,
    system: &broadcast::Sender<SystemEvent>,
    st: &mut LoopState,
    cfg: &SyncActorConfig,
    commanded: bool,
) {
    // ---- read ----------------------------------------------------------
    // The read instant is stamped *before* the read issues: an op queued
    // between the stamp and the reply was not seen by this cycle's push, so
    // the merge must protect its fields from the pull's stamps (the
    // self-clobber guard on `SyncReport::Completed`). Conservative by
    // construction — over-protection only keeps a newer local intent.
    // Wall-clock note: every stamp flows through the injected `Clock` seam;
    // production runs engine and actor on one `SystemClock`.
    let read_at = cfg.clock.now();
    let started = std::time::Instant::now();
    let persisted = match state.read_state().await {
        Ok(persisted) => persisted,
        Err(err) => {
            // Engine mid-restart (or a corrupted store): back off, no
            // report — the engine re-nudges or the poll tick retries.
            tracing::error!(?err, "cycle aborted: engine state unreadable");
            st.failure_streak += 1;
            st.retry_after_floor = Duration::ZERO;
            return;
        }
    };

    // ---- derive --------------------------------------------------------
    let owned = owned_boards(id, &persisted.boards);
    if owned.is_empty() {
        if commanded {
            tracing::debug!("cycle skipped: no board bound for this connector");
            send_report(
                reports,
                SyncReport::Failed {
                    connector: id.clone(),
                    kind: SyncErrorKind::NoBoard,
                    pushes: Vec::new(),
                    read_at: cfg.clock.now(),
                    board: None,
                    duration: started.elapsed(),
                    features: st.features,
                },
            )
            .await;
        }
        return;
    }
    // Memory proportional to the live set: slots for remote ids that left
    // the derived set are pruned (D-10); boards in the set never lose
    // their slot to set changes.
    let live: std::collections::BTreeSet<RemoteBoardId> =
        owned.iter().map(|(_, binding)| binding.board).collect();
    st.cache
        .stacks_active
        .retain(|board, _| live.contains(board));
    st.cache
        .stacks_archived
        .retain(|board, _| live.contains(board));
    st.documents.retain(|board, _| live.contains(board));

    let tables = EntityTables {
        tasks: &persisted.tasks,
        stacks: &persisted.stacks,
        labels: &persisted.labels,
        boards: &persisted.boards,
    };
    // Bound cards' (archived, remote_seen), for the listing-lag detail
    // refresh (tier-2 verified: Deck's listings lag its cache, so an
    // archive/unarchive can carry a stale version stamp that LWW would
    // silently drop).
    let local_cards: LocalCardStamps = persisted
        .tasks
        .values()
        .filter_map(|t| {
            let r = t.remote.as_ref()?;
            Some(((r.board, r.card), (t.archived, t.remote_seen)))
        })
        .collect();

    // The board-less outbox ops (owning entity rows wiped) are cycle-level
    // bookkeeping: they ride the *first* owned board's planning pass, once
    // per cycle — the planner synthetically completes no-ops for missing
    // entities (D-13). Empty set ⇒ they strand unattempted.
    let orphan_ops: Vec<taskboard_domain::PendingOp> = persisted
        .outbox
        .iter()
        .filter(|entry| entry.op.owning_board(&tables).is_none())
        .cloned()
        .collect();

    let was_offline = st.offline;
    let mut any_completed = false;
    for (index, (board_id, binding)) in owned.iter().enumerate() {
        let target = binding.board;
        tracing::info!(board = target.get(), connector = %id, "board sync started");
        let mut board_ops: Vec<taskboard_domain::PendingOp> = persisted
            .outbox
            .iter()
            .filter(|entry| entry.op.owning_board(&tables) == Some(*board_id))
            .cloned()
            .collect();
        if index == 0 {
            board_ops.extend(orphan_ops.iter().cloned());
        }

        // ---- push (before pull: the R3 hazard, ADR 0007) ---------------
        let mut pushes: Vec<PushOutcome> = Vec::new();
        let mut deferred_flush: Option<taskboard_domain::PushGroup> = None;
        if !board_ops.is_empty() {
            let index = taskboard_domain::RemoteIndex::from_bindings(
                persisted
                    .tasks
                    .iter()
                    .filter_map(|(id, t)| Some((t.remote?, *id))),
                persisted
                    .stacks
                    .iter()
                    .filter_map(|(id, s)| Some((s.remote?, *id))),
                persisted
                    .labels
                    .iter()
                    .filter_map(|(id, l)| Some((l.remote?, *id))),
            );
            // Board-scoped overlay: refs of *other* boards must never be
            // visible here — `card_of` stamps the cycle's board id, and an
            // unfiltered overlay would mis-tag foreign refs.
            let overlay = BindingOverlay::from_index(target, &index);
            let mut executor = PushExecutor::new(client, overlay, st.features);
            for group in plan_pushes(&board_ops, tables) {
                // The flush executes post-pull (D-S4 item 4's position
                // ruling): document groups are stashed, never executed
                // in the push phase.
                if matches!(group.target, taskboard_domain::PushTarget::Document(_)) {
                    deferred_flush = Some(group);
                    continue;
                }
                match executor.execute(&group, &board_ops, tables).await {
                    GroupOutcome::Outcomes(mut outcomes) => pushes.append(&mut outcomes),
                    GroupOutcome::Aborted { landed } => {
                        // Transport failure: no pull — the snapshot would
                        // predate the unfinished pushes anyway. The landed
                        // outcomes travel as evidence; the episode edge
                        // fires once and the cycle ends (D-12).
                        pushes.extend(landed);
                        fail_cycle(
                            id,
                            st,
                            reports,
                            system,
                            SyncErrorKind::Network,
                            Duration::ZERO,
                            Some(target),
                            started,
                            pushes,
                            read_at,
                        )
                        .await;
                        return;
                    }
                }
            }
        }

        // ---- pull -------------------------------------------------------
        let local_board = persisted
            .boards
            .get(board_id)
            .map(|b| (b.title.clone(), b.color.as_str().to_owned()));
        let pull = pull_snapshot(
            client,
            target,
            &persisted.validators,
            &mut st.cache,
            local_board,
            &local_cards,
            cfg.clock.as_ref(),
        )
        .await;
        let pulled = match pull {
            Ok(pulled) => pulled,
            Err(err) => {
                let kind = classify_read(&err);
                let network = matches!(kind, SyncErrorKind::Network);
                fail_cycle(
                    id,
                    st,
                    reports,
                    system,
                    kind,
                    advertised_floor(&err),
                    Some(target),
                    started,
                    pushes,
                    read_at,
                )
                .await;
                if network {
                    // Shared transport down: remaining boards unattempted,
                    // no reports for them (go_offline stamps their rows).
                    return;
                }
                continue;
            }
        };

        // ---- sidecar evidence (D10): discovery + download ---------------
        // The snapshot and pushes are unaffected evidence: no download or
        // decode row below may fail the cycle (D-S7's invariant).
        let snapshot = pulled.snapshot;
        let validators = pulled.validators;
        let (app_data, discovery, _detail_attachments) = match sidecar_evidence(
            client,
            target,
            &snapshot,
            pulled.detail_attachments,
            st,
        )
        .await
        {
            Ok(read) => read,
            Err(err) => {
                let kind = classify_read(&err);
                let network = matches!(kind, SyncErrorKind::Network);
                fail_cycle(
                    id,
                    st,
                    reports,
                    system,
                    kind,
                    advertised_floor(&err),
                    Some(target),
                    started,
                    pushes,
                    read_at,
                )
                .await;
                if network {
                    return;
                }
                continue;
            }
        };

        // ---- flush (between pull and report, D-S4 item 4) ---------------
        // The ceremony's write arms wake exactly when the flush has work
        // (lazy, D-S1; zero sidecar cost otherwise, D-S8).
        let has_work = flush::has_flush_work(
            deferred_flush.as_ref(),
            &app_data,
            tables,
            &persisted.reminders.rules,
            &persisted.tracking.entries,
            read_at,
            *board_id,
            st.features.card_startdate,
        );
        if has_work {
            let action = anchor::anchor_action(&discovery, true);
            let read_evidence = &app_data;
            let attempt = !matches!(
                read_evidence,
                AppDataEvidence::Unreadable(
                    AppDataUnreadable::Transport
                        | AppDataUnreadable::Decode
                        | AppDataUnreadable::FormatTooNew { .. }
                )
            );
            if attempt {
                let verdict = flush::run_flush(flush::FlushCtx {
                    client,
                    target,
                    features: st.features,
                    read_at,
                    snapshot: &snapshot,
                    evidence: read_evidence,
                    action: &action,
                    group: deferred_flush.as_ref(),
                    outbox: &board_ops,
                    tasks: &persisted.tasks,
                    stacks: &persisted.stacks,
                    rules: &persisted.reminders.rules,
                    dismissals: &persisted.reminders.dismissals,
                    entries: &persisted.tracking.entries,
                })
                .await;
                match verdict {
                    flush::FlushVerdict::Idle => {
                        // Ops stay queued; the cycle still reports Completed.
                    }
                    flush::FlushVerdict::Applied {
                        outcomes,
                        stamp,
                        document,
                    } => {
                        pushes.extend(outcomes);
                        // The cache refresh: the next cycle must not
                        // re-GET our own write (D-S4 item 4).
                        st.documents.insert(
                            target,
                            CachedAppData {
                                last_modified: stamp.last_modified,
                                filesize: stamp.filesize,
                                document: *document,
                            },
                        );
                    }
                    flush::FlushVerdict::AppliedUncached { outcomes } => {
                        pushes.extend(outcomes);
                    }
                    flush::FlushVerdict::DeadLettered { primary } => {
                        pushes.push(PushOutcome {
                            op: primary,
                            result: PushResult::Rejected {
                                kind: SyncErrorKind::BadRequest,
                            },
                        });
                    }
                }
            }
        }

        // ---- report (one per board) -------------------------------------
        any_completed = true;
        let sent = send_report(
            reports,
            SyncReport::Completed {
                connector: id.clone(),
                snapshot: Box::new(snapshot),
                validators,
                pushes,
                read_at,
                duration: started.elapsed(),
                app_data,
                features: st.features,
            },
        )
        .await;
        if !sent {
            tracing::warn!("report dropped: engine report channel closed");
        }
        tracing::info!(board = target.get(), "board sync completed");
    }

    if any_completed {
        // Cycle-level success bookkeeping: the backoff resets only when at
        // least one board completed; the restore edge fires once, after
        // the report (the engine nudge lands on this actor's absorber).
        st.failure_streak = 0;
        if was_offline {
            // Report-then-restore: the Completed reports landed inside the
            // board loop above; the per-connector restore edge fires once,
            // in the cycle's closing block.
            let _ = system.send(SystemEvent::ConnectorRestored(id.clone()));
            tracing::info!(connector = %id, "connector restored");
        }
        st.offline = false;
    }
    tracing::info!("sync cycle completed");
}

/// The advertised `Retry-After` delay of a rate-limited failure, `ZERO`
/// when the failure carries no usable hint. (A 429 during *push* never
/// reaches here: `GroupOutcome::Aborted` swallows the `DeckError` — the
/// next cycle's pull meets the same limiter and installs the floor then.)
fn advertised_floor(err: &DeckError) -> Duration {
    match err {
        DeckError::RateLimited {
            retry_after: Some(d),
        } => *d,
        _ => Duration::ZERO,
    }
}

/// Transport-classified failures broadcast `ConnectorLost` for this
/// actor's own connector exactly once per offline episode and advance
/// the backoff; other kinds only back off.
#[allow(clippy::too_many_arguments)] // the cycle context is the argument list
async fn fail_cycle(
    id: &taskboard_domain::ConnectorId,
    st: &mut LoopState,
    reports: &mpsc::Sender<SyncReport>,
    system: &broadcast::Sender<SystemEvent>,
    kind: SyncErrorKind,
    floor: Duration,
    board: Option<taskboard_domain::RemoteBoardId>,
    started: std::time::Instant,
    pushes: Vec<PushOutcome>,
    read_at: chrono::DateTime<chrono::Utc>,
) {
    if matches!(kind, SyncErrorKind::Network) && !st.offline {
        let _ = system.send(SystemEvent::ConnectorLost(id.clone()));
        st.offline = true;
        tracing::warn!(connector = %id, "connector lost");
    }
    st.failure_streak = st.failure_streak.saturating_add(1);
    // Freshest hint wins: a stale advertisement never outlives the next
    // failure observation; success ignores the field entirely (streak 0
    // schedules `poll_interval`).
    st.retry_after_floor = floor;
    tracing::warn!(?kind, streak = st.failure_streak, "sync cycle failed");
    send_report(
        reports,
        SyncReport::Failed {
            connector: id.clone(),
            kind,
            pushes,
            read_at,
            board,
            duration: started.elapsed(),
            features: st.features,
        },
    )
    .await;
}

/// `None` when the reports channel closed (engine restart): the cycle's
/// work is discarded — the next cycle re-reads.
async fn send_report(reports: &mpsc::Sender<SyncReport>, report: SyncReport) -> bool {
    if reports.send(report).await.is_err() {
        tracing::warn!("report dropped: engine report channel closed");
        return false;
    }
    true
}

/// The per-board sidecar evidence (D10): scan the pulled snapshot for
/// exact-title anchor matches, fetch the missing card details (one GET
/// per unbound match — normally zero), apply the ceremony's action face,
/// and download the document only when the metadata pair says the
/// content changed (D11's cache). Success-only caching: only
/// `Document`-class evidence touches `LoopState.documents`.
#[allow(clippy::type_complexity)] // the triple is the read's products
async fn sidecar_evidence(
    client: &DeckClient,
    target: RemoteBoardId,
    snapshot: &RemoteBoardSnapshot,
    mut details: std::collections::HashMap<RemoteCardRef, Vec<crate::model::Attachment>>,
    st: &mut LoopState,
) -> Result<
    (
        AppDataEvidence,
        anchor::AnchorDiscovery,
        std::collections::HashMap<RemoteCardRef, Vec<crate::model::Attachment>>,
    ),
    crate::error::DeckError,
> {
    // A dead board's reads answer 404/403: the ceremony is skipped
    // entirely, evidence `Absent` (D10 item 1).
    if snapshot.board.deleted_at.is_some() {
        return Ok((
            AppDataEvidence::Absent,
            anchor::AnchorDiscovery::default(),
            details,
        ));
    }
    let mut discovery = anchor::scan_snapshot(snapshot, &details);
    for card_ref in discovery.unconfirmed.clone() {
        if details.contains_key(&card_ref) {
            continue;
        }
        match client
            .card(target.get(), card_ref.stack.get(), card_ref.card.get())
            .await
        {
            Ok(detail) => {
                details.insert(card_ref, detail.attachments);
            }
            // A vanished match keeps the discovery honest without failing
            // the cycle (the detail-refresh 404 carve-out's precedent).
            Err(crate::error::DeckError::NotFound | crate::error::DeckError::Forbidden) => {}
            Err(err) => return Err(err),
        }
    }
    discovery = anchor::scan_snapshot(snapshot, &details);
    let Some(candidate) = anchor::pick(&discovery).cloned() else {
        // No confirmed carrier: `Absent`, no download (the decision
        // table's silent state; the write arms wake only on flush work).
        return Ok((AppDataEvidence::Absent, discovery, details));
    };
    if candidate.tombstoned {
        return Ok((AppDataEvidence::Absent, discovery, details));
    }
    if let Some(cached) = st.documents.get(&target)
        && cached.last_modified == candidate.last_modified
        && cached.filesize == candidate.filesize
    {
        // Metadata equal ⇒ skip the content GET; re-folding the cached
        // document is idempotent (chunk 4's guarantee).
        return Ok((
            AppDataEvidence::Document(Box::new(cached.document.clone())),
            discovery,
            details,
        ));
    }
    match client
        .download_attachment(
            target.get(),
            candidate.card.stack.get(),
            candidate.card.card.get(),
            candidate.attachment_id,
        )
        .await
    {
        Ok(bytes) => {
            let evidence = anchor::build_evidence(&bytes, target);
            if let AppDataEvidence::Document(document) = &evidence {
                st.documents.insert(
                    target,
                    CachedAppData {
                        last_modified: candidate.last_modified,
                        filesize: candidate.filesize,
                        document: (**document).clone(),
                    },
                );
            }
            Ok((evidence, discovery, details))
        }
        Err(err) => {
            // The reason travels typed, never silence (the loud fence's
            // point); the snapshot and pushes are unaffected evidence.
            tracing::warn!(
                board = target.get(),
                ?err,
                "sidecar document download failed; folding nothing this cycle"
            );
            Ok((
                AppDataEvidence::Unreadable(AppDataUnreadable::Transport),
                discovery,
                details,
            ))
        }
    }
}

/// Assembles the complete board snapshot from three conditional reads
/// (decisions 9 and 11). All-or-nothing: any read failing after retries
/// fails the whole pull — never a partial snapshot.
#[allow(clippy::too_many_lines)] // three endpoints + confirm/synthesis, one order
#[allow(clippy::too_many_arguments)]
/// The pull's products: the complete snapshot, the conditional-read
/// validators, and the attachment metadata captured from every bound
/// card's detail refresh (D10 item 1 — zero marginal GETs).
struct Pulled {
    snapshot: RemoteBoardSnapshot,
    validators: BoardPullValidators,
    detail_attachments: std::collections::HashMap<RemoteCardRef, Vec<crate::model::Attachment>>,
}

#[allow(clippy::too_many_lines)] // three endpoints + confirm/synthesis + detail capture, one order
async fn pull_snapshot(
    client: &DeckClient,
    target: RemoteBoardId,
    persisted: &std::collections::BTreeMap<ValidatorKey, SyncValidators>,
    cache: &mut PullCache,
    local_board: Option<(String, String)>,
    local_cards: &LocalCardStamps,
    clock: &dyn Clock,
) -> Result<Pulled, crate::error::DeckError> {
    let boards_v = persisted
        .get(&ValidatorKey::Boards)
        .map(|v| Validators {
            etag: v.etag.clone(),
            last_modified: v.last_modified.clone(),
        })
        .unwrap_or_default();
    let boards_raw = client.fetch_boards(&boards_v).await?;
    let (boards_data, boards_val) = fill(
        cache.boards.take(),
        boards_raw,
        client.fetch_boards(&Validators::default()),
    )
    .await?;
    let listed = boards_data
        .as_ref()
        .and_then(|boards| boards.iter().find(|b| b.id == target.get()).cloned());
    cache.boards = boards_data;

    // Board absence from the listing is a tombstone, confirmed in a second
    // step (decision 11) — a filtered listing must never cascade-destruct.
    let (board, labels, dead_board) = match listed {
        Some(listed_board) => {
            // The listing entry is only a hint; labels and board content
            // are authoritative in the board *detail* payload (tier-2
            // verified: the boards listing carries `labels: []` even for
            // labelled boards on Nextcloud 35).
            match client.board(target.get()).await {
                Ok(detail) => (
                    mapping::map_board(&detail),
                    inline_labels(&detail),
                    !detail.is_live(),
                ),
                Err(err)
                    if matches!(
                        err,
                        crate::error::DeckError::NotFound | crate::error::DeckError::Forbidden
                    ) =>
                {
                    // A listed-but-unreadable board is dead (tier-2
                    // verified: a soft-deleted board's detail answers 403
                    // while the lagging listing may still claim it live).
                    tracing::debug!(
                        board = target.get(),
                        ?err,
                        "listed board unreadable: treating as dead"
                    );
                    // The delete observation is stamped "now": Deck does
                    // not advance `lastModified` on a soft delete (tier-2
                    // verified), so the listed stamps would lose every LWW
                    // comparison against the local board's `remote_seen`
                    // and the tombstone could never be adopted. The
                    // deletion itself is the freshest fact we hold.
                    let mut dead = mapping::map_board(&listed_board);
                    dead.deleted_at = Some(clock.now());
                    dead.last_modified = dead.last_modified.max(clock.now());
                    (dead, inline_labels(&listed_board), true)
                }
                Err(err) => return Err(err),
            }
        }
        None => match client.board(target.get()).await {
            Ok(board) => (
                mapping::map_board(&board),
                inline_labels(&board),
                !board.is_live(),
            ),
            Err(err @ (crate::error::DeckError::NotFound | crate::error::DeckError::Forbidden)) => {
                tracing::warn!(
                    board = target.get(),
                    ?err,
                    "board gone: synthesizing tombstone"
                );
                let (title, color) = local_board.unwrap_or_else(|| (String::new(), String::new()));
                (
                    RemoteBoard {
                        id: target,
                        title,
                        color,
                        archived: false,
                        deleted_at: Some(clock.now()),
                        last_modified: clock.now(),
                    },
                    Vec::new(),
                    true,
                )
            }
            Err(err) => return Err(err),
        },
    };

    let stacks_v = |key: &ValidatorKey| {
        persisted
            .get(key)
            .map(|v| Validators {
                etag: v.etag.clone(),
                last_modified: v.last_modified.clone(),
            })
            .unwrap_or_default()
    };
    let (active, active_v) = fetch_stacks_listing(
        client,
        target,
        StackFilter::Active,
        stacks_v(&ValidatorKey::Stacks(target)),
        cache.stacks_active.entry(target).or_default(),
        dead_board,
    )
    .await?;
    let (archived, archived_v) = fetch_stacks_listing(
        client,
        target,
        StackFilter::Archived,
        stacks_v(&ValidatorKey::ArchivedStacks(target)),
        cache.stacks_archived.entry(target).or_default(),
        dead_board,
    )
    .await?;

    // Merge both listings: the archived entries carry `archived = true`;
    // a card listed live wins over its archived-listing twin.
    let mut stacks: Vec<RemoteStack> = Vec::new();
    let mut tasks: Vec<RemoteTask> = Vec::new();
    let mut seen_cards: std::collections::BTreeSet<taskboard_domain::RemoteCardId> =
        std::collections::BTreeSet::new();
    for stack in &active {
        stacks.push(mapping::map_stack(stack, target));
        for card in &stack.cards {
            tasks.push(mapping::map_card(card, target));
            seen_cards.insert(taskboard_domain::RemoteCardId(card.id));
        }
    }
    for stack in &archived {
        if !stacks.iter().any(|s| s.id.stack == RemoteStackId(stack.id)) {
            stacks.push(mapping::map_stack(stack, target));
        }
        for card in &stack.cards {
            if seen_cards.insert(taskboard_domain::RemoteCardId(card.id)) {
                let mut view = mapping::map_card(card, target);
                view.archived = true;
                tasks.push(view);
            }
        }
    }

    // Detail refresh for every *bound* card (tier-2 verified on Nextcloud
    // 35: an archive/unarchive does not advance the card's last_modified —
    // no stamp-based rule can ever observe the flag — and the listings lag
    // Deck's cache besides). The authoritative per-card detail settles the
    // flag; one GET per bound card per cycle is negligible at kiosk scale.
    // Unbound (freshly echoed) cards keep the listing view. Per-card
    // *conditional* detail fetches are the scaling path (backlog).
    //
    // When the flag disagrees with the local one, the view is stamped with
    // the observation time: the server offers no version stamp for the
    // archive flag, so "observed now" is the only truthful recency and
    // LWW adopts it (same reasoning as the synthesized dead-board stamp).
    //
    // All-or-nothing: a bound card whose authoritative detail cannot be
    // read fails the whole cycle — reporting the stale listing view would
    // merge its stamp first and lock the truth out of LWW forever. A 404
    // detail keeps the listing view (read-your-writes lag; presence is
    // re-observed next cycle).
    //
    // A dead board skips the refresh entirely: every read of a dead board
    // answers 404/403 (tier-2 verified), the tombstone snapshot is valid
    // from the empty listings alone, and per-card details would fail the
    // cycle forever — the cascade could never land.
    let mut detail_attachments: std::collections::HashMap<
        RemoteCardRef,
        Vec<crate::model::Attachment>,
    > = std::collections::HashMap::new();
    if !dead_board {
        for view in &mut tasks {
            let Some(&(local_archived, _)) = local_cards.get(&(view.id.board, view.id.card)) else {
                continue;
            };
            match client
                .card(target.get(), view.id.stack.get(), view.id.card.get())
                .await
            {
                Ok(detail) => {
                    detail_attachments.insert(view.id, detail.attachments.clone());
                    let mut refreshed = mapping::map_card(&detail, target);
                    if refreshed.archived != local_archived {
                        refreshed.last_modified = refreshed.last_modified.max(clock.now());
                    }
                    *view = refreshed;
                }
                Err(crate::error::DeckError::NotFound) => {}
                Err(err) => return Err(err),
            }
        }
    }

    let validators = BoardPullValidators {
        boards: to_sync(&boards_val),
        stacks: to_sync(&active_v),
        archived_stacks: to_sync(&archived_v),
    };
    let snapshot = RemoteBoardSnapshot {
        board,
        stacks,
        tasks,
        labels,
    };
    Ok(Pulled {
        snapshot,
        validators,
        detail_attachments,
    })
}

/// One stacks listing with cache fill and the dead-board 404 carve-out
/// (all three reads of a dead board are expected to 404 — empty listings
/// make the tombstone snapshot valid, decision 11).
async fn fetch_stacks_listing(
    client: &DeckClient,
    target: RemoteBoardId,
    filter: StackFilter,
    validators: Validators,
    cache_slot: &mut Option<Vec<Stack>>,
    dead_board: bool,
) -> Result<(Vec<Stack>, Validators), crate::error::DeckError> {
    let fetched = match client.fetch_stacks(target.get(), filter, &validators).await {
        Ok(fetched) => fetched,
        Err(crate::error::DeckError::NotFound | crate::error::DeckError::Forbidden)
            if dead_board =>
        {
            // All reads of a dead board answer 404/403 (tier-2 verified):
            // empty listings make the tombstone snapshot valid.
            tracing::debug!(board = target.get(), ?filter, "dead board: empty listing");
            return Ok((Vec::new(), Validators::default()));
        }
        Err(err) => return Err(err),
    };
    let (data, validators) = fill(
        cache_slot.take(),
        fetched,
        client.fetch_stacks(target.get(), filter, &Validators::default()),
    )
    .await?;
    cache_slot.clone_from(&data);
    Ok((data.unwrap_or_default(), validators))
}

/// Conditional fetch with cache fill: `304` resolves from the in-memory
/// cache; a cache miss forces one unconditional refetch (warmth rule).
async fn fill<T>(
    cached: Option<T>,
    fetched: Fetch<T>,
    refetch: impl Future<Output = Result<Fetch<T>, crate::error::DeckError>>,
) -> Result<(Option<T>, Validators), crate::error::DeckError>
where
    T: Clone,
{
    if let Some(data) = fetched.data {
        return Ok((Some(data), fetched.validators));
    }
    let Some(cached) = cached else {
        tracing::warn!("304 with a cold cache: unconditional refetch");
        let fresh = refetch.await?;
        return Ok((fresh.data, fresh.validators));
    };
    tracing::debug!("conditional read hit the cache (304)");
    Ok((Some(cached), fetched.validators))
}

fn inline_labels(board: &Board) -> Vec<RemoteLabel> {
    board
        .labels
        .iter()
        .map(|label: &Label| mapping::map_label(label, RemoteBoardId(board.id)))
        .collect()
}

fn to_sync(validators: &Validators) -> SyncValidators {
    SyncValidators {
        etag: validators.etag.clone(),
        last_modified: validators.last_modified.clone(),
    }
}

#[cfg(test)]
mod guard_tests {
    use super::*;
    use std::collections::BTreeMap;

    fn board(
        id: u128,
        connector: Option<&str>,
        remote: Option<u64>,
        deleted: bool,
    ) -> taskboard_domain::Board {
        taskboard_domain::Board {
            id: taskboard_domain::BoardId::from(uuid::Uuid::from_u128(id)),
            remote: remote.map(|board| taskboard_domain::RemoteBoardBinding {
                connector: taskboard_domain::ConnectorId::from(connector.unwrap_or("nextcloud")),
                board: RemoteBoardId(board),
            }),
            title: "board".into(),
            color: taskboard_domain::entities::Color::new("0000ff"),
            archived: false,
            deleted,
            clocks: taskboard_domain::entities::BoardClocks {
                title: taskboard_domain::entities::BoardClocks::epoch(),
                color: taskboard_domain::entities::BoardClocks::epoch(),
                archived: taskboard_domain::entities::BoardClocks::epoch(),
                deleted: taskboard_domain::entities::BoardClocks::epoch(),
            },
            remote_seen: None,
        }
    }

    fn boards(
        rows: &[taskboard_domain::Board],
    ) -> BTreeMap<taskboard_domain::BoardId, taskboard_domain::Board> {
        rows.iter().map(|b| (b.id, b.clone())).collect()
    }

    fn owned_remote(rows: &[taskboard_domain::Board]) -> Vec<u64> {
        owned_boards(
            &taskboard_domain::ConnectorId::from("nextcloud"),
            &boards(rows),
        )
        .into_iter()
        .map(|(_, binding)| binding.board.get())
        .collect()
    }

    #[test]
    fn finalized_tombstone_drops_out_of_the_set() {
        // After the finalize (binding cleared), the tombstone no longer
        // holds a binding: the derived set drops it next cycle.
        assert!(
            owned_remote(&[board(1, None, None, true)]).is_empty(),
            "a finalized tombstone has no binding to derive from"
        );
    }

    #[test]
    fn tombstoned_board_stays_in_the_set() {
        // R9c drain: a deleting board's binding survives until its
        // tombstone push finalizes — the derived set keeps it so the
        // delete drains through the owning connector.
        assert_eq!(owned_remote(&[board(1, None, Some(7), true)]), vec![7]);
    }

    #[test]
    fn fresh_actor_derives_the_set_from_persisted_bindings() {
        // Decision 13: a restarted actor targets the persisted binding
        // with no message at all.
        assert_eq!(owned_remote(&[board(1, None, Some(3), false)]), vec![3]);
    }

    #[test]
    fn foreign_tagged_boards_are_excluded() {
        // The N=2 predicate, testable at N=1 with a hand-tagged fixture:
        // another connector's board is not this actor's to sync.
        let rows = [
            board(1, None, Some(3), false),
            board(2, Some("other"), Some(9), false),
        ];
        assert_eq!(owned_remote(&rows), vec![3]);
    }

    #[test]
    fn the_set_is_in_board_id_order() {
        let first = board(1, None, Some(30), false);
        let second = board(2, None, Some(10), false);
        // Board ids in insertion (ordered-map) order, remote ids reversed:
        // the cycle walks board-id order, not remote-id order.
        assert_eq!(owned_remote(&[first, second]), vec![30, 10]);
    }
}
