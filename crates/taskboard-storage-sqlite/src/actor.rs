// SPDX-License-Identifier: MIT OR Apache-2.0
//! The storage actor: an mpsc command loop in front of one repository.
//!
//! Every [`StorageCommand`] carries a `oneshot` reply with the typed
//! result — the actor replies, it never broadcasts errors (plan decision
//! 13). mpsc FIFO ordering makes "await the reply of the last Apply" the
//! natural flush barrier for the CLI exit path. The actor adds no policy:
//! repo semantics are the port contract's.

use std::sync::Arc;

use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

use taskboard_domain::persistence::{
    PersistedState, PersistenceAction, RepositoryError, TaskRepository,
};

use crate::repo::SqliteTaskRepository;

/// Storage actor inbox: the channel-bearing envelope this crate owns, with
/// the domain payloads inside (payload/envelope split).
#[derive(Debug)]
pub enum StorageCommand {
    /// Apply a batch atomically; replies with the typed outcome.
    Apply {
        /// The batch (entity upserts + outbox/validator/status transitions).
        actions: Vec<PersistenceAction>,
        /// Request-response seam for the caller.
        reply: oneshot::Sender<Result<(), RepositoryError>>,
    },
    /// Full hydration; replies with the typed outcome.
    Load {
        /// Request-response seam for the caller.
        reply: oneshot::Sender<Result<PersistedState, RepositoryError>>,
    },
}

/// Cloneable handle to a running storage actor. Dropping the last handle
/// closes the inbox and the actor loop exits.
#[derive(Debug, Clone)]
pub struct StorageHandle {
    tx: mpsc::Sender<StorageCommand>,
}

impl StorageHandle {
    /// Applies a batch through the actor and awaits its reply.
    ///
    /// # Errors
    ///
    /// [`RepositoryError`] from the repository, or `Unavailable` when the
    /// actor is gone (inbox closed / reply dropped).
    pub async fn apply(&self, actions: Vec<PersistenceAction>) -> Result<(), RepositoryError> {
        tracing::debug!(actions = actions.len(), "storage command: apply");
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(StorageCommand::Apply { actions, reply })
            .await
            .map_err(|_| RepositoryError::Unavailable)?;
        rx.await.map_err(|_| RepositoryError::Unavailable)?
    }

    /// Loads the persisted state through the actor and awaits its reply.
    ///
    /// # Errors
    ///
    /// [`RepositoryError`] from the repository, or `Unavailable` when the
    /// actor is gone (inbox closed / reply dropped).
    pub async fn load(&self) -> Result<PersistedState, RepositoryError> {
        tracing::debug!("storage command: load");
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(StorageCommand::Load { reply })
            .await
            .map_err(|_| RepositoryError::Unavailable)?;
        rx.await.map_err(|_| RepositoryError::Unavailable)?
    }
}

/// Spawns the actor loop onto the current runtime; returns the handle and
/// the task's `JoinHandle` (the future Phase 5 bootstrap awaits it for a
/// clean shutdown once all handles are dropped).
#[must_use]
pub fn spawn_storage_actor(repo: Arc<SqliteTaskRepository>) -> (StorageHandle, JoinHandle<()>) {
    let (tx, mut rx) = mpsc::channel::<StorageCommand>(64);
    let join = tokio::spawn(async move {
        tracing::info!("storage actor started");
        while let Some(command) = rx.recv().await {
            match command {
                StorageCommand::Apply { actions, reply } => {
                    let result = repo.apply(actions).await;
                    if let Err(err) = &result {
                        tracing::warn!(?err, "storage apply failed");
                    }
                    // A dropped receiver just means the caller stopped
                    // caring; the outcome is already persisted.
                    let _ = reply.send(result);
                }
                StorageCommand::Load { reply } => {
                    let result = repo.load().await;
                    if let Err(err) = &result {
                        tracing::warn!(?err, "storage load failed");
                    }
                    let _ = reply.send(result);
                }
            }
        }
        tracing::info!("storage actor stopped");
    });
    (StorageHandle { tx }, join)
}
