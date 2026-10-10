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
//! use taskboard_app::{bootstrap, secrets};
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
//! let app = bootstrap(config, store).await?;
//!
//! let _state = app.engine.shared_state(); // lock-free UI reads
//! let outcome = app.shutdown().await;     // graceful, bounded
//! assert!(!outcome.actor_aborted);
//! # Ok(())
//! # }
//! ```

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{broadcast, mpsc};
use tokio::task::JoinHandle;

use taskboard_domain::{SyncCommand, SyncReport, SyncStateReader, SystemEvent, TaskRepository};
use taskboard_state::{EngineHandle, spawn_state_engine};
use taskboard_storage_sqlite::{StorageHandle, spawn_storage_actor};
use taskboard_sync_nextcloud::{DeckClient, spawn_sync_actor};

use crate::config::{AppConfig, AppMode, ConfigError};
use crate::secrets::{CredentialError, CredentialStore};

/// Channel capacity of the system broadcast (`NetworkLost`,
/// `NetworkRestored`, `Shutdown`).
pub const SYSTEM_BROADCAST_CAPACITY: usize = 64;
/// Channel capacity of the sync-command inbox (`SyncNow`/`SetBoard`
/// nudges; "full" already means "coalesced").
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
    /// The state engine failed to boot (hydration).
    #[error("state engine failed to start")]
    Engine(#[from] taskboard_state::EngineStartupError),
    /// The Deck client rejected the server URL.
    #[error("deck client construction failed")]
    Client(#[from] taskboard_sync_nextcloud::DeckError),
    /// The configured `[app] mode` has no frontend yet (Phase 7).
    #[error("app mode not implemented yet")]
    ModeNotImplemented(AppMode),
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
    sync_commands: mpsc::Sender<SyncCommand>,
    storage: StorageHandle,
    storage_join: JoinHandle<()>,
    engine_join: JoinHandle<()>,
    sync_join: JoinHandle<()>,
}

impl std::fmt::Debug for App {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("App")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl App {
    /// A second sender into the sync actor's inbox — the Phase 6
    /// `SetBoard` seam (`boards select`). Keeping it open is also why
    /// actor shutdown keys on `Shutdown`, never on inbox close.
    #[must_use]
    pub fn sync_commands(&self) -> mpsc::Sender<SyncCommand> {
        self.sync_commands.clone()
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
    pub async fn shutdown(self) -> ShutdownOutcome {
        let timeout = Duration::from_secs(self.config.app.shutdown_timeout_secs);
        tracing::info!("shutdown: flushing the engine");
        self.engine.flush().await;
        tracing::info!("shutdown: broadcasting Shutdown");
        // No live receivers is fine (engine-only setups in tests).
        let _ = self.system.send(SystemEvent::Shutdown);

        let actor_aborted = join_bounded(self.sync_join, timeout, "sync actor").await;
        join_bounded(self.engine_join, timeout, "state engine").await;

        tracing::info!("shutdown: stopping the storage actor");
        drop(self.storage);
        if let Err(join_err) = self.storage_join.await {
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
pub async fn bootstrap(
    config: AppConfig,
    credentials: Arc<dyn CredentialStore>,
) -> Result<App, BootstrapError> {
    if let mode @ (AppMode::Desktop | AppMode::Kiosk) = config.app.mode {
        return Err(BootstrapError::ModeNotImplemented(mode));
    }
    config.validate()?;
    let server = config.server_url().to_owned();
    let username = config.username().to_owned();
    tracing::info!(
        server = %server,
        username = %username,
        store = ?config.nextcloud.credential_store,
        db = %config.db_path().display(),
        mode = ?config.app.mode,
        poll = config.sync.poll_interval_secs,
        "resolved configuration"
    );

    // Credential resolution before anything spawns: a missing secret
    // must not leave half a graph behind.
    let password = credentials.load(&server, &username)?;
    let client = DeckClient::new(&server, &username, password.expose())?;

    let db_path = config.db_path().to_owned();
    if let Some(parent) = db_path.parent()
        && !parent.as_os_str().is_empty()
    {
        tokio::fs::create_dir_all(&parent)
            .await
            .map_err(BootstrapError::StorageDir)?;
    }
    tracing::info!(db = %db_path.display(), "opening database");
    let repo = Arc::new(taskboard_storage_sqlite::open(&db_path).await?);
    let (storage, storage_join) = spawn_storage_actor(repo);

    let (system, system_rx_engine) = broadcast::channel::<SystemEvent>(SYSTEM_BROADCAST_CAPACITY);
    let (sync_commands, sync_rx) = mpsc::channel::<SyncCommand>(SYNC_COMMAND_CAPACITY);
    let (reports, report_rx) = mpsc::channel::<SyncReport>(SYNC_REPORT_CAPACITY);

    // The engine reaches the database only through the storage actor
    // (single-writer discipline): the handle *is* the port adapter.
    let repo_port: Arc<dyn TaskRepository> = Arc::new(storage.clone());
    let spawned = spawn_state_engine(
        repo_port,
        Arc::new(taskboard_domain::idgen::UuidV7Generator),
        Arc::new(taskboard_domain::SystemClock),
        sync_commands.clone(),
        report_rx,
        system_rx_engine,
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

    let sync_join = spawn_sync_actor(
        client,
        Arc::new(engine.clone()) as Arc<dyn SyncStateReader>,
        sync_rx,
        reports,
        system.clone(),
        config.sync_actor_config(),
    );
    tracing::info!("actor graph spawned: storage, state engine, sync");

    Ok(App {
        config,
        engine,
        system,
        sync_commands,
        storage,
        storage_join,
        engine_join,
        sync_join,
    })
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
