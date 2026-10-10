// SPDX-License-Identifier: MIT OR Apache-2.0
//! The State Engine actor: a sequential command loop over pure
//! transition functions, publishing immutable [`AppState`] snapshots
//! through `ArcSwap` and repaint triggers through `broadcast` (ADR 0006).
//!
//! Per message the loop reads the injected clock once, then — for
//! mutating work — plans the actions (domain), applies the batch to the
//! repository, and *only then* interprets the same batch into memory.
//! Storage failure therefore needs no rollback: memory never advanced.
//!
//! The engine owns its command inbox, its signal broadcaster, and the
//! `Arc<ArcSwap<AppState>>`. Construction *takes* the external channel
//! halves (sync reports, system events, sync commands) — the app
//! bootstrap owns the channel graph and keeps the matching halves.

use std::sync::Arc;

use arc_swap::ArcSwap;
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio::task::JoinHandle;

use taskboard_domain::{
    AppState, Clock, CommandOutcome, EngineSignal, IdGenerator, PersistedState, PersistenceAction,
    RepositoryError, StateCommand, SyncCommand, SyncPhase, SyncStateReader, SyncStatus,
    SystemEvent, TaskRepository, ValidatorKey, apply_push_report, apply_sync_report, plan_command,
};

use crate::core::EngineCore;
use crate::error::{EngineStartupError, ExecuteError};

/// Generous command buffer: UI dispatch is fire-and-forget `try_send`
/// (a panic in a sync UI callback is never correct), and a full buffer is
/// unreachable at human input rates.
const COMMAND_BUFFER: usize = 256;

const SIGNAL_BUFFER: usize = 64;

/// The engine inbox: channel-bearing envelopes owned by this crate, with
/// the domain payloads inside (payload/envelope split).
#[derive(Debug)]
pub enum EngineCommand {
    /// Execute one [`StateCommand`]; the optional reply is the command
    /// receipt (CLI `execute`). `None` is the fire-and-forget UI path.
    Execute {
        /// The user intent.
        command: StateCommand,
        /// Request-response seam; a received reply implies the batch (if
        /// any) is durable.
        reply: Option<oneshot::Sender<Result<CommandOutcome, ExecuteError>>>,
    },
    /// Durability barrier: replies once every previously accepted command
    /// was applied, interpreted, and published (FIFO ordering makes the
    /// empty body correct — it settles reply-less `dispatch`es).
    Flush {
        /// Reply when the command queue is settled.
        reply: oneshot::Sender<()>,
    },
    /// The sync actor's read envelope (the domain's `SyncStateReader`
    /// port). Processed in the sequential loop, so a reply reflects the
    /// settled post-interpret state — same durability story as `Flush`.
    /// May carry the memory-only `Syncing` transient; consumers must not
    /// branch on `sync.phase`.
    ReadState {
        /// The full persisted shape (outbox, bindings, validators).
        reply: oneshot::Sender<Result<PersistedState, RepositoryError>>,
    },
}

/// Cloneable handle to a running State Engine. Readers never need this
/// crate: the UI consumes [`EngineHandle::shared_state`] and
/// [`EngineHandle::subscribe`], both domain-typed.
#[derive(Debug, Clone)]
pub struct EngineHandle {
    commands: mpsc::Sender<EngineCommand>,
    signals: broadcast::Sender<EngineSignal>,
    state: Arc<ArcSwap<AppState>>,
}

impl EngineHandle {
    /// Executes a command and awaits its receipt. The receipt implies
    /// durability (the loop replies only after `repo.apply` succeeded).
    ///
    /// # Errors
    ///
    /// [`ExecuteError::Rejected`] for semantic rejections,
    /// [`ExecuteError::Storage`] when the persistence batch failed (the
    /// published state is unchanged), [`ExecuteError::EngineGone`] when
    /// the engine loop is stopped.
    pub async fn execute(&self, command: StateCommand) -> Result<CommandOutcome, ExecuteError> {
        let (reply, rx) = oneshot::channel();
        self.commands
            .send(EngineCommand::Execute {
                command,
                reply: Some(reply),
            })
            .await
            .map_err(|_| ExecuteError::EngineGone)?;
        rx.await.map_err(|_| ExecuteError::EngineGone)?
    }

    /// Fire-and-forget dispatch for sync UI callbacks: never blocks, never
    /// panics. A full or closed inbox is logged and the command dropped —
    /// unreachable at human input rates.
    pub fn dispatch(&self, command: StateCommand) {
        let result = self.commands.try_send(EngineCommand::Execute {
            command,
            reply: None,
        });
        match result {
            Ok(()) => {}
            Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
                tracing::error!("command dropped: engine inbox full");
            }
            Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                tracing::error!("command dropped: engine stopped");
            }
        }
    }

    /// Durability barrier: resolves once every previously sent command
    /// (including reply-less `dispatch`es) was applied, interpreted, and
    /// published. The CLI exit path awaits this.
    pub async fn flush(&self) {
        let (reply, rx) = oneshot::channel();
        if self
            .commands
            .send(EngineCommand::Flush { reply })
            .await
            .is_err()
        {
            return; // engine gone; nothing left to wait for
        }
        let _ = rx.await;
    }

    /// Subscribes to state-change signals (broadcast).
    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<EngineSignal> {
        self.signals.subscribe()
    }

    /// The UI-facing, lock-free read side.
    #[must_use]
    pub fn shared_state(&self) -> Arc<ArcSwap<AppState>> {
        Arc::clone(&self.state)
    }
}

/// The sync actor's read port over the engine envelope (ADR 0005: sync
/// reads the outbox/bindings/validators *via* the engine, never direct
/// SQL). A stopped engine surfaces as
/// [`RepositoryError::Unavailable`] — the port's transient error class,
/// mirroring `execute`'s `EngineGone` semantics.
impl SyncStateReader for EngineHandle {
    fn read_state(
        &self,
    ) -> taskboard_domain::BoxFuture<'_, Result<PersistedState, RepositoryError>> {
        Box::pin(async {
            let (reply, rx) = oneshot::channel();
            self.commands
                .send(EngineCommand::ReadState { reply })
                .await
                .map_err(|_| RepositoryError::Unavailable)?;
            rx.await.map_err(|_| RepositoryError::Unavailable)?
        })
    }
}

/// Spawns the State Engine onto the current runtime.
///
/// Boot hydrates from the repository and swaps the initial [`AppState`]
/// into the `ArcSwap` *before* the loop starts (the UI paints instantly
/// from cache; no boot signal — subscribers cannot have existed yet).
///
/// # Errors
///
/// [`EngineStartupError::Load`] when the boot hydration failed; the
/// engine is not started in that case.
#[allow(clippy::too_many_arguments)] // the injected seams ARE the constructor
pub async fn spawn_state_engine(
    repo: Arc<dyn TaskRepository>,
    ids: Arc<dyn IdGenerator>,
    clock: Arc<dyn Clock>,
    sync_out: mpsc::Sender<SyncCommand>,
    sync_reports: mpsc::Receiver<taskboard_domain::SyncReport>,
    system: broadcast::Receiver<SystemEvent>,
) -> Result<(EngineHandle, JoinHandle<()>), EngineStartupError> {
    let persisted = repo.load().await.map_err(EngineStartupError::Load)?;
    // Log the counts straight off the hydrated data — never clone the
    // whole state just to print its sizes.
    tracing::info!(
        boards = persisted.boards.len(),
        stacks = persisted.stacks.len(),
        tasks = persisted.tasks.len(),
        labels = persisted.labels.len(),
        outbox = persisted.outbox.len(),
        "state engine hydrated"
    );
    let core = EngineCore::from_persisted(persisted);

    let shared = Arc::new(ArcSwap::from_pointee(core.app()));
    let (commands, command_rx) = mpsc::channel(COMMAND_BUFFER);
    let (signals, _) = broadcast::channel(SIGNAL_BUFFER);
    let handle = EngineHandle {
        commands,
        signals: signals.clone(),
        state: Arc::clone(&shared),
    };
    let join = tokio::spawn(run_loop(
        core,
        EngineParts {
            repo,
            ids,
            clock,
            sync_out,
        },
        shared,
        signals,
        command_rx,
        sync_reports,
        system,
    ));
    Ok((handle, join))
}

/// The loop's injected seams, grouped so no helper takes more than a
/// couple of arguments.
struct EngineParts {
    repo: Arc<dyn TaskRepository>,
    ids: Arc<dyn IdGenerator>,
    clock: Arc<dyn Clock>,
    sync_out: mpsc::Sender<SyncCommand>,
}

#[allow(
        clippy::single_match_else, // select! arms read best as match arms
        clippy::too_many_lines // one biased loop; the arms ARE the design
)]
async fn run_loop(
    mut core: EngineCore,
    parts: EngineParts,
    shared: Arc<ArcSwap<AppState>>,
    signals: broadcast::Sender<EngineSignal>,
    mut commands: mpsc::Receiver<EngineCommand>,
    mut sync_reports: mpsc::Receiver<taskboard_domain::SyncReport>,
    mut system: broadcast::Receiver<SystemEvent>,
) {
    let EngineParts {
        repo,
        ids,
        clock,
        sync_out,
    } = parts;
    let mut commands_open = true;
    let mut reports_open = true;
    let mut system_open = true;

    while commands_open || reports_open || system_open {
        tokio::select! {
            biased; // deterministic drain order: commands, reports, system

            maybe_command = commands.recv(), if commands_open => match maybe_command {
                Some(EngineCommand::Execute { command, reply }) => {
                    execute_command(
                        &mut core,
                        &shared,
                        &signals,
                        (&repo, ids.as_ref(), clock.as_ref(), &sync_out),
                        command,
                        reply,
                    )
                    .await;
                }
                Some(EngineCommand::Flush { reply }) => {
                    // FIFO: everything before this was applied, interpreted,
                    // and published.
                    let _ = reply.send(());
                }
                Some(EngineCommand::ReadState { reply }) => {
                    let _ = reply.send(Ok(core.persisted_view()));
                }
                None => commands_open = false,
            },

            maybe_report = sync_reports.recv(), if reports_open => match maybe_report {
                Some(report) => {
                    ingest_sync_report(
                        &mut core,
                        &shared,
                        &signals,
                        (&repo, ids.as_ref(), clock.as_ref()),
                        report,
                    )
                    .await;
                }
                None => {
                    // The sync actor died; the engine is a daemon and must
                    // not follow it.
                    tracing::warn!("sync report channel closed; continuing without sync");
                    reports_open = false;
                }
            },

            maybe_event = system.recv(), if system_open => match maybe_event {
                Ok(SystemEvent::Shutdown) => {
                    tracing::info!("state engine stopping: shutdown event");
                    return;
                }
                Ok(SystemEvent::NetworkLost) => {
                    go_offline(&mut core, &shared, &signals, &repo, clock.as_ref()).await;
                }
                Ok(SystemEvent::NetworkRestored) => {
                    // No phase change: the next report is the evidence; a
                    // self-declared "idle" would be a guess.
                    forward_sync_now(&sync_out);
                }
                Err(broadcast::error::RecvError::Closed) => system_open = false,
                Err(broadcast::error::RecvError::Lagged(missed)) => {
                    tracing::warn!(missed, "system event backlog overflowed");
                }
            },
        }
    }
    tracing::info!("state engine stopped: all inbox sources closed");
}

/// Publishes the core's projection and signals the repaint — swap first,
/// signal after, so a waking UI reads fresh state.
fn publish(
    core: &EngineCore,
    shared: &ArcSwap<AppState>,
    signals: &broadcast::Sender<EngineSignal>,
) {
    shared.store(Arc::new(core.app()));
    let _ = signals.send(EngineSignal::StateUpdated);
}

/// The seams tuple: `(repo, ids, clock, sync_out)`.
type Seams<'a> = (
    &'a Arc<dyn TaskRepository>,
    &'a dyn IdGenerator,
    &'a dyn Clock,
    &'a mpsc::Sender<SyncCommand>,
);

async fn execute_command(
    core: &mut EngineCore,
    shared: &Arc<ArcSwap<AppState>>,
    signals: &broadcast::Sender<EngineSignal>,
    (repo, ids, clock, sync_out): Seams<'_>,
    command: StateCommand,
    reply: Option<oneshot::Sender<Result<CommandOutcome, ExecuteError>>>,
) {
    tracing::debug!("command received");
    let now = clock.now();

    if let StateCommand::RequestSync = command {
        let forwarded = forward_sync_now(sync_out);
        let outcome = if forwarded {
            if core.mark_syncing() {
                publish(core, shared, signals);
            }
            CommandOutcome::SyncRequested
        } else {
            CommandOutcome::NoOp
        };
        if let Some(reply) = reply {
            let _ = reply.send(Ok(outcome));
        }
        return;
    }

    let plan = match plan_command(&core.app(), core.outbox(), &command, ids, now) {
        Ok(plan) => plan,
        Err(rejection) => {
            tracing::warn!(?rejection, "command rejected");
            if let Some(reply) = reply {
                let _ = reply.send(Err(ExecuteError::Rejected(rejection)));
            }
            return;
        }
    };

    if plan.actions.is_empty() {
        // Accepted no-op: nothing persisted, published, or signaled.
        if let Some(reply) = reply {
            let _ = reply.send(Ok(plan.outcome));
        }
        return;
    }

    if let Err(err) = repo.apply(plan.actions.clone()).await {
        // Memory never advanced (interpret runs only after a
        // successful apply) — no rollback needed.
        tracing::error!(?err, "persisted batch lost; state unchanged");
        if let Some(reply) = reply {
            let _ = reply.send(Err(ExecuteError::Storage(err)));
        }
        return;
    }
    let changed = core.interpret(&plan.actions, now);
    // Nudge iff the batch enqueued >= 1 op; a successful forward marks
    // the memory-only `Syncing` transient. One mutating message publishes
    // at most once: the entity change and the transient land in a single
    // swap + signal (the nudge is a channel send, safe before publish —
    // the report it triggers cannot be ingested until this message
    // finishes).
    let enqueued_ops = plan
        .actions
        .iter()
        .any(|action| matches!(action, PersistenceAction::EnqueueOp(_)));
    let marked_syncing = enqueued_ops && forward_sync_now(sync_out) && core.mark_syncing();
    if changed || marked_syncing {
        publish(core, shared, signals);
    }
    if let Some(reply) = reply {
        let _ = reply.send(Ok(plan.outcome));
    }
}

/// Fire-and-forget nudge. A dead sync actor is a `debug!` event — no sync
/// actor exists in engine-only tests, and the Phase 4 actor owns dedupe.
fn forward_sync_now(sync_out: &mpsc::Sender<SyncCommand>) -> bool {
    match sync_out.try_send(SyncCommand::SyncNow) {
        Ok(()) => {
            tracing::debug!("sync nudge forwarded");
            true
        }
        Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
            // The Phase 4 actor dedupes while a cycle runs; a full inbox
            // means a cycle is already pending — the nudge did its job.
            tracing::debug!("sync nudge coalesced: channel full");
            true
        }
        Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
            tracing::debug!("sync nudge dropped: no sync actor");
            false
        }
    }
}

async fn ingest_sync_report(
    core: &mut EngineCore,
    shared: &Arc<ArcSwap<AppState>>,
    signals: &broadcast::Sender<EngineSignal>,
    (repo, ids, clock): (&Arc<dyn TaskRepository>, &dyn IdGenerator, &dyn Clock),
    report: taskboard_domain::SyncReport,
) {
    tracing::debug!("sync report ingested");
    let now = clock.now();
    let actions = match report {
        taskboard_domain::SyncReport::Completed {
            snapshot,
            validators,
            pushes,
            read_at,
        } => {
            let (merged, mut batch) = apply_sync_report(
                &core.app(),
                core.outbox(),
                &snapshot,
                &pushes,
                read_at,
                ids,
                now,
            );
            append_status_if_changed(core, &mut batch, &merged.sync);
            // The binding may have been adopted by this very batch, so the
            // keys derive from the post-merge board — "its own binding" is
            // the engine's after it ingests the report.
            append_validators_if_changed(core, &mut batch, &validators, &merged);
            batch
        }
        taskboard_domain::SyncReport::Failed { kind, pushes, .. } if pushes.is_empty() => {
            vec![PersistenceAction::UpsertSyncStatus(SyncStatus {
                phase: SyncPhase::Failed { last_error: kind },
                last_success: core.sync_status().last_success,
                pending_ops: 0, // ignored: derived at apply time
            })]
        }
        // Evidence-then-verdict (phase 4 decision 3): the cycle's completed
        // pushes land even though the pull aborted — no snapshot merge, no
        // reconciliation, no cascade (that is `apply_push_report`'s job).
        taskboard_domain::SyncReport::Failed {
            pushes, read_at, ..
        } => {
            let (merged, mut batch) =
                apply_push_report(&core.app(), core.outbox(), &pushes, read_at, ids, now);
            append_status_if_changed(core, &mut batch, &merged.sync);
            batch
        }
    };

    if actions.is_empty() {
        // A no-change cycle (fast path, nothing merged): nothing to do.
        return;
    }

    match repo.apply(actions.clone()).await {
        Err(err) => {
            // Discard the report: state unchanged, the next cycle
            // re-delivers the same server truth.
            tracing::error!(?err, "sync batch lost; report discarded");
        }
        Ok(()) => {
            if core.interpret(&actions, now) {
                publish(core, shared, signals);
            }
        }
    }
}

/// Appends the status write only when the stable part (phase or
/// `last_success`) actually changed; `pending_ops` is derived, never stored.
fn append_status_if_changed(
    core: &EngineCore,
    batch: &mut Vec<PersistenceAction>,
    merged: &SyncStatus,
) {
    let current = core.sync_status();
    if merged.phase != current.phase || merged.last_success != current.last_success {
        batch.push(PersistenceAction::UpsertSyncStatus(merged.clone()));
    }
}

/// Appends one `UpsertValidators` per key derived from the report's
/// `BoardPullValidators`, keyed against the engine's own board binding
/// (phase 4 decision 4 — the actor never reasons about storage keys).
/// No-op when no board is bound or nothing differs from the stored values.
fn append_validators_if_changed(
    core: &EngineCore,
    batch: &mut Vec<PersistenceAction>,
    validators: &taskboard_domain::BoardPullValidators,
    merged: &AppState,
) {
    let Some(board) = merged
        .boards
        .values()
        .find(|b| b.remote.is_some())
        .and_then(|b| b.remote)
    else {
        return;
    };
    let incoming = [
        (ValidatorKey::Boards, &validators.boards),
        (ValidatorKey::Stacks(board), &validators.stacks),
        (
            ValidatorKey::ArchivedStacks(board),
            &validators.archived_stacks,
        ),
    ];
    for (key, value) in incoming {
        // An empty bundle carries nothing reusable and would wrongly clear
        // a warm key after a header-less 200; keep the stored one instead.
        if *value == taskboard_domain::SyncValidators::default() {
            continue;
        }
        if core.validators().get(&key) != Some(value) {
            batch.push(PersistenceAction::UpsertValidators(key, value.clone()));
        }
    }
}

async fn go_offline(
    core: &mut EngineCore,
    shared: &Arc<ArcSwap<AppState>>,
    signals: &broadcast::Sender<EngineSignal>,
    repo: &Arc<dyn TaskRepository>,
    clock: &dyn Clock,
) {
    // Idempotent: already-offline is a no-action no-op.
    if core.sync_status().phase == SyncPhase::Offline {
        return;
    }
    let now = clock.now();
    let actions = vec![PersistenceAction::UpsertSyncStatus(SyncStatus {
        phase: SyncPhase::Offline,
        last_success: core.sync_status().last_success,
        pending_ops: 0, // ignored: derived at apply time
    })];
    match repo.apply(actions.clone()).await {
        Err(err) => tracing::error!(?err, "offline transition lost"),
        Ok(()) => {
            if core.interpret(&actions, now) {
                publish(core, shared, signals);
            }
        }
    }
}
