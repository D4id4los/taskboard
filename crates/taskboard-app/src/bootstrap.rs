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
/// `NetworkRestored`, the per-connector `ConnectorLost`/`Restored`
/// episodes, `Shutdown`).
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

/// One registry entry: the connector's config-minted id (the registry
/// key — named routing and the authority's membership), its inbox
/// sender (the pair-mate of the receiver its module spawned with), and
/// its join handle.
struct SyncActor {
    id: taskboard_domain::ConnectorId,
    commands: mpsc::Sender<SyncCommand>,
    join: JoinHandle<()>,
}

/// The cloneable connector-lifecycle capability (round-1 chunk 3,
/// DEV-1): attach/detach/registry-query over the *shared* registry,
/// engine, reports fan-in, and authority — everything the connector
/// actors need, none of the process-shell ownership (storage, lock,
/// shutdown). The UI settings request handler runs in a spawned task
/// and cannot borrow `&App`; `Arc<App>` + `try_unwrap` would put
/// graceful shutdown at the mercy of task-drop timing, so this split
/// (process shell vs connector lifecycle) is the seam. Bodies moved
/// verbatim from `App` (every REPL/CLI/daemon call site untouched —
/// the `App` methods are one-line delegates now).
#[derive(Clone)]
pub struct ConnectorControl {
    connectors: Arc<std::sync::Mutex<Vec<SyncActor>>>,
    engine: EngineHandle,
    reports: mpsc::Sender<SyncReport>,
    system: broadcast::Sender<SystemEvent>,
    actor_config: taskboard_sync_nextcloud::SyncActorConfig,
    shutdown_timeout_secs: u64,
    authority: crate::authority::AuthorityHandle,
}

impl ConnectorControl {
    /// Assembles the handle over the booted graph's shared halves
    /// (bootstrap's construction site; `App::connector_control` hands
    /// out clones).
    fn new(
        connectors: Arc<std::sync::Mutex<Vec<SyncActor>>>,
        engine: EngineHandle,
        reports: mpsc::Sender<SyncReport>,
        system: broadcast::Sender<SystemEvent>,
        actor_config: taskboard_sync_nextcloud::SyncActorConfig,
        shutdown_timeout_secs: u64,
        authority: crate::authority::AuthorityHandle,
    ) -> Self {
        Self {
            connectors,
            engine,
            reports,
            system,
            actor_config,
            shutdown_timeout_secs,
            authority,
        }
    }

    /// Whether any connector is registered — moved verbatim from
    /// `App::connector_attached`.
    ///
    /// # Panics
    ///
    /// Only if the connector-lifecycle mutex is poisoned by a panic while
    /// held (unreachable: the critical sections never panic).
    #[must_use]
    pub fn connector_attached(&self) -> bool {
        !self.connectors.lock().expect("connector lock").is_empty()
    }

    /// The named lookup for any targeted send (design §3.3 option (b)'s
    /// accessor): the connector's inbox sender when the name is
    /// registered, else `None`. The routing table is the registry; the
    /// engine fan-out stays broadcast-only.
    ///
    /// # Panics
    ///
    /// Only if the connector-lifecycle mutex is poisoned by a panic
    /// while held (unreachable: the critical sections never panic).
    #[must_use]
    pub fn connector_commands(
        &self,
        id: &taskboard_domain::ConnectorId,
    ) -> Option<mpsc::Sender<SyncCommand>> {
        self.connectors
            .lock()
            .expect("connector lock")
            .iter()
            .find(|actor| &actor.id == id)
            .map(|actor| actor.commands.clone())
    }

    /// Named detach (D5-7): stops only the named connector — the
    /// session/settings swap uses it so a second live connector
    /// survives the rewrite of the default entry. Unknown name ⇒
    /// [`DetachOutcome::NoConnector`] (idempotent). Moved verbatim from
    /// `App::detach_connector_named`.
    ///
    /// # Panics
    ///
    /// Only if the connector-lifecycle mutex is poisoned by a panic
    /// while held (unreachable: the critical sections never panic).
    pub async fn detach_connector_named(
        &self,
        id: &taskboard_domain::ConnectorId,
    ) -> DetachOutcome {
        // One lock scope for the lookup and the removal: the position is
        // never stale (a concurrent attach/detach cannot slip between).
        let actor = {
            let mut registry = self.connectors.lock().expect("connector lock");
            registry
                .iter()
                .position(|actor| &actor.id == id)
                .map(|position| registry.remove(position))
        };
        let Some(actor) = actor else {
            tracing::info!(connector = %id, "detach: no connector under that name");
            return DetachOutcome::NoConnector;
        };
        let timeout = Duration::from_secs(self.shutdown_timeout_secs);
        let aborted = stop_actor(actor, timeout).await;
        // Registry hook: the membership shrinks.
        self.authority.detach(id);
        if aborted {
            DetachOutcome::TimedOut
        } else {
            tracing::info!(connector = %id, "detach: connector dropped");
            DetachOutcome::Detached
        }
    }

    /// Attaches a fresh sync connector under `name` (the registry key;
    /// the mirror of [`ConnectorControl::detach_connector_named`]):
    /// guards the name, rewires the engine's nudge registry (push + FIFO
    /// ack), spawns the actor over a fresh command pair and a clone of
    /// the permanently parked reports sender, and registers it. The
    /// actor derives its board set from the persisted bindings tagged
    /// `name` and fires its first cycle immediately; awaiting settling
    /// (bounded, offline-first) is the caller's job, not the primitive's.
    /// Moved verbatim from `App::attach_connector`.
    ///
    /// # Errors
    ///
    /// [`AttachError::ConnectorNameInUse`] when `name` is already
    /// registered (detach first — re-attach after detach is permitted);
    /// [`AttachError::Engine`] when the engine could not take the push;
    /// [`AttachError::Draining`] when it declined one (a raced shutdown).
    ///
    /// # Panics
    ///
    /// Only if the connector-lifecycle mutex is poisoned by a panic
    /// while held (unreachable: the critical sections never panic).
    pub async fn attach_connector(
        &self,
        name: taskboard_domain::ConnectorId,
        client: DeckClient,
    ) -> Result<(), AttachError> {
        // 1. Guard: the name is free (lock released at the statement's
        //    end — never across the awaits below).
        if let Err(err) = ensure_name_free(&self.connectors, &name) {
            tracing::info!(connector = %name, "attach: refused, the name is in use");
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

        // 4. Spawn over the fan-in clone, tagged with the registered
        //    name (D-7: the attach path holds a ceremony-built client;
        //    the module factory is the boot path's constructor).
        let join = spawn_sync_actor(
            name.clone(),
            client,
            Arc::new(self.engine.clone()) as Arc<dyn SyncStateReader>,
            commands_rx,
            self.reports.clone(),
            self.system.clone(),
            self.actor_config.clone(),
        );

        // 5. Register — the vacancy may have been lost to a concurrent
        //    attach while the engine ack was in flight; the refused
        //    actor is unwound outside the lock (the engine's now-dead
        //    entry self-prunes on the next fan-out).
        let timeout = Duration::from_secs(self.shutdown_timeout_secs);
        let actor = SyncActor {
            id: name.clone(),
            commands,
            join,
        };
        if let Some(unwound) = try_register(&self.connectors, actor) {
            tracing::info!(connector = %name, "attach: the name was taken while the engine ack was in flight; unwinding");
            stop_actor(unwound, timeout).await;
            return Err(AttachError::ConnectorNameInUse(name));
        }
        // Registry hook: membership grows; a set that was
        // all-down-nonempty closes loudly.
        self.authority.attach(&name);
        tracing::info!(connector = %name, "attach: connector attached");
        Ok(())
    }
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
    /// enforced by [`ConnectorControl::attach_connector`]'s guard and by
    /// bootstrap (zero or one at boot). Lifecycle bookkeeping only — the
    /// mutex is never held across `.await`, and actor messaging stays
    /// the typed `mpsc` `SyncCommand` channel (AGENTS §3). Shared with
    /// the [`ConnectorControl`] handle (round-1 DEV-1) so the request
    /// forwarder's spawned task can drive the connector lifecycle
    /// without an `App` reference.
    connectors: Arc<std::sync::Mutex<Vec<SyncActor>>>,
    /// The cloneable connector-lifecycle capability (round-1 DEV-1):
    /// the seam the UI settings forwarder holds.
    control: ConnectorControl,
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
    /// The all-down network authority's hooks (DF-7): the attach/
    /// detach registry hooks and the broadcast sender for its global
    /// emissions. The task itself is `authority_join`.
    authority: crate::authority::AuthorityHandle,
    authority_join: JoinHandle<()>,
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
    /// The cloneable connector-lifecycle handle (round-1 DEV-1): the
    /// seam the spawned request forwarder holds — cheap to clone, no
    /// `App` reference, no shutdown ownership.
    #[must_use]
    pub fn connector_control(&self) -> ConnectorControl {
        self.control.clone()
    }

    /// Whether the sync connector is still attached (WI-H): `false`
    /// under `standalone` or after [`App::detach_connector`]. The
    /// daemon-side `status` answer reports it so a detached daemon does
    /// not masquerade as a syncing one. A one-line delegate onto the
    /// [`ConnectorControl`] body (round-1 DEV-1: moved verbatim).
    ///
    /// # Panics
    ///
    /// Only if the connector-lifecycle mutex is poisoned by a panic while
    /// held (unreachable: the critical sections never panic).
    #[must_use]
    pub fn connector_attached(&self) -> bool {
        self.control.connector_attached()
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
            let id = actor.id.clone();
            aborted |= stop_actor(actor, timeout).await;
            // Registry hook: the membership shrinks; a drain to an
            // empty set (the standalone world) stays silent.
            self.authority.detach(&id);
        }
        if aborted {
            DetachOutcome::TimedOut
        } else {
            tracing::info!("detach: connector dropped");
            DetachOutcome::Detached
        }
    }

    /// The named lookup for any targeted send (design §3.3 option (b)'s
    /// accessor): the connector's inbox sender when the name is
    /// registered, else `None`. The routing table is the registry; the
    /// engine fan-out stays broadcast-only.
    ///
    /// # Panics
    ///
    /// Only if the connector-lifecycle mutex is poisoned by a panic
    /// while held (unreachable: the critical sections never panic).
    #[must_use]
    pub fn connector_commands(
        &self,
        id: &taskboard_domain::ConnectorId,
    ) -> Option<mpsc::Sender<SyncCommand>> {
        self.connectors
            .lock()
            .expect("connector lock")
            .iter()
            .find(|actor| &actor.id == id)
            .map(|actor| actor.commands.clone())
    }

    /// Named detach (D5-7, the named registry's mirror of
    /// [`App::detach_connector`]): stops only the named connector — the
    /// session swap uses it so a second live connector survives the
    /// rewrite of the default entry. Unknown name ⇒
    /// [`DetachOutcome::NoConnector`] (idempotent). A one-line delegate
    /// onto the [`ConnectorControl`] body (round-1 DEV-1: moved
    /// verbatim).
    ///
    /// # Panics
    ///
    /// Only if the connector-lifecycle mutex is poisoned by a panic
    /// while held (unreachable: the critical sections never panic).
    pub async fn detach_connector_named(
        &self,
        id: &taskboard_domain::ConnectorId,
    ) -> DetachOutcome {
        self.control.detach_connector_named(id).await
    }

    /// Attaches a fresh sync connector under `name` (the registry key;
    /// the mirror of [`App::detach_connector_named`]). A one-line
    /// delegate onto the [`ConnectorControl`] body (round-1 DEV-1:
    /// moved verbatim).
    ///
    /// # Errors
    ///
    /// [`AttachError::ConnectorNameInUse`] when `name` is already
    /// registered (detach first — re-attach after detach is permitted);
    /// [`AttachError::Engine`] when the engine could not take the push;
    /// [`AttachError::Draining`] when it declined one (a raced shutdown).
    ///
    /// # Panics
    ///
    /// Only if the connector-lifecycle mutex is poisoned by a panic
    /// while held (unreachable: the critical sections never panic).
    pub async fn attach_connector(
        &self,
        name: taskboard_domain::ConnectorId,
        client: DeckClient,
    ) -> Result<(), AttachError> {
        self.control.attach_connector(name, client).await
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
            connectors,
            control,
            reports,
            storage,
            storage_join,
            engine_join,
            notifier_join,
            authority_join,
            authority: _,
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

        // The authority exited on the Shutdown broadcast long before;
        // the bounded join only bounds a wedged task. Ordering: actors
        // → authority → reports drop → engine (D5-13). Each bounded
        // join's budget fact is aggregated: any abort at the budget is
        // a degraded drain (design D-6's exit-code contract).
        let authority_budget_hit = join_bounded(authority_join, timeout, "authority").await;

        // The control handle holds clones of the reports sender and the
        // engine handle; it dies here so the ordering contract below
        // holds (any stray control clone outliving shutdown degrades to
        // the engine join's bounded abort, never a hang forever).
        drop(control);

        // THE ordering contract: App is the last holder of the reports
        // inbox's parked sender — every actor's clone died with its join
        // above — so this drop is what closes the reports channel and
        // lets the engine's linger-drain finish. Keeping the sender past
        // this point would hang the engine join to its bounded abort.
        drop(reports);

        let engine_budget_hit = join_bounded(engine_join, timeout, "state engine").await;

        // The consumer exits on its own once the engine's exit closed
        // the channel; the bounded join only bounds a wedged sink.
        let notifier_budget_hit = join_bounded(notifier_join, timeout, "notifier").await;

        tracing::info!("shutdown: stopping the storage actor");
        drop(storage);
        if let Err(join_err) = storage_join.await {
            tracing::warn!(?join_err, "storage actor join failed");
        }
        tracing::info!(actor_aborted, "shutdown complete");
        ShutdownOutcome {
            actor_aborted,
            budget_hit: authority_budget_hit | engine_budget_hit | notifier_budget_hit,
        }
    }
}

/// Result of the graceful shutdown sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShutdownOutcome {
    /// The sync actor was aborted at the timeout instead of joining.
    pub actor_aborted: bool,
    /// Any other bounded join (authority, state engine, notifier)
    /// aborted at the timeout instead of joining.
    pub budget_hit: bool,
}

impl ShutdownOutcome {
    /// The degraded-drain predicate (design D-6's exit-code contract,
    /// in exactly one place): any bounded join aborted at its budget.
    /// The IPC/web server drains' budget fact reaches the exit decision
    /// through [`exit_if_degraded`]'s `server_budget_hit` parameter —
    /// the two join types predate the outcome type.
    #[must_use]
    pub fn degraded(&self) -> bool {
        self.actor_aborted || self.budget_hit
    }
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

/// Typed attach refusals (the app layer's naming policy lives here,
/// not in the engine).
#[derive(Debug, thiserror::Error)]
pub enum AttachError {
    /// A connector under this name is already registered (detach first —
    /// re-attach after detach is permitted). The payload names the
    /// offender (the guard that was the count-≤-1 `AlreadyAttached`,
    /// same refusal, named — design §3.3).
    #[error("a connector is already attached")]
    ConnectorNameInUse(taskboard_domain::ConnectorId),
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

/// The name-free half of the attach guard, checked before anything
/// spawns (the registration re-check in [`App::attach_connector`]
/// closes the concurrent-caller race).
fn ensure_name_free(
    connectors: &std::sync::Mutex<Vec<SyncActor>>,
    name: &taskboard_domain::ConnectorId,
) -> Result<(), AttachError> {
    if connectors
        .lock()
        .expect("connector lock")
        .iter()
        .any(|actor| &actor.id == name)
    {
        Err(AttachError::ConnectorNameInUse(name.clone()))
    } else {
        Ok(())
    }
}

/// Atomically registers `actor` if its name is still free — the
/// re-check and the push share one lock acquisition, closing the
/// concurrent-caller race the pre-spawn [`ensure_name_free`] leaves
/// open. Returns the actor back when the name was taken; the caller
/// owns stopping it.
fn try_register(
    connectors: &std::sync::Mutex<Vec<SyncActor>>,
    actor: SyncActor,
) -> Option<SyncActor> {
    let mut registry = connectors.lock().expect("connector lock");
    if registry.iter().any(|registered| registered.id == actor.id) {
        Some(actor)
    } else {
        registry.push(actor);
        None
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
    let specs = crate::connectors::configured_connectors(&config)?;
    let modules: Vec<Arc<dyn crate::connectors::ConnectorModule>> = specs
        .iter()
        .map(|spec| match spec.backend {
            SyncBackend::Nextcloud => Arc::new(crate::connectors::NextcloudModule::new(
                spec.name.clone(),
                spec.server_url.clone(),
                spec.username.clone(),
                credentials.clone(),
            )) as Arc<dyn crate::connectors::ConnectorModule>,
            // The synthesis never yields a standalone spec; a module list
            // built by hand (tests) simply never contains one.
            SyncBackend::Standalone => unreachable!("standalone synthesizes no connector"),
        })
        .collect();
    bootstrap_with_modules(config, credentials, notifier_sink, modules).await
}

/// The boot-time notifier sink: `[reminders] sink` resolved against
/// `[app] mode` (D-R13). One `Arc` per process — the consumer holds it
/// for its lifetime; no live swap exists (D-R13). The resolution
/// matrix behavior is owned by `notifications::resolve_sink`'s tests;
/// this glue adds the `Arc` and nothing else.
#[must_use]
pub fn resolve_notifier_sink(config: &AppConfig) -> Arc<dyn NotificationSink> {
    match crate::notifications::resolve_sink(config.reminders.sink, config.app.mode) {
        crate::notifications::SinkKind::Desktop => {
            Arc::new(crate::notifications::DesktopSink::default())
        }
        crate::notifications::SinkKind::Log => Arc::new(crate::notifications::LogSink),
    }
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
        || {
            // The const is `"nextcloud"`; proven kebab by the pinned
            // unit test in `connectors.rs`, so the mint cannot fail.
            taskboard_domain::ConnectorId::from_kebab(crate::connectors::DEFAULT_CONNECTOR_NAME)
                .expect("DEFAULT_CONNECTOR_NAME is kebab")
        },
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
        mpsc::channel::<taskboard_domain::NotifierMessage>(NOTIFIER_CAPACITY);

    // The all-down authority (DF-7): seeded from the module set (all
    // up) and subscribed *before any module spawns*, so a first-cycle
    // failure cannot race the subscription.
    let authority_state = std::sync::Arc::new(std::sync::Mutex::new(
        crate::authority::AuthorityState::new(modules.iter().map(|m| m.id().clone())),
    ));
    let authority = crate::authority::AuthorityHandle::new(authority_state, system.clone());
    let authority_join = crate::authority::spawn_authority(authority.clone(), system.subscribe());
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
        // The first module consumes the boot pair: its sender *is* the
        // engine's boot nudge-registry entry.
        let (commands, commands_rx) = if let Some(pair) = boot_pair.take() {
            pair
        } else {
            // Every later module needs its own fan-out entry: the
            // registry push is what makes the engine's nudges reach
            // it (chunk 5 makes N ≥ 2 reachable, so the fan-out
            // must grow with the module set).
            let (commands, commands_rx) = mpsc::channel::<SyncCommand>(SYNC_COMMAND_CAPACITY);
            match engine.attach_connector(commands.clone()).await {
                Ok(taskboard_state::AttachReceipt::Attached) => {}
                Ok(taskboard_state::AttachReceipt::Draining) => {
                    tracing::warn!("boot: the engine is draining; the module still spawns");
                }
                // The engine loop exits only on shutdown or a
                // closed reports inbox — both impossible between
                // its spawn and this wiring (both senders are
                // owned below), so the push cannot fail here.
                Err(err) => {
                    unreachable!("the engine exited between spawn and module wiring: {err:?}")
                }
            }
            (commands, commands_rx)
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
                    id: spawned.id,
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
                    authority_join,
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

    let connectors = Arc::new(std::sync::Mutex::new(connectors));
    let control = ConnectorControl::new(
        Arc::clone(&connectors),
        engine.clone(),
        reports.clone(),
        system.clone(),
        config.sync_actor_config(),
        config.app.shutdown_timeout_secs,
        authority.clone(),
    );

    Ok(App {
        config,
        engine,
        system,
        connectors,
        control,
        reports,
        storage,
        storage_join,
        engine_join,
        notifier_join,
        authority,
        authority_join,
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
    authority_join: JoinHandle<()>,
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
    join_bounded(authority_join, timeout, "authority").await;
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
pub(crate) async fn wait_for_boot_pull(engine: &EngineHandle, timeout: Duration) {
    let mut signals = engine.subscribe();
    let deadline = tokio::time::Instant::now() + timeout;
    let mut seen_syncing = false;
    loop {
        let state = engine.shared_state().load_full();
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

/// The fixed small bound of rung 2's one salvage attempt at an engine
/// flush (design D-6). A parameter on [`salvage_flush`] so the
/// hermetic give-up test can force the expiry deterministically.
pub const SALVAGE_FLUSH_BUDGET: Duration = Duration::from_secs(2);

/// Rung 2's salvage (design D-6/§10 Q4–Q6): one bounded attempt at an
/// engine flush while the engine and storage actors are still alive.
/// Best-effort by design — expiry (or an engine already gone) warns
/// once and returns `false`, and the caller proceeds to exit at the
/// forcing signal's code; the exit code is never shaped by the
/// flush's fate. The abort-before-flush order is the caller's
/// ([`abort_connectors`] first): the flush roundtrips the engine and
/// storage actors, so those must stay alive through it.
pub async fn salvage_flush(app: &App, budget: Duration) -> bool {
    match tokio::time::timeout(budget, app.engine.flush()).await {
        Ok(()) => true,
        Err(_elapsed) => {
            tracing::warn!("rung 2 salvage flush missed its budget; exiting without it");
            false
        }
    }
}

/// Rung 2's actor aborts (design D-6): drains the connector registry
/// and aborts every sync actor immediately — no `Detach` send, no join
/// wait. The sync actors are the wedgers an in-flight cycle pins (the
/// engine/storage/notifier/authority actors are *not* aborted here:
/// the salvage flush needs engine+storage, and process exit is the
/// rest's terminator moments later).
///
/// # Panics
///
/// Only if the connector-lifecycle mutex is poisoned by a panic while
/// held (unreachable: the critical sections never panic).
pub fn abort_connectors(app: &App) {
    let actors: Vec<SyncActor> = app
        .connectors
        .lock()
        .expect("connector lock")
        .drain(..)
        .collect();
    for actor in actors {
        tracing::warn!(connector = %actor.id, "rung 2: aborting connector actor");
        actor.join.abort();
    }
}

/// The degraded-drain exit flip (design D-6's exit-code contract): a
/// drain that aborted any bounded join — sync actor, other actor
/// ([`ShutdownOutcome::degraded`]), or an IPC/web server drain past
/// its budget — is not a clean drain and must not masquerade as one:
/// `tracing::error!` + `exit(1)`. Production-path-only glue that exits
/// the process, so it carries no test of its own; the predicate
/// (`ShutdownOutcome::degraded`) and the aggregation
/// (`App::shutdown`) carry the behavior. `CoreStillShared` keeps its
/// eyre route to exit 1 — consistent with code 1's meaning.
pub fn exit_if_degraded(outcome: ShutdownOutcome, server_budget_hit: bool) {
    if outcome.actor_aborted {
        tracing::warn!("sync actor did not stop in time and was aborted");
    }
    if server_budget_hit {
        tracing::warn!("a server drain (ipc/web) did not finish in time");
    }
    if outcome.degraded() || server_budget_hit {
        tracing::error!("drain degraded; exiting 1");
        std::process::exit(1);
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
mod shutdown_outcome_tests {
    use super::ShutdownOutcome;

    /// T4 (design D-6): the degraded predicate is exactly the disjoin
    /// of the two abort facts — one definition, one place.
    #[test]
    fn shutdown_outcome_degraded_truth_table() {
        let clean = ShutdownOutcome {
            actor_aborted: false,
            budget_hit: false,
        };
        let actor = ShutdownOutcome {
            actor_aborted: true,
            budget_hit: false,
        };
        let budget = ShutdownOutcome {
            actor_aborted: false,
            budget_hit: true,
        };
        let both = ShutdownOutcome {
            actor_aborted: true,
            budget_hit: true,
        };
        assert!(!clean.degraded());
        assert!(actor.degraded());
        assert!(budget.degraded());
        assert!(both.degraded());
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
        /// Optional per-`SyncNow` event script: each popped
        /// `SystemEvent` is broadcast after the cycle's report — the
        /// authority test's actor-side episode control.
        events: Arc<Mutex<VecDeque<SystemEvent>>>,
        /// The actor's cycle counter (the test's observable of "this
        /// connector answered the nudge").
        cycles: Arc<std::sync::atomic::AtomicUsize>,
    }

    impl FakeModule {
        fn new(id: &str, script: Vec<SyncReport>) -> Self {
            Self::with_events(id, script, Vec::new())
        }

        fn with_events(id: &str, script: Vec<SyncReport>, events: Vec<SystemEvent>) -> Self {
            Self {
                id: taskboard_domain::ConnectorId::from(id),
                script: Arc::new(Mutex::new(script.into())),
                events: Arc::new(Mutex::new(events.into())),
                cycles: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            }
        }

        /// How many `SyncNow` cycles this connector's actor ran.
        fn cycles(&self) -> usize {
            self.cycles.load(std::sync::atomic::Ordering::SeqCst)
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
            system: broadcast::Sender<SystemEvent>,
            _cfg: taskboard_sync_nextcloud::SyncActorConfig, // unused; signature parity only
        ) -> Result<SpawnedConnector, AttachError> {
            let script = Arc::clone(&self.script);
            let events = Arc::clone(&self.events);
            let cycles = Arc::clone(&self.cycles);
            let join = tokio::spawn(async move {
                while let Some(command) = commands.recv().await {
                    match command {
                        SyncCommand::SyncNow => {
                            cycles.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                            let next = script.lock().expect("script lock").pop_front();
                            if let Some(report) = next
                                && reports.send(report).await.is_err()
                            {
                                return; // engine gone
                            }
                            // The cycle's whole episode script broadcasts
                            // with its report (report-then-event order).
                            let queued: Vec<SystemEvent> =
                                events.lock().expect("event lock").drain(..).collect();
                            for event in queued {
                                let _ = system.send(event);
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
            app_data: taskboard_domain::AppDataEvidence::Absent,
            duration: std::time::Duration::ZERO,
            connector: taskboard_domain::ConnectorId::from("fakemodule"),
            snapshot: Box::new(snapshot_of(77)),
            validators: taskboard_domain::BoardPullValidators {
                boards: taskboard_domain::SyncValidators::default(),
                stacks: taskboard_domain::SyncValidators::default(),
                archived_stacks: taskboard_domain::SyncValidators::default(),
            },
            pushes: Vec::new(),
            read_at: chrono::Utc.timestamp_opt(1_700_000_001, 0).unwrap(),
            features: taskboard_domain::DeckFeatures::CONSERVATIVE,
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

    // ---- the N=2 exit criterion, hermetic half (D5-18) ----------------

    fn completed_validators() -> taskboard_domain::BoardPullValidators {
        taskboard_domain::BoardPullValidators {
            boards: taskboard_domain::SyncValidators::default(),
            stacks: taskboard_domain::SyncValidators::default(),
            archived_stacks: taskboard_domain::SyncValidators::default(),
        }
    }

    fn completed(connector: &str, board: u64, at: chrono::DateTime<chrono::Utc>) -> SyncReport {
        SyncReport::Completed {
            app_data: taskboard_domain::AppDataEvidence::Absent,
            duration: std::time::Duration::ZERO,
            connector: taskboard_domain::ConnectorId::from(connector),
            snapshot: Box::new(snapshot_of(board)),
            validators: completed_validators(),
            pushes: Vec::new(),
            read_at: at,
            features: taskboard_domain::DeckFeatures::CONSERVATIVE,
        }
    }

    fn any_client() -> DeckClient {
        DeckClient::new("https://nc.example", "u", "t").expect("client")
    }

    /// A12 (round-1 chunk 3, DEV-1): the `ConnectorControl` handle
    /// drives the same attach/detach flow without an `&App` borrow —
    /// the spawned settings forwarder's shape. The `App` methods stay
    /// untouched (they are covered by the suites above); this covers
    /// the handle itself, and the clean shutdown after dropping it pins
    /// the reports-sender ordering.
    #[tokio::test]
    async fn the_control_handle_drives_attach_and_detach_without_an_app_reference() {
        use taskboard_domain::{CommandOutcome, StateCommand};
        let mut config = AppConfig::default();
        let dir = tempfile::tempdir().expect("tempdir");
        config.storage.db_path = Some(dir.path().join("taskboard.db"));
        // Standalone: an empty registry to attach into.
        config.sync.backend = Some(SyncBackend::Standalone);
        // #417: the attached client targets a dead identity host, so
        // detach joins the actor only after its startup probe + first
        // cycle's connect failure; make the join budget explicit
        // instead of leaning on the 5 s default under load.
        config.app.shutdown_timeout_secs = 60;
        config.validate().expect("config valid");

        let app = bootstrap_with_modules(
            config,
            Arc::new(RefusingStore),
            Arc::new(crate::notifications::LogSink),
            Vec::new(),
        )
        .await
        .expect("standalone boot");
        assert!(!app.connector_attached());

        let control = app.connector_control();
        let name = taskboard_domain::ConnectorId::from("nextcloud");
        control
            .attach_connector(name.clone(), any_client())
            .await
            .expect("attach through the handle");
        assert!(control.connector_attached());
        assert!(control.connector_commands(&name).is_some());

        assert_eq!(
            control.detach_connector_named(&name).await,
            DetachOutcome::Detached
        );
        assert_eq!(
            control.detach_connector_named(&name).await,
            DetachOutcome::NoConnector,
            "idempotent through the handle"
        );
        // The handle dies before shutdown: the reports-sender ordering
        // contract stays with `App` (the drop-before-engine-join pin).
        drop(control);

        let outcome = app
            .engine
            .execute(StateCommand::RequestSync)
            .await
            .expect("nudge accepted");
        assert_eq!(outcome, CommandOutcome::NoOp, "standalone fail-closed");
        let outcome = app.shutdown().await;
        assert!(!outcome.actor_aborted);
    }

    /// Polls a predicate against the shared state with a wall-clock
    /// deadline (never a sleep; the fake actors answer over real I/O).
    async fn wait_until(
        app: &crate::bootstrap::App,
        deadline: Duration,
        pred: impl Fn(&taskboard_domain::AppState) -> bool,
    ) {
        let end = std::time::Instant::now() + deadline;
        loop {
            if pred(&app.engine.shared_state().load_full()) {
                return;
            }
            assert!(
                std::time::Instant::now() < end,
                "predicate not satisfied within the deadline"
            );
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }

    /// The N=2 registry over two `FakeModule`s (the exit criterion's
    /// hermetic half): both modules registered under their names, the
    /// nudge fans to both (each script lands its own adopted board —
    /// adoption mints the default tag; the per-board *tagged* fixture
    /// is the tier-2 test's), the named lookup resolves exactly the
    /// registered names, a double attach refuses by name, and a named
    /// detach stops only that connector (the other still answers a
    /// nudge).
    #[tokio::test]
    async fn two_connectors_live_simultaneously_over_the_named_registry() {
        use taskboard_domain::{CommandOutcome, StateCommand};
        let mut config = AppConfig::default();
        let dir = tempfile::tempdir().expect("tempdir");
        config.storage.db_path = Some(dir.path().join("taskboard.db"));
        config.validate().expect("config valid");

        let t1 = chrono::Utc.timestamp_opt(1_700_000_001, 0).unwrap();
        let t2 = chrono::Utc.timestamp_opt(1_700_000_002, 0).unwrap();
        let module_a = Arc::new(FakeModule::new("a", vec![completed("a", 11, t1)]));
        let module_b = Arc::new(FakeModule::new(
            "b",
            vec![completed("b", 22, t1), completed("b", 22, t2)],
        ));
        let modules: Vec<Arc<dyn ConnectorModule>> = vec![module_a.clone(), module_b.clone()];

        let app = bootstrap_with_modules(
            config,
            Arc::new(RefusingStore),
            Arc::new(crate::notifications::LogSink),
            modules,
        )
        .await
        .expect("boot");

        // Both names resolve; an unknown name does not.
        let id = |name: &str| taskboard_domain::ConnectorId::from(name);
        assert!(app.connector_commands(&id("a")).is_some());
        assert!(app.connector_commands(&id("b")).is_some());
        assert!(app.connector_commands(&id("ghost")).is_none());

        // A double attach refuses by name (the b11 guard, named).
        let err = app
            .attach_connector(id("a"), any_client())
            .await
            .expect_err("the name is live");
        assert!(
            matches!(err, crate::bootstrap::AttachError::ConnectorNameInUse(ref name) if name.as_str() == "a"),
            "the refusal names the offender"
        );

        // The nudge fans to both: both boards' rows settle.
        let outcome = app
            .engine
            .execute(StateCommand::RequestSync)
            .await
            .expect("nudge accepted");
        assert_eq!(outcome, CommandOutcome::SyncRequested);
        wait_until(&app, Duration::from_secs(5), |state| {
            state.sync.iter().count() == 2
                && state.sync.iter().all(|(_, row)| {
                    matches!(row.phase, taskboard_domain::SyncPhase::Idle)
                        && row.last_success.is_some()
                })
        })
        .await;
        assert_eq!(module_a.cycles(), 1, "a answered the first nudge");
        assert_eq!(module_b.cycles(), 1, "b answered the first nudge");

        // The named detach stops only `a`; `b` still answers a nudge.
        assert_eq!(
            app.detach_connector_named(&id("a")).await,
            DetachOutcome::Detached
        );
        assert!(
            app.connector_commands(&id("a")).is_none(),
            "a is gone from the registry"
        );
        assert!(app.connector_attached(), "b survives the named detach");
        let outcome = app
            .engine
            .execute(StateCommand::RequestSync)
            .await
            .expect("nudge accepted");
        assert_eq!(outcome, CommandOutcome::SyncRequested);
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while module_b.cycles() < 2 {
            assert!(
                std::time::Instant::now() < deadline,
                "b never answered the post-detach nudge"
            );
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert_eq!(
            module_a.cycles(),
            1,
            "a ran no cycle after its named detach"
        );

        let outcome = app.shutdown().await;
        assert!(!outcome.actor_aborted);
    }

    /// The authority over a booted N=2 with scripted episodes (D5-13's
    /// integrated pin): a's lost is silent, b's lost fires the global
    /// `NetworkLost`, b's restored fires `NetworkRestored`.
    #[tokio::test]
    async fn the_authority_counts_scripted_episodes_across_two_connectors() {
        use taskboard_domain::{CommandOutcome, StateCommand};
        let mut config = AppConfig::default();
        let dir = tempfile::tempdir().expect("tempdir");
        config.storage.db_path = Some(dir.path().join("taskboard.db"));
        config.validate().expect("config valid");

        let lost = |id: &str| SystemEvent::ConnectorLost(taskboard_domain::ConnectorId::from(id));
        let restored =
            |id: &str| SystemEvent::ConnectorRestored(taskboard_domain::ConnectorId::from(id));
        let modules: Vec<Arc<dyn ConnectorModule>> = vec![
            Arc::new(FakeModule::new("a", vec![])),
            Arc::new(FakeModule::with_events(
                "b",
                vec![],
                vec![lost("a"), lost("b"), lost("b"), restored("b")],
            )),
        ];

        let app = bootstrap_with_modules(
            config,
            Arc::new(RefusingStore),
            Arc::new(crate::notifications::LogSink),
            modules,
        )
        .await
        .expect("boot");
        let mut events = app.system.subscribe();

        // One nudge drives b's whole event script; the authority must
        // emit exactly the two crossings (a's lost alone never crosses).
        let outcome = app
            .engine
            .execute(StateCommand::RequestSync)
            .await
            .expect("nudge accepted");
        assert_eq!(outcome, CommandOutcome::SyncRequested);

        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        let mut globals = Vec::new();
        while tokio::time::Instant::now() < deadline && globals.len() < 2 {
            if let Ok(Ok(global @ (SystemEvent::NetworkLost | SystemEvent::NetworkRestored))) =
                tokio::time::timeout(Duration::from_millis(100), events.recv()).await
            {
                globals.push(global);
            }
        }
        assert_eq!(
            globals,
            vec![SystemEvent::NetworkLost, SystemEvent::NetworkRestored],
            "exactly the two all-down crossings, in order"
        );

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
