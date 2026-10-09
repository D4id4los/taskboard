// SPDX-License-Identifier: MIT OR Apache-2.0
//! The sync actor: one scheduled cycle — read (via the engine's
//! `SyncStateReader` port) → push (coalesced, dependency-ordered,
//! fetch-before-write) → pull (three conditional reads into a complete
//! `RemoteBoardSnapshot`) → report (evidence-then-verdict), with the
//! offline finite state machine around it (ADR 0007).
//!
//! The actor is stateless across restarts except the injected client: all
//! durable facts arrive via `read_state`; the in-memory pull cache is a
//! rebuildable optimization. Network events are *produced* here
//! (`SystemEvent::NetworkLost`/`NetworkRestored` on the injected
//! broadcast) and `Shutdown` is the only event consumed from it — this
//! actor is the network authority.

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use tokio::sync::{broadcast, mpsc};
use tokio::task::JoinHandle;

use taskboard_domain::{
    BoardPullValidators, EntityTables, PushOutcome, RemoteBoard, RemoteBoardId,
    RemoteBoardSnapshot, RemoteLabel, RemoteStack, RemoteStackId, RemoteTask, SyncCommand,
    SyncErrorKind, SyncReport, SyncStateReader, SyncValidators, SystemEvent, ValidatorKey,
    plan_pushes,
};

use crate::client::{DeckClient, Fetch, Validators};
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
}

/// Spawns the sync actor onto the current runtime.
///
/// All channel halves are injected (the bootstrap owns the graph, mirroring
/// [`taskboard_domain::TaskRepository`]-style seams): `commands` is the
/// engine's nudge/SetBoard inbox, `reports` carries cycle reports to the
/// engine, `system` is the broadcast this actor publishes network events
/// on (and subscribes to for `Shutdown`).
#[allow(clippy::too_many_arguments)] // the injected seams ARE the constructor
#[must_use]
pub fn spawn_sync_actor(
    client: DeckClient,
    state: Arc<dyn SyncStateReader>,
    commands: mpsc::Receiver<SyncCommand>,
    reports: mpsc::Sender<SyncReport>,
    system: broadcast::Sender<SystemEvent>,
    cfg: SyncActorConfig,
) -> JoinHandle<()> {
    tokio::spawn(run(client, state, commands, reports, system, cfg))
}

/// In-memory cache of the last decoded listings, fill for `304`s and
/// cleared on `SetBoard`. A cold cache with warm validators forces one
/// unconditional refetch — never a partial snapshot (decision 9).
#[derive(Debug, Default)]
struct PullCache {
    boards: Option<Vec<Board>>,
    stacks_active: Option<Vec<Stack>>,
    stacks_archived: Option<Vec<Stack>>,
}

impl PullCache {
    fn clear(&mut self) {
        *self = Self::default();
    }
}

/// The loop's mutable state (ADR 0007 decision 12).
struct LoopState {
    target: Option<RemoteBoardId>,
    cache: PullCache,
    rerun_requested: bool,
    failure_streak: u32,
    offline: bool,
}

#[allow(
    clippy::single_match_else, // select! arms read best as match arms
    clippy::too_many_lines // one biased loop; the arms ARE the design
)]
async fn run(
    client: DeckClient,
    state: Arc<dyn SyncStateReader>,
    mut commands: mpsc::Receiver<SyncCommand>,
    reports: mpsc::Sender<SyncReport>,
    system: broadcast::Sender<SystemEvent>,
    cfg: SyncActorConfig,
) {
    tracing::info!("sync actor started");
    let mut sys_rx = system.subscribe();
    let mut sys_open = true;
    let mut st = LoopState {
        target: None,
        cache: PullCache::default(),
        rerun_requested: false,
        failure_streak: 0,
        offline: false,
    };
    // First cycle fires immediately; without a bound target it is a
    // silent skip (no `NoBoard` report on poll ticks — decision 10).
    let mut deadline = tokio::time::Instant::now();

    loop {
        tokio::select! {
            biased;

            cmd = commands.recv() => match cmd {
                Some(SyncCommand::SetBoard(id)) => {
                    if st.target == Some(id) {
                        continue; // re-SetBoard to the same id: no-op
                    }
                    st.target = Some(id);
                    st.cache.clear();
                    cycle(&client, &state, &reports, &system, &mut st, &cfg, true).await;
                    deadline = reschedule(&st, &cfg);
                }
                Some(SyncCommand::SyncNow) => {
                    cycle(&client, &state, &reports, &system, &mut st, &cfg, true).await;
                    deadline = reschedule(&st, &cfg);
                }
                None => {
                    tracing::info!("sync actor stopping: command inbox closed");
                    return;
                }
            },

            () = tokio::time::sleep_until(deadline) => {
                cycle(&client, &state, &reports, &system, &mut st, &cfg, false).await;
                deadline = reschedule(&st, &cfg);
            },

            event = sys_rx.recv(), if sys_open => match event {
                Ok(SystemEvent::Shutdown) => {
                    tracing::info!("sync actor stopping: shutdown event");
                    return;
                }
                Ok(SystemEvent::NetworkLost | SystemEvent::NetworkRestored) => {
                    // This actor *is* the network authority; events from
                    // elsewhere are ignored.
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
                SyncCommand::SetBoard(id) if st.target != Some(id) => {
                    st.target = Some(id);
                    st.cache.clear();
                    st.rerun_requested = true;
                }
                SyncCommand::SetBoard(_) => {}
            }
        }
        if st.rerun_requested {
            st.rerun_requested = false;
            deadline = tokio::time::Instant::now();
        }
    }
}

/// The next wake-up: `poll_interval` after success, the failure backoff
/// after a failure streak.
fn reschedule(st: &LoopState, cfg: &SyncActorConfig) -> tokio::time::Instant {
    let delay = if st.failure_streak == 0 {
        cfg.poll_interval
    } else {
        poll_backoff(st.failure_streak, cfg.backoff_initial, cfg.backoff_max)
    };
    tokio::time::Instant::now() + delay
}

/// One cycle. `commanded` distinguishes nudges/`SetBoard` (which report
/// `NoBoard`) from poll ticks (which silently skip without one).
#[allow(clippy::too_many_lines)] // the cycle IS the design; splitting hides the order
async fn cycle(
    client: &DeckClient,
    state: &Arc<dyn SyncStateReader>,
    reports: &mpsc::Sender<SyncReport>,
    system: &broadcast::Sender<SystemEvent>,
    st: &mut LoopState,
    _cfg: &SyncActorConfig,
    commanded: bool,
) {
    let Some(target) = st.target else {
        if commanded {
            tracing::debug!("cycle skipped: no board bound");
            send_report(
                reports,
                SyncReport::Failed {
                    kind: SyncErrorKind::NoBoard,
                    pushes: Vec::new(),
                },
            )
            .await;
        }
        return;
    };
    tracing::info!(board = target.get(), "sync cycle started");

    // ---- read ----------------------------------------------------------
    let persisted = match state.read_state().await {
        Ok(persisted) => persisted,
        Err(err) => {
            // Engine mid-restart (or a corrupted store): back off, no
            // report — the engine re-nudges or the poll tick retries.
            tracing::error!(?err, "cycle aborted: engine state unreadable");
            st.failure_streak += 1;
            return;
        }
    };
    let tables = EntityTables {
        tasks: &persisted.tasks,
        stacks: &persisted.stacks,
        labels: &persisted.labels,
    };
    let bound_board = persisted.boards.values().find(|b| b.remote == Some(target));

    // ---- push (before pull: the R3 hazard, ADR 0007) -------------------
    let mut pushes: Vec<PushOutcome> = Vec::new();
    if !persisted.outbox.is_empty() {
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
        let overlay = BindingOverlay::from_index(target, &index);
        let mut executor = PushExecutor::new(client, overlay);
        for group in plan_pushes(&persisted.outbox, tables) {
            match executor.execute(&group, &persisted.outbox, tables).await {
                GroupOutcome::Outcomes(mut outcomes) => pushes.append(&mut outcomes),
                GroupOutcome::Aborted => {
                    // Transport failure: no pull — the snapshot would
                    // predate the unfinished pushes anyway.
                    fail_cycle(st, reports, system, SyncErrorKind::Network, pushes).await;
                    return;
                }
            }
        }
    }

    // ---- pull ----------------------------------------------------------
    let local_board = bound_board.map(|b| (b.title.clone(), b.color.as_str().to_owned()));
    let pull = pull_snapshot(
        client,
        target,
        &persisted.validators,
        &mut st.cache,
        local_board,
    )
    .await;
    let (snapshot, validators) = match pull {
        Ok(pulled) => pulled,
        Err(err) => {
            let kind = classify_read(&err);
            fail_cycle(st, reports, system, kind, pushes).await;
            return;
        }
    };

    // ---- report --------------------------------------------------------
    let was_offline = st.offline;
    st.failure_streak = 0;
    let sent = send_report(
        reports,
        SyncReport::Completed {
            snapshot: Box::new(snapshot),
            validators,
            pushes,
        },
    )
    .await;
    if sent && was_offline {
        // Report first, then the restore broadcast: the resulting engine
        // nudge lands on this actor's dedupe absorber (one benign
        // coalesced rerun, ADR 0007).
        let _ = system.send(SystemEvent::NetworkRestored);
        tracing::info!("network restored");
    }
    st.offline = false;
    tracing::info!("sync cycle completed");
}

/// Transport-classified failures broadcast `NetworkLost` exactly once per
/// offline episode and advance the backoff; other kinds only back off.
async fn fail_cycle(
    st: &mut LoopState,
    reports: &mpsc::Sender<SyncReport>,
    system: &broadcast::Sender<SystemEvent>,
    kind: SyncErrorKind,
    pushes: Vec<PushOutcome>,
) {
    if matches!(kind, SyncErrorKind::Network) && !st.offline {
        let _ = system.send(SystemEvent::NetworkLost);
        st.offline = true;
        tracing::warn!("network lost");
    }
    st.failure_streak = st.failure_streak.saturating_add(1);
    tracing::warn!(?kind, streak = st.failure_streak, "sync cycle failed");
    send_report(reports, SyncReport::Failed { kind, pushes }).await;
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

/// Assembles the complete board snapshot from three conditional reads
/// (decisions 9 and 11). All-or-nothing: any read failing after retries
/// fails the whole pull — never a partial snapshot.
#[allow(clippy::too_many_lines)] // three endpoints + confirm/synthesis, one order
async fn pull_snapshot(
    client: &DeckClient,
    target: RemoteBoardId,
    persisted: &std::collections::BTreeMap<ValidatorKey, SyncValidators>,
    cache: &mut PullCache,
    local_board: Option<(String, String)>,
) -> Result<(RemoteBoardSnapshot, BoardPullValidators), crate::error::DeckError> {
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
        Some(board) => {
            let dead = !board.is_live();
            (mapping::map_board(&board), inline_labels(&board), dead)
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
                        deleted_at: Some(Utc::now()),
                        last_modified: Utc::now(),
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
        &mut cache.stacks_active,
        dead_board,
    )
    .await?;
    let (archived, archived_v) = fetch_stacks_listing(
        client,
        target,
        StackFilter::Archived,
        stacks_v(&ValidatorKey::ArchivedStacks(target)),
        &mut cache.stacks_archived,
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
    Ok((snapshot, validators))
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
        Err(crate::error::DeckError::NotFound) if dead_board => {
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
