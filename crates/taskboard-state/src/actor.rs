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
//! bootstrap owns the channel graph and keeps the matching halves. The
//! one exception is the system-event emission seam: the engine holds it
//! only as a `broadcast::WeakSender`, so the external producers own that
//! channel's lifetime — when the last of them dies, the engine's final
//! input closes and the all-sources-closed exit becomes reachable.

use std::sync::Arc;

use arc_swap::ArcSwap;
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio::task::JoinHandle;

use taskboard_domain::{
    AppState, Clock, CommandOutcome, EngineSignal, IdGenerator, Notification, PersistedState,
    PersistenceAction, ReminderFiring, RepositoryError, StateCommand, SyncCommand, SyncPhase,
    SyncStateReader, SyncStatus, SystemEvent, TaskRepository, ValidatorKey, apply_push_report,
    apply_sync_report, notifications_for, plan_command,
};

use crate::config::ReminderEngineConfig;
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
pub enum EngineEnvelope {
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
    /// Registers a connector's command sender with the nudge registry:
    /// push, never replace (the engine is connector-count-agnostic; the
    /// app layer owns the at-most-one guard). The reply settles after the
    /// push — FIFO over the sequential loop makes the ack the proof that
    /// every later nudge fans out to this sender (the `Flush` durability
    /// story).
    AttachConnector {
        /// The connector's `SyncCommand` inbox sender.
        sync_out: mpsc::Sender<SyncCommand>,
        /// Ack; see [`AttachReceipt`].
        reply: oneshot::Sender<AttachReceipt>,
    },
}

/// The engine's answer to a connector attach.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachReceipt {
    /// The sender is on the nudge registry.
    Attached,
    /// Declined: the engine is draining (post-`Shutdown`); the sender
    /// was not registered.
    Draining,
}

/// Typed attach-transport failures: the same event class
/// [`ExecuteError::EngineGone`] models for `execute`, kept separate
/// because attach is not a `StateCommand`.
#[derive(Debug, thiserror::Error)]
pub enum AttachError {
    /// The engine loop is stopped (or died before the ack).
    #[error("state engine unavailable")]
    EngineGone,
}

/// Cloneable handle to a running State Engine. Readers never need this
/// crate: the UI consumes [`EngineHandle::shared_state`] and
/// [`EngineHandle::subscribe`], both domain-typed.
#[derive(Debug, Clone)]
pub struct EngineHandle {
    commands: mpsc::Sender<EngineEnvelope>,
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
            .send(EngineEnvelope::Execute {
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
        let result = self.commands.try_send(EngineEnvelope::Execute {
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
            .send(EngineEnvelope::Flush { reply })
            .await
            .is_err()
        {
            return; // engine gone; nothing left to wait for
        }
        let _ = rx.await;
    }

    /// Registers a connector's command sender with the engine's nudge
    /// registry (push, never replace). The receipt settles after the push
    /// (FIFO): once it arrives, every later nudge fans out to this sender.
    ///
    /// # Errors
    ///
    /// [`AttachError::EngineGone`] when the engine loop is stopped (or
    /// died before the ack).
    pub async fn attach_connector(
        &self,
        sync_out: mpsc::Sender<SyncCommand>,
    ) -> Result<AttachReceipt, AttachError> {
        let (reply, rx) = oneshot::channel();
        self.commands
            .send(EngineEnvelope::AttachConnector { sync_out, reply })
            .await
            .map_err(|_| AttachError::EngineGone)?;
        rx.await.map_err(|_| AttachError::EngineGone)
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
                .send(EngineEnvelope::ReadState { reply })
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
/// The engine holds the system-event seam only as a
/// [`broadcast::WeakSender`]: the caller's (and any other external
/// producer's) strong sender owns that channel's lifetime, and when the
/// last of them drops, the engine exits.
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
    system_out: broadcast::Sender<SystemEvent>,
    system: broadcast::Receiver<SystemEvent>,
    notifier: mpsc::Sender<Notification>,
    reminder_cfg: ReminderEngineConfig,
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
            // Weak by design (#316): the engine must not pin its own
            // system channel open — external producers own its lifetime,
            // and their death is what makes the all-sources exit
            // reachable.
            system_out: system_out.downgrade(),
            notifier,
            reminders: reminder_cfg,
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
    system_out: broadcast::WeakSender<SystemEvent>,
    notifier: mpsc::Sender<Notification>,
    reminders: ReminderEngineConfig,
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
    mut commands: mpsc::Receiver<EngineEnvelope>,
    mut sync_reports: mpsc::Receiver<taskboard_domain::SyncReport>,
    mut system: broadcast::Receiver<SystemEvent>,
) {
    let EngineParts {
        repo,
        ids,
        clock,
        sync_out,
        system_out,
        notifier,
        reminders: reminder_cfg,
    } = parts;
    // Edge-trigger state for `StorageFailed`: the event fires on the
    // ok->fail transition (a dying disk fires once per failure epoch,
    // not once per command) and any successful persistence resets it.
    let mut storage_failing = false;
    // The nudge registry: one entry per registered connector. The boot
    // entry is the constructor's sender — under `standalone` its
    // receiver is dropped at boot, so the entry is dead by construction
    // and the first fan-out prunes it (nudges fail closed either way).
    let mut sync_outs = vec![sync_out];
    let mut commands_open = true;
    let mut reports_open = true;
    let mut system_open = true;
    // Drain mode (ADR 0008): after `SystemEvent::Shutdown` the engine
    // declines new work but keeps ingesting sync reports until the
    // reports channel closes — the sync actor owns the sender, so its
    // exit is exactly when the last cycle's evidence has landed.
    let mut draining = false;
    // The reminder tick arm (design §5.2): the first tick is immediate —
    // an undelivered reminder from before a reboot fires on the next
    // boot's first tick — and every wake re-arms at the fixed cadence.
    // With `enabled == false` the select! precondition keeps the sleep
    // future from ever being constructed, so a disabled engine performs
    // zero timer work (the paused-time suites stay auto-advance-free).
    let mut next_tick = tokio::time::Instant::now();
    let mut timer_armed = reminder_cfg.enabled;

    loop {
        if draining && !reports_open {
            tracing::info!("state engine stopped: drain complete");
            return;
        }
        if !draining && !commands_open && !reports_open && !system_open {
            break;
        }
        tokio::select! {
            biased; // deterministic drain order: commands, reports, system

            maybe_command = commands.recv(), if commands_open => match maybe_command {
                Some(EngineEnvelope::Execute { command, reply }) => {
                    if draining {
                        // Declined, not executed: shutdown stopped new
                        // work. The reply mirrors a stopped engine.
                        tracing::debug!("command declined: engine draining");
                        if let Some(reply) = reply {
                            let _ = reply.send(Err(ExecuteError::EngineGone));
                        }
                        continue;
                    }
                    execute_command(
                        &mut core,
                        &shared,
                        &signals,
                        (&repo, ids.as_ref(), clock.as_ref(), &mut sync_outs, &system_out),
                        command,
                        reply,
                        &mut storage_failing,
                    )
                    .await;
                }
                Some(EngineEnvelope::Flush { reply }) => {
                    // FIFO: everything before this was applied, interpreted,
                    // and published. In drain mode the pre-drain prefix is
                    // already settled, so the immediate reply preserves the
                    // barrier semantics.
                    let _ = reply.send(());
                }
                Some(EngineEnvelope::ReadState { reply }) => {
                    let _ = reply.send(Ok(core.persisted_view()));
                }
                Some(EngineEnvelope::AttachConnector { sync_out, reply }) => {
                    if draining {
                        // Mirrors Execute's decline: no new work — and no
                        // new connectors — after Shutdown.
                        tracing::debug!("connector attach declined: engine draining");
                        let _ = reply.send(AttachReceipt::Draining);
                    } else {
                        sync_outs.push(sync_out);
                        let _ = reply.send(AttachReceipt::Attached);
                    }
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
                    if draining {
                        // The sync actor exited after Shutdown: the drain
                        // is complete, checked at the top of the loop.
                        tracing::info!("sync report channel closed; drain complete");
                    } else {
                        // The sync actor died; the engine is a daemon and must
                        // not follow it.
                        tracing::warn!("sync report channel closed; continuing without sync");
                    }
                    reports_open = false;
                }
            },

            maybe_event = system.recv(), if system_open => match maybe_event {
                Ok(SystemEvent::Shutdown) => {
                    if draining {
                        // Already draining: idempotent (G6).
                        tracing::debug!("shutdown event ignored: engine already draining");
                    } else {
                        tracing::info!("state engine entering shutdown drain");
                        draining = true;
                        // Drain disarms the timer outright: timer wakes
                        // are not ignored, they never happen. An
                        // undelivered reminder survives to the next
                        // boot's first tick.
                        timer_armed = false;
                    }
                }
                Ok(SystemEvent::NetworkLost) if !draining => {
                    go_offline(&mut core, &shared, &signals, &repo, clock.as_ref()).await;
                }
                Ok(SystemEvent::NetworkRestored) if !draining => {
                    // No phase change: the next report is the evidence; a
                    // self-declared "idle" would be a guess.
                    forward_sync_now(&mut sync_outs);
                }
                Ok(SystemEvent::NetworkLost | SystemEvent::NetworkRestored) => {
                    tracing::debug!("network event ignored: engine draining");
                }
                // The engine never acts on its own StorageFailed: it is
                // the producer (compiler-forced arm).
                Ok(SystemEvent::StorageFailed) => {}
                Err(broadcast::error::RecvError::Closed) => system_open = false,
                Err(broadcast::error::RecvError::Lagged(missed)) => {
                    tracing::warn!(missed, "system event backlog overflowed");
                }
            },

            () = tokio::time::sleep_until(next_tick), if timer_armed => {
                reminder_tick(
                    &mut core,
                    &shared,
                    &signals,
                    (&repo, clock.as_ref(), &system_out, &notifier),
                    &reminder_cfg,
                    &mut storage_failing,
                )
                .await;
                next_tick = tokio::time::Instant::now() + reminder_cfg.tick;
            }
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

/// The seams tuple: `(repo, ids, clock, sync_outs, system_out)`.
type Seams<'a> = (
    &'a Arc<dyn TaskRepository>,
    &'a dyn IdGenerator,
    &'a dyn Clock,
    &'a mut Vec<mpsc::Sender<SyncCommand>>,
    &'a broadcast::WeakSender<SystemEvent>,
);

async fn execute_command(
    core: &mut EngineCore,
    shared: &Arc<ArcSwap<AppState>>,
    signals: &broadcast::Sender<EngineSignal>,
    (repo, ids, clock, sync_outs, system_out): Seams<'_>,
    command: StateCommand,
    reply: Option<oneshot::Sender<Result<CommandOutcome, ExecuteError>>>,
    storage_failing: &mut bool,
) {
    tracing::debug!("command received");
    let now = clock.now();

    if let StateCommand::RequestSync = command {
        let forwarded = forward_sync_now(sync_outs);
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
        note_storage_failure(storage_failing, system_out);
        if let Some(reply) = reply {
            let _ = reply.send(Err(ExecuteError::Storage(err)));
        }
        return;
    }
    *storage_failing = false;
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
    let marked_syncing = enqueued_ops && forward_sync_now(sync_outs) && core.mark_syncing();
    if changed || marked_syncing {
        publish(core, shared, signals);
    }
    if let Some(reply) = reply {
        let _ = reply.send(Ok(plan.outcome));
    }
}

/// Fire-and-forget nudge, fanned out to every registered connector.
/// Forwarded iff any send lands (`Ok`, or `Full` — a full inbox means a
/// cycle is already pending); `Closed` entries are pruned right here,
/// one mechanism covering both dead paths: a standalone boot's dropped
/// receiver and a detached actor's. A dead connector is a `debug!`
/// event — no connector exists in engine-only tests, and the sync actor
/// owns dedupe.
fn forward_sync_now(sync_outs: &mut Vec<mpsc::Sender<SyncCommand>>) -> bool {
    let mut forwarded = false;
    sync_outs.retain(|out| match out.try_send(SyncCommand::SyncNow) {
        Ok(()) => {
            tracing::debug!("sync nudge forwarded");
            forwarded = true;
            true
        }
        Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
            tracing::debug!("sync nudge coalesced: channel full");
            forwarded = true;
            true
        }
        Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
            tracing::debug!("sync nudge dropped: no sync actor");
            false
        }
    });
    forwarded
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

/// The shared `StorageFailed` edge trigger (one epoch semantics): the
/// event fires on the ok->fail transition only, and any successful
/// persistence resets the edge. Both write paths — commands and reminder
/// ticks — surface a dying disk as the same banner signal.
fn note_storage_failure(
    storage_failing: &mut bool,
    system_out: &broadcast::WeakSender<SystemEvent>,
) {
    if !*storage_failing {
        // Weak by design (#316): upgrade failure means every external
        // producer died and the channel is closed — no receiver can ever
        // appear (a WeakSender cannot subscribe), so the skipped
        // emission is unobservable, the same silent no-op as
        // send-to-zero-receivers.
        if let Some(system_out) = system_out.upgrade() {
            let _ = system_out.send(SystemEvent::StorageFailed);
        }
    }
    *storage_failing = true;
}

/// The tick-path seams tuple: `(repo, clock, system_out, notifier)`.
type TickSeams<'a> = (
    &'a Arc<dyn TaskRepository>,
    &'a dyn Clock,
    &'a broadcast::WeakSender<SystemEvent>,
    &'a mpsc::Sender<Notification>,
);

/// One reminder tick, shaped like [`ingest_sync_report`]: read the clock
/// once, evaluate the pure firings, persist the delivery markers, and
/// only then notify — the marker is the notification permit, so nothing
/// fires on a failed batch (durable-then-notify, design §6). A quiet
/// tick is a no-op: nothing persisted, published, or signaled.
async fn reminder_tick(
    core: &mut EngineCore,
    shared: &Arc<ArcSwap<AppState>>,
    signals: &broadcast::Sender<EngineSignal>,
    (repo, clock, system_out, notifier): TickSeams<'_>,
    cfg: &ReminderEngineConfig,
    storage_failing: &mut bool,
) {
    let now = clock.now();
    let firings = core.due_reminder_firings(&cfg.defaults, now);
    if firings.is_empty() {
        return;
    }
    let actions = delivery_batch(&firings, now);
    if let Err(err) = repo.apply(actions.clone()).await {
        // Nothing fires: the delivery marker was never durably recorded,
        // and memory never advanced either.
        tracing::error!(?err, "reminder delivery batch lost; nothing fires");
        note_storage_failure(storage_failing, system_out);
        return;
    }
    *storage_failing = false;
    let payloads = notifications_for(&firings, core.tasks());
    if core.interpret(&actions, now) {
        publish(core, shared, signals);
    }
    send_notifications(notifier, payloads).await;
}

/// Maps the firings onto the batch that settles them: one
/// `RecordReminderDelivery` per firing, stamped at the tick's clock read.
fn delivery_batch(
    firings: &[ReminderFiring],
    now: chrono::DateTime<chrono::Utc>,
) -> Vec<PersistenceAction> {
    firings
        .iter()
        .map(|firing| {
            PersistenceAction::RecordReminderDelivery(taskboard_domain::ReminderDelivery {
                reminder: firing.reminder,
                occurrence: firing.occurrence,
                delivered_at: now,
            })
        })
        .collect()
}

/// Outbound delivery, after the markers are durable. `send`, not
/// `try_send`: a full channel backpressures the tick (at-most-once by
/// construction; drop-on-full would be a lost toast by choice). A closed
/// channel is a `debug!` no-op — headless configs are legal.
async fn send_notifications(notifier: &mpsc::Sender<Notification>, payloads: Vec<Notification>) {
    for payload in payloads {
        if notifier.send(payload).await.is_err() {
            tracing::debug!("notification dropped: no notifier consumer");
        }
    }
}
