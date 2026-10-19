// SPDX-License-Identifier: AGPL-3.0-only
//! The application bootstrap (ADR 0008): one async function wires the
//! full actor graph and returns an [`App`] handle — the "in-process
//! core" every frontend (daemon, Phase 6 CLI subcommands, Phase 7 UI)
//! starts from.
//!
//! Ordering: the failure-prone pure steps (mode check, config
//! validation, credential resolution, client construction) run before
//! anything spawns; the only failing spawn is the engine's boot
//! hydration, whose cleanup drops the storage handle and joins the
//! storage actor, leaking nothing.
//!
//! # Usage
//!
//! ```
//! use std::sync::Arc;
//!
//! use taskboard_app::config::AppConfig;
//! use taskboard_app::{bootstrap, notifications::LogSink, secrets};
//!
//! // Tests inject a fixed store; production picks KeyringStore or
//! // EnvCredentialStore from `config.nextcloud.credential_store`.
//! #[derive(Debug)]
//! struct FixedStore(secrets::AppPassword);
//!
//! impl secrets::CredentialStore for FixedStore {
//!     fn load(&self, _: &str, _: &str) -> Result<secrets::AppPassword, secrets::CredentialError> {
//!         Ok(self.0.clone())
//!     }
//!     fn store(&self, _: &str, _: &str, _: secrets::AppPassword) -> Result<(), secrets::CredentialError> {
//!         Err(secrets::CredentialError::Unsupported)
//!     }
//!     fn delete(&self, _: &str, _: &str) -> Result<(), secrets::CredentialError> {
//!         Err(secrets::CredentialError::Unsupported)
//!     }
//! }
//!
//! # async fn demo(dir: std::path::PathBuf) -> Result<(), Box<dyn std::error::Error>> {
//! let mut config = AppConfig::default();
//! config.nextcloud.server_url = Some("http://127.0.0.1:9".into()); // refused fast
//! config.nextcloud.username = Some("alice".into());
//! config.storage.db_path = Some(dir.join("taskboard.db"));
//! config.validate()?;
//!
//! let store: Arc<dyn secrets::CredentialStore> =
//!     Arc::new(FixedStore(secrets::AppPassword::new("app-password")));
//! let app = bootstrap(config, store, Arc::new(LogSink)).await?;
//!
//! let _state = app.engine.shared_state(); // lock-free UI reads
//! let outcome = app.shutdown().await;     // graceful, bounded
//! assert!(!outcome.actor_aborted);
//! # Ok(())
//! # }
//! ```

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{broadcast, mpsc};
use tokio::task::JoinHandle;

use taskboard_domain::{
    SyncCommand, SyncPhase, SyncReport, SyncStateReader, SystemEvent, TaskRepository,
};
use taskboard_state::{AttachReceipt, EngineHandle, spawn_state_engine};
use taskboard_storage_sqlite::{StorageHandle, spawn_storage_actor};
use taskboard_sync_nextcloud::{DeckClient, spawn_sync_actor};

use crate::config::{AppConfig, ConfigError, SyncBackend};
use crate::lockfile::{InstanceLock, LockError};
use crate::notifications::{NOTIFIER_CAPACITY, NotificationSink};
use crate::secrets::{CredentialError, CredentialStore};

/// Channel capacity of the system broadcast (`NetworkLost`,
/// `NetworkRestored`, `Shutdown`).
pub const SYSTEM_BROADCAST_CAPACITY: usize = 64;
/// Channel capacity of the sync-command inbox (`SyncNow` nudges; "full"
/// already means "coalesced").
pub const SYNC_COMMAND_CAPACITY: usize = 32;
/// Channel capacity of the sync-report inbox (one report per cycle).
pub const SYNC_REPORT_CAPACITY: usize = 32;

/// Bootstrap failure classes, typed end to end.
#[derive(Debug, thiserror::Error)]
pub enum BootstrapError {
    /// The config was rejected by validation.
    #[error("configuration rejected")]
    Config(#[from] ConfigError),
    /// Credential resolution failed.
    #[error("credential resolution failed")]
    Credential(#[from] CredentialError),
    /// The database could not be opened.
    #[error("storage open failed")]
    Storage(#[from] taskboard_storage_sqlite::OpenError),
    /// The database directory could not be created.
    #[error("storage directory could not be created")]
    StorageDir(#[source] std::io::Error),
    /// The instance lock sidecar could not be opened or written.
    #[error("instance lock could not be acquired")]
    LockIo(#[source] std::io::Error),
    /// The state engine failed to boot (hydration).
    #[error("state engine failed to start")]
    Engine(#[from] taskboard_state::EngineStartupError),
    /// The Deck client rejected the server URL.
    #[error("deck client construction failed")]
    Client(#[from] taskboard_sync_nextcloud::DeckError),
    /// Another taskboard instance already holds this database.
    #[error("another taskboard instance holds this database")]
    InstanceLocked {
        /// The locked database path.
        db: PathBuf,
        /// The holder PID recorded in the lock sidecar (best-effort,
        /// diagnostic only).
        holder_pid: Option<u32>,
    },
}

/// One registry entry: the connector's inbox sender (the pair-mate of
/// the receiver its module spawned with) and its join handle.
struct SyncActor {
    commands: mpsc::Sender<SyncCommand>,
    join: JoinHandle<()>,
}

/// A running taskboard core. Not `Clone` — the shutdown sequence
/// consumes it exactly once.
pub struct App {
    /// The resolved, validated configuration.
    pub config: AppConfig,
    /// The state engine handle.
    pub engine: EngineHandle,
    /// The system broadcast (the engine and the sync actor's events).
    pub system: broadcast::Sender<SystemEvent>,
    /// The attached sync connectors (WI-J widened WI-H's
    /// `Option<SyncActor>` slot). The v1 invariant is length ≤ 1,
    /// enforced by [`App::attach_connector`]'s guard and by bootstrap
    /// (zero or one at boot). Lifecycle bookkeeping only — the mutex is
    /// never held across `.await`, and actor messaging stays the typed
    /// `mpsc` `SyncCommand` channel (AGENTS §3).
    connectors: std::sync::Mutex<Vec<SyncActor>>,
    /// The config-minted default connector id — the single mint every
    /// binding tag, storage row, and engine plan carries (DF-3).
    default_connector: taskboard_domain::ConnectorId,
    /// The twin of the engine's permanent reports receiver (design
    /// §4.2.3): every attached actor spawns with a `clone()`, N actors
    /// sharing one fan-in inbox, so the engine's select loop needs no
    /// `reports_open` juggling. Parking a *sender* is semantically inert
    /// (an empty open channel never fires `recv()`) — but it is the last
    /// thing holding the linger-drain open, so [`App::shutdown`] drops
    /// it before the engine join or the drain deadlocks.
    reports: mpsc::Sender<SyncReport>,
    storage: StorageHandle,
    storage_join: JoinHandle<()>,
    engine_join: JoinHandle<()>,
    notifier_join: JoinHandle<()>,
    /// The per-database instance lock, held for the process lifetime.
    _lock: InstanceLock,
}

impl std::fmt::Debug for App {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("App")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl App {
    /// Whether the sync connector is still attached (WI-H): `false`
    /// under `standalone` or after [`App::detach_connector`]. The
    /// daemon-side `status` answer reports it so a detached daemon does
    /// not masquerade as a syncing one.
    ///
    /// # Panics
    ///
    /// Only if the connector-lifecycle mutex is poisoned by a panic while
    /// held (unreachable: the critical sections never panic).
    #[must_use]
    pub fn connector_attached(&self) -> bool {
        !self.connectors.lock().expect("connector lock").is_empty()
    }

    /// Connector-only detach (WI-H option b): takes the sync actor out,
    /// sends it `SyncCommand::Detach` (graceful — an in-flight cycle
    /// completes first, decision D4), and joins it bounded by the
    /// configured `shutdown_timeout_secs` (abort at the budget — the
    /// credential dies with the task either way). Idempotent: a
    /// `standalone` core or an already-detached one answers
    /// [`DetachOutcome::NoConnector`]. Post-detach, the engine's registry
    /// entry feeds a closed receiver, so its next fan-out prunes it and
    /// `RequestSync` answers `NoOp`; the engine keeps serving local
    /// edits.
    ///
    /// # Panics
    ///
    /// Only if the connector-lifecycle mutex is poisoned by a panic
    /// while held (unreachable: the critical sections never panic).
    pub async fn detach_connector(&self) -> DetachOutcome {
        let actors: Vec<SyncActor> = self
            .connectors
            .lock()
            .expect("connector lock")
            .drain(..)
            .collect();
        if actors.is_empty() {
            tracing::info!("detach: no connector attached");
            return DetachOutcome::NoConnector;
        }
        let timeout = Duration::from_secs(self.config.app.shutdown_timeout_secs);
        let mut aborted = false;
        for actor in actors {
            aborted |= stop_actor(actor, timeout).await;
        }
        if aborted {
            DetachOutcome::TimedOut
        } else {
            tracing::info!("detach: connector dropped");
            DetachOutcome::Detached
        }
    }

    /// Attaches a fresh sync connector to the running core (WI-J, the
    /// mirror of [`App::detach_connector`]): guards the at-most-one
    /// invariant, rewires the engine's nudge registry (push + FIFO ack),
    /// spawns the actor over a fresh command pair and a clone of the
    /// permanently parked reports sender, and registers it. The actor
    /// self-targets from the persisted board binding and fires its first
    /// cycle immediately; awaiting settling (bounded, offline-first) is
    /// the caller's job, not the primitive's.
    ///
    /// # Errors
    ///
    /// [`AttachError::AlreadyAttached`] when a connector is attached
    /// (detach first — re-attach after detach is permitted);
    /// [`AttachError::Engine`] when the engine could not take the push;
    /// [`AttachError::Draining`] when it declined one (a raced shutdown).
    ///
    /// # Panics
    ///
    /// Only if the connector-lifecycle mutex is poisoned by a panic
    /// while held (unreachable: the critical sections never panic).
    pub async fn attach_connector(&self, client: DeckClient) -> Result<(), AttachError> {
        // 1. Guard: the v1 count-≤-1 invariant (lock released at the
        //    statement's end — never across the awaits below).
        if let Err(err) = ensure_vacant(&self.connectors) {
            tracing::info!("attach: refused, a connector is already attached");
            return Err(err);
        }

        // 2. Fresh command pair for the new actor (the boot pair belongs
        //    to the boot actor; a detached one is dead).
        let (commands, commands_rx) = mpsc::channel(SYNC_COMMAND_CAPACITY);

        // 3. Engine rewire: push + ack (FIFO — nudges fan out to this
        //    sender from the ack onward).
        let receipt = match self.engine.attach_connector(commands.clone()).await {
            Ok(receipt) => receipt,
            Err(err) => {
                tracing::warn!("attach: the state engine is unavailable");
                return Err(AttachError::Engine(err));
            }
        };
        match receipt {
            AttachReceipt::Attached => {}
            AttachReceipt::Draining => {
                tracing::info!("attach: declined, the engine is draining");
                return Err(AttachError::Draining);
            }
        }

        // 4. Spawn over the fan-in clone, tagged with the config-minted
        //    default id (D-7: the attach path holds a ceremony-built
        //    client; the module factory is the boot path's constructor).
        let join = spawn_sync_actor(
            self.default_connector.clone(),
            client,
            Arc::new(self.engine.clone()) as Arc<dyn SyncStateReader>,
            commands_rx,
            self.reports.clone(),
            self.system.clone(),
            self.config.sync_actor_config(),
        );

        // 5. Register — the vacancy may have been lost to a concurrent
        //    attach while the engine ack was in flight; the refused
        //    actor is unwound outside the lock (the engine's now-dead
        //    entry self-prunes on the next fan-out).
        let timeout = Duration::from_secs(self.config.app.shutdown_timeout_secs);
        if let Some(unwound) = try_register(&self.connectors, SyncActor { commands, join }) {
            tracing::info!("attach: vacancy lost to a concurrent attach; unwinding");
            stop_actor(unwound, timeout).await;
            return Err(AttachError::AlreadyAttached);
        }
        tracing::info!("attach: connector attached");
        Ok(())
    }

    /// The graceful shutdown sequence (ADR 0008 §5): engine flush →
    /// `Shutdown` broadcast → bounded sync-actor join (abort at the
    /// timeout) → bounded engine join (abort as last resort) → drop the
    /// storage handle and await its actor. Bounded; never panics.
    ///
    /// # Errors
    ///
    /// Never fails; the outcome reports whether the sync actor had to be
    /// aborted.
    ///
    /// # Panics
    ///
    /// Only if the connector-lifecycle mutex is poisoned by a panic while
    /// held (unreachable: the critical sections never panic).
    pub async fn shutdown(self) -> ShutdownOutcome {
        // Destructure up front: the parked reports sender's drop below
        // is structural, not something a field-access cleanup can
        // reorder by accident.
        let App {
            config,
            engine,
            system,
            default_connector: _,
            connectors,
            reports,
            storage,
            storage_join,
            engine_join,
            notifier_join,
            _lock,
        } = self;
        let timeout = Duration::from_secs(config.app.shutdown_timeout_secs);
        tracing::info!("shutdown: flushing the engine");
        engine.flush().await;
        tracing::info!("shutdown: broadcasting Shutdown");
        // No live receivers is fine (engine-only setups in tests).
        let _ = system.send(SystemEvent::Shutdown);

        // Bound the lock to the drain: the guard must never span a
        // join's `.await`.
        let actors: Vec<SyncActor> = connectors
            .lock()
            .expect("connector lock")
            .drain(..)
            .collect();
        let mut actor_aborted = false;
        for actor in actors {
            actor_aborted |= stop_actor(actor, timeout).await;
        }

        // THE ordering contract: App is the last holder of the reports
        // inbox's parked sender — every actor's clone died with its join
        // above — so this drop is what closes the reports channel and
        // lets the engine's linger-drain finish. Keeping the sender past
        // this point would hang the engine join to its bounded abort.
        drop(reports);

        join_bounded(engine_join, timeout, "state engine").await;

        // The consumer exits on its own once the engine's exit closed
        // the channel; the bounded join only bounds a wedged sink. A
        // log-line flusher missing its budget is not
        // process-outcome-relevant (`ShutdownOutcome` is unchanged).
        join_bounded(notifier_join, timeout, "notifier").await;

        tracing::info!("shutdown: stopping the storage actor");
        drop(storage);
        if let Err(join_err) = storage_join.await {
            tracing::warn!(?join_err, "storage actor join failed");
        }
        tracing::info!(actor_aborted, "shutdown complete");
        ShutdownOutcome { actor_aborted }
    }
}

/// Result of the graceful shutdown sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShutdownOutcome {
    /// The sync actor was aborted at the timeout instead of joining.
    pub actor_aborted: bool,
}

/// The daemon-shutdown tail found a second strong reference to the
/// core: the owned graceful sequence cannot run. The two
/// ownership-free salvage steps (engine flush + `Shutdown` broadcast)
/// have already been performed when this is returned.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("the core is still shared at daemon shutdown")]
pub struct CoreStillShared {
    /// Advisory strong-refcount observed at detection — evidence for
    /// the log line, not a lock: by the time it is read, other
    /// references may already have dropped.
    pub strong_refs: usize,
}

/// Outcome of [`App::detach_connector`] (WI-H).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetachOutcome {
    /// The sync actor acknowledged the detach and joined within the
    /// budget; the credential is destroyed.
    Detached,
    /// No connector existed to detach (`standalone` core, or already
    /// detached) — idempotent success.
    NoConnector,
    /// The actor missed the join budget and was aborted; the credential
    /// died with the task either way.
    TimedOut,
}

/// Typed attach refusals (the app layer's count-≤-1 policy lives here,
/// not in the engine).
#[derive(Debug, thiserror::Error)]
pub enum AttachError {
    /// A connector is already attached (v1: at most one; detach first —
    /// re-attach after detach is permitted).
    #[error("a connector is already attached")]
    AlreadyAttached,
    /// The state engine could not take the push (stopped).
    #[error("state engine unavailable")]
    Engine(#[from] taskboard_state::AttachError),
    /// The state engine declined the push (draining — a raced shutdown).
    #[error("state engine is shutting down")]
    Draining,
    /// A module's credential resolution failed (the boot loop maps this
    /// onto [`BootstrapError::Credential`] — wrapped, not duplicated).
    #[error("credential resolution failed")]
    Credential(#[from] CredentialError),
    /// A module's Deck client rejected the server URL (the boot loop maps
    /// this onto [`BootstrapError::Client`]).
    #[error("deck client construction failed")]
    Client(#[from] taskboard_sync_nextcloud::DeckError),
}

/// The vacancy half of the at-most-one guard, checked before anything
/// spawns (the registration re-check in [`App::attach_connector`]
/// closes the concurrent-caller race).
fn ensure_vacant(connectors: &std::sync::Mutex<Vec<SyncActor>>) -> Result<(), AttachError> {
    if connectors.lock().expect("connector lock").is_empty() {
        Ok(())
    } else {
        Err(AttachError::AlreadyAttached)
    }
}

/// Atomically registers `actor` if the registry is still vacant — the
/// re-check and the push share one lock acquisition, closing the
/// concurrent-caller race the pre-spawn [`ensure_vacant`] leaves open.
/// Returns the actor back when the vacancy was lost; the caller owns
/// stopping it.
fn try_register(
    connectors: &std::sync::Mutex<Vec<SyncActor>>,
    actor: SyncActor,
) -> Option<SyncActor> {
    let mut registry = connectors.lock().expect("connector lock");
    if registry.is_empty() {
        registry.push(actor);
        None
    } else {
        Some(actor)
    }
}

/// Stops one connector actor: sends it `SyncCommand::Detach` (graceful
/// — an in-flight cycle completes first), then joins it bounded (abort
/// at the budget). Returns whether the join had to abort.
async fn stop_actor(actor: SyncActor, timeout: Duration) -> bool {
    if actor.commands.send(SyncCommand::Detach).await.is_err() {
        // The actor already exited on its own (e.g. a Shutdown
        // broadcast): the connector is gone all the same.
        tracing::info!("sync actor already gone");
    }
    join_bounded(actor.join, timeout, "sync actor").await
}

/// Awaits a join handle with a deadline; on timeout aborts and reaps.
async fn join_bounded(join: JoinHandle<()>, timeout: Duration, what: &str) -> bool {
    let mut join = join;
    match tokio::time::timeout(timeout, &mut join).await {
        Ok(result) => {
            if let Err(join_err) = result {
                tracing::warn!(actor = what, ?join_err, "actor join returned an error");
            }
            false
        }
        Err(_elapsed) => {
            tracing::warn!(actor = what, "join timed out; aborting");
            join.abort();
            let _ = join.await;
            true
        }
    }
}

/// Wires the full graph and spawns every actor onto the current runtime.
///
/// Spawn order: storage actor → state engine → sync actor. Failing steps
/// abort with typed errors and leak nothing: the only mid-sequence
/// failure is engine hydration, whose cleanup drops the storage handle
/// and joins its actor.
///
/// # Errors
///
/// [`BootstrapError`] — typed, see the variant docs.
#[allow(clippy::result_large_err)] // the error is constructed once at boot
#[allow(clippy::too_many_lines)] // one wiring sequence, kept flat for review
pub async fn bootstrap(
    config: AppConfig,
    credentials: Arc<dyn CredentialStore>,
    notifier_sink: Arc<dyn NotificationSink>,
) -> Result<App, BootstrapError> {
    // The module list derives from the legacy scalar config
    // (synthesis-of-one; the loop body below is the seam a second
    // connector kind plugs into — a match arm, nothing else).
    let specs = crate::connectors::configured_connectors(&config);
    let modules: Vec<Arc<dyn crate::connectors::ConnectorModule>> = specs
        .iter()
        .map(|spec| match spec.backend {
            SyncBackend::Nextcloud => Arc::new(crate::connectors::NextcloudModule::new(
                spec.name.clone(),
                config.server_url().to_owned(),
                config.username().to_owned(),
                credentials.clone(),
            )) as Arc<dyn crate::connectors::ConnectorModule>,
            // The synthesis never yields a standalone spec; a module list
            // built by hand (tests) simply never contains one.
            SyncBackend::Standalone => unreachable!("standalone synthesizes no connector"),
        })
        .collect();
    bootstrap_with_modules(config, credentials, notifier_sink, modules).await
}

/// The registry-loop seam: production `bootstrap` synthesizes the
/// Nextcloud module and delegates; app tests pass a `FakeModule` to
/// control the connector without a server.
#[allow(clippy::result_large_err)] // the error is constructed once at boot
#[allow(clippy::too_many_lines)] // one wiring sequence, kept flat for review
async fn bootstrap_with_modules(
    config: AppConfig,
    _credentials: Arc<dyn CredentialStore>,
    notifier_sink: Arc<dyn NotificationSink>,
    modules: Vec<Arc<dyn crate::connectors::ConnectorModule>>,
) -> Result<App, BootstrapError> {
    // No `[app] mode` gate here: the core is mode-agnostic, and the
    // REPL (ADR 0011) serves any mode's console. The daemon/UI entry
    // (main) refuses `Desktop`/`Kiosk` itself until Phase 7 supplies
    // those frontends.
    config.validate()?;

    // One core per database: acquire the instance lock before anything
    // is opened or spawned; the OS releases it on process death. The
    // database directory is created first — the lock sidecar lives
    // next to the db file, so an unusable path surfaces as the
    // `Storage`/`StorageDir` class, not a lock error.
    let db_path = config.db_path().to_owned();
    if let Some(parent) = db_path.parent()
        && !parent.as_os_str().is_empty()
    {
        tokio::fs::create_dir_all(&parent)
            .await
            .map_err(BootstrapError::StorageDir)?;
    }
    let lock = InstanceLock::acquire(&db_path).map_err(|err| match err {
        LockError::Held { holder_pid } => BootstrapError::InstanceLocked {
            db: db_path.clone(),
            holder_pid,
        },
        LockError::Io(source) => BootstrapError::LockIo(source),
    })?;

    // The default connector id: the first module's config-minted name,
    // else the stable constant (inert under standalone — no bindings
    // exist to decode; a legacy bound db flipped to standalone decodes
    // its NULLs against the stable name and nothing consumes the tag).
    let default_connector = modules.first().map_or_else(
        || taskboard_domain::ConnectorId::from(crate::connectors::DEFAULT_CONNECTOR_NAME),
        |module| module.id().clone(),
    );
    tracing::info!(
        connector = %default_connector,
        db = %config.db_path().display(),
        mode = ?config.app.mode,
        poll = config.sync.poll_interval_secs,
        store = ?config.nextcloud.credential_store,
        "resolved configuration"
    );
    tracing::info!(db = %db_path.display(), "opening database");
    let repo = Arc::new(taskboard_storage_sqlite::open(&db_path, default_connector.clone()).await?);
    let (storage, storage_join) = spawn_storage_actor(repo);

    let (system, system_rx_engine) = broadcast::channel::<SystemEvent>(SYSTEM_BROADCAST_CAPACITY);
    let (sync_commands, sync_rx) = mpsc::channel::<SyncCommand>(SYNC_COMMAND_CAPACITY);
    let (reports, report_rx) = mpsc::channel::<SyncReport>(SYNC_REPORT_CAPACITY);
    let (notifier, notifications) =
        mpsc::channel::<taskboard_domain::Notification>(NOTIFIER_CAPACITY);
    // Spawned before the engine: the immediate first tick can notify
    // during hydration-heavy boots, and ordering the consumer first is
    // free. The consumer exits when the engine's exit closes the
    // channel.
    let notifier_join = crate::notifications::spawn_notifier(notifications, notifier_sink);

    // The engine reaches the database only through the storage actor
    // (single-writer discipline): the handle *is* the port adapter.
    let repo_port: Arc<dyn TaskRepository> = Arc::new(storage.clone());
    let spawned = spawn_state_engine(
        repo_port,
        Arc::new(taskboard_domain::idgen::UuidV7Generator),
        Arc::new(taskboard_domain::SystemClock),
        sync_commands.clone(),
        report_rx,
        system.clone(),
        system_rx_engine,
        notifier,
        config.reminder_engine_config(),
        default_connector.clone(),
    )
    .await;

    let (engine, engine_join) = match spawned {
        Ok(pair) => pair,
        Err(err) => {
            // Cleanup: dropping the storage handle closes the actor's
            // inbox; the actor drains and exits. Nothing else spawned.
            tracing::error!(?err, "bootstrap failed: engine did not start");
            drop(storage);
            let _ = storage_join.await;
            return Err(err.into());
        }
    };

    // The registry loop (the N=2 exit criterion's seam): one module per
    // configured connector, each spawning over its own command pair. The
    // first module consumes the *boot* pair — its sender is the engine's
    // boot nudge-registry entry. N ∈ {0, 1} this chunk; the loop's shape
    // is the seam, not speculative plural plumbing.
    let timeout = Duration::from_secs(config.app.shutdown_timeout_secs);
    let mut connectors: Vec<SyncActor> = Vec::new();
    let mut boot_pair: Option<(mpsc::Sender<SyncCommand>, mpsc::Receiver<SyncCommand>)> =
        Some((sync_commands.clone(), sync_rx));
    for module in &modules {
        let (commands, commands_rx) = match boot_pair.take() {
            Some(pair) => pair,
            None => mpsc::channel::<SyncCommand>(SYNC_COMMAND_CAPACITY),
        };
        match module.build_and_spawn(
            commands_rx,
            Arc::new(engine.clone()) as Arc<dyn SyncStateReader>,
            // The actor reports through a clone of the permanently
            // parked sender (N actors, one fan-in inbox).
            reports.clone(),
            system.clone(),
            config.sync_actor_config(),
        ) {
            Ok(spawned) => {
                connectors.push(SyncActor {
                    commands,
                    join: spawned.join,
                });
            }
            Err(err) => {
                return Err(unwind_after_module_failure(
                    connectors,
                    sync_commands,
                    system,
                    reports,
                    engine_join,
                    timeout,
                    storage,
                    storage_join,
                    err,
                )
                .await);
            }
        }
    }
    if let Some((_, rx)) = boot_pair {
        // Standalone: the boot command receiver is dropped immediately —
        // the engine sees the closed-channel world its loop already
        // handles, and its nudges fail closed (the boot registry entry
        // prunes on the first fan-out). The reports sender is *not*
        // dropped: it parks in `App` (below) as the fan-in inbox's twin
        // for a later attach.
        drop(rx);
    }
    tracing::info!("actor graph spawned: storage, state engine, notifier");

    Ok(App {
        config,
        engine,
        system,
        default_connector,
        connectors: std::sync::Mutex::new(connectors),
        reports,
        storage,
        storage_join,
        engine_join,
        notifier_join,
        _lock: lock,
    })
}

/// A module whose `build_and_spawn` failed after the engine already
/// spawned: unwind like a failed boot — close every engine input (the
/// registry senders, the parked reports twin, the system channel after
/// one `Shutdown`), then bounded-join the engine and drain the storage
/// actor. Nothing keeps running behind the error; the typed
/// `BootstrapError` maps from the module's `AttachError` (wrap, not
/// duplicate — D-14).
#[allow(clippy::too_many_arguments)] // the owned halves ARE the unwind
async fn unwind_after_module_failure(
    connectors: Vec<SyncActor>,
    sync_commands: mpsc::Sender<SyncCommand>,
    system: broadcast::Sender<SystemEvent>,
    reports: mpsc::Sender<SyncReport>,
    engine_join: JoinHandle<()>,
    timeout: Duration,
    storage: StorageHandle,
    storage_join: JoinHandle<()>,
    err: AttachError,
) -> BootstrapError {
    tracing::error!(?err, "bootstrap failed: connector module did not start");
    drop(connectors);
    drop(sync_commands);
    let _ = system.send(SystemEvent::Shutdown);
    drop(system);
    drop(reports);
    join_bounded(engine_join, timeout, "state engine").await;
    drop(storage);
    if let Err(join_err) = storage_join.await {
        tracing::warn!(?join_err, "storage actor join failed");
    }
    match err {
        AttachError::Credential(source) => BootstrapError::Credential(source),
        AttachError::Client(source) => BootstrapError::Client(source),
        other => unreachable!("build_and_spawn surfaces only credential/client errors: {other:?}"),
    }
}

/// Subscribes to the system broadcast and logs every event — the
/// daemon-side observability for `StorageFailed` (warn) and the network
/// events (info). Exits on channel close or a `Lagged` backlog; the
/// Phase 7 UI banner is the real consumer.
#[must_use]
pub fn spawn_system_event_logger(
    system: &broadcast::Sender<SystemEvent>,
) -> tokio::task::JoinHandle<()> {
    let mut events = system.subscribe();
    tokio::spawn(async move {
        loop {
            match events.recv().await {
                Ok(SystemEvent::StorageFailed) => {
                    tracing::warn!("persistence failed; state unchanged (StorageFailed)");
                }
                Ok(event) => tracing::info!(?event, "system event"),
                Err(broadcast::error::RecvError::Lagged(missed)) => {
                    tracing::warn!(missed, "system event backlog overflowed");
                    return;
                }
                Err(broadcast::error::RecvError::Closed) => return,
            }
        }
    })
}

/// Gives the boot cycle's first pull (a `Syncing` → settled
/// transition) a bounded chance to land before the verb reads the
/// state. Only a cycle *in flight right now* is awaited; a settled
/// hydrated status (the common case) returns immediately — freshness
/// beyond the last sync is `taskboard sync`'s job. A board-less db can
/// only ever skip silently (`NoBoard`), so it returns immediately; a
/// deadline expiry degrades gracefully — offline-first: local data
/// serves the command.
pub(crate) async fn wait_for_boot_pull(app: &App, timeout: Duration) {
    let mut signals = app.engine.subscribe();
    let deadline = tokio::time::Instant::now() + timeout;
    let mut seen_syncing = false;
    loop {
        let state = app.engine.shared_state().load_full();
        match state.sync.aggregate().phase {
            SyncPhase::Syncing => seen_syncing = true,
            _ if seen_syncing => return,
            // The cycle already settled before we started watching.
            _ if state.sync.aggregate().last_success.is_some()
                || matches!(
                    state.sync.aggregate().phase,
                    SyncPhase::Offline | SyncPhase::Failed { .. }
                ) =>
            {
                return;
            }
            // No live board: the cycle can only skip silently.
            _ if !state.boards.values().any(|b| !b.deleted) => return,
            _ => {}
        }
        if tokio::time::Instant::now() >= deadline {
            tracing::warn!("boot pull did not settle in time; serving local data");
            return;
        }
        let _ = tokio::time::timeout(Duration::from_millis(200), signals.recv()).await;
    }
}

/// The daemon-shutdown tail: after the IPC server's drain released its
/// reference, hand the core's exclusive ownership to the graceful
/// shutdown sequence.
///
/// If some other strong reference to the core still exists, the owned
/// sequence cannot run (it is written as a one-shot destructuring whose
/// actor-join ordering depends on `App` being the *last* holder of the
/// reports sender). The salvage then performs exactly the two
/// ownership-free steps — an engine flush and the `Shutdown` broadcast —
/// and the failure surfaces typed; actor joins and the storage drain
/// are skipped (a stray ref pins the reports sender, so the engine join
/// could only stall into its bounded abort).
///
/// # Errors
///
/// [`CoreStillShared`] when the refcount exceeds one at the tail: the
/// daemon reports failure through the regular runtime-error path
/// instead of the process dying to a panic.
pub async fn shutdown_from_arc(app: Arc<App>) -> Result<ShutdownOutcome, CoreStillShared> {
    match Arc::try_unwrap(app) {
        Ok(app) => Ok(app.shutdown().await),
        Err(app) => {
            let strong_refs = Arc::strong_count(&app);
            tracing::error!(strong_refs, "core still shared at daemon shutdown");
            app.engine.flush().await;
            // No live receivers is fine, as in the owned sequence.
            let _ = app.system.send(SystemEvent::Shutdown);
            Err(CoreStillShared { strong_refs })
        }
    }
}

/// Daemon lifecycle: await `ctrl_c` (and SIGTERM on unix), then run the
/// graceful shutdown sequence. The second-signal force-exit lives in
/// `main` (thin glue, deliberately untested).
pub async fn run_until_signal(app: App) -> ShutdownOutcome {
    wait_for_signal().await;
    tracing::info!("shutdown signal received");
    app.shutdown().await
}

/// Resolves once a termination signal arrives: `ctrl_c` everywhere,
/// SIGTERM additionally on unix (container stop and systemd send
/// SIGTERM). Registration failure degrades to waiting on `ctrl_c` only,
/// never panics.
pub async fn wait_for_signal() {
    #[cfg(unix)]
    {
        let mut term =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).ok();
        let terminated = async {
            match term.as_mut() {
                Some(stream) => {
                    stream.recv().await;
                }
                None => std::future::pending::<()>().await,
            }
        };
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            () = terminated => {}
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

#[cfg(test)]
mod connector_module_tests {
    use super::*;
    use crate::connectors::{ConnectorModule, SpawnedConnector};
    use chrono::TimeZone as _;
    use std::collections::VecDeque;
    use std::sync::Mutex;
    use taskboard_domain::RemoteBoardSnapshot;

    /// Handwritten test fake (AGENTS §5 — fakes over mocks): spawns a
    /// scripted task over the *real* channels; every `SyncNow` pops the
    /// next queued [`SyncReport`], `Detach` (or a closed inbox) stops it.
    /// Gives the registry-loop test connector control without wiremock.
    #[derive(Debug)]
    struct FakeModule {
        id: taskboard_domain::ConnectorId,
        script: Arc<Mutex<VecDeque<SyncReport>>>,
    }

    impl FakeModule {
        fn new(id: &str, script: Vec<SyncReport>) -> Self {
            Self {
                id: taskboard_domain::ConnectorId::from(id),
                script: Arc::new(Mutex::new(script.into())),
            }
        }
    }

    impl ConnectorModule for FakeModule {
        fn id(&self) -> &taskboard_domain::ConnectorId {
            &self.id
        }

        fn build_and_spawn(
            &self,
            mut commands: mpsc::Receiver<SyncCommand>,
            _engine_reader: Arc<dyn SyncStateReader>,
            reports: mpsc::Sender<SyncReport>,
            _system: broadcast::Sender<SystemEvent>,
            _cfg: taskboard_sync_nextcloud::SyncActorConfig, // unused; signature parity only
        ) -> Result<SpawnedConnector, AttachError> {
            let script = Arc::clone(&self.script);
            let join = tokio::spawn(async move {
                while let Some(command) = commands.recv().await {
                    match command {
                        SyncCommand::SyncNow => {
                            let next = script.lock().expect("script lock").pop_front();
                            if let Some(report) = next
                                && reports.send(report).await.is_err()
                            {
                                return; // engine gone
                            }
                        }
                        SyncCommand::Detach => return,
                    }
                }
            });
            Ok(SpawnedConnector {
                id: self.id.clone(),
                join,
            })
        }
    }

    fn snapshot_of(board: u64) -> RemoteBoardSnapshot {
        use taskboard_domain::{RemoteBoard, RemoteBoardId};
        RemoteBoardSnapshot {
            board: RemoteBoard {
                id: RemoteBoardId(board),
                title: "remote board".into(),
                color: "00ff00".into(),
                archived: false,
                deleted_at: None,
                last_modified: chrono::Utc.timestamp_opt(1_700_000_000, 0).unwrap(),
            },
            stacks: Vec::new(),
            tasks: Vec::new(),
            labels: Vec::new(),
        }
    }

    /// The registry loop at N=1 over a `FakeModule`: the engine's nudge
    /// fans out to the module's actor, the scripted `Completed` lands, and
    /// the adopted board's status row settles — connector control without
    /// a server. `SpawnedConnector`'s registry key is the module's id.
    #[tokio::test]
    async fn registry_loop_wires_the_fake_module_end_to_end() {
        let mut config = AppConfig::default();
        let dir = tempfile::tempdir().expect("tempdir");
        config.storage.db_path = Some(dir.path().join("taskboard.db"));
        config.validate().expect("config valid");

        let completed = SyncReport::Completed {
            snapshot: Box::new(snapshot_of(77)),
            validators: taskboard_domain::BoardPullValidators {
                boards: taskboard_domain::SyncValidators::default(),
                stacks: taskboard_domain::SyncValidators::default(),
                archived_stacks: taskboard_domain::SyncValidators::default(),
            },
            pushes: Vec::new(),
            read_at: chrono::Utc.timestamp_opt(1_700_000_001, 0).unwrap(),
        };
        let module: Arc<dyn ConnectorModule> =
            Arc::new(FakeModule::new("fakemodule", vec![completed]));

        let app = bootstrap_with_modules(
            config,
            Arc::new(RefusingStore),
            Arc::new(crate::notifications::LogSink),
            vec![module],
        )
        .await
        .expect("boot");

        let outcome = app
            .engine
            .execute(taskboard_domain::StateCommand::RequestSync)
            .await
            .expect("nudge accepted");
        assert_eq!(
            outcome,
            taskboard_domain::CommandOutcome::SyncRequested,
            "the nudge fanned out to the fake module's actor"
        );
        app.engine.flush().await;

        let state = app.engine.shared_state().load_full();
        let adopted = state.boards.values().next().expect("board adopted");
        let binding = adopted.remote.as_ref().expect("adopt-new mint");
        assert_eq!(binding.board, taskboard_domain::RemoteBoardId(77));
        assert_eq!(
            binding.connector.as_str(),
            "fakemodule",
            "the registry key (the module's id) tags the adopted binding"
        );
        let row = state.sync.get(adopted.id).expect("status row");
        assert_eq!(row.phase, taskboard_domain::SyncPhase::Idle);
        assert!(row.last_success.is_some(), "the cycle settled the row");
        assert_eq!(state.sync.aggregate().pending_ops, 0);

        let outcome = app.shutdown().await;
        assert!(!outcome.actor_aborted);
    }

    /// A credential-refusing store: `build_and_spawn`'s failure maps onto
    /// the existing typed `BootstrapError` variant (D-14: wrap, not
    /// duplicate) — and unwinds the already-spawned engine.
    #[derive(Debug)]
    struct RefusingStore;

    impl crate::secrets::CredentialStore for RefusingStore {
        fn load(
            &self,
            _: &str,
            _: &str,
        ) -> Result<crate::secrets::AppPassword, crate::secrets::CredentialError> {
            Err(crate::secrets::CredentialError::NotFound)
        }
        fn store(
            &self,
            _: &str,
            _: &str,
            _: crate::secrets::AppPassword,
        ) -> Result<(), crate::secrets::CredentialError> {
            Err(crate::secrets::CredentialError::Unsupported)
        }
        fn delete(&self, _: &str, _: &str) -> Result<(), crate::secrets::CredentialError> {
            Err(crate::secrets::CredentialError::Unsupported)
        }
    }

    #[tokio::test]
    async fn module_build_failure_maps_onto_the_typed_bootstrap_error() {
        let mut config = AppConfig::default();
        let dir = tempfile::tempdir().expect("tempdir");
        config.storage.db_path = Some(dir.path().join("taskboard.db"));
        config.nextcloud.server_url = Some("https://cloud.example.com".into());
        config.nextcloud.username = Some("alice".into());
        config.validate().expect("config valid");
        // A resolved nextcloud backend synthesizes the real module, whose
        // credential load refuses: the loop maps the error and unwinds.
        let err = bootstrap(
            config,
            Arc::new(RefusingStore),
            Arc::new(crate::notifications::LogSink),
        )
        .await
        .expect_err("credential failure surfaces");
        assert!(matches!(err, BootstrapError::Credential(_)));
    }
}
